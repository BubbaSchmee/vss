# VSS — Vowel Step Sequencer

Open-source VST3 audio effect for Ableton Live 12 Standard (Windows). A 3-band
formant filter whose vowel is chosen by a tempo-synced 16-step sequencer, so
any bass "talks" in riddim/dubstep patterns without automation lanes.

**Status: pre-alpha, Windows only.**

## Install

Copy `target/bundled/VSS.vst3` to:

```
C:\Program Files\Common Files\VST3
```

Then rescan plugins in your DAW.

## Build

```
git clone <this repo>
cd vss
cargo xtask bundle vss --release
```

Requires Rust (stable, MSVC toolchain) and the MSVC Build Tools. The bundled
plugin is written to `target/bundled/VSS.vst3` (a CLAP build is emitted
alongside it, free with `nih-plug`).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
