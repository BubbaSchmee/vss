# VSS — Vowel Step Sequencer

Open-source VST3 audio effect for Ableton Live 12 Standard (Windows). A 3-band
formant filter whose vowel is chosen by a tempo-synced 16-step sequencer, so any
bass "talks" in riddim/dubstep patterns without automation lanes.

This file is the single source of truth. Every agent reads it first. Change it
only when the orchestrator says so.

## Stack (fixed)
- Rust stable MSVC (installed at `%USERPROFILE%\.cargo\bin`; add to PATH in every shell).
- `nih_plug` from git `https://github.com/robbert-vdh/nih-plug.git`, feature `assert_process_allocs`.
- GUI: `nih_plug_egui` (same git repo). Functional look, dark theme. No custom-drawn knobs.
- Bundling: `nih_plug_xtask` via `cargo xtask bundle vss --release` → `target/bundled/VSS.vst3`.
  Also emit CLAP (free with nih-plug). VST3 is the deliverable.
- License GPL-3.0-or-later (required by VST3 terms). Vendor "VSS Open Source", url github.com/BubbaSchmee/vss.
- pluginval at `%LOCALAPPDATA%\pluginval\pluginval.exe`, strictness 5, must pass:
  `pluginval.exe --strictness-level 5 --validate-in-process --validate target/bundled/VSS.vst3`
- Windows only. No macOS/Linux claims anywhere.

## Crate layout
```
Cargo.toml            workspace: members = ["xtask"]; [lib] crate-type = ["cdylib","lib"]
.cargo/config.toml    alias xtask = "run --package xtask --release --"
xtask/                nih_plug_xtask::main()
src/lib.rs            Plugin impl + exports (nih_export_vst3!, nih_export_clap!)
src/params.rs         VssParams (contract below) + Vowel/Rate enums + VOWELS table
src/dsp/formant.rs    FormantFilter (per channel): 3 RBJ bandpass biquads, glide, coefficient update
src/dsp/sequencer.rs  Sequencer: beat position → step index, swing, free-run
src/dsp/mod.rs        drive (tanh) helper, db<->gain
src/editor.rs         egui editor
examples/demo.rs      renders demo/vss-demo.wav (std-only WAV writer, 16-bit PCM 48 kHz)
tests/                sequencer timing + formant peak tests (see Verification)
tools/live-debug.bat  sets NIH_LOG and launches Live
tools/analyze.py      analyzes a resampled session WAV (numpy/scipy present)
tools/CHECKLIST.md    manual Live session checklist
sessions/.gitkeep     user drops recorded WAVs here (gitignored except .gitkeep)
.github/workflows/ci.yml
README.md, LICENSE, .gitignore
```

## Parameter contract (ids are stable; never rename)
| id | type | range / variants | default | unit |
|---|---|---|---|---|
| `formant_shift` | Float | -12.0 .. 12.0 | 0.0 | st |
| `resonance` | Float | 0.0 .. 1.0 | 0.5 | maps to Q 2..20 (exponential) |
| `drive` | Float | 0.0 .. 24.0 | 0.0 | dB into tanh, 0 = bypassed (unity, no tanh) |
| `steps` | Int | 1 .. 16 | 8 | |
| `rate` | Enum Rate | Quarter, Eighth, EighthTriplet, Sixteenth, SixteenthTriplet | Eighth | display "1/4","1/8","1/8T","1/16","1/16T" |
| `swing` | Float | 0.0 .. 1.0 | 0.0 | 1.0 = full triplet feel (odd steps delayed by step_len/3) |
| `glide` | Float | 0.0 .. 500.0 | 20.0 | ms, log skew |
| `step_1` .. `step_16` | Enum Vowel | A, E, I, O, U, Hold | pattern below | |
| `mix` | Float | 0.0 .. 1.0 | 1.0 | dry/wet, linear |
| `output_gain` | Float | -24.0 .. 12.0 | 0.0 | dB |

Default pattern (steps 1..16): A O A E U O A I A O A E U O I A.
Display names: "Formant Shift", "Resonance", "Drive", "Steps", "Rate", "Swing",
"Glide", "Step 1".."Step 16", "Mix", "Output".

Float params use nih_plug smoothing where audible (formant_shift, resonance: 20 ms; mix,
output_gain, drive: 10 ms).

## Vowel table (Peterson & Barney 1952, male) — F1/F2/F3 Hz, relative band gains
| Vowel | F1 | F2 | F3 | gains (dB) |
|---|---|---|---|---|
| A | 730 | 1090 | 2440 | 0, -6, -12 |
| E | 530 | 1840 | 2480 | 0, -6, -12 |
| I | 270 | 2290 | 3010 | 0, -6, -12 |
| O | 570 | 840 | 2410 | 0, -6, -12 |
| U | 300 | 870 | 2240 | 0, -6, -12 |

Hold = keep the previous step's vowel target (no new glide). If step 1 is Hold, use A.

## DSP
Per channel (stereo, channels processed independently with identical coefficients):
1. Three RBJ bandpass biquads (constant 0 dB peak gain form), centers `F_k * 2^(formant_shift/12)`,
   clamped to [40 Hz, 0.45*sr]. Q = 2 * 10^(resonance) (0→2, 1→20).
