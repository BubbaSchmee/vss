//! Renders `demo/vss-demo.wav` (SPEC.md "Verification" 4): 8 bars at 150 BPM of a 55 Hz saw
//! gated to 1/8 notes, through the real engine with the default pattern, resonance 0.6 and
//! 6 dB drive. 16-bit PCM stereo, 48 kHz, std-only WAV writer.
//!
//! `cargo run --example demo`. `tests/formant.rs` also includes this file to check the render.

use std::io::Write;
use vss::dsp::{BlockParams, Engine, FrameParams};
use vss::params::{Rate, DEFAULT_PATTERN};

pub const SAMPLE_RATE: u32 = 48_000;
pub const BPM: f64 = 150.0;
/// Samples per 1/8 note at 150 BPM / 48 kHz.
pub const EIGHTH: usize = 9_600;
const BARS: usize = 8;
const BLOCK: usize = 512;

/// The rendered demo as stereo frames.
pub fn render() -> Vec<[f32; 2]> {
    let total = BARS * 8 * EIGHTH;
    let mut engine = Engine::new();
    engine.set_sample_rate(SAMPLE_RATE as f32, 2);
    let block = BlockParams {
        steps: 8,
        rate: Rate::Eighth,
        swing: 0.0,
        glide_ms: 20.0,
        pattern: DEFAULT_PATTERN,
    };
    let frame = FrameParams {
        formant_shift: 0.0,
        resonance: 0.6,
        drive_db: 6.0,
        mix: 1.0,
        output_gain_db: 0.0,
    };

    // Naive 55 Hz saw, gated for the first 75 % of every 1/8 with 1 ms / 5 ms linear ramps.
    // ponytail: naive saw aliases ~-50 dB at 55 Hz; PolyBLEP it if the demo ever sounds gritty.
    let mut phase = 0.0f64;
    let mut out = Vec::with_capacity(total);
    for n in 0..total {
        if n % BLOCK == 0 {
            let pos_beats = n as f64 * BPM / (60.0 * SAMPLE_RATE as f64);
            engine.begin_block(true, Some(pos_beats), Some(BPM), None, &block);
        }
        let t = n % EIGHTH;
        let gate_len = EIGHTH * 3 / 4;
        let env = if t >= gate_len {
            0.0
        } else {
            (t as f32 / 48.0)
                .min(1.0)
                .min((gate_len - t) as f32 / 240.0)
        };
        // 0.655 (-3.7 dBFS) puts the render at about -14 dBFS RMS with the saw-calibrated makeup.
        let saw = (2.0 * phase - 1.0) as f32 * 0.655 * env;
        phase = (phase + 55.0 / SAMPLE_RATE as f64).fract();

        let mut l = saw;
        let mut r = saw;
        engine.process_frame([&mut l, &mut r], &frame);
        out.push([l, r]);
    }
    out
}

fn write_wav(path: &std::path::Path, frames: &[[f32; 2]]) -> std::io::Result<()> {
    let data_len = (frames.len() * 4) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&2u16.to_le_bytes()); // channels
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&4u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in frames {
        for &s in frame {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::File::create(path)?.write_all(&bytes)
}

fn main() -> std::io::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/vss-demo.wav");
    let frames = render();
    write_wav(&path, &frames)?;
    println!("wrote {} ({} frames)", path.display(), frames.len());
    Ok(())
}
