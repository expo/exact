# Compare first-commit CPU samples using applyprof's disjoint call-path groups (LLP 1172.001 §4 step 3).
# Input: xctrace time-profile and os-signpost XML exports named <prefix>-<app>-<round>{,-signposts}.xml.
import bisect
import collections
import json
import math
from pathlib import Path
import statistics
import sys
import xml.etree.ElementTree as ET

root = Path('target/bareref')
prefix = sys.argv[1] if len(sys.argv) > 1 else 'matched'
GROUPS = ['View creation / hierarchy', 'Layout / navigation / commit', 'Bridging',
          'Accessibility / focus', 'Lookup tables', 'Property application', 'Other preparation / cleanup', 'Text', 'Canvas']


def xml(path):
    tree = ET.parse(path)
    ids = {e.attrib['id']: e for e in tree.iter() if 'id' in e.attrib}

    def get(e):
        while e is not None and 'ref' in e.attrib:
            e = ids[e.attrib['ref']]
        return e
    return tree, get


def intervals(path):
    tree, get = xml(path)
    begins, pairs, seen = {}, [], set()
    for row in tree.findall('.//row'):
        cols = list(row)
        if len(cols) < 7 or get(cols[6]).text != 'PushFirstCommit':
            continue
        time, kind, identity = int(get(cols[0]).text), get(cols[3]).text, get(cols[5]).text
        key = (time, kind, identity)
        if key in seen:
            continue
        seen.add(key)
        if kind == 'Begin':
            begins[identity] = time
        elif kind == 'End':
            pairs.append((begins.pop(identity), time))
    assert not begins
    return sorted(pairs)


def runtime(frame, get):
    binary = get(frame.find('binary'))
    binary = binary.attrib.get('name', '') if binary is not None else ''
    name = frame.attrib.get('name', '')
    source = get(frame.find('source/path'))
    return (source is not None and 'compiler-generated' in (source.text or '')
            or name.startswith(('DYLD-STUB', 'objc_msgSend', 'dynamic_cast', 'protocol witness', '_stringCompare', '_swift'))
            or binary.startswith(('libswift', 'libobjc', 'libsystem', 'libdyld'))
            or any(t in name for t in ['specialized Dictionary.', 'specialized Array.', 'outlined ', 'specialized Set.',
                                      'specialized Sequence.', 'partial apply', 'thunk for', 'specialized Collection.',
                                      'specialized BidirectionalCollection.']))


def group(names, charged):
    # applyprof's order and predicates are preserved; UIKit equivalents extend each group to the bare app.
    if 'Presenter.apply(_:)' in names:
        names = names[:names.index('Presenter.apply(_:)')]
    n, head = ' '.join(names), ' '.join(names[:5])
    if any(t in n for t in ['CanvasesHost.', 'CanvasesHostIOS.', 'Canvases.']):
        return 'Canvas'
    if any(t in head for t in ['Dictionary.', 'DictionaryStorage', 'Hashable', 'Set.', 'hashValue', 'ChromeIndex.']):
        return 'Lookup tables'
    if any(t in head for t in ['swift_dynamicCast', 'dynamic_cast', 'objc_msgSend', 'swift_bridgeObject', 'swift_getObjectType']):
        return 'Bridging'
    if any(t in n for t in ['Accessibility', 'accessibility', 'accessible', 'FocusSearch', 'syncKeyboardFocus', 'KeyFocus', 'autofocus']):
        return 'Accessibility / focus'
    if any(t in n for t in ['TextRaster', 'paintChangedText', 'paintPresentedText', 'paintVisibleText', 'paragraph', 'Paragraph',
                          'TextIndex', 'CoreText', 'TextSpec', 'textRaster', 'textIsVisible', 'TextClips', 'UILabel', 'UITextField',
                          'NSStringDrawing', 'NSAttributedString', 'TextKit', 'CTFont', 'CTLine', 'CTRun']):
        return 'Text'
    if any(t in n for t in ['placeChildren', 'NodeView.init(', 'NodeView.__allocating_init', 'Presenter.release', 'NodeView.forget',
                          'didMoveTo', '_didMoveFromWindow', '_addSubview', 'removeFromSuperview', '_postMovedFromSuperview',
                          '_willMoveToWindow', 'UIViewCommonInit', 'NodePool', 'initWithFrame:']):
        return 'View creation / hierarchy'
    if any(t in n for t in ['NavigationHost', 'layout', 'Layout', 'Geometry', 'applyFrame', 'Presenter.frame', 'moveOrPlace',
                          'regrid', 'Grid', 'CA::Context::commit', 'CA::Transaction::commit', 'fitDocument', 'CollectionHost',
                          'ColorSchemes', 'PaintOrder', 'UINavigationController', 'setViewControllers']):
        return 'Layout / navigation / commit'
    if any(t in n for t in ['applyProps', 'applyStyle', 'applyOp', 'applySurface', 'applyBox', 'updateMaterial', 'syncVibrancy',
                          'material', 'Tint', 'tint', 'pressPaint', 'applyLanguage', 'applyWakeLock', 'applyOrientationLock',
                          'setFrame:', 'setBounds:', 'setBackgroundColor:', 'setCornerRadius:']):
        return 'Property application'
    if charged.startswith('Screen.make'):
        return 'View creation / hierarchy'
    return 'Other preparation / cleanup'


