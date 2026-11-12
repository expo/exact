# Incremental compilation of an Apple app module

Measured on 2026-11-11 at Exact commit
`5f5b3064a893ddcf98291a64fab9074b1c82581f`, before an incremental app module
is designed (LLP 1156.003 D1). It changes no production build command. The
fixture is about forty Swift files sharing a 2 MB generated binding, as in the
chat app whose 31 to 42 s edits prompted it.

With unchanged sources left untouched, five one-line edits take a median
30.915 s with the existing command and 0.261 s incrementally. Rewriting all
forty snapshot files with identical bytes makes the incremental command
compile all forty again (13.980 s), despite stable paths.

## Fixture and environment

The Mac has an Apple M5 Ultra (36 cores: 12 performance, 24 efficiency), 256 GiB
of memory, macOS 27.0.1 (26A434), Xcode 27.2 (27B52), and Apple Swift 6.4.2
(`swiftlang-6.4.2.1.9`, driver 1.168.6). The SDK is MacOSX27.2.sdk; the target
is `arm64-apple-macosx14.0` and the language mode is Swift 5, as in the app-module
command. Bun is 1.4.2.

The forty sources are:

- `Binding.swift`: 2,000,964 bytes, containing 1,743 generated public structs.
  Each has eight `Int64` fields, an initializer, a computed property, an array
  encoder, and an updater. Its initial SHA-256 is
  `20b26fb7ca942edc7f09eaa68747ba1fd57ace895ab80a69ac41fee4525631fc`.
- `Feature00.swift` through `Feature36.swift`: each calls the computed property
  of a different generated struct. An edit changes one integer literal in
  `Feature00.swift`'s function body, preserving its signature.
- `Entry.swift`: exports `fixture_value`, which sums those 37 functions.
- `Removable.swift`: independently exports `removed_source_marker`.

Sources stay at stable absolute paths under this worktree's ignored `target/`.
Only changed files are written between ordinary edit samples. Each mode has
its own output directory, and both use one module cache. There is no `-wmo`,
`-j` override, package compilation, app linking, signing, installation, or UI.

## Commands and measurement

The baseline is `moduleArgs` in `host/apple/build.mjs`, with this fixture replacing
the app's sources and no package objects or external frameworks:

```sh
xcrun --sdk macosx swiftc \
  -module-cache-path "$work/module-cache" \
  -parse-as-library -emit-library -O -swift-version 5 \
  -module-name ExactAppModules "${sources[@]}" \
  -o "$work/baseline/libexact_modules.dylib" \
  -target arm64-apple-macosx14.0 -v -driver-show-job-lifecycle
```

The incremental command has the same arguments, its own stable output path, and:

```sh
-incremental -enable-batch-mode \
  -output-file-map "$work/incremental/outputs.json" \
  -driver-show-incremental
```

The output map's empty key names `incremental/master.swiftdeps`. Each absolute
source path maps to `object`, `swift-dependencies`, and `dependencies` outputs
named after that source in the incremental directory. This compiler keeps its
driver graph in `master.priors` and per-source records in `*.swiftdeps`. No
module interface is emitted: production's app-module command emits only a
library too.

Bun times the complete driver with a monotonic clock. Compile time spans the
first `Starting Compiling` through the last `Finished Compiling` event; link
time spans `Starting Linking` through `Finished Linking`. These are elapsed
times, including any concurrent jobs, rather than sums of frontend CPU time.
The remaining wall time includes driver startup and dependency scheduling.
Pipe delivery limits the precision of those phase boundaries. Counts come
from the executed frontend commands' `-primary-file` arguments. Object
modification times also confirm reuse after an ordinary feature edit; unchanged
object bytes can retain their timestamps even when a frontend runs.

Every successful build is loaded in a fresh C process with
`dlopen(RTLD_NOW | RTLD_LOCAL)`. It asserts `fixture_value` and the presence or
absence of `removed_source_marker`. The value is 666 initially, 667 through 671
for the five edits, and 672 after changing the binding's getter.

The primary series runs under `taskpolicy -a`. A child process reports
`qos_class_main() == qos_class_self() == 33` (`QOS_CLASS_USER_INTERACTIVE`);
the inherited nice value is 5. The modes run sequentially, alternating their
order on successive edits. Other lanes share the machine, so these are local
wall times under contention, not an isolated machine benchmark. The initial
1/5/15-minute load averages were 9.89/15.82/17.17; after the series they were
32.32/19.95/17.50.

## Repeated one-line edits