2. Glide: the three *current* center frequencies move toward the target vowel's frequencies with a
   one-pole in the log-frequency domain, time constant = glide ms (0 ms = jump). Recompute biquad
   coefficients every 32 samples (or on any parameter change), not per sample.
3. Sum the three band outputs weighted by the gains column, then apply a fixed makeup constant
   calibrated so a full-scale (0 dBFS) 55 Hz sawtooth through vowel A at default resonance (0.5) and
   shift 0 peaks at about 1.0. Document the constant. (Revised 2026-09-28: the earlier white-noise
   calibration put real bass at about +10 dBFS at the defaults.)
4. Drive: if drive > 0: `tanh(x * g) / tanh(g)` with `g = db_to_gain(drive)`, else pass-through.
5. Mix: `dry*(1-mix) + wet*mix`. Then output_gain.
No latency. No allocation in `process()` (assert_process_allocs will panic if violated).
Guard against denormals; reset filter state on `reset()`.

## Sequencer
- Step length in beats: 1/4=1.0, 1/8=0.5, 1/8T=1/3, 1/16=0.25, 1/16T=1/6.
- Position source: `context.transport()` at buffer start: `pos_beats()`, `tempo()`, `playing`.
  Advance per sample: `beats_per_sample = tempo / 60 / sample_rate`. Evaluate the step index
  **per sample** so boundaries inside a buffer are exact.
- Step index = `floor(beat / step_len) mod steps`, with swing: every odd step (1,3,5.. zero-based)
  starts later by `swing * step_len / 3`. Implement as: within each 2-step pair, the boundary sits at
  `step_len + swing*step_len/3` instead of `step_len`.
- Free-run: when `playing == false` (Live transport stopped, user jamming Serum live), keep an
  internal beat counter advancing at host tempo (fallback 120 BPM if tempo is None). On the
  transition stopped→playing, snap to host `pos_beats`. Essential: the user auditions with the
  transport stopped.
- Loop wrap: while playing and the host reports an active loop range, samples extrapolated past
  `loop_end` inside a buffer map to `loop_start + overshoot`, so a loop wrap mid-buffer never plays
  the wrong step.
- Pattern length = `steps`; changing `steps` takes effect at the next step boundary.
- Publish the current step index to the GUI via `Arc<AtomicUsize>`; GUI polls at repaint.

## Logging (nih_log!, always compiled, low volume)
- On `initialize`: sample rate, max buffer size.
- On transport play/stop change and on tempo change: state, tempo, pos_beats.
- On each step change: step index, vowel, pos_beats — ONLY when the NIH_LOG env var is set (check
  once in initialize, store a bool). Normal sessions must do no per-step I/O on the audio thread.
Panics are captured by nih_plug's hook into the same NIH_LOG file.

## GUI (nih_plug_egui)
- Window 760×380, fixed size. Dark theme.
- Top row: ParamSlider for Formant Shift, Resonance, Drive, Glide, Swing, Mix, Output; Rate combo; Steps slider.
- Grid: 16 columns × 6 rows (A E I O U Hold). Clicking a cell sets that step's vowel via
  `setter.begin_set_parameter / set_parameter / end_set_parameter`. Columns ≥ `steps` drawn dimmed.
  Current step column highlighted using the shared atomic. Repaint continuously while open
  (`ctx.request_repaint_after(33 ms)`).
- Header: "VSS — Vowel Step Sequencer" + version.

## Verification (must all pass before handoff)
1. `cargo test` — tests use std only:
   - sequencer: at 150 BPM / 48 kHz, for each Rate and swing ∈ {0, 0.5, 1}, the step index at sample
     positions matches a reference computed independently in the test; free-run advances when not
     playing; snaps on play.
   - formant: render a 55 Hz sawtooth (2 s, 48 kHz) through FormantFilter for each vowel with
     glide 0, shift 0, resonance 0.7. Measure magnitude via Goertzel over a ±10 % sweep around each
     table frequency; assert the local peak lies within 5 % of F1 and F2 (F3 within 8 %).
   - drive: unity at drive 0; no NaN for ±10.0 input at drive 24.
2. `cargo xtask bundle vss --release` succeeds.
3. pluginval strictness 5 passes on `target/bundled/VSS.vst3`.
4. `cargo run --example demo` writes `demo/vss-demo.wav`: 8 bars @150 BPM, synthesized 55 Hz saw
   bass gated to 1/8 notes, default pattern, resonance 0.6, drive 6 dB. Committed to the repo.
5. `python tools/analyze.py sessions/<file>.wav --bpm 150 --rate 1/8 --steps 8` prints per-step
   detected dominant formant frequency vs expected vowel, and timing drift in ms.

## CI (.github/workflows/ci.yml)
windows-latest; dtolnay/rust-toolchain@stable; Swatinem/rust-cache; `cargo test`;
`cargo xtask bundle vss --release`; download pluginval_Windows.zip from the latest release and run
strictness 5; upload `target/bundled/VSS.vst3` as artifact `VSS-windows`. On tags `v*`: zip the
bundle and create a GitHub Release with softprops/action-gh-release.

## Non-goals for v1
Vocoder/sidechain mode, unvoiced noise, internal carrier, per-step level, preset browser,
custom knob graphics, macOS/Linux, MIDI input.
