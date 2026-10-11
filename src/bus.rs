//! RR-BUS integration (riddim-rack SPEC "RR-BUS"): VSS publishes `Step Gate`, `Vowel` and `Step`,
//! and has three CV-in slots that add `amount x value` to the normalized value of a target param.
//!
//! Threads:
//! - host/GUI thread: [`Modulator::register`] (called from `initialize`/`reset`), [`Link`] methods.
//! - audio thread: only [`Modulator::begin_block`], [`Modulator::next`], [`Modulator::publish`].
//!   They do atomic loads/stores, a `try_read` on a slot (never blocks, never allocates).
//!
//! GUI -> audio hand-off: each CV slot is a `RwLock<Option<Subscription>>`. The GUI takes the write
//! lock only for the pointer swap and drops the old subscription after unlocking; the audio thread
//! `try_read`s, and on contention keeps the slot's last value for that block.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use nih_plug::prelude::*;
use rr_bus::{Bus, Instance, Kind, Source, Subscription};

use crate::params::{CvTarget, VssParams};

/// One-pole smoothing time for every CV-in offset.
pub const SMOOTH_MS: f32 = 5.0;
/// Number of CV-in slots.
pub const SLOTS: usize = 3;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// One CV-in slot, shared between the GUI (bind/unbind/rebind, activity LED) and the audio thread.
struct Slot {
    sub: RwLock<Option<Subscription>>,
    /// `|last read value|` (f32 bits), for the activity LED.
    activity: AtomicU32,
}

impl Slot {
    /// Audio thread. Unbound or lost reads 0; a contended slot keeps `last`.
    #[inline]
    fn read(&self, last: f32) -> f32 {
        let v = match self.sub.try_read() {
            Ok(g) => g.as_ref().and_then(|s| s.read()).unwrap_or(0.0),
            Err(_) => last,
        };
        let v = if v.is_finite() { v } else { 0.0 };
        self.activity.store(v.abs().min(1.0).to_bits(), Ordering::Relaxed);
        v
    }
}

/// State shared by the plugin and its editor: the bus handle, the registered instance (for the
/// label) and the three CV-in slots.
pub struct Link {
    params: Arc<VssParams>,
    bus: Mutex<Bus>,
    instance: Mutex<Option<Arc<Instance>>>,
    slots: [Slot; SLOTS],
}

impl Link {
    /// A link on a disabled bus; `initialize` opens the real one with [`Link::set_bus`].
    pub fn new(params: Arc<VssParams>) -> Arc<Link> {
        let slot = || Slot { sub: RwLock::new(None), activity: AtomicU32::new(0) };
        Arc::new(Link {
            params,
            bus: Mutex::new(Bus::disabled()),
            instance: Mutex::new(None),
            slots: [slot(), slot(), slot()],
        })
    }

    pub fn set_bus(&self, bus: Bus) {
        *lock(&self.bus) = bus;
    }

    pub fn bus(&self) -> Bus {
        lock(&self.bus).clone()
    }

    /// The instance label: live from the bus once registered, else the persisted one.
    pub fn label(&self) -> String {
        match lock(&self.instance).as_ref() {
            Some(i) => i.label(),
            None => self.params.instance_label.read().map(|l| l.clone()).unwrap_or_default(),
        }
    }

    /// GUI thread: renames the instance and persists the label the bus actually took (rr-bus
    /// keeps labels unique, so a duplicated track's copy may become `"<label> (2)"`).
    pub fn set_label(&self, label: &str) {
        let mut live = String::new();
        if let Some(i) = lock(&self.instance).as_ref() {
            i.set_label(label);
            live = i.label();
        }
        if let Ok(mut l) = self.params.instance_label.write() {
            *l = if live.is_empty() { label.to_string() } else { live };
        }
    }