Five edits to `Feature00.swift` reduce median driver wall time from 30.915 s
to 0.261 s (118.6×). The baseline compiles all forty primary files every
time. Incremental compilation runs only `Feature00.swift`; the binding, entry,
other 36 features, and removable file are skipped. The changed object is the
only object whose modification time advances. All five pairs return the
expected different executable value.

| Edit | Baseline compile | Baseline link | Baseline wall | Incremental compile | Incremental link | Incremental wall |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 30.744 | 0.095 | 30.915 | 0.053 | 0.098 | 0.261 |
| 2 | 30.037 | 0.095 | 30.206 | 0.052 | 0.094 | 0.254 |
| 3 | 32.711 | 0.097 | 32.888 | 0.053 | 0.097 | 0.264 |
| 4 | 33.462 | 0.095 | 33.640 | 0.054 | 0.095 | 0.263 |
| 5 | 28.659 | 0.093 | 28.830 | 0.054 | 0.092 | 0.256 |
| Median | 30.744 | 0.095 | 30.915 | 0.053 | 0.095 | 0.261 |

All table times are seconds. Link time is about 0.1 s; retaining compiled
sources accounts for the gain.

## Snapshot copies

Refreshing all forty source modification times with identical bytes schedules
all forty primaries. Baseline compile/link/wall times are
28.553/0.092/28.722 s; incremental times are 13.777/0.094/13.980 s. No object
modification time changes: this compiler can retain an identical object even
after running its frontend. The executed jobs, rather than output timestamps
alone, establish the rebuild count.

`host/apple/arms.mjs` currently uses `copyFileSync` for every app source in
`snapshotSources`. A separate Bun probe confirms that each copy refreshes the
destination modification time, including when the source is unchanged. Stable
paths alone do not give that copy pattern the ordinary-edit result. Leaving
unchanged files untouched is a condition of the 0.261 s measurement. No
file-hashing flag or alternative snapshot implementation was tested.

## Other edits and source removal

Each row below is one sample; times are seconds. “Primaries” counts source
files compiled, not frontend processes: the baseline uses one frontend job per
primary, while a full batch-mode compile uses two batches here. “Empty object
records” does not mean a cold filesystem or SDK cache.

| Case | Mode | Compile | Link | Wall | Primaries |
| --- | --- | ---: | ---: | ---: | ---: |
| Empty object records | baseline | 33.001 | 0.097 | 33.182 | 40 |
| Empty object records | incremental | 31.115 | 0.104 | 31.314 | 40 |
| Unchanged command | baseline | 31.687 | 0.093 | 31.865 | 40 |
| Unchanged command | incremental | 0.000 | 0.000 | 0.107 | 0 |
| Binding getter body | baseline | 28.869 | 0.099 | 29.044 | 40 |
| Binding getter body | incremental | 31.877 | 0.120 | 32.134 | 1 |
| Binding stored field | baseline | 30.971 | 0.127 | 31.200 | 40 |
| Binding stored field | incremental | 36.448 | 0.115 | 36.702 | 2 |
| Remove source | baseline | 33.190 | 0.108 | 33.395 | 39 |
| Remove source | incremental | 16.010 | 0.137 | 16.292 | 39 |
| After output cleanup | incremental | 0.000 | 0.000 | 0.152 | 0 |

Changing the first getter body compiles only `Binding.swift` incrementally;
adding a stored field to `Binding0000` also compiles `Feature00.swift`, queued
after discovering the changed dependency. Neither case saves time here: the
large generated file still dominates. These single samples, on a shared Mac,
do not establish an incremental-compiler regression.

Removing `Removable.swift` and its output-map entry causes Swift to report:

> Incremental compilation has been disabled

because an input from the previous compilation is absent. It compiles all
thirty-nine remaining files and links only their objects. The new library
returns 672, and `dlsym("removed_source_marker")` is null.

The removed `Removable.o` and `Removable.swiftdeps` remain on disk. A separate
link of every `.o` still in that directory produces a library that returns
672 **and still exports the removed marker**. Deleting the old map entry’s
`Removable.o`, `Removable.swiftdeps`, and `Removable.d` outputs removes this
hazard. The next incremental invocation compiles and links nothing, takes
0.152 s, and its existing library still returns 672 with the marker absent.
Linking a fresh glob of the 39 remaining objects also returns 672 with the
marker absent.
Swift’s input-list invalidation does not clean up removed outputs.

All 23 primary-series build/load checks passed, as did the separate stale and
cleaned object-glob checks. Commands, driver logs, lifecycle
timestamps, and raw rows remain locally under
`target/apple-module-measurement/interactive/` (`results.json`, and each
mode’s case-named `.log` files). They are ignored measurement artifacts; this
report contains the results needed beyond this worktree.

