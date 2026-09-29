# VSS manual Live session checklist

0. `export CARGO_TARGET_DIR=/d/cargo-target/vss` before any cargo command
   on this machine (C: drive is full). The bundle this produces is
   `D:\cargo-target\vss\bundled\VSS.vst3`.

Logs:
- Plugin log: `%LOCALAPPDATA%\VSS\vss-live.log`
- Live's own log: `%AppData%\Ableton\Live 12.4.6\Preferences\Log.txt`

Note: close Live completely before running `tools/live-debug.bat`, otherwise
`NIH_LOG` never reaches the plugin. Delete `%LOCALAPPDATA%\VSS\vss-live.log`
first for a clean log.

## Steps

1. Live > Preferences > Plug-Ins > set "Use VST3 Plug-In Custom Folder" to
   `C:\Users\kolby\Documents\Ableton\Custom Plugins` (no admin needed). The
   built bundle is copied there from `D:\cargo-target\vss\bundled\VSS.vst3`;
   after a rebuild, re-copy it and rescan.
2. Live > Preferences > Plug-Ins > Rescan.
3. Launch Live via `tools/live-debug.bat` (sets `NIH_LOG`, prints the log path).
4. New MIDI track. Load Serum 2, init saw patch.
5. 8-bar clip on that track holding one sustained low note.
6. Add VSS after Serum on the same track.
7. Set project tempo to 150 BPM.

## Checks (expected result)

8. Plugin loads with no warning dialog.
9. GUI opens, shows the default pattern.
10. Transport STOPPED, hold the MIDI key: vowels still cycle (free-run).
11. Press Play: step 1 locks to bar start.

Click Live's window before pressing Space; set values by dragging (typing
into slider text boxes may not receive keystrokes in Live).
12. Change tempo to 140: sequencer follows.
13. Change Rate to 1/8T and Swing up: audibly different, triplet/shuffled feel.
14. Save the set, close, reopen: pattern and knob values are unchanged.
15. Automate Formant Shift: moves with no clicks/zipper noise.
16. Resample 8 bars to a new audio track, export WAV into `sessions/`, named
    `YYYY-MM-DD-serum-150-1-8.wav`.
17. Run:
    ```
    py -3.12 tools/analyze.py sessions/<file> --bpm 150 --rate 1/8 --steps 8
    ```

## After a session, send back

- `%LOCALAPPDATA%\VSS\vss-live.log`
- `%AppData%\Ableton\Live 12.4.6\Preferences\Log.txt`
- the exported WAV
- which checklist item (if any) failed

Note: use `py -3.12` for analyze.py. The default Python 3.14 on this machine has a broken scipy (BLAS DLL missing).

## Analyzer sanity check

`py -3.12 tools/analyze.py demo/vss-demo.wav --bpm 150 --rate 1/8 --steps 8` must print
8/8 matches with automatic offset alignment (no `--offset-ms`). If it doesn't, the analyzer
itself is broken — fix it before trusting any session WAV result.

Only the `N/8 matches` line is pass/fail. The drift table prints WARN at the
default 20 ms glide by design — that is not a failure.

## GUI / GPU warning

The GUI needs OpenGL 3.2. Opening the editor window over Remote Desktop, in
a VM, or without a GPU driver can crash Live — audio without the window open
is unaffected. If a Live session must run headless/remote, don't open the
VSS window; use Live's generic parameter panel instead.