    /// The persisted `"<label> · <name>"` strings (empty = none).
    pub fn persisted_sources(&self) -> [String; SLOTS] {
        self.params.cv_sources.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// GUI thread: binds slot `i` to `persisted` (`""` unbinds) and persists the string.
    pub fn bind(&self, i: usize, persisted: &str) {
        let new = (!persisted.is_empty()).then(|| Subscription::bind_persisted(&self.bus(), persisted));
        let old = std::mem::replace(&mut *self.slots[i].sub.write().unwrap_or_else(|e| e.into_inner()), new);
        drop(old); // freed here, outside the lock and off the audio thread
        self.slots[i].activity.store(0, Ordering::Relaxed);
        if let Ok(mut s) = self.params.cv_sources.write() {
            s[i] = persisted.to_string();
        }
    }

    /// Re-binds every slot from the persisted strings (state load).
    pub fn bind_persisted(&self) {
        for (i, s) in self.persisted_sources().iter().enumerate() {
            self.bind(i, s);
        }
    }

    /// GUI thread (~4x/s while the editor is open) and background task (~4x/s from `process`):
    /// re-resolves lost subscriptions and keeps the instance's heartbeat fresh while the host is
    /// not processing.
    pub fn refresh(&self) {
        for slot in &self.slots {
            if let Ok(g) = slot.sub.read() {
                if let Some(s) = g.as_ref() {
                    s.rebind_if_lost();
                }
            }
        }
        if let Some(i) = lock(&self.instance).as_ref() {
            i.heartbeat();
        }
    }

    /// Slot `i` has a binding that is currently live.
    pub fn is_bound(&self, i: usize) -> bool {
        self.slots[i].sub.read().map(|g| g.as_ref().is_some_and(|s| !s.is_lost())).unwrap_or(false)
    }

    /// `|last value|` read by slot `i`, 0..1.
    pub fn activity(&self, i: usize) -> f32 {
        f32::from_bits(self.slots[i].activity.load(Ordering::Relaxed))
    }
}

/// The audio-side half: owns the registered instance and the published sources, and turns the
/// subscriptions into smoothed per-slot offsets.
pub struct Modulator {
    link: Arc<Link>,
    reg: Option<Registered>,
    last: [f32; SLOTS],
    goal: [f32; SLOTS],
    smooth: [f32; SLOTS],
    coef: f32,
    /// Samples since `process` last scheduled a background [`Link::refresh`].
    since_refresh: usize,
}

struct Registered {
    inst: Arc<Instance>,
    gate: Source,
    vowel: Source,
    step: Source,
}

impl Modulator {
    pub fn new(link: Arc<Link>) -> Self {
        Self { link, reg: None, last: [0.0; SLOTS], goal: [0.0; SLOTS], smooth: [0.0; SLOTS], coef: 1.0, since_refresh: 0 }
    }

    pub fn link(&self) -> &Arc<Link> {
        &self.link
    }

    /// Host thread, from `initialize` (which the host re-runs on every activation and after every
    /// state load): registers once and keeps that record, so subscribers to this instance stay
    /// bound; re-applies the persisted label and binds the persisted sources. Allocates.
    pub fn initialize(&mut self, bus: Bus) {
        if self.reg.as_ref().is_some_and(|r| r.inst.is_enabled()) {
            let saved = self.link.params.instance_label.read().map(|l| l.clone()).unwrap_or_default();
            if !saved.is_empty() {
                self.link.set_label(&saved);
            }
        } else {
            self.drop_registration(); // frees a stale record first so the label can be reused
            self.link.set_bus(bus);
            self.register();
        }
        self.link.bind_persisted();
        self.smooth = [0.0; SLOTS];
        self.goal = [0.0; SLOTS];
    }

    /// Host thread, from `reset`: a host that paused `process` for over 2 s gets reclaimed, so
    /// register again when the record is gone. Allocates only in that case.
    pub fn ensure_registered(&mut self) {
        if !self.reg.as_ref().is_some_and(|r| r.inst.is_enabled()) {
            self.drop_registration();
            self.register();
        }
    }

    fn drop_registration(&mut self) {
        self.reg = None;
        *lock(&self.link.instance) = None;
    }