## Reproduction

Generate the binding and feature files with Bun; this generator defines their
contents, including the measured binding's size and digest:

```js
import { mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
const src = resolve('target/apple-module-compile/sources');
mkdirSync(src, { recursive: true });
let binding = '', count = 0;
while (binding.length < 2_000_000) {
  const n = String(count).padStart(4, '0');
  binding += `public struct Binding${n} {\n`;
  for (let j = 0; j < 8; j++) binding += `    public var field${j}: Int64\n`;
  binding += '    public init(_ seed: Int64) {\n';
  for (let j = 0; j < 8; j++) binding += `        field${j} = seed &+ ${j}\n`;
  binding += '    }\n    public var adjusted: Int64 { field0 &+ 0 }\n';
  binding += '    public func encode() -> [Int64] {\n';
  binding += '        [field0, field1, field2, field3, field4, field5, field6, field7]\n    }\n';
  binding += '    public mutating func update(_ values: [Int64]) {\n';
  for (let j = 0; j < 8; j++) {
    binding += `        if values.count > ${j} { field${j} = values[${j}] }\n`;
  }
  binding += '    }\n}\n';
  count++;
}
writeFileSync(resolve(src, 'Binding.swift'), binding);
for (let i = 0; i < 37; i++) {
  const f = String(i).padStart(2, '0'), b = String(i).padStart(4, '0');
  writeFileSync(resolve(src, `Feature${f}.swift`),
    `public func feature${f}(_ seed: Int64) -> Int64 {\n` +
    `    Binding${b}(seed).adjusted &+ 0\n}\n`);
}
writeFileSync(resolve(src, 'Entry.swift'),
  '@_cdecl("fixture_value")\npublic func fixtureValue() -> Int64 {\n' +
  '    var value: Int64 = 0\n' + Array.from({ length: 37 }, (_, i) =>
    `    value &+= feature${String(i).padStart(2, '0')}(${i})\n`).join('') +
  '    return value\n}\n');
writeFileSync(resolve(src, 'Removable.swift'),
  '@_cdecl("removed_source_marker")\n' +
  'public func removedSourceMarker() -> Int64 { 999 }\n');
const out = resolve(src, '../incremental');
mkdirSync(out, { recursive: true });
const map = { '': { 'swift-dependencies': resolve(out, 'master.swiftdeps') } };
for (const file of readdirSync(src).filter(f => f.endsWith('.swift')).sort()) {
  const stem = resolve(out, file.slice(0, -6));
  map[resolve(src, file)] = {
    object: `${stem}.o`, 'swift-dependencies': `${stem}.swiftdeps`,
    dependencies: `${stem}.d`,
  };
}
writeFileSync(resolve(out, 'outputs.json'), JSON.stringify(map, null, 2) + '\n');
```

Set `work="$PWD/target/apple-module-compile"`, collect the sorted source paths
with `sources=("$work"/sources/*.swift)`, and create `$work/baseline`. Run the
measurement process through `taskpolicy -a`. Start without compiled objects or
dependency records, run both commands, then run both unchanged. Change only
`Feature00.swift`'s final literal from 0 to 1, 2, 3, 4, and 5, building both modes
after each write. Next rewrite every source with identical bytes to test a
snapshot copy that refreshes modification times. Change the first binding
getter's `field0 &+ 0` to `field0 &+ 1`, build both, then add
`public var extra: Int64 = 1` to `Binding0000` and build both again.

Finally delete `Removable.swift` and its map entry. Build with the remaining
source list. Check the obsolete object's existence and deliberately link a
glob of all remaining `.o` files to expose stale symbols. Delete the removed
entry's three output paths and build from the current map once more. Link a
fresh object glob to check that the removed symbol is absent after cleanup.

## Limits

This synthetic binding reproduces the reported compile cost but is not the
chat app's generated binding. It has no Foundation, AppKit, UIKit, Objective-C
bridge, third-party framework, Exact module seam, or macro work. Its dependency
graph is deliberately explicit; different generated APIs can invalidate more
consumers. Five edit samples on one shared Mac and one compiler do not predict
other machines or toolchains.

These numbers cover compiling and linking the module library only. Production's
arm cache already avoids invoking the compiler for unchanged inputs; the
unchanged-command comparison measures the compiler, not a warm full-app build.
The experiment keeps source and output paths stable and does not qualify
production's temporary arm paths, concurrent builds, interrupted records,
configuration changes, or reuse between apps. It makes no claim about iOS,
tvOS, devices, startup, or an integrated edit-to-running-app loop. Production
incremental compilation remains deferred.
