#!/usr/bin/env python3
"""Analyze a VSS session WAV: per-step vowel detection + timing drift.

Usage:
    analyze.py FILE.wav --bpm 150 --rate 1/8 --steps 8 [--swing 0]
               [--pattern "A O A E U O A I"] [--offset-ms N]
    analyze.py --selftest
"""
import argparse
import sys
import numpy as np
from scipy.io import wavfile
from scipy.signal import welch, butter, sosfilt

VOWELS = {
    "A": (730, 1090),
    "E": (530, 1840),
    "I": (270, 2290),
    "O": (570, 840),
    "U": (300, 870),
}
DEFAULT_PATTERN = "A O A E U O A I A O A E U O I A".split()
RATE_BEATS = {"1/4": 1.0, "1/8": 0.5, "1/8T": 1.0 / 3, "1/16": 0.25, "1/16T": 1.0 / 6}


def step_len_seconds(bpm, rate):
    return RATE_BEATS[rate] * 60.0 / bpm


def step_boundaries(bpm, rate, steps, swing):
    """Expected step-start times (s) for one pattern cycle, per SPEC Sequencer:
    odd (zero-based) steps are delayed by swing*step_len/3."""
    step_len = step_len_seconds(bpm, rate)
    times = []
    t = 0.0
    for i in range(steps):
        times.append(t)
        dur = step_len + (swing * step_len / 3 if i % 2 == 0 else -swing * step_len / 3)
        t += dur
    return np.array(times), step_len


def step_durations(step_len, swing, steps):
    """Per-step duration (s), matching step_boundaries' spacing."""
    return np.array(
        [step_len * (1 + swing / 3 if i % 2 == 0 else 1 - swing / 3) for i in range(steps)]
    )


def classify_vowel(freqs):
    """Nearest vowel by log-frequency distance to F1/F2 table, given 1-2 peak Hz."""
    if len(freqs) == 0:
        return "?", []
    f1 = freqs[0]
    f2 = freqs[1] if len(freqs) > 1 else freqs[0]
    best, best_d = "?", float("inf")
    for v, (vf1, vf2) in VOWELS.items():
        d = (np.log(f1) - np.log(vf1)) ** 2 + (np.log(f2) - np.log(vf2)) ** 2
        if d < best_d:
            best_d, best = d, v
    return best, freqs


def peak_freqs(x, sr, n_peaks=2, band=(200, 4000)):
    if len(x) < 16:
        return []
    nperseg = min(1024, len(x))
    f, p = welch(x, fs=sr, nperseg=nperseg)
    mask = (f >= band[0]) & (f <= band[1])
    f, p = f[mask], p[mask]
    if len(f) < 3:
        return []
    # local maxima
    peaks = [i for i in range(1, len(p) - 1) if p[i] > p[i - 1] and p[i] > p[i + 1]]
    peaks.sort(key=lambda i: p[i], reverse=True)
    return sorted(f[i] for i in peaks[:n_peaks])


def classify_step(x, sr, start, dur):
    """Classify the vowel of the step's steady-state window (20%-80% of its duration,
    matching the demo's note gate so the transient/glide at the edges is excluded)."""
    lo, hi = start + 0.2 * dur, start + 0.8 * dur
    seg = x[int(lo * sr) : int(hi * sr)]
    return classify_vowel(peak_freqs(seg, sr))


def find_offset(x, sr, expected, durs, pattern_len, steps, pattern):
    """Scan candidate offsets over one full pattern length (~2 ms steps), classify every
    step's vowel at each candidate, and pick the offset with the most pattern matches
    (ties broken by smallest |offset|, which falls out of scanning from 0 upward)."""
    fine_ms = 2.0
    n_search = max(1, int(pattern_len * 1000 / fine_ms))
    best_off, best_matches = 0.0, -1
    for k in range(n_search):
        off = pattern_len * k / n_search
        matches = 0
        for i in range(steps):
            v, _ = classify_step(x, sr, expected[i] + off, durs[i])
            if v == pattern[i % len(pattern)]:
                matches += 1
        if matches > best_matches:
            best_matches, best_off = matches, off
    return best_off


# ponytail: linear scan over the pattern length is O(steps / fine_ms), fine for an 8-16
# step demo/session clip; upgrade to a coarse-to-fine search if patterns get much longer.
def boundary_drift(x, sr, tb, prev_v, cur_v, search_ms=40, step_ms=2, win_ms=20):
    """Drift (ms) of the actual vowel-classification transition vs the expected boundary
    tb: slide a short window across tb+-search_ms and find where the classified vowel
    flips from prev_v to cur_v. None if there's no vowel change to detect (e.g. a repeat)
    or no flip is found in range."""
    if prev_v == cur_v:
        return None
    win = win_ms / 1000.0
    shifts = np.arange(-search_ms, search_ms + 1e-9, step_ms)
    classes = [classify_step(x, sr, tb + sh / 1000.0, win)[0] for sh in shifts]
    for idx in range(len(shifts) - 1):
        if classes[idx] == prev_v and cur_v in classes[idx + 1 : idx + 4]:
            j = idx + 1 + classes[idx + 1 :].index(cur_v)
            return float(shifts[j])
    return None