    fn register(&mut self) {
        let bus = self.link.bus();
        let inst = Arc::new(bus.register("VSS"));
        let saved = self.link.params.instance_label.read().map(|l| l.clone()).unwrap_or_default();
        if !saved.is_empty() {
            inst.set_label(&saved);
        }
        // Persist the default (or the de-duplicated saved label) so a saved set re-resolves it.
        let live = inst.label();
        if !live.is_empty() && live != saved {
            if let Ok(mut l) = self.link.params.instance_label.write() {
                *l = live;
            }
        }
        let reg = Registered {
            gate: inst.publish("Step Gate", Kind::Gate),
            vowel: inst.publish("Vowel", Kind::Unipolar),
            step: inst.publish("Step", Kind::Unipolar),
            inst,
        };
        *lock(&self.link.instance) = Some(reg.inst.clone());
        self.reg = Some(reg);
    }

    /// Audio thread, once per block: heartbeat, read the three slots and set the smoothing goals.
    /// A slot whose target is `Off` has goal 0.
    pub fn begin_block(&mut self, targets: [CvTarget; SLOTS], amounts: [f32; SLOTS], sample_rate: f32) {
        if let Some(r) = &self.reg {
            r.inst.heartbeat();
        }
        for i in 0..SLOTS {
            let raw = self.link.slots[i].read(self.last[i]);
            self.last[i] = raw;
            self.goal[i] = if targets[i] == CvTarget::Off { 0.0 } else { amounts[i] * raw };
        }
        self.coef = 1.0 - (-1.0 / (SMOOTH_MS * 0.001 * sample_rate.max(1.0))).exp();
    }

    /// Audio thread, once per block of `n` samples: true about every 250 ms, when `process`
    /// should schedule a background [`Link::refresh`] (lost sources must re-resolve without the
    /// editor being open).
    pub fn refresh_due(&mut self, n: usize, sample_rate: f32) -> bool {
        self.since_refresh += n;
        let due = self.since_refresh as f32 >= 0.25 * sample_rate;
        if due {
            self.since_refresh = 0;
        }
        due
    }

    /// Audio thread, once per block after processing: publish `(Step Gate, Vowel, Step)`.
    pub fn publish(&self, signals: (f32, f32, f32)) {
        if let Some(r) = &self.reg {
            r.gate.set(signals.0);
            r.vowel.set(signals.1);
            r.step.set(signals.2);
        }
    }

    /// Audio thread, per sample: advance the 5 ms one-pole and return each slot's normalized
    /// offset. Settled values snap to the goal so an idle slot is exactly 0.
    #[inline]
    pub fn next(&mut self) -> [f32; SLOTS] {
        for i in 0..SLOTS {
            let d = self.goal[i] - self.smooth[i];
            self.smooth[i] = if d.abs() < 1e-6 { self.goal[i] } else { self.smooth[i] + self.coef * d };
        }
        self.smooth
    }

    /// The current smoothed per-slot offsets without advancing.
    pub fn current(&self) -> [f32; SLOTS] {
        self.smooth
    }
}

/// Sums per-slot offsets into one offset per target (`CvTarget` index; `Off`'s entry is unused).
#[inline]
pub fn target_offsets(targets: [CvTarget; SLOTS], slot_offsets: [f32; SLOTS]) -> [f32; 6] {
    let mut o = [0.0; 6];
    for i in 0..SLOTS {
        if targets[i] != CvTarget::Off {
            o[targets[i].to_index()] += slot_offsets[i];
        }
    }
    o
}

/// `plain` with `offset` added in normalized space, clamped 0..1, back to plain. Exact
/// pass-through when the offset is 0. Never touches the host-visible parameter value.
#[inline]
pub fn modulate(p: &FloatParam, plain: f32, offset: f32) -> f32 {
    if offset == 0.0 {
        plain
    } else {
        p.preview_plain((p.preview_normalized(plain) + offset).clamp(0.0, 1.0))
    }
}
