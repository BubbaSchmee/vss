# VSS manual Live session checklist

Logs:
- Plugin log: `%LOCALAPPDATA%\VSS\vss-live.log`
- Live's own log: `%AppData%\Ableton\Live 12.4.6\Preferences\Log.txt`

## Steps

1. Copy `target/bundled/VSS.vst3` to `C:\Program Files\Common Files\VST3`.
2. Live > Preferences > Plug-Ins > rescan.
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