def analyze(x, sr, bpm, rate, steps, swing, pattern, offset_s):
    expected, step_len = step_boundaries(bpm, rate, steps, swing)
    durs = step_durations(step_len, swing, steps)
    pattern_len = expected[-1] + step_len if steps > 0 else step_len
    if offset_s is None:
        offset_s = find_offset(x, sr, expected, durs, pattern_len, steps, pattern)
    expected_abs = expected + offset_s

    rows = []
    any_mismatch = False
    for i in range(steps):
        detected, peaks = classify_step(x, sr, expected_abs[i], durs[i])
        expected_v = pattern[i % len(pattern)]
        match = detected == expected_v
        if not match:
            any_mismatch = True
        rows.append((i, expected_v, detected, peaks, match))

    drifts = []
    for i in range(steps):
        prev_v = pattern[(i - 1) % len(pattern)]
        cur_v = pattern[i % len(pattern)]
        drifts.append(boundary_drift(x, sr, expected_abs[i], prev_v, cur_v))
    valid_drifts = [d for d in drifts if d is not None]

    print(f"{'step':>4} {'expected':>8} {'detected':>8} {'peaks(Hz)':>20} {'match':>5}")
    for i, ev, dv, peaks, match in rows:
        peak_str = ",".join(f"{p:.0f}" for p in peaks)
        print(f"{i:>4} {ev:>8} {dv:>8} {peak_str:>20} {'yes' if match else 'no':>5}")
    n_match = sum(1 for r in rows if r[4])
    print(f"\n{n_match}/{steps} matches (offset {offset_s * 1000:+.1f} ms)")

    print()
    print(f"{'boundary':>8} {'drift(ms)':>10}")
    for i, d in enumerate(drifts):
        print(f"{i:>8} {'n/a' if d is None else f'{d:+.1f}':>10}")
    mean_drift = float(np.mean(valid_drifts)) if valid_drifts else 0.0
    max_drift = float(max(valid_drifts, key=abs)) if valid_drifts else 0.0
    print(f"mean drift: {mean_drift:+.1f} ms, max drift: {max_drift:+.1f} ms")
    if abs(max_drift) > 15.0:
        print(f"WARN: |max drift| {max_drift:+.1f} ms > 15 ms (glide makes transitions soft; not a failure)")

    ok = not any_mismatch
    return ok, rows, mean_drift, max_drift


def synth_vowel_saw(vowel, freq, sr, dur):
    n = int(sr * dur)
    t = np.arange(n) / sr
    saw = 2 * (t * freq - np.floor(0.5 + t * freq))
    f1, f2 = VOWELS[vowel]
    f3 = f2 * 1.3
    out = np.zeros(n)
    for center, gain_db in ((f1, 0), (f2, -6), (f3, -12)):
        sos = butter(2, [max(20, center * 0.85), center * 1.15], btype="band", fs=sr, output="sos")
        out += sosfilt(sos, saw) * (10 ** (gain_db / 20))
    return out


def selftest():
    sr = 48000
    bpm = 150
    rate = "1/8"
    steps = 8
    swing = 0.0
    pattern = DEFAULT_PATTERN[:steps]
    expected, step_len = step_boundaries(bpm, rate, steps, swing)
    bar_len = 4 * 60.0 / bpm
    total_dur = 2 * bar_len
    n = int(sr * total_dur)
    x = np.zeros(n)
    t = 0.0
    i = 0
    while t < total_dur:
        v = pattern[i % steps]
        dur = step_len
        seg = synth_vowel_saw(v, 55, sr, dur)
        lo = int(t * sr)
        hi = min(n, lo + len(seg))
        x[lo:hi] += seg[: hi - lo]
        t += dur
        i += 1

    ok, rows, mean_drift, max_drift = analyze(x, sr, bpm, rate, steps, swing, pattern, offset_s=0.0)
    mismatches = [r for r in rows if not r[4]]
    # Only the vowel decode is asserted: this synthetic signal splices filters with no glide,
    # so the onset transient (and thus timing drift) isn't representative of the real plugin.
    assert not mismatches, f"selftest vowel mismatches: {mismatches}"
    print("SELFTEST PASSED")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file", nargs="?")
    ap.add_argument("--bpm", type=float, default=150)
    ap.add_argument("--rate", default="1/8", choices=list(RATE_BEATS))
    ap.add_argument("--steps", type=int, default=8)
    ap.add_argument("--swing", type=float, default=0.0)
    ap.add_argument("--pattern", default=None)
    ap.add_argument("--offset-ms", type=float, default=None)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        selftest()
        return 0

    if not args.file:
        ap.error("FILE.wav required unless --selftest")

    pattern = args.pattern.split() if args.pattern else DEFAULT_PATTERN[: args.steps]

    sr, data = wavfile.read(args.file)
    if data.ndim > 1:
        data = data.mean(axis=1)
    if np.issubdtype(data.dtype, np.integer):
        data = data.astype(np.float64) / np.iinfo(data.dtype).max
    else:
        data = data.astype(np.float64)

    offset_s = args.offset_ms / 1000.0 if args.offset_ms is not None else None
    ok, rows, mean_drift, max_drift = analyze(
        data, sr, args.bpm, args.rate, args.steps, args.swing, pattern, offset_s
    )
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
