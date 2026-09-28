# VSS — Vowel Step Sequencer

Open-source VST3 audio effect for Ableton Live 12 Standard (Windows). A 3-band
formant filter whose vowel is chosen by a tempo-synced 16-step sequencer, so
any bass "talks" in riddim/dubstep patterns without automation lanes.

**Status: pre-alpha, Windows only.**

Copyright (C) 2026 VSS Open Source contributors

This program is free software: you can redistribute it and/or modify it
under the terms of the GNU General Public License as published by the Free
Software Foundation, either version 3 of the License, or (at your option)
any later version. This program is distributed in the hope that it will be
useful, but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General
Public License for more details.

VST is a trademark of Steinberg Media Technologies GmbH.

## Build

```
git clone https://github.com/BubbaSchmee/vss.git
cd vss
cargo xtask bundle vss --release
```

Requires Rust (stable, MSVC toolchain) and the MSVC Build Tools. The bundled
plugin is written to `target/bundled/VSS.vst3` (a CLAP build is emitted
alongside it, free with `nih-plug`).

## Install

The built plugin is the **folder** `target/bundled/VSS.vst3`. Pick one:

**(a) Copy to the system VST3 folder (needs an elevated prompt):**

```
robocopy /E target\bundled\VSS.vst3 "C:\Program Files\Common Files\VST3\VSS.vst3"
```

Then rescan plugins in Live.

**(b) Add a custom VST3 folder in Live (no admin needed):**

Live > Preferences > Plug-Ins > add `target\bundled` as a custom VST3
folder, then Rescan.

## Known limitations

- The GUI needs OpenGL 3.2. Opening the editor window over Remote Desktop,
  in a VM, or without a GPU driver can crash the host — audio processing
  without the window open is unaffected. Use Live's generic parameter panel
  in those environments instead.
- At 125%/150% Windows display scaling the editor window renders at 1:1
  pixels and looks small.
- Windows only; macOS untested.
- Typing values into slider text boxes may not receive keystrokes in Live —
  drag the slider instead.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) and
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for third-party components.
