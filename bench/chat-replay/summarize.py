#!/usr/bin/env python3
"""Prints the chat replay table (README.md, "Metrics") for result files written by run.mjs:
    summarize.py <label>=<file>[,<file>...] ...
Each label's files are runs of one app; the table prints the median over them. Launch times come from every run.
A file from a run with --sample 0 has no scroll samples and gives time to message 1 and pacing; one with sampling
gives the missing-content figures (its pacing is perturbed by the sampling and is not printed)."""
import json, statistics, sys

def samples(d, phase):
    """The phase's samples. Results from before the probe had a `rest` phase labelled the rest at the top, after message
    1 was reached and before the check confirmed it, `up`: those samples are left out here."""
    s = [x for x in d['samples'] if x[1] == phase]
    if phase == 'up' and not any(p['phase'] == 'rest' for p in d['phases']) and d.get('topMs') is not None:
        up = next(p for p in d['phases'] if p['phase'] == 'up')
        s = [x for x in s if x[0] < up['start'] + d['topMs']]
    return s

def frame_times(d):
    """Each frame's time on the clock samples use: when it shows. Results with a content height but no `rest` phase
    dated frames a frame early (the callback's `timestamp`) and samples when they show; older ones dated both early."""
    shift = len(d['frames'][0]) > 4 and not any(p['phase'] == 'rest' for p in d['phases']) if d['frames'] else False
    return [x[0] + x[2] if shift else x[0] for x in d['frames']]

def phase_stats(d, phase):
    s = samples(d, phase)
    if not s: return None
    n = len(s)
    return {
        'incomplete %': 100 * sum(1 for x in s if x[4] or x[5] or x[6] > 0) / n,
        'empty bubble %': 100 * sum(1 for x in s if x[4]) / n,
        'unpainted photo %': 100 * sum(1 for x in s if x[5]) / n,
        'blank band %': 100 * sum(1 for x in s if x[6] > 0) / n,
        'mostly blank %': 100 * sum(1 for x in s if x[7] >= 0.5) / n,
        'missing area %': 100 * sum(x[7] for x in s) / n,
    }

SPEEDS = [(0, 8000), (8000, 25000), (25000, 40000), (40000, 150000)]

def by_speed(d, phase):
    """Incomplete % of the phase's sampled frames by the list's speed over the 3 frames before each (pt/s). A window in
    which the content grew or shrank by over 1,000 pt (a page landing, which moves the offset to keep the rows in
    place; estimates corrected as rows are measured move it by less) or that moved faster than 150,000 pt/s is a
    jump, not a scroll, and is left out."""
    import bisect
    ts = frame_times(d); off = [x[3] for x in d['frames']]
    tall = [x[4] if len(x) > 4 else None for x in d['frames']]
    n = {b: 0 for b in SPEEDS}; bad = {b: 0 for b in SPEEDS}
    for x in samples(d, phase):
        i = bisect.bisect_left(ts, x[0] - 0.5)
        if i < 3 or i >= len(ts) or ts[i] == ts[i - 3]: continue
        if None not in tall[i - 3:i + 1] and max(tall[i - 3:i + 1]) - min(tall[i - 3:i + 1]) > 1000: continue
        v = abs(off[i] - off[i - 3]) / ((ts[i] - ts[i - 3]) / 1000)
        for b in SPEEDS:
            if b[0] <= v < b[1]: n[b] += 1; bad[b] += bool(x[4] or x[5] or x[6] > 0)
    return {f'{phase}: incomplete % at {b[0] // 1000}k–{b[1] // 1000}k pt/s': 100 * bad[b] / n[b] if n[b] >= 20 else None for b in SPEEDS} | \
           {f'{phase}: share of frames under 8k pt/s %': 100 * n[SPEEDS[0]] / sum(n.values()) if sum(n.values()) else None}

def pacing(d, phase):
    ph = next((p for p in d['phases'] if p['phase'] == phase), None)
    if not ph: return None
    f = [x for x in d['frames'] if ph['start'] <= x[0] < ph['end']]
    if len(f) < 2: return None
    late = [x for x in f[1:] if x[1] > 1.5 * x[2]]
    secs = (ph['end'] - ph['start']) / 1000
    dts = sorted(x[1] for x in f[1:])
    return {'late/s': len(late) / secs, 'late %': 100 * len(late) / (len(f) - 1), 'worst ms': dts[-1],
            'fps': len(f) / secs, 'main cpu %': 100 * ph['mainCpuMs'] / (ph['end'] - ph['start'])}

def median(vals):
    vals = [v for v in vals if v is not None]
    return statistics.median(vals) if vals else None

rows = {}
for arg in sys.argv[1:]:
    label, files = arg.split('=', 1)
    runs = [json.load(open(f)) for f in files.split(',')]
    sampled = [r for r in runs if r['sample'] > 0]
    plain = [r for r in runs if r['sample'] == 0]
    row = {}
    # A launch time a run never saw is null (-1000 from runs before nulls) and is left out; time to message 1 comes
    # only from runs without sampling, whose pacing the classification does not slow.
    seen = lambda v: v if v is not None and v >= 0 else None
    row['first content ms'] = median([seen(r['firstContentMs']) for r in runs])
    row['ready ms'] = median([seen(r['readyMs']) for r in runs])
    row['to message 1 s'] = median([r['topMs'] / 1000 for r in plain if r.get('topMs') is not None])
    for ph in ('up', 'flings'):
        stats = [phase_stats(r, ph) for r in sampled]
        stats = [s for s in stats if s]
        for k in (stats[0] if stats else {}):
            row[f'{ph}: {k}'] = median([s[k] for s in stats])
        if ph == 'up':
            speeds = [by_speed(r, ph) for r in sampled]
            for k in (speeds[0] if speeds else {}):
                row[k] = median([x[k] for x in speeds])
        pac = [pacing(r, ph) for r in plain]
        pac = [p for p in pac if p]
        for k in (pac[0] if pac else {}):
            row[f'{ph}: {k}'] = median([p[k] for p in pac])
    row['runs'] = f'{len(sampled)}+{len(plain)} @{runs[0]["maxFps"]}Hz'
    rows[label] = row

labels = list(rows)
keys = [k for k in rows[labels[0]]]
for l in labels[1:]: keys += [k for k in rows[l] if k not in keys]
w = max(len(k) for k in keys)
cw = max(14, max(len(l) for l in labels) + 2)
print(' ' * w + ''.join(f'{l:>{cw}}' for l in labels))
for k in keys:
    cells = []
    for l in labels:
        v = rows[l].get(k)
        cells.append(f'{v:>{cw}}' if isinstance(v, str) else f'{"—":>{cw}}' if v is None else f'{v:>{cw}.1f}')
    print(f'{k:<{w}}' + ''.join(cells))