apps = {}
for app in ['exact', 'bare']:
    all_records, groups, tree_groups, functions = [], collections.Counter(), collections.Counter(), collections.Counter()
    run_rows, stack_rows = [], []
    for run in [1, 2, 3]:
        stem = root / f'{prefix}-{app}-{run}'
        records = json.loads(stem.with_suffix('.json').read_text())
        pairs = intervals(Path(str(stem) + '-signposts.xml'))
        assert len(pairs) == len(records) == 15, (app, run, len(pairs), len(records))
        starts = [x[0] for x in pairs]
        tree, get = xml(stem.with_suffix('.xml'))
        cpu = collections.Counter()
        for row in tree.findall('.//row'):
            cols = list(row)
            if 'Main Thread' not in get(cols[1]).attrib.get('fmt', ''):
                continue
            time = int(get(cols[0]).text)
            i = bisect.bisect_right(starts, time) - 1
            if i < 0 or time > pairs[i][1]:
                continue
            frames = [get(e) for e in get(cols[-1]) if e.tag == 'frame']
            names = [f.attrib.get('name', '') for f in frames]
            weight = float(get(cols[5]).text) / 1e6
            if any(t in ' '.join(names) for t in ['PushProbe.', 'ApplyProfile.add(', 'ApplyProfile.begin(', 'ApplyProfile.end(',
                                                '$defer #1 <A> () in static ApplyProfile.time', '$defer #1 () in Presenter.apply(_:)',
                                                '$defer #9 () in Presenter.apply(_:)']):
                continue
            if not frames:
                continue
            charged = next((f for f in frames if not runtime(f, get)), frames[0])
            category = group(names, charged.attrib.get('name', ''))
            groups[category] += weight
            cpu[i] += weight
            functions[charged.attrib.get('name', '')] += weight
            is_tree = ('Presenter.apply(_:)' in names and any('ExactSession.applyInput(' in n for n in names)
                       and 'Agent.tap(_:)' in names and category != 'Canvas')
            if is_tree:
                tree_groups[category] += weight
            stack_rows.append(dict(run=run, push=i + 1, ms=weight, category=category,
                                   tree=is_tree, charged=charged.attrib.get('name', ''), names=names))
        assert all(cpu[i] > 0 for i in range(len(records))), (app, run, 'push without CPU samples')
        run_rows.append(dict(run=run, count=len(records), wallMedian=statistics.median(r['wallMs'] for r in records),
                             cpuMedian=statistics.median(r['cpuMs'] for r in records), sampleMs=sum(cpu.values())))
        for i, record in enumerate(records):
            record['sampleMs'] = cpu[i]
        all_records += records
    apps[app] = dict(records=all_records, groups=groups, treeGroups=tree_groups,
                     runs=run_rows, functions=functions.most_common(20))
    (root / f'{prefix}-{app}-stacks.json').write_text(json.dumps(stack_rows))


def median(rows, key):
    return statistics.median(r[key] for r in rows)


def p90(rows, key):
    return sorted(r[key] for r in rows)[math.ceil(.9 * len(rows)) - 1]


lines = ['| Scope | Exact median / p90 ms | Bare median / p90 ms | Median ratio |', '|---|---:|---:|---:|']
for key, label in [('wallMs', 'Tap to first commit, elapsed'), ('cpuMs', 'Main-thread CPU'), ('sampleMs', 'Sample-weight equivalent')]:
    a, b = apps['exact']['records'], apps['bare']['records']
    lines.append(f'| {label} | {median(a,key):.3f} / {p90(a,key):.3f} | {median(b,key):.3f} / {p90(b,key):.3f} | {median(a,key)/median(b,key):.2f}× |')
lines += ['', '| CPU group | Exact ms/push | Bare ms/push | Exact − bare ms/push |', '|---|---:|---:|---:|']
for group_name in GROUPS:
    a, b = apps['exact']['groups'][group_name] / 45, apps['bare']['groups'][group_name] / 45
    lines.append(f'| {group_name} | {a:.3f} | {b:.3f} | {a-b:+.3f} |')
a, b = sum(apps['exact']['groups'].values())/45, sum(apps['bare']['groups'].values())/45
lines.append(f'| Total | {a:.3f} | {b:.3f} | {a-b:+.3f} |')
(root / f'{prefix}-tables.md').write_text('\n'.join(lines) + '\n')
(root / f'{prefix}-analysis.json').write_text(json.dumps(apps, indent=2))
print('\n'.join(lines))
