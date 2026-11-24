# Contract: a complete working guide for agents

> **Lookup reference: start at [Start here](start-here.md).** Building an app needs
> only that page, and a [recipe](recipes/) answers most questions after it. Open this
> guide at a section for what no recipe covers, or grep it; don't read it front to back.

Use this guide to author, change, inspect, and verify a current Exact application.
It covers the language implemented on `main` on 2026-10-02. Use the [human guide](contract-for-humans.md)
for explanations and complete examples, and the [grammar reference](contract-grammar.md)
for exact forms, built-in functions, tags, and event payloads.

The compiler and executable fixtures are authoritative. Read the repository's
`AGENTS.md`, `rules/RULES.md`, and `rules/DEFERRED.md` before making changes here.
This guide is documentation, not an additional policy layer. Design documents
from Exact's predecessor describe that predecessor and do not establish current
capabilities.

## Contents

- [Orient and choose the right layer](#orient-and-choose-the-right-layer)
- [The implementation loop](#the-implementation-loop)
- [Language inventory](#language-inventory)
- [Values, expressions, and functions](#values-expressions-and-functions)
- [State and action semantics](#state-and-action-semantics)
- [Composition and lifetime](#composition-and-lifetime)
- [Data requests and side effects](#data-requests-and-side-effects)
  - [A TypeScript package for the data module](#a-typescript-package-for-the-data-module)
  - [Native sign-in](#native-sign-in-exactsign-in)
  - [Tools: what a call answers](#tools-what-a-call-answers)
- [Views, layout, and interaction](#views-layout-and-interaction)
  - [Tabular numbers](#tabular-numbers)
  - [Announcing a change](#announcing-a-change)
  - [Sound, MIDI and media](#sound-midi-and-media)
- [Routes and web documents](#routes-and-web-documents)
- [Tabs and stacks](#tabs-and-stacks)
- [Time, motion, graphics, and platform facts](#time-motion-graphics-and-platform-facts)
- [Inspection and testing](#inspection-and-testing)
- [Repair common mistakes](#repair-common-mistakes)
- [Completion criteria](#completion-criteria)

## Orient and choose the right layer

1. Identify the app, its root `app.contract`, `app.json`, data module, imports,
   authored tests, and requested target hosts. An external app uses `EXACT_APP_DIR`
   with the same Exact build/driver scripts.
2. Read the nearest working example for the feature. Search `contract/corpus/`,
   `contract/cli/tests/it/`, and the app's sources before designing new syntax.
3. Decide whether the change belongs in Contract, the app's data module, its
   optional native/GPU module, or the host. Do not extend the compiler to avoid
   writing an ordinary data-source function.
4. Check the current branch and dirty files. Preserve other work. Do not commit
   unrelated staged paths or use `git stash` in this repository.

| Work | Normal home |
| --- | --- |
| View structure, UI state, event binding | `.contract` |
| Derived labels, record copies, bounded list transforms, a presentation order by one key (`sortBy`) | Contract expressions / `fn` |
| Network, authentication, storage, a stored or locale order, domain algorithms | App TypeScript/Rust data module |
| Device facts | Reserved source with an admitted shape |
| A system control (button, list, switch, picker, menu, tabs, bars) | Contract's native form ([below](#views-layout-and-interaction)); an app native module only where none exists |
| What only the platform's own object can do (a gesture recognizer, a bar's look, drawing into a view) | An access hatch: `hatch="word"` on the node, native code handed its view ([Exact developer reference](reference.md#access-hatches)); `bun exact.mjs hatch <word>` writes the stubs |
| Canvas 2D drawing | Data module's canvas surface |
| GPU scene or game | Optional GPU/game artifact |
| App identity, grants/deploy selection, module placement | App manifest and data-module declarations |

Windows apps default to ink, or CPU when the manifest declares `game`;
`host.windows.painter: "ink"` or `"cpu"` selects either, and `EXACT_PAINTER`
overrides both. Headless drives explicitly use CPU unless
overridden, so inspect the desktop capture too. See the
[Windows painter choices](reference.md#windows-painter) for device selection
and the other platforms' defaults.

For the implementation, the relevant authorities are:

| Question | Source |
| --- | --- |
| Tokens, indentation, escapes | [`lexer.rs`](../contract/syntax/src/lexer.rs) |
| Declarations, statements, elements | [`parser.rs`](../contract/syntax/src/parser.rs) |
| Precedence, templates, callbacks | [`parser/expr.rs`](../contract/syntax/src/parser/expr.rs) |
| Records, locals, callbacks | [`types/records.rs`](../contract/types/src/records.rs), [`types/actions.rs`](../contract/types/src/actions.rs), [`types/lists.rs`](../contract/types/src/lists.rs) |
| Tags, aliases, authored CSS names | [`lower/tags.rs`](../contract/lower/src/tags.rs) |
| Kernel property declarations | [`schema.json`](../kernel/tables/schema.json) |
| Function signatures | [`format.json`](../plan/tables/format.json), field `stdlib`, plus compiler intrinsics |
| Commands and argument checks | [`types/checks.rs`](../contract/types/src/checks.rs) |
| Event payloads and arities | [`analyze/lib.rs`](../contract/analyze/src/lib.rs) |
| Imports and file boundaries | [`cli/sources.rs`](../contract/cli/src/sources.rs) |
| CLI flags and output | [`cli/main.rs`](../contract/cli/src/main.rs), [`cli/build.rs`](../contract/cli/src/build.rs), [`cli/lib.rs`](../contract/cli/src/lib.rs) (diagnostic JSON) |

The design is recorded in LLP 1006, LLP 1017.000 and LLP 1035.005.000.
A proposal in a design document is not an implemented grammar production.

## The implementation loop

In an app made by `exact new`, run its own `exact.mjs` from the app's directory.
`bun exact.mjs update` regenerates that file, so the app's own verbs go in
`app.json`'s `commands` (`"seed": ["bun", "seed.mjs"]` is `bun exact.mjs seed
web`, run in the app's directory), which it reads and update leaves alone.
A command that prepares files a build reads but the app's repository does not
hold, such as a script that fetches photos into a folder Git ignores, goes in
`app.json`'s `before_build` (`["bun", "scripts/fetch.mjs"]`, where `bun` is
the Bun that runs `exact.mjs`, so it runs on every host): each verb that
builds, runs, compiles or checks the app runs it first in the app's
directory, and stops if it fails. Make it finish at once when the files are there.
`exact.mjs`'s `contract` verb is Exact's compiler, with paths relative to where you run it:

```sh
bun exact.mjs contract types app.contract -o app.contract.d.ts   # what app.ts imports
bun exact.mjs contract build app.contract --json
bun exact.mjs contract symbols app.contract
bun exact.mjs contract fmt --stdout app.contract
bun exact.mjs contract vocab padding                              # is it accepted, and how
bun exact.mjs web
```

Inside the Exact checkout, for its own apps, the same commands are
`cargo run -q -p contract -- <command> apps/<name>/app.contract …` and
`bun host/web/dev.mjs --app <name>`. Run `bun install --frozen-lockfile` there once.

To preview a component declared in your app's root, add a frame such as
`frames/card.contract` inside the app. Its first component is the preview's
root. It can pass props to the app's components and use the app's data handlers.

```text
use Inspector from "../app.contract"
component Frame
  view
    Inspector(title="Card")
```

Run `bun exact.mjs web` from the app directory and open
`/__dev/frames/card/` on the dev server. Each `frames/<name>.contract` has
its own URL. Editing a frame refreshes the pages whose compiled content
changes. An error appears in the affected frame, which keeps its last good
view while you edit.

To supply sample data, place a TypeScript module named `card.fixture.mts`
beside `card.contract`. Export an object keyed by the app's TypeScript
source names. Each handler receives the same arguments as its handler in
`app.ts`. This frame gives `frameData(title)` a fixed size.

```ts
import type { TypeScriptSources } from '../app.contract.d.ts';

export default {
  frameData: ([title]) => ({ title, width: 320, height: 200 }),
} satisfies Partial<TypeScriptSources>;
```

The frame calls the fixture for `frameData` and the app for every other
source. A fixture can return an empty list or throw to exercise an error
state. Editing or removing the fixture updates its frame. The app's release
build and other frames use their own handlers.

From the Exact checkout, build the frame with
`bun host/web-js/build.mjs /path/to/app --frame frames/card.contract --out /path/to/app/target/card`
and serve the output directory. The app supplies the design, theme and declared
data sources. A frame that needs navigation declares its own routes.

Use `build --json` before and after the edit. It returns one JSON array with all
independent diagnostics it can collect, capped at 20; success is `[]`. It is the
whole output: stderr stays empty (but for a note while the compiler first
builds), and `bun exact.mjs contract build`
appends `app.ts`'s type errors (`typescript-TS2322`, …) to the same array. Keep the
exit status: 0 success, 1 compilation/I/O failure, 2 invalid invocation. Read all
diagnostics, including related locations, before making the next repair. Each has
a `severity`: `"error"` refuses the build; `"warning"` (after the errors, also on a
success) builds and runs but is not what you meant, such as a header node the iOS
bar hides (`lower-header-unplaced`: beside the heading in its own box), a route's `header` a box below the route,
which iOS leaves in the page (`lower-header-nested`), a heading header at the head of
an app with no navigation route (`lower-header-routeless`: a lone `navigationKey`
is only a root; add a navigation root holding
a separately keyed screen whose first child is the header, or give the header
`appearance="none"` to keep it drawn as written on iOS), a content heading that says the bar's title
again (`lower-title-twice`), a tab bar of one tab (`lower-single-tab`), a
`button` with no words and no `aria-label` (`lower-button-unlabelled`), a dialog's
role on a box (`lower-dialog-role`), an `aria-level` with no `role="heading"`
(`lower-level-without-heading`), a content-box `width="100%"` with padding, wider than
its parent by the padding (`lower-width-padding`), a positive written border width without
a written line style (`lower-border-unstyled`), a box whose fixed width ceiling cannot grow
with its text at accessibility sizes (`lower-text-px-width`; use `rem` for the width and its bounds), a bare
`line-height=28`, which is 28 times the font size, not 28 points (`lower-line-height-multiplier`),
screens swapped by an `if` over a state, at the top of the view or inside a route (`lower-screen-by-state`), a count before a plural noun, `${n} cards`
(`lower-count-plural`), a row of the app's records that searches another table of them,
`users.filter(u => u.id == p.userId)` in `for p in posts` (`lower-row-lookup`), a
row that walks its own list, `tokens.slice(0, i)` in `for t, i in tokens`
(`lower-row-own-list`), a
symbol by an alias, not its role (`lower-symbol-alias`), a
`performanceNow()` time sent to a source (`type-performance-now-persisted`), the
launch's `time.epochAtZero` or `time.seed` as a resource's identifying argument, or a
value equal to one at launch (`time.epochAtZero + tick * 60000` while `tick` starts at
0), which keeps its kept answer from ever showing at launch: pass it after `with`
(`lower-resource-launch-identity`), a
state read after its own action assigned it (`analyze-read-after-assign`), a
literal colour that stays the same in dark mode (`lower-colour-one-scheme`: a build's
text says several in one line, its first three sites; `--json` names each), a `list` holding a `section` with no `appearance`, which is neither
a grouped list nor a list of cells (`lower-list-section`), and a
name or key a host would refuse or ignore at run time: a
swipe row's `swipeTrailing` naming a button outside its `scroll` (`lower-swipe-wiring`),
a navigation root's `navigationKey` that never meets its routes' (`lower-navigation-key`),
a `navigationSplit` off the root route (`lower-navigation-split`), an `image` source
that is a bare word, neither a symbol nor a file (`lower-image-source`), an `image`, `video` or `audio`
file outside `assets/`, which the app would ship without (`lower-asset-path`), and a literal `RoundButton`
label of more than the 7 characters its circle holds (`lower-round-button-label`); each names its repair, and fixing it is part of the edit.

Formatting is explicit. `fmt --stdout` previews, `fmt --check` checks, and plain
`fmt` writes. Formatting changes spacing and breaks only: a result that would
parse to a different program is refused (`fmt-tree-change`) and nothing is
written. Avoid formatting unrelated files. `symbols` reports definitions
and references, with component interfaces and inferred action effects. Search
by exact name with `symbols file.contract --name name`.

After compiling, build and drive the actual app. From an `exact new` app:

```sh
bun exact.mjs test web                          # the app's *.test.contract; builds a stale web app first
bun exact.mjs agent web tree "tap add" state logs "screenshot out.png"
bun exact.mjs agent web --size 390x844 "screenshot phone.png"   # a phone-sized viewport
```

On macOS and the iOS simulator, `agent` and `test` check the native build once
at startup and use it for every launch, including an authored test's `reload`
(LLP 1012.001.000 D1). Edits made during the run are checked by the next run.

The web carrier opens at 420×900; `--size <w>x<h>` (before the operations) opens
another viewport, and a test's first step `size <w>x<h>` does the same for that test.
`resize <w>x<h>`, an operation and a test step, resizes it mid-drive as a person
dragging the window's edge would: the browser's viewport, a macOS window, the
Linux presenter (the driver's op also takes `resize <w> <h>`; a test step takes only
`<w>x<h>`). iOS refuses a mid-drive `resize`; a size given at launch (`--size`, a
test's `size` line) is laid out at that viewport, scaled to fit inside the device's safe
area, so no safe-area inset, layout region or route's clearance is the device's, only
what UIKit's bars cover. A fold is read where it crosses the viewport.
`close`, an operation and a test step, presses the window's close button as ⌘W
or the red button would (the agent's window is never key, so a typed ⌘W reaches
nothing): the window asks its `beforeunload` first, and the reply says
`closed: true`, or `closed: false` for a window a handler kept (the web's
"Leave site?" is answered "Stay"). macOS and the web; iOS and Linux refuse it.
`--storage <name>` gives a drive a named scratch store, kept between drives on
macOS, iOS and Linux until one passes `--reset-storage`, and on web Chrome when the
next drive passes `--keep-storage` (a browser profile served on one port per
name; a Firefox or WebKit drive starts fresh each time, so a name there lasts
for the drive, including its `reload`). A native drive that names none uses the
store `default`, kept the same way, as the app keeps its files from launch to
launch. A drive that compares with a reference (`screenshot … compare`) starts
empty on every host unless it passes `--keep-storage`, so a plain drive after
one can show what an earlier plain drive stored. A web drive without one has no
app storage: an `app:/` file read finds an empty store (code `ENOENT`, as a
fresh install's first launch does), and a write or SQLite request is refused (`storage
is unavailable in agent mode unless the drive names a scratch store (--storage
<name>)`), so an app that keeps its data there does nothing on a write; a chosen document (`doc:/`)
still opens under its grant. That store, and an authored test's own store, includes
a data module's `secret.keep`: files in the named scratch tree on Apple and Linux,
and the page's `localStorage` on the web ([web sessions](recipes/web-sessions.md)).
Apple native modules use that store's data, cache and temporary roots too; a fresh
drive clears them, and only picked temporary documents are cleared on each launch.
A nameless drive keeps no secrets.
To show what survives a restart on any host, use an authored test's `reload` step (below).

The driver prints each reply in a short text form. A `tap` or `type` is one line
(`tapped #15 "add" · delivery platform · epoch 2 · clock 0 ms`); a failed one starts
with `ERROR`. `state` prints one line a section (`slots {"count":1}`) and leaves out
the empty ones (null, `[]`, `{}`); one over 8 KB prints its size, its small sections,
the others' sizes, and how to read one (`state resources`, `state feed.0`) or all of it
(`state --full`). `state <name>` reads a state, derive, resource or section of the
record, even where a view carries the same name. Every other reply is one line too
(`clock 600 ms · settled · epoch 5`, `screenshot out.png · 420×900 · …`).
`--json` prints every field of every reply, one JSON object a line.
`--scale <n>` shrinks each screenshot to n pixels a point: an iPhone's are
1206×2622 at its own 3, 402×874 with `--scale 1`, one pixel a point as on the
web, at about a ninth of the pixels. An app's `bun exact.mjs agent ios|macos`
takes `--scale 1` unless told otherwise; ask for more only to check fine pixels.
On a physical iPhone (`agent ios --device`), a still screenshot takes the display
through Xcode (`pixels="display"`), including video, system bars and the visible
software keyboard. Its dimensions are in screen points (`captureSpace="screen"`)
when the app can confirm them. Otherwise the driver keeps the original PNG at
its native pixel size and reports those dimensions with `scale: null`.
A still taken during a held contact also uses the display and skips the motion wait.
The driver checks app focus before and after capture. If focus changes or it
cannot confirm focus, the reply notes that the display may show another app
or a system overlay. Scaling a display still keeps its embedded RGB colour profile.
`check` uses the same display source on a phone. If the capture cannot be
matched to the app's observation, `check` retries once and reports an
incomplete check if the retry fails.
`window app` and timed films take the app's drawing (`pixels="app"`).
On a simulator, `screenshot <png> display` takes the full screen and reports the
same screen geometry and keyboard fields. A display reply names authored header
nodes UIKit's navigation bar omits under `unshown`.
On a simulator a `window` screenshot's pixels are the display's (`pixels="display"`):
glass, blur and the keyboard as the device draws them. So are a plain one's where glass
or a blur lies in the page; where the display cannot be read (the app not in front, an
overlay) the reply says its materials are approximate, and why.

Inside the Exact checkout, for Caltrain:

```sh
bun host/web/build.mjs caltrain-web
bun scripts/agent.mjs web tree "tap change-station" "type station-search Palo" state logs
bun scripts/agent.mjs web --test apps/caltrain/app.test.contract
```

For an external app driven from the Exact root, set `EXACT_APP_DIR` on both build
and drive. A stale
artifact is a failed verification; rebuild what the driver names. A successful
Cargo rlib build does not prove a native app launches or behaves correctly.

tvOS uses the iOS presenter and plan: `bun host/apple/build.mjs --tvos
caltrain-apple --run` builds and launches on an Apple TV simulator. TypeScript
apps provision lean Hermes for `tvos-simulator` once per machine. On a container,
`focusGuide="auto"` makes an entering remote move return to its last focused
descendant, or its first focusable descendant. Other hosts ignore it, including
when the value is bound. The agent driver has no tvOS target yet.

A script drives the same session in JavaScript: `const s = await open({ host:
'web', app, epoch, timeZone, storage, size: [390, 844] })` from `scripts/agent.mjs`
(`size` is `[width, height]`, or the CLI's `'390x844'`), then
`s.tap(target, opts)`, `s.type(target, text | { key })`, `s.clock(arg)`,
`s.tree()`, `s.state()`, `s.logs()`, `s.layout()`, `s.screenshot(path)`,
`s.resize(width, height)`, `s.prefer({ … })`, `s.perf(target)` and `s.close()` — the
CLI's operations by the same names. Their replies are the CLI's `--json` output: `s.tree()`
is `{ roots, nodes: [{ id, type, depth, props: { testId, text, value, … }, children }] }`,
every node in one preorder list (`depth` and `children` give the nesting),
with `generation`, `epoch`, `incarnation` and `clock` on replies. `generation` advances
once per committed replacement or carried reboot; `epoch` continues across it, while
a new kernel can repeat `incarnation`. `state.replacement` names root slots dropped
because they are gone (`no place`) or their type changed (`type changed`).
`s.state()` is `{ slots, derives, resources,
pending, … }`. `app` is the app's name; an app outside the Exact
checkout is found through `EXACT_APP_DIR` (its directory), as its own `exact.mjs agent`
sets it, so a script run with plain `bun` sets it to the app's directory too
(`EXACT_APP_DIR=/path/to/app bun verify.mjs`; a script that `app.ts` never imports
is no build input); the working directory alone does
not select the app, and `webDist` alone does not select an app. `s.op(request)` is the
host's wire beneath them: it addresses views by numeric `id`, and it refuses a
request it would answer by doing nothing (a `target`, an unknown op, a web
`tap` with no browser input behind it). A reload or a raw browser step goes
through `s.carrier` (`reset({keep: true})` reloads the current browser history
entry with its store and agent launch facts, keeping Back and Forward entries;
`evaluate`, and on Chrome `call`, drive raw browser steps;
`evaluate` takes an expression string or a function of no arguments, which runs in the page,
so it sees none of the script's variables).

Use the existing six repository checks for repository changes. Do not add a
new global check or fixture framework for an ordinary app edit. For documentation,
compile complete examples, parse authored tests, and check local links.

## Language inventory

| Form | Placement | Meaning |
| --- | --- | --- |
| `use A, B as C from "./file.contract"` | File | Names from another file; nothing unnamed comes along (LLP 1091); `contract fmt --uses app.contract` writes the lines `contract-use-missing` names |
| `use Card from "@acme/ui"` / `use Activity from "exact:motion"` | File | A package's names (from `node_modules`), or a built-in's: `exact:motion`, `exact:content` ([content components](#content-components-exactcontent)) |
| `shape Name` | File | Finite record with typed fields |
| `fn name(arg: T): U = expr` | File | Effect-free, nonrecursive expression function |
| `style Name` | File | Literal style attributes |
| `keyframes Name` | File | Constant animation frames |
| `font "Family" = "path.ttf"` | File | Bundled family; block form supports faces |
| `routes nav` | Root file | Route table and implicit router state |
| `component Name` | File | Root (first component) or reusable child |
| `test "description"` | Test file, normally | Agent interaction steps; not a plan assertion block |
| `props` / `inject` | Component | Typed interface / inherited bindings |
| `provide` | Component | Whole-view provider bindings, one per line |
| `slot` | Component | Accept caller-authored children |
| `state name = expr` / `state name: T = expr` | Component | Stored state; its type is inferred unless declared |
| `derive name = expr` | Component | Dependency-driven computed value |
| `resource name = source(args) as shape T` | Root or bounded child | Reactive data request |
| `mutation name as shape T [queue \| parallel]` | Root or bounded child | Optional reply slot for explicit sends |
| `action name(args)` | Component | Event transaction; effects inferred |
| `task name mount` / `task name when cond [key=expr]` | Root or bounded child | One timer/frame schedule or keyed change action, always or while `cond` holds |
| `view` | Component | UI tree |

Top-level declarations begin in column 1. Indent with spaces. Comments use `//`.
Use double quotes for strings and backticks for templates. Names admit CSS
hyphens, so write `a - b` for subtraction between identifiers. Units are quoted
(`"12px"`, `"50%"`), not number suffixes. Element attribute continuation lines
are deeper than the element and begin with `name=`. Parentheses permit multiline
calls and expressions without creating an indentation block.

Only sixteen words are refused as names, and only where a name is bound (a
state, prop, parameter, `let`, `for` item, shape name, …): `when`, `if`, `else`,
`for`, `in`, `match`, `case`, `as`, `fn`, `and`, `or`, `not`, `true`, `false`,
`none`, `some`. Every other keyword (`key`, `state`, `from`, `refresh`, `view`, …)
is an ordinary name there, so `action searchKey(key: string)` and `action refresh`
compile. Shape fields, named arguments and members take any word
([grammar](contract-grammar.md#lexical-rules)).

`contract vocab` prints the card, what an app writes in one read (at most 12 KB: every
function, the tags with their attributes and events, each part's call, the test steps,
the agent's operations, and how to do a second screen, a sheet, a timer, colour, data, a
test; `docs/card.md` is the same text, checked in); `contract vocab --all` lists every built-in tag, attribute, CSS
property and standard function the compiler accepts, with each property's value kind and
default and each function's signature; `contract vocab <name>` answers for one (a function's rules and
an example too: `contract vocab formatDate`), or says what to write instead
(`contract vocab sort`). Check there before guessing at CSS or a function.

## Values, expressions, and functions

An expression Contract accepts means what the same TypeScript means, except as
this table says (LLP 1116.016 D1; `semantics/corpus/typescript/` checks each
against JavaScript), and a TypeScript form Contract refuses is refused with
what to write. Nothing runs because a value changed (no `useEffect` or watch).

| Contract | Because |
| --- | --- |
| No truthiness: a condition is a `bool` (`if name != ""`) | every condition says what it tests |
| Absence is an option (`none`, `some(x)`, `match`), not `undefined` or `null`; no `??` or `?.` | a value whose type has no absence is never absent |
| A value never changes: no `push`, `sort`, `xs[i] = …`; `concat`, a record copy | an action is one transaction |
| `==` compares lists and records by value; no `===` | values have no identity |
| A string is well-formed: a lone surrogate becomes U+FFFD (`"😀".slice(0, 1)`) | every host holds Unicode scalar values |
| A stored number is finite: a commit that would put `NaN` or `Infinity` in a state or derive is refused, and the write that caused it does not land | every host can carry it |
| A resource or `send` argument must contain only finite numbers, including inside lists, records and options; the runner refuses the call before asking its source and names the argument position | Rust and TypeScript sources accept the same values on every host, whether placed on the main thread or a worker |
| `toFixed` and the format functions print `""` for a non-finite number | one rule for every format |
| Names hold hyphens, so subtraction takes spaces (`a - b`) | CSS's names |
| A binding's list work is bounded (65,536 steps); a row's own binding has its own bound, so a row that searches another table costs rows × items (`lower-row-lookup`), and one that walks its own `for`'s list grows with it until the binding is unset (`lower-row-own-list`) | determinism on every host |
| No comparator sort: `sortBy(xs, x => key)` orders by one number or string key, stably (`"desc"` third; a second key is an earlier `sortBy`). Grouping, joining, aggregates and a stored or locale order are the data module's; the view names an order it relies on where it reads it (`loadTasksByDueDate()`) | a view computes what it shows, the same on every host |

- Types are `number`, `string`, `bool`, declared shapes, `option<T>`, `T[]`,
  and `action` for behavior interfaces. Do not emit authored `any` or object types.
- `[a, b, c]` is a list of its items, which may span lines and keep a trailing
  comma; the items have one type, met as a ternary's arms are (`[none,
  some(1)]` is an `option<number>[]`; `[1, "a"]` is `type-list-item`).
  `xs.concat(ys)`, `xs.slice(start, end?)`, `xs.includes(x)` and
  `xs.indexOf(x)` are the web's array methods: `[...xs, x]` is
  `xs.concat([x])`, `includes` finds a string, number or bool (by
  SameValueZero, as the web does) and `indexOf` says where (by `===`: `NaN` is
  never found, `-1` for none). Over text, `s.indexOf(t)` is a position in
  UTF-16 code units, and `s.split(", ")` cuts text into a `string[]`
  (recipient chips). A list the
  screen keeps for the session — a selection, open or collapsed ids, per-row
  offsets — is `state` built this way; a list the app keeps across launches, or
  a server owns, belongs to the data module, and so do grouping, aggregates
  and a stored order (a view orders what it holds with `sortBy`):

  ```contract
  state collapsed = []
  action toggle(id: string)
    collapsed = collapsed.includes(id) ? collapsed.filter(c => c != id) : collapsed.concat([id])
  ```
- `none` and `[]` need an inferable element type. A state initialized by either
  usually gets that information from later assignments; a typed argument or
  the other conditional/match arm can also supply it, and a state may declare
  it: `state picked: option<Item> = none`, `state ids: string[] = []`.
- An option directly inside an option is refused (`type-option-option`):
  `option<option<T>>` written, `some(none)`, `first` or `at` of a
  `option<T>[]`, a mutation `as shape option<T>`. The web erases `some`,
  so `some(none)` would be `none` there; hold the inner option in a record
  field. A type nests at most 64 deep, through shapes, lists and options
  (`type-too-deep`).
- One evaluation (an action body, a derive, a binding, a key, an argument)
  takes at most 65,536 list steps (each `map`/`filter` body run, each item
  `join` prints or `concat`/`slice`/`split` keep, each item `includes` or
  `indexOf` scans), builds strings of at most 64 MiB of UTF-8, and values of at
  most 2²⁴ values and 64 MiB of string bytes. Every target refuses the same
  step with the same reason: an action is refused with nothing changed; a prop
  or style binding's row is unset, with one journal line naming its site and
  the trap until it evaluates to a value again, and the app goes on; an `if`,
  `match` or `for` subject, a key, a row's state initializer, a surface
  argument or a virtualized list's own options stops the runner (LLP 1090,
  LLP 1017.003 D3). Every build keeps `Trap(Kind { pc: N })` verbatim, then
  names the evaluation and bound in words: a derive or action's name, a resource
  argument, a state initializer, or the view and plan site with its event
  argument, subject, key, initializer, surface argument or list option. The JS
  target names the site alone. Arguments count from 1. A binding already names
  its site and row, so it adds only the bound. An inlined child derive or `fn`
  names its reader; the pc is local to that code body, not a source location.
- A state's initializer runs before any resource answers and before any derive:
  it reads props, injects and the states declared above it, nothing else
  (`type-initializer-scope`). Derive a value from a resource instead, or keep
  per-row state in a component used inside `for`.
- `Shape(field=value, …)` constructs every field exactly once.
  `Shape(base, field=value, …)` copies a base of the same shape and replaces fields.
  The one positional base comes first. Compiler-owned shapes are not constructible.
- Use field access, arithmetic, comparisons, boolean operators, ternaries, and
  exhaustive option matches. There is no truthiness or optional chaining.
- Inline match is `match x { case some(v) => a, case none => b }`; the comma is
  required and the `some` arm comes first. The block statement/view form has
  separate indented `case` arms.
- `xs.map((x, i) => expr)` and `xs.filter(x => bool)` return values, not nodes.
  `xs.join(separator)` accepts primitive items. `first(xs)` and `xs.at(i)`
  return options; `at` supports negative indices.
- Two strings compare with `<`, `<=`, `>`, `>=` in UTF-16 code-unit order, as on
  the web (`end > start` for `"HH:MM"` times). `s.slice(0, -1)`,
  `s.replaceAll(find, with)` and `s.toLowerCase()` are the web's string methods.
- Numbers have `floor`, `ceil`, `round` (JavaScript's `Math.round`: `round(-2.5)` is
  -2), `min`, `max`, `%`, `formatNumber` (`"compact"` is `1.2K`, `"decimal"` `1,234.5`,
  `"percent"` `26%`, `"currency", "USD"` `$1,481.47`), `toFixed` and `formatDecimal`. A
  field's text is a number through `match parseNumber(s) { case some(n) => …, case
  none => … }`: a decimal numeral, trimmed, or `none` (`"12px"`, `""`).
- Money is a count of cents printed with `formatDecimal(cents, 2)`, which is
  exact (`1234` is `"12.34"`, `-5` is `"-0.05"`). A price held as dollars
  becomes cents with `round(price * 100)`, which is right for a price of at
  most two decimals under a trillion (`19.99` is `1999`, though `19.99 * 100`
  is `1998.9999999999998`); past about `3.5e13` the double cannot hold the
  cents, and a half cent such as `1.005` has no exact binary value and
  becomes `100`, so keep money in cents from the source when amounts can
  have more places (tax, a split bill). `formatDecimal` of a number that is
  not an integer prints `""`, so round first. `x.toFixed(2)` is the web's
  `x.toFixed(2)`, for a measured number (`km.toFixed(1)`): it rounds the
  binary value too (`(1.005).toFixed(2)` is `"1.00"`) and sums of dollars drift
  (`0.1 + 0.2`), which is why money is counted in cents. Both take the digits as a whole-number literal (`toFixed` 0–100,
  `formatDecimal` 0–20), never a variable, and print `""` for `NaN` or
  `Infinity` (JavaScript's `toFixed` prints `"NaN"`).

```contract
component Cart
  state cents = 1999
  state km = 12.34
  view
    column
      text `Total $${formatDecimal(cents, 2)}` testId="total"
      text `${km.toFixed(1)} km` testId="distance"
```
- Dates: `formatDate(ms, offset, "iso")` is `YYYY-MM-DD`, and `calendarDiff(from, to,
  "years")` (or `"months"`) is the whole periods between two such dates as an
  `option<number>`, counted as an age is (a Feb 29 birthday has its year on Mar 1).
  `formatDate(due, 0, "medium")` labels a picked `YYYY-MM-DD` day directly;
  a key keeps its calendar day at every valid offset ([dates recipe](recipes/dates-and-days.md)).
  `formatDay(then, now, offset, "day")` names a day from `now` (`Today`, `Yesterday`, else
  the date; `"day-time"` adds the time, `"list"` is a list row's, as Mail's, and
  `"section"` a chat's day separator: a weekday within the week, else `Thu, Oct 15`).
- `formatTime`, `formatDate`, `formatDay` and `formatNumber` follow the strings table
  the app shows: with `strings/de.json` shown they print `26.09.2026`, `13:02`,
  `1.234,5` and `Heute`, and `setLocale` moves them with the text; the examples above
  are en-US, which an app with no tables or only English ones prints. Do not write a
  formatter of your own per language, and do not key one on `resolvedLocale`. A
  table's region is its formats' (`en-GB.json`, which may be `{}`, prints `26 Sep
  2026`); dates are Gregorian everywhere
  ([formatting by locale](contract-grammar.md#formatting-by-locale)).
- There is no general list append: add an item to
  resource-backed data in its source and answer the updated list (a mutation that
  `refreshes` the list's resource, or its own answer).
- The web's string and array methods are written as methods, with their names
  and meanings: `s.trim()`, `s.includes(q)`, `xs.filter(x => …)`, `n.toFixed(2)`,
  `xs.length`. The free spelling (`trim(s)`) is refused with the edit
  (`syntax-method-form`), and so is a method Contract lacks (`xs.reduce(…)`,
  `syntax-method-call`). The rest of the roster is functions: `first(xs)`,
  `round(n)`, `parseNumber(s)`, `formatNumber(…)`. There are no object
  literals, spreads, general lambdas, array indexing,
  assignment expressions, or JavaScript built-ins by implication.
- `fn label(done: bool): string = done ? "Done" : "Open"` is a function: parameters
  and the return type are explicit, after `:` (not `->`). Its body is one expression over
  its parameters and standard calls (including `performanceNow()`), without component-state
  capture or recursion. Pass an app value in; do not invent an ambient reference.
  A `fn` named like a standard function (`fn indexOf`) shadows it in every
  free call of the app, so a standard function added later never breaks an
  app that had the name first; an action or a host command keeps its own rule.
  A method call (`xs.indexOf(x)`) is always the built-in: calling one whose name
  the file also declares is refused (`syntax-method-shadowed`).
- Named arguments belong to component uses, record constructors,
  `t("key", placeholder=value)`, `empty(field=value)`, canvas `surface=` bindings,
  the commands `share(…)`, `scrollIntoView(…)`, `playSound(…)` and
  `stopSounds(…)`, and a native module's sources and sends
  (`showNotification(…)`). Every other function takes positional arguments.

For the full roster and special calls, see
[standard functions and intrinsics](contract-grammar.md#standard-functions-and-intrinsics).
For record behavior in a callback, use [records](../contract/corpus/records.contract)
and [the record tests](../contract/cli/tests/it/records.rs).

## State and action semantics

Action reads see the starting state. Writes commit together. They do not provide
imperative read-after-write visibility. Bind intermediate calculations with `let`:

```contract
component App
  state count = 0
  state doubled = 0
  action advance
    let next = count + 1
    count = next
    doubled = next * 2
  view
    button press=advance testId="advance"
      text `${count}/${doubled}` testId="result"
```

The first press yields `1/2`. Writing `doubled = count * 2` instead would use the
old count and yield `1/0`. The same snapshot rule applies inside nested branches
and to locals declared after a state assignment, and a derive read after the
action assigns its input is the starting state's derive. The compiler warns when
a state is read that way (`analyze-read-after-assign`), naming the line that
assigned the slot, but not when a derive is: read the local you assigned. It
also warns of a send and an assignment of one newest-wins mutation on one path
(`analyze-send-assigned`): the assignment forgets the request in flight and keeps
its value, whichever comes first, so a retry hides the old result while
`pending(m)` instead, and a logout sends through a mutation of its own.

A local is immutable, block-scoped, and visible only after its declaration. It
cannot shadow an in-scope name. Two disjoint arms may declare the same local name.
An action can assign only its own writable state/mutation slots, never a prop,
derive, resource value, or arbitrary record field. Replace a record with a copied
record. `send` targets a mutation owned by that component.

Permitted statements: assignment, `let`, `send`, `refresh`, known host command,
a call of an action, `if`/`else` (`else if c` is `else` around one nested `if`),
and option `match`. No loops: compute the list or value with `map`, `filter` or
`sum` and assign it once. No `return`: a value to compute is a `fn` or a derive.
The compiler
infers effects from the body, through its calls; `writes` is a refusal, not an
optional annotation. Use `symbols` when you need the inferred write set.

An action calls an action of its own component, an `action` prop or an injected
action by name with its parentheses (`play()`, `arrive(path)`; a bare `play` is not a
statement), anywhere a statement goes (LLP 1089). The call is
the callee's statements run where it stands, in the same commit: one rollback,
and the callee reads the state the action started with, as every statement does.
A name that is a host command stays the command. Never give an action, an `action`
prop or an inject a host command's name (`close`, `reload`, `setScheme`, `focus`, …)
if you will call it: the call is refused, `syntax-call-ambiguous`, so a host command
added later can't silently take it over. Binding such a name is allowed. A call gives no value; compute
values with `fn`. Write a repeated sequence once and call it:

```contract
component App
  state location = "/"
  state query = ""
  state sel = 0
  action arrive(path: string)
    location = path
    query = ""
    sel = 0
  action openItem(path: string)
    arrive(path)
  action goHome
    arrive("/")
  view
    column
      button press=openItem("/docs") testId="open"
        text location testId="where"
      button press=goHome testId="home"
        text "home"
```

Because the callee sees the starting state, a slot its caller assigned first
is stale behind the call, and the compiler warns of that read as it does in the
action's own body (`analyze-read-after-assign`, naming both frames): `sel = next`
then `follow()`, where `follow` reads `sel`. Pass the value it should see instead:
`follow(next)` for the new one, or a `let` bound before the assignment for the
old one. Two calls that each read and write one slot (`move(1, 0)` then
`move(0, 1)`) are named the same way: both read the starting cell, and the last
write wins. Calls in exclusive branches
(`if k == "ArrowUp" …` then `if k == "ArrowDown" …`) are separate paths.

A derive is not mutable storage, an async effect, or a timer. Derive cycles are
refused. Avoid unnecessary state that can be calculated from existing values.

### Window state and shared settings

Use `state conversation = "" persist window` for a selection that belongs to
one window. The browser keeps it in `sessionStorage`. Reloading the tab restores
it. An independent tab or a link with `target="_blank"` starts fresh. A named
window with an opener initially receives a copy and then keeps its own values.
AppKit and UIKit restore the values when the system restores the window or scene.

### A remembered setting: `persist`

A root `state` that ends in `persist` is kept across launches (LLP 1116 D5), as
SwiftUI's `@AppStorage` or a React `useState` over `localStorage`:

```contract
component App
  state tipPercent = 18 persist
  state units = "C" persist
  action toggleUnits
    units = units == "C" ? "F" : "C"
  view
    button press=toggleUnits testId="units"
      text `${tipPercent}% · °${units}`
```

- Before the first frame the store is read: a kept value of the state's type
  replaces the initializer's. A missing one keeps the initializer's, and so does
  one of another shape (the state's type changed since), which `logs` names
  (`persisted state units: the stored value does not fit string: …`).
- After a commit changes the state, a value within the limit is saved to the page's
  `localStorage` (`exact.state.<name>`), the app's `UserDefaults` domain on Apple
  (never the Keychain), or a file under the app's data directory on Linux. The key
  is the state's name, so renaming the state forgets what was kept.
- Small settings only: a number, string (an `enum<…>` too) or bool, or an option or
  list of one, at most 16 KB as JSON. A kept enum member a later version dropped is
  of another shape: the initializer's value stands. A longer value leaves the last stored value intact, and
  `logs` reports it once until it changes. `persist` on a record, a list of records
  or a state holding an action is `type-persist-type`: keep app data in a source.
  A child component's state is `type-persist-child`; a derive, `syntax-persist-derive`.
- Each authored test starts from an empty store, and its `reload` reads back what
  it kept. A drive with `--storage <name>` keeps persisted state in that store
  between drives (on the web, the next one passes `--keep-storage`; on a native host
  `--reset-storage` empties it); a drive without one starts from the declared values and keeps
  nothing past the drive. The recipe:
  [a remembered setting](recipes/persisted-setting.md).

### Editing a value: the field's contract

An `input` handler should update raw draft state on every edit so live search,
validation and button enablement follow typing. A text field fires `change` on
Enter or blur; its handler should normalize the draft. Use `submit` for explicit
submission.

A text field is re-set only when what its `value` binding reads changes, never
after each keystroke (React writes the bound value back; Exact does not, so a
half-typed `-` or `1.` survives). That makes the contract:

- **while editing**, the field is bound to raw text in state that `input`
  always writes;
- **validation** reads the parsed value (`parseNumber`, a trim, a length) and
  shows a hint, without touching the text;
- **on commit** (`change`, which a text field fires on Enter and on blur), the
  action writes the accepted value and writes the normalized text back into the
  draft, which changes the binding and redraws the field.
  Since `change` also fires when an edited field loses focus, it is no
  "submit on Return": a form submitting on it submits on a tap away from the
  field (an eye, a "Forgot password?"). Submit on Return with `submit`, which
  runs on Return alone, after the commit (and never for an IME's Return).

A field bound straight to the accepted value breaks this: an action that
normalizes `-2` to the `0` it already held leaves the binding unchanged, so the
field keeps showing `-2`. So does normalizing while the person types: a `task … when draft != …`, a
timer or an `input` action that rewrites the draft (an empty field back to
`"1"`) puts text back under the caret mid-edit, and the next keystroke lands
after it (`1` then `3` reads `13`). Normalize only in `change`.

```contract
component Quantity
  state count = 1
  state draft = "1"
  action edit(text: string)
    draft = text
  action commit(text: string)
    match parseNumber(text)
      case some(n)
        count = max(0, round(n))
        draft = `${max(0, round(n))}`
      case none
        draft = `${count}`
  view
    column gap=8
      input value=draft input=edit change=commit inputmode="numeric" aria-label="Quantity" testId="qty"
      text (match parseNumber(draft) { case some(n) => (n < 0 ? "Must be 0 or more" : ""), case none => "Enter a number" }) testId="qty-hint"
      text `Ordered: ${count}` testId="qty-count"
```

`type "qty" "-2"` leaves `-2` in the field with the hint; `type "qty" key
"Enter"` commits, and the field reads `0`. A checkbox bound to a resource's
field follows the same rule from the other side: it shows the resource's value,
so it snaps back until the save answers and the resource is read again (LLP 1102
§3.16).

## Composition and lifetime

The first component is the root. Each component use is `Name(prop=value, …)`.
Supply every declared prop exactly once; props have no defaults (pass `none` for
an option). An `action` prop or inject takes an action name or `action(args)` with
captured arguments; the eventual event payload is appended at invocation.
To choose between actions, pass one action that makes the choice in its body.
A conditional supplied as an action prop or inject, including through a derive,
is refused at the argument or provider binding.
A child derive that aliases an action name or captured call can supply either binding.

Value props and injects replace value reads only: beside `min: number`,
`min(4, 9)` still calls the function, in views and actions alike.
An `action` prop or inject named like an app `fn` is refused when an expression
in the component calls that name and inlining could substitute the action into
the call. Rename either declaration. Handler-only uses are allowed. An
expression's `match` or arrow binder protects its bound name; a view's `for`
or `match` binder does not protect call heads.

A derive supplies a value, even when its expression is a function call.
Beside `fn pick`, `derive pick = pick(1)` makes `${pick}` read that result;
`pick(5)` still calls the function.
States and device-fact resources also supply values only: beside `fn pick`,
`state pick = 7` makes `${pick}` read `7`, while `pick(5)` calls the function.
These names leave calls intact in derives, state initializers, actions and slot fills.

A child's action may call its `action` props and injected actions, with the
arguments after those captured where they were bound: the parent's action runs
in the same commit, and the child never writes the parent's state itself. A
swipe that decides, in the child, to tell its parent resets its own state, then
calls the prop last in an `if` arm:

```text
component Row
  props
    id: string
    archive: action
  state dx = 0
  action release(dy: number)
    dx = 0
    if dy > 120
      archive(id)
```

Children may hold state, derives, actions, resources, mutations and tasks
at bounded uses
([A feature's requests](#a-features-requests)). A child used under `for` owns row state keyed to that row's identity.
Stable keys matter when content is inserted, removed, filtered, or reordered.
A child's derive is compiled once per component use and evaluated at each read
in that instance's frame. A computed prop or inject argument shares that
representation. Each read uses the reader's list and call budgets and can
trap there. A literal stays inline, so the web compiler can fold
`size == "sm"` when a component takes the enum argument `size="sm"`.
A root derive evaluates
once per settlement, so put a calculation there when its result should be
shared across readers. Action-body reads, action calls and `fn` calls still
expand in place. The web build reports its entry's bytes and each component's
own plan instructions and compiled instance count. Shared bodies count per
use; a `for` is one compiled instance regardless of its runtime item count.


A root that has outgrown the 1,500-line cap keeps its members and moves them
into files by feature: `members "./composer.contract"` in its body, and in that
file a `members App` block of states, derives, resources, mutations, actions,
tasks and sequences. The program is the one-file program, and every file sees
every member; each file `use`s the shapes and `fn`s it calls
(`docs/contract-grammar.md`, "Components and actions").

A state cannot start from a resource, but a child's state can start from a prop.
So a form that edits a saved record is a child made once the record is in: the
source answers `loaded: true`, the placeholder says `false`, and the child's
states take the record's fields when it is made. The initializer runs once per
child: a later `draft` (a refresh, a normalized answer) does not reset the form,
and while a refresh is out `saved` keeps its value, so the editor stays.

A native host (and the wasm web target; the default JS target keeps none) can make
that child from a kept answer. When the data module is not ready at boot, a resource
whose source read device state (a file, a database, a secret, a watched topic) shows
the last answer the runner kept for it (asked with the same identifying
arguments, and at most 256 KiB counting them, 512 KiB for all of them together; `with` values are not counted), so
the form's states take those fields. A larger answer is not kept, and an older one
kept for the resource is forgotten, so a launch never shows an answer since replaced;
`logs` says `too big to keep` once, naming the resource and what to do. An
identifying argument that changes every launch (the launch's time or seed) never
matches the kept one: pass it after `with`, as `resource lib = library() with
time.epochAtZero`; the compiler and `logs` both say so. A source declared in
`app.contract` keeps no answer, as the launch asks it at boot, and neither does a
resource whose identifying arguments read a declared resource, which a launch
starts on its placeholder: pass such an argument after `with` when it does not
identify the answer. An identifying argument that differs from the kept one and
is `""` or `none`, or reads a device-state resource still on its placeholder (an
id an action sets once a session answers, a search field's empty query), is not
known yet: the kept answer waits, `logs` says so, and it shows once the arguments
reach the ones it was kept for, also after the data module is ready, unless an
answer for other arguments was kept in its place first. A known value that
differs means the kept answer is not used; `logs` names the argument and the
state or resource it reads, never an identifying value.
An answer that must never reach the device's storage (a one-time code, a message
shown once) ends its line with `transient`: `resource code = otp(request) as shape
Code transient`. It is never kept, a launch starts from its placeholder, and an
answer an earlier build kept is forgotten at the next launch. To keep no answers at
all, an app's grants decide (a Health grant keeps none).
Keep a first screen's resources small and move their bulk into a resource the first
frame does not need. A write the resource does not hear about (a file or database save from
a mutation without `refreshes saved`, not a `store` write it reads) leaves that
answer behind: after a restart the form opens with the old fields and keeps them
when the fresh answer lands. Refresh the resource after each write, as below, or key
the child by a string or number from the answer (`for d in [saved] key=…`) so a
different answer makes it again.

```contract
shape Draft
  name: string
  loaded: bool

component App
  resource saved = loadDraft() as shape Draft else empty(name="", loaded=false)
  mutation stored as shape Draft refreshes saved
  action keep(name: string)
    send stored = saveDraft(name)
  view
    main
      if saved.loaded
        Editor(draft=saved, keep=keep)

component Editor
  props
    draft: Draft
    keep: action
  state name = draft.name
  action edit(value: string)
    name = value
  action save
    keep(name)
  view
    column
      input value=name input=edit aria-label="Name" testId="name"
      button "Save" press=save testId="save"
```

A component `provide` section lists `name = expr` or bare `name` entries. A
descendant declares `inject` with typed fields. The nearest provider in the
component-use chain wins and covers the providing component's whole view.
The old view wrapper `provide name = expr` is refused. Use a small wrapper
component if a subsection needs different values.

`slot` declares a receiving component's one content slot. `children` places the
caller's nodes in its view. The fill keeps its caller's variables and providers,
not the receiving component's provider override. Do not invent named slots or
render-function props.

### One element shown in another place: `portal`

An `if` arm creates and destroys, and a key keeps a row only within its own
list, so declaring a playing video again in a detail screen restarts it. Write
it once, outside the screens, and name where it shows with `portal` (LLP
1013.002). The same element moves; its state, handlers and lifetime stay its
declaration's.

```contract
component App
  state open = false
  state alive = true
  action toggle
    open = not open
  view
    column
      button press=toggle
        text (open ? "Close" : "Open")
      box id="card-player" width=168 height=148
      box id="detail-player" width="100%" height=320
      if alive
        column portal=(open ? "detail-player" : "card-player")
          video "assets/motion.mp4" autoplay=true muted=true playsinline=true loop=true
```

- The destination is the first element with that `id` in declaration order in
  the app root. It must be a plain box (`box`, `view`, `column`, `row`, `main`,
  `section`, `article`, `aside`, `nav` or `footer` with no `role`,
  `navigationKey` or `popover`) that its parent lays out. A list's row, a
  table's part, a route's header item, a button's face, a dialog or a route
  can't receive content or move; the compiler refuses what it can prove.
- The element goes after the destination's own children. An empty value
  leaves it where it is written.
- With no destination (a missing id, its own descendant, a cycle, a box inside
  hidden content) it is alive but not shown: no space, no input, not in
  `tree`, journaled once. A muted autoplay video there pauses at its position
  and plays on from it when shown (Firefox keeps it playing).
- Its press, pointer-button and key events bubble through where it is written,
  not where it is shown; hover, focus on the web and the painted hosts' pan,
  swipe and long press still follow where it is shown.
  Colours, `color-scheme` and modes come from where it is shown (on the web;
  native hosts still resolve design-token modes at the declaration).
- Removing the declaration removes it; a new one starts fresh.
- Hosts: the web, macOS, iOS and Android move the same element, and a playing
  video plays on through the move; Linux and Windows move it with no video
  decoder. A page rendered ahead leaves it out and builds afresh. The
  reference's "A `portal` element on each host" has each host's table.
  `docs/recipes/keep-a-playing-video.md` is a card that opens into a detail;
  `apps/shared-elements`' Player and Flight screens show both forms.

Imports resolve local `.contract` declarations, with cycle and app-boundary
checks. Paths begin with `./` and cannot contain `..` segments. They do not load app logic or expand the available built-in function
roster. Compiler-owned router names exist only with a `routes` declaration.

### A feature's requests

A feature written as a child (a tab's screen, a pushed screen) owns its
resources as it owns its state: it declares them. Each instance asks for its
own after the render that makes it, and lets go when the render destroys it, so
an answer that arrives after a screen was popped changes nothing. A tab's root
screen lives for the session, hidden while another tab shows; a pushed screen
lives from the push to the pop.

Permission to ask is `when`, not visibility: a hidden instance's ungated
resource keeps its answer and asks again when its arguments change or it is
refreshed. Gate a tab's resource on its tab showing, by a prop the parent
passes:

```contract
routes nav
  tab today "/"
  tab history "/history"

component App
  view
    main
      for t in nav.tabs key=t.name
        column role="tabpanel" display=(nav.tab == t.name ? "flex" : "none")
          for e in t.stack key=e.id
            if e.name == "history"
              History(shown=(nav.tab == t.name))
            else
              text "Nothing logged yet."

component History
  props
    shown: bool
  state range = "week"
  resource days = days(range) when shown as shape string[] else empty()
  action reload
    refresh days
  view
    column
      button press=reload
        text "Reload"
      for d in days key=d
        text d
```

`days` asks nothing until the History tab first shows. While the tab is hidden it
is idle and keeps its last answer; when the tab shows again it is asked again,
and that answer shows until the new one lands (`current(days)` is false until
then). `refresh days` reaches only the instance whose action ran: two instances
ask, refresh and answer apart, even with equal arguments. A state initializer
does not read its own component's resources, nor, through a prop, those of a
component around it with no `if`, `match` or `for` between them
(`type-state-reads-request`); a derive or the view does.

A component may own a mutation the same way (LLP 1035.005.004 D6). Each
instance sends it on its own, with its own `pending`, failure, queue and answer,
and its `refreshes` names the component's own resources. Its `then` runs in the
instance that sent it and may call an action prop, which is how the answer
reaches the root: Shelf's `BookScreen` owns `mutation quoted … then detected`,
and `detected` calls `showQuote(lines)`, passed as `showQuote=showQuote(e.params.id)`.
Leaving the screen forgets what it sent: a reply after that changes nothing, its
`then` does not run, and a queued send not yet asked is dropped.

A component that owns a resource, mutation or task is used where its instances are
bounded: outside every region, under `if` and `match`, or in a row of
`stack(nav)`, `t.stack` or `nav.tabs`. Under any other `for`, or in a virtualized
`list`, it is refused (`type-child-resource`): declare the root's resource over
the list (`resource feeds = load(id) for id in ids key=id`) and pass each row its
answer, or declare the mutation in the root and pass each row an action that
sends it (`add=addWork`). Only the root declares a resource over a list, so a
screen that needs one request per element takes the root's list resource as a
prop.

A child's task runs in its instance's frame and ends
when that instance is destroyed. A hidden instance stays alive; gate its task
with `when active` if it should stop while hidden. Data two features read
belongs to the root, which passes it down; name its declarations with the
feature's prefix (`historyDays`), since the root and its members files share one
set of names.

### Tabular numbers

Use `font-variant-numeric="tabular-nums"` for **numeric columns** and **values that
change in place**: prices in a ledger, timetable times, timers, scores, counters,
progress percentages and recalculated totals.

Use default spacing for prose, ordinary labels, static standalone statistics,
and fields merely accepting numbers. Choose by how the value is used, regardless
of its size or currency symbol: static columns need alignment, and updating
headlines need stable digits.

```contract
component App
  state balance = "$1,234.50"
  view
    text balance font-variant-numeric="tabular-nums"
```

Apply it to numeric text, keeping the platform font. It makes digits equal-width
without monospacing letters. Font support is required; unsupported fonts remain
unchanged. The property inherits. `normal` restores the font's default, which may
already be tabular; Exact does not accept `proportional-nums`.

Equal-width digits do not fix the whole value's width: `99` → `100`, signs,
precision, separators and units can still move content. Reserve space for the
formatted range when layout must stay still. Align numeric columns to a common
edge (usually right for left-to-right numbers), with consistent units and precision
where appropriate. Digits alone do not align decimals; do not pad with spaces or
misleading zeroes.

Supported for ordinary text on macOS, iOS, tvOS, Linux, Windows, Android and both
web targets, including prerendered pages, and for Android's native `input` and
`textarea`. Exceptions:

- **Terminal:** ordinary digits already occupy equal-width cells. Explicit
  `font-variant-numeric` settings are currently rejected.
- **Native buttons:** unsupported. Keep their typography; use separate `text`
  for a readout. This property also does not style Canvas/GPU drawing or native hatches.
- **Text wrapping around shapes on web:** a paragraph using `tabular-nums` skips
  Exact's wrapping along image or shape outlines and uses ordinary browser layout.
  Keep readouts separate from prose that needs this wrapping.

`exact:content` components set their own typography: `SummaryCard` uses tabular
digits for `value` and `detail`. Use `text` for a static statistic needing default
spacing.

Check the actual font, locale and host, including fallback digits, `11` → `88`,
`99` → `100` and large accessibility text. Widths can differ across hosts. Keep
locale-aware formatting and accessible labels. See
[a numeric readout](recipes/numeric-readout.md) for sizing.

### Content components: `exact:content`

The platform answers the controls and the chrome; the app's own content is the
author's to design (LLP 1116.000 D3): a summary at the top, bars where numbers
compare, rows with a hierarchy, an empty state that says what to do.
`exact:content` is a built-in library of nine components that draw it with
nothing but roles (`-exact-secondary-background`, `-exact-secondary-label`,
`AccentColor`, the system hues, the text styles, symbols, the determinate
`progress`), so dark mode, Dynamic Type and the tint come with them:

```contract
use SummaryCard, StatBar, RingProgress, DoneMark, EmptyState, RowText from "exact:content"

shape Day
  key: string
  name: string
  ml: number

fn ml(n: number): string = `${formatNumber(n, "decimal")} ml`

component Water
  state total = 750
  state days = [Day(key="mon", name="Mon", ml=1800), Day(key="tue", name="Tue", ml=2250)]
  action drink
    total = total + 250
  view
    scroll height="100%" accent-color="-exact-system-teal"
      column gap=16 padding=16
        RingProgress(label="of 2,000 ml", value=ml(total), fraction=total / 2000, tint=none)
        SummaryCard(label="Today", value=ml(total), detail="of 2,000 ml", progress=some(total / 2000), symbol=some("symbol:activity"))
        for d in days key=d.key
          StatBar(label=d.name, value=ml(d.ml), fraction=d.ml / 2000, tint=none)
        row justify-content="space-between"
          for d in days key=d.key
            DoneMark(caption=d.name, done=d.ml >= 2000)
        if total == 0
          EmptyState(symbol="symbol:calendar", title="No water yet", message="Each glass you log adds to today.")
            button "Log a glass" press=drink buttonStyle="bordered-prominent"
        for d in days key=d.key
          RowText(title=d.name, detail="", value=some(ml(d.ml)))
```

| Component | Props | Draws |
| --- | --- | --- |
| `SummaryCard` | `label: string`, `value: string`, `detail: string`, `progress: option<number>`, `symbol: option<string>` | a card: the symbol and label, the value large in tabular figures, the detail beside it where the card is wide and under it where it is narrow, the fraction as `progress` |
| `StatBar` | `label: string`, `value: string`, `fraction: number`, `tint: option<string>` | the label and value (the value under the label where both do not fit) over a bar `fraction` (held to 0–1) of the width: the accent short of the goal, the system green at 1 or past it, `tint` (a colour role) when given; one `meter` to a reader |
| `RingProgress` | `label: string`, `value: string`, `fraction: number`, `tint: option<string>` | a ring filled `fraction` of the way round from the top, in the bar's colours over a grey track, the value large in its centre and the label under it, at the default body's sizes at every text size; 200 across, down to 120 in a narrower box; one `progressbar` to a reader |
| `DoneMark` | `caption: string`, `done: bool` | a filled check in the system green, or an empty circle in grey, with `caption` (short: a weekday; `""` for none) under it; one image to a reader, named by its caption and state ("Mon, done") |
| `EmptyState` | `symbol: string`, `title: string`, `message: string`; a slot | a centred symbol (none at the accessibility text sizes), the title as a heading, the message; the nodes indented under the use (your `button`) under them; a `scroll` where it is taller than its space (in a flex column, give the column `min-height=0`) |
| `RowText` | `title: string`, `detail: string`, `value: option<string>` | a title, a secondary line under it (`""` for none) and a trailing value in tabular figures (under them where they do not fit on one line): a grouped list `row`'s content, or any list's |
| `Badge` | `label: string`, `tint: option<string>` | a status in a word ("Full", "Cancelled") in a capsule, the caption style semibold, keeping its own size beside a title; `none` the neutral grey, `some("AccentColor")`, or a system hue under white ink (dark on the light hues: green, mint, teal, cyan, yellow, orange); Windows contrast themes use the system highlight/text pair for these tints; read as its text |
| `RoundButton` | `label: string`, `tint: option<string>`, `press: action`, `disabled: bool`, `testId: string` | the platform's own button as an 80-point circle (the Clock's Start, Stop, Lap): filled with `tint` (a role: `"AccentColor"`, `"-exact-system-red"`), ink the host picks for contrast; a light hue (green, mint, teal, cyan, yellow, orange) is the platform's tinted style, the hue on the hue at low opacity; `none` is the neutral grey with the label's ink; the title stays the default body size at every text size |
| `AnswerTile` | `label: string`, `status: string`, `press: action`, `testId: string` | a full-width tile for one answer, the label in the headline style; `status` `"idle"` (a neutral fill), `"selected"` (the accent's outline and checkmark), `"correct"` (the system green, dark ink, a check), `"wrong"` (the system red, white ink, a cross), `"dimmed"` (the rest, once told), or `"chosen"` (the person's own pick once told or closed: the selected look, disabled, so a closed poll still marks the vote while the rest dim); told, dimmed and chosen tiles are disabled; as a grouped list's row it is the list's cell on every host (`-exact-row-look`), the checkmark its accessory; a reader hears "Quito, correct" |

- Props have no defaults: pass `none` for an option. A symbol is an `image` source:
  a role (`"symbol:calendar"`) draws on every host, `"symbol:sf/<name>"` on Apple only.
- They are in the app's accent: `accent-color` on the root (or any ancestor) tints
  them as it tints the controls, on the web and iOS (macOS keeps the system accent,
  as AppKit does). A state is a second colour: progress
  short of its goal is the accent, a goal met (`fraction` 1 or more, a card's
  `progress` at 1) the system green, done against not done green against grey. A
  `tint` overrides it: a budget gone over passes `some("-exact-system-red")`, a
  breakdown whose bars are shares rather than goals passes `some("AccentColor")`.
- They draw no chrome, and two controls: `RoundButton` and `AnswerTile`, the
  emphasis a timer's or a quiz's screen needs, which paint on a `button` would
  make your own box. Any other action is your own `button`, beside a card or
  indented under `EmptyState`. Inset-grouped lists are for settings and forms; a
  content screen is a `scroll` of these.
- Each fills its container's width; arrange them with CSS. In a `flex-wrap="wrap"`
  row, `flex="1 1 320px"` puts a card beside the bars on a wide window and stacks
  them on a phone.
- The source is `contract/lib/content.contract`. To change one, copy it into the app.

## Data requests and side effects

Import a module's shared sources and sends from its interface package. A
platform's own names and types come from its implementation in both Contract
and TypeScript. Tracking consent, for example, is `@exact/tracking-consent/apple`:
elsewhere its status reads denied and unavailable, and its requests fail
`NotSupportedError` (LLP 1133.028 D12–D13).

An implementation import must name the implementation bound on the targets it
covers. An implicit replacement fails the bake; an explicit selection in
`app.json` makes the imported name use its declared default or refusal there.
The agent identifies an implementation's own name as
`<extension>.<name>#<import-path>` in `state.extensions` and `prefer extension`;
shared names keep `<extension>.<name>`.

`@exact/language-model` tools name functions in the data module's `tools`
export, as its own properties: a class's methods are its prototype's and are
not found. A handler receives the input object, `Store`, and `Storage`. Baking
checks literal declarations it can resolve in Contract and the data module's
runtime dependencies against that export and each declared input schema
(`language-model-tool-handler`). Type-only dependencies introduce no tools.
For Contract declarations, annotate `export const tools: LanguageModelTools`
with the generated type from `./app.contract.d.ts` to infer each handler's input.
Computed declarations, including objects that are mutated or escape to other
code, and a `tools` value whose type has only an index signature keep their
runtime checks.

Choose the mechanism from its lifetime:

| Requirement | Mechanism |
| --- | --- |
| Value follows query arguments | `resource result = source(args) as shape T` |
| One HTTP request read into a shape (a `GET`, a GraphQL query), with no data module | a `source` declared in `app.contract` ([Declared sources](#declared-sources)) |
| Source needs changing context while the same kept answer is suitable at boot | `resource result = source(identity) with context as shape T` |
| Ask nothing until a condition holds (nothing selected, a query too short, a page not open) | `resource result = source(args) when cond as shape T`: idle while `cond` is false, asked again, forced, when it turns true |
| Explicit command with a reply | `mutation reply as shape T`, then `send reply = source(args)` |
| Re-request current resource arguments | `refresh result` |
| Refresh reads around a mutation | `mutation … refreshes resourceA, resourceB` |
| Show a write before its reply (optimistic) | `refreshes` on the mutation, and the source's `overlay` |
| React once to a settled mutation | `mutation … then actionName` |
| Writes that must all land, in order | `mutation … queue`: one in flight, later sends wait their turn |
| Writes a person may repeat before the first answers: a rename, a save | `mutation … parallel`: every send asked at once, each reply lands |
| Run once after a delay, while a condition holds (a toast, a debounce) | `task … when cond` with `after(ms, action)` |
| Repeat while a condition holds (a game tick, a pulse) | `task … when cond` with `every(ms, action)` |
| Act when a sheet or route opens (scroll the chosen item into view, load its draft) | In the screen component: `task reveal when active key=id` with `change(reveal)`; pass its activity and entry id as props |
| Pending indicator | `pending(resourceOrMutationName)` |
| "Loading" for a new question, not for a refresh | `not current(resourceName)`: false while it shows its placeholder, an answer to other call arguments, or a value kept across `when`'s idleness; a refresh of the same arguments leaves it true |
| Resource request failed without an answer | `failed(resourceOrMutationName)` |
| Why it failed, to branch on or show | `failure(resourceOrMutationName)`: `none`, or `some({ code, message, reason })`, `code` a domain refusal code when supplied, otherwise one of `offline`, `timeout`, `refused`, `shape`, `storage`, `error`; `reason` a Snapback refusal's code (`SLOT_FULL`), else `""` |
| Initial resource fallback | `else empty(field=constant)`, or `else source(values)` answered once at build |

A timer's step that reloads a resource runs the data module before the next frame, and
the frame waits for it: a module that rebuilds every row each second drops a frame of a
scrolling list each second, and `verify ios` says when the data module is in a late
frame's time. Rebuild only what the tick changed (keep last tick's rows, remake those
whose values moved), and leave out of the rows what the list does not show: a detail
screen's chart points belong to a resource the detail reads.

Resources read as their declared type. Mutations read as `option<T>` and start at
`none`, so a mutation's `T` is not itself an option. Do not treat a resource as an optional wrapper unless its declared type
itself is optional. A mutation reply is unwrapped with match.

An answer is held to its shape exactly: a field the shape does not declare, at any
depth, fails the resource as a thrown error does (TypeScript lets a spread such as
`{ ...row }` through). The view keeps its placeholder or last value and
`failed(x)` is true, so a banner on `failed` reads "unreachable" for what is a
shape mismatch: `state` names the field under `failed`, a CLI drive says so on
stderr, and a failing `expect` names it. Project a backend row onto the shape
field by field (`({ id: row.id, title: row.title })`).

`failure(x)` says why, for the view to branch on: `none` while `x` has not
failed, else `some` of a `Failure`, whose `code` is the nonempty domain `reason`, or the infrastructure class the same on every host (`docs/contract-grammar.md`), whose `message`
is the text `state.failed` shows (for a developer; it differs by host), and
whose `reason` is the string the source's error carried as `reason` (a
Snapback refusal's code), else `""`:

```contract
shape Item
  id: string

component Items
  resource items = loadItems() as shape Item[] else empty()
  derive banner = match failure(items) {
    case some(f) => f.code == "offline" ? "You're offline" : f.code == "shape" ? "This app needs an update" : "Couldn't load items",
    case none => ""
  }
  view
    column
      if banner != ""
        text banner testId="banner"
```

`offline`, `timeout`, `refused` and `storage` reach the view only when the
TypeScript module lets the `fetch` or storage rejection through (rethrows it, or
does not catch it): an error it makes of its own, one for an HTTP 500 among
them, is `error`. Its own error keeps its `message`, and `failure(x).message`
is that text. An error with a string `reason` property keeps that too, as
`failure(x).reason`: the snapback4 driver's `refused` throws a server's
refusal with its code there, so a query's `require … else NOT_MANAGER` is
`f.code == "NOT_MANAGER"` in the view. Mutations expose their latest admitted request failure too; a new send, assignment or success clears it. `bun scripts/agent.mjs web "fail fetch https://api…"` fails
those fetches as a lost connection, so the banner says "You're offline".

`with` takes one or more expressions, before `as shape`, and appends them to the
source's arguments. All arguments still trigger re-asks and identify live
requests. Only an eligible persisted answer admitted while the source is unready
at boot matches the call arguments alone; it can then stand while activation is
pending. Without `with`, every argument identifies that kept answer. Keep
account/tenant IDs and representation choices in the call, credentials below
the seam, and use `refresh`/`refreshes` for counters whose only purpose is another
ask. The default web JS target keeps no persisted resource answers
(LLP 1027.005).

Resource inputs are not asynchronous prerequisites, and HTTP request lanes do
not make resources depend on one another. A resource that reads another resource
uses its current displayed value; it does not wait for the upstream request to
succeed. While that request is pending, the value may be a retained answer, and
`not pending(upstream)` does not prove the latest request succeeded.

An authenticated source must await the app's shared current-session helper
before issuing its protected request. It must verify that the session authorizes
the principal named by the resource call. The helper and auth generation are app
code, not Exact APIs. The app must advance its auth generation and clear its
authorized principal whenever the principal changes or signs out. A request
that must belong to that auth attempt should take the generation as a resource
argument and return the principal and generation as fields in its answer. The
view should show private data only when both fields match the currently
authorized principal and generation. Checking only the owner cannot distinguish
signing into A, then B, then A again. A pending session resource, or one that is
merely not pending, does not prove authorization.

Before committing a refreshed session or shared owner-sensitive cache write,
recheck the generation at the commit point, including inside queued work. When
possible, partition persistent caches by owner so one owner's valid result can
still populate its cache after the view changes. If writes share a slot, guard
the commit by owner and auth generation. Do not suppress a valid per-owner cache
write only because another request became current. See the
[fetch/loading/error recipe](recipes/fetch-loading-error.md) for retained-answer
and owner-gating patterns. The [REST starter](patterns/rest/app.contract) uses
public development identities and does not implement an authenticated session
lifecycle.

The current request owns its answer; older replies cannot overwrite a newer
request. A newer send or an assignment drops a newest-wins mutation's in-flight
reply. It does not undo storage operations the source has already started; those
operations and any serialized storage chain behind them run to completion. Other
source work, such as a fetch, may be aborted when an answer is replaced (see the
[runtime reference](reference.md#what-a-data-module-can-use)).

The action sending a newest-wins mutation (neither `queue` nor `parallel`) must
not also assign its slot to `none`. Assignments commit after sends, so the reset
can discard the new reply even when it appears first. Keep the mutation for its
reply and hide an old error while `pending(added)`, as the [after-a-write
recipe](recipes/after-a-write.md) does. `none` means the slot contains no reply,
not that the write failed: use `failure(added)` for a request failure, and match
a shaped reply's `ok` or error for a domain refusal.

An action that sends one mutation twice on one path is refused
(`analyze-send-twice`) because the second send would forget the first reply, counting
the sends of the actions it calls. Every `if` is read as one that can run: two
sends are separate paths only as arms of one `if … else if … else` or `match`,
or in `if`s testing one unchanged name against different literals. A `match`
binder and an outer name it shadows are different values; testing them against
different literals does not make the paths separate. Send one
combined request, use a mutation per request, or declare the mutation `queue`
(LLP 1092) or `parallel` (LLP 1161.000). A
`queue` mutation (`mutation wrote as shape Ack queue then afterWrote`) keeps one
request in flight; every later send waits, in order, with the arguments it was
made with, and is asked after the reply before it and that reply's `then`, so
`then` runs once per reply, in send order. `pending(m)` is true while a send is
in flight or waits: send while it is pending, since `not pending(m)` means the
spinner is off, not that a send may be skipped. Assigning a queue's slot forgets
nothing — every reply still lands over it — so a mutation that must drop a late
reply (a session's sign-in) does not declare `queue`. At most 64 sends wait; the
65th refuses its action.

A `parallel` mutation (`mutation renamed as shape Ack parallel refreshes issues`)
asks every send when it is made, each its own request, and lands every reply in
whatever order they come: two renames, two saves of one form, two comments. A
reply lands in the slot unless a later send has already concluded, so a late
reply never overwrites a newer outcome; `failure(m)` shows a send's failure
unless a later send has already concluded, and an earlier send's success leaves
a later one's failure standing. `pending(m)` is true while any send is out, an
assignment forgets no request, at most 64 sends are in flight (the 65th refuses
its action), and it takes no `then` (`type-parallel-then`), since its replies
land in any order. Its source is called once per send, with several calls
outstanding: one that reads local storage, changes it and writes it back must
order its own work, or the mutation should be `queue`. Keep the default,
newest-wins, for a reply that must be droppable, such as a sign-in.

`refreshes` asks its resources again when the reply lands; a mutation the source
answers synchronously (without a request) has landed, so its resources are asked
in the sending commit. A send still in flight does not ask them; what it writes
shows before its reply only through the data module's `overlay`, if it exports
one (below). `then` is parameterless, runs once at the host's next clock advance
as a new commit (under the driver, an input's own answer's `then` before the
input's reply), reads the latest answer, does not run for a failure that brought
no answer, and cannot send its own mutation; to repeat, use a task (see
"Repeating while a condition holds"). Do not mistake the scheduling boundary for
a general async workflow or a per-reply event log.

A long poll (`/changes?since=…&wait=…`) is a stream too: add
`exactLongPoll: { cursor: 'since', from: 'seq' }` beside `exactStream`, and the
host asks again with each reply's cursor, skipping heartbeats and backing off
on failures (`docs/reference.md`, LLP 1016.000 D8). Re-read on each event with
`resource view = load() with live.seq`.

A bidirectional WebSocket uses a resource's TypeScript
`fetch(url, { exactStream(event, socket) { … } })` (LLP 1016.000 D7).
Keep the handle in module state by resource identity; after its `open` event,
a mutation source can call `socket.send(textOrBytes)` or `socket.close(code,
reason)`. The mapper may also send and close on its own socket while it
answers `open` or a `message` (a subscribe, an Ack per frame), with no
`fetch`. A mapper may be `async` (inflating a frame with `DecompressionStream`,
splitting a large parse with `await scheduler.yield()`): later frames wait for
it, in order; after its first `await` it sends as an answer's turn does
natively and cannot send on the web. Sends run after the commit that settles
the mutation or the frame stands, in order, and have bounded admission. A mutation's source that
sends answers now or with a promise, and cannot also `fetch` (whichever
comes second refuses it). A resource's answer may send its own
request on its module's socket and await the reply, with no timer, and so
may a mutation whose source answers with a promise: what each turn of it
sends goes out once the commit that ran that turn stands, and stays sent if
it is let go (on the web build a later turn has no commit of its own: what it
sends goes as the turn ends, once the commit that asked the answer stands).
Refreshing or removing the
resource invalidates the old handle. **TypeScript on the web JS target,
macOS, iOS and Linux**; the wasm target and Rust sources remain
receive-only. See
[the reference](reference.md#generate-typescript-data-source-types) for bounds and events.

A large response needs a parse that yields. On native hosts each turn of an
answer has 100 ms of its own (CPU) time by default: from its call, or from the `fetch` or
storage step it awaited, until it next awaits one. A turn over that is refused;
a resource keeps its value and is asked again twice before it fails, so a parse
that passes its tests on a 300 KB thread fails on a 1 MB one. For an indivisible
call such as signing, `app.ts` can set one source’s budget with
`export const turnBudgets = { signOrder: 400 }` (100 to 1000 ms).
`await scheduler.yield()` (the web's [Prioritized Task
Scheduling](https://wicg.github.io/scheduling-apis/#dom-scheduler-yield)) ends
the turn, and the answer goes on in a new one with its own 100 ms. Yield every
few thousand items in a parse or tree walk:

```ts
type Node = { id: string; body: string };
type Post = { id: string; text: string };

export async function toPosts(nodes: Node[]): Promise<Post[]> {
  const posts: Post[] = [];
  for (let i = 0; i < nodes.length; i++) {
    if (i % 2000 === 1999) await scheduler.yield();
    posts.push({ id: nodes[i].id, text: nodes[i].body });
  }
  return posts;
}
```

The function that yields is `async`, and so is every caller that awaits it.
On the web the page has no such budget, and a yield lets it paint mid-parse.

### A TypeScript package for the data module

Source capture skips dot directories at every depth in the app and in
`typescript.sources` mounts, including `backend/.elixir_ls/` and `server/.venv/`.
Keep source code outside those directories. Source symlinks remain refused.

A package whose runtime entry is TypeScript source must also provide declaration
files for the type check. This applies to installed packages and linked `file:`
or workspace dependencies, including packages outside the app folder. The checker
stages package manifests and `.d.ts`, `.d.mts` and `.d.cts` files; it does not stage
package source (LLP 1027 D5). The bundler captures runtime code separately.

For example, a package with `index.ts` and `index.d.ts` can export:

```json
{
  "name": "my-data-helper",
  "type": "module",
  "exports": {
    ".": { "types": "./index.d.ts", "default": "./index.ts" }
  }
}
```

Put `types` before `default`. Supply declarations for each imported subpath, and
make their imports resolve to declarations too. Keep declarations in step with
source: generate them with the package's own TypeScript configuration using
`declaration` and `emitDeclarationOnly`, or maintain them by hand for a small API.
A `module outside captured app` refusal naming a package's TypeScript source
means the checker reached source outside its stage; check that entry's declaration
export and its declaration imports. Moving the package into the app does not
supply the missing declarations.

### Declared sources

A source that is one HTTP request read into a shape is declared in the root
file, `app.contract`, and needs no data module (LLP 1162.001). Its lines are
`fetch()`'s ([the recipe](recipes/declared-source.md) has whole examples):

```contract
shape Current
  temperature_2m: number

source weather(latitude: number, longitude: number) as shape Current
  url "https://api.open-meteo.com/v1/forecast?latitude={latitude}&longitude={longitude}&current=temperature_2m"
  pointer "/current"

component App
  resource now = weather(52.52, 13.41) as shape Current
  view
    text `${now.temperature_2m}°`
```

| Line | Meaning |
| --- | --- |
| `url "…"` | Required. `{param}` is the argument as text (a `string`, `number` or `bool`), percent-encoded as RFC 6570 encodes it. The origin is written out before any `{…}`. |
| `method "…"` | `GET` (the default), `POST`, `PUT`, `PATCH` or `DELETE`. |
| `header "Name" "value"` | One header. The value may name a parameter, `{param}`, and a secret the data module keeps, `{secret:name}`; a header the browser sets itself (`Cookie`, `Origin`, …) is refused. |
| `body json` | The parameters as one JSON object by name, `Content-Type: application/json`. |
| `graphql "…"` | A `POST` of `{"query": document, "variables": {param: value, …}}`; no `method` or `body` beside it. |
| `pointer "…"` | Where the answer is in the reply: an RFC 6901 JSON Pointer, `"/data/items"`; the whole reply when it is left out. |

- **The grant is the origin.** A declared source reaches its URL's origin and nothing
  else, on every host, with no `grants` line; an update that declares a new origin is a
  new binary, as a new grant is. A `{secret:name}` needs `secret.keep name` in the data
  module's grants; while the secret is absent nothing is sent and the resource fails
  with `refused`.
- **The answer is read field by field.** A field the shape doesn't declare is left out
  (unlike a data module's answer, which is held to its shape exactly), an absent or
  `null` `option` field is `none`, and anything else missing fails it with `shape`.
  Rename or compute in a `derive`.
- **A failure** is a TypeScript `fetch`'s that is let through: `offline`, `timeout` or
  `refused`, or `error` for a status outside 200–299, a reply that is not JSON, or a
  GraphQL `errors` (its first error's `extensions.code` is `failure(x).reason`).
- **Reads and writes.** A resource's request is a read: it overlaps others and a `GET`
  keeps the GET cache; `refresh` asks past it. A `send` to a declared source with
  another method, or a GraphQL mutation, is a write. Nothing is asked at build: a
  declared resource shows its `else`, or its shape's zero, until the launch's answer.
- **What stays in a data module:** token refresh, pagination across requests, offline
  queues and retries, an optimistic `overlay`, streams and sockets, uploads, form bodies,
  and an origin from `app.json`'s `env`. An app with both keeps `app.ts`, and with it
  the JavaScript engine on native hosts; an app whose sources are all declared has
  neither.

### Optimistic writes: the overlay

To show a write before its reply, the mutation declares `refreshes` and the data
module exports an `overlay`:

```contract
shape Item
  text: string

component List
  resource items = loadItems() as shape Item[]
  mutation added as shape Item refreshes items
  action add(text: string)
    send added = addItem(text)
  view
    column
      for item in items key=item.text
        text item.text
```

```ts
import type { Overlay } from './app.contract.d.ts';
type Item = { text: string };
export const overlay: Overlay = (source, args, answer, writes) => {
  if (source !== 'loadItems') return undefined;    // other resources show their answers
  const shown = [...(answer as Item[])];
  for (const w of writes) {
    if (w.source !== 'addItem') continue;
    const [text] = w.args as [string];
    if (shown.some(i => i.text === text)) continue; // the answer has it
    if (w.answered) w.keep();                       // the server has not caught up yet
    shown.push({ text });
  }
  return shown as never; // `Result<S>` cannot be narrowed from a generic `S`
};
```

From the send, derives and views read `items` as the overlay's value. Without an
overlay they read the answer, and the write shows only once a refreshed answer
has it. The runner calls `overlay` for each resource a write affects, with:

- `source` and `args`: the resource's source name and arguments. Resources can
  share a source with different arguments; check `args` when a write belongs to
  one of them.
- `answer`: the resource's current answer. Copy it rather than changing it.
- `writes`: the sends that affect it, in send order. Each has `id`, `mutation`
  (the mutation's name), `source` and `args` (the send's), `reply` once its reply
  has landed, and `answered`: its reply has landed and `answer` was asked after
  that, so `answer` should include it.

A write shows until the first answer asked after its reply landed. If that
answer does not include it yet (a server that is not read-after-write
consistent), call `keep()` on it to go on showing it. The runner does not ask
again on its own, so a kept write shows until a later answer (from `refresh`,
another write's reply, or new arguments) that the overlay does not keep it over.
Returning `undefined`, throwing, or returning a value outside the resource's
shape shows the answer and stops showing every answered write; the journal says
why for the last two.

A write that ends without landing stops showing in that commit, with no undo
code, and the resource is asked again: its reply fails, its wake expires, or a
newest-wins mutation's send is replaced by a newer send or an assignment to its
slot. If that ask fails, `failed(items)` is true and `items` keeps its last
answer. A `parallel` mutation's writes each show until their own reply and the
answer asked after it, and one's failure takes nothing else with it; the overlay
receives the writes that have landed, in the order they landed, then those still
out, in send order.

- The overlay runs synchronously inside the commit, so it cannot await, and it
  has no effects: `fetch`, storage, the store (`get` included), `native` and
  `crypto` (`crypto.getRandomValues()`, `crypto.randomUUID()` and every
  `crypto.subtle` call) refuse with an error naming the call, the same on every
  host: a call that returns a promise rejects, any other throws. The
  runner reuses its value while its inputs are unchanged, so compute it from
  them alone.
- Writes are not saved: after a relaunch or a reload only answers show, and a
  resource a write was showing in is asked again.
- A source whose module runs on a worker shows answers only, said once in the
  journal.
- A Rust source implements `DataSource::overlay`, returning
  `Ok(Some(Overlaid { value, keep }))`, or `Ok(None)` to show the answer.
- The agent's `state` shows what the overlay shows; `state.writes` lists the
  writes still showing, as `{ id, mutation, landed }`, when there are any.

A failed resource retains its value or placeholder, with `pending=false` and
`failed=true`. Argument changes, refresh, or successful answers clear the failure.
A domain error returned as a record is an answer and must be handled as data.

Infer the actual source interface from the Contract:

```sh
bun exact.mjs contract types app.contract -o app.contract.d.ts
bun exact.mjs contract rust app.contract -o /tmp/shapes.rs
```

Use those generated declarations with the existing TypeScript/Rust integration.
A mixed app imports `TypeScriptSources` and `TypeScriptAnswer` from
`app.contract.d.ts`; its `build_mixed_with` bake excludes Rust-owned providers.
Run `bun exact.mjs contract build app.contract` to generate and check that narrowed
declaration. Standalone `contract types` has no Rust roster; `Sources` always
describes the complete seam. See [the provider types](reference.md#generate-typescript-data-source-types).
A shape has no exported name in the `.d.ts`: name one by its source,
`type Recipe = Result<'recipe'>` (a list's element: `Result<'recipes'>[number]`;
an optional answer is `… | null`, so `NonNullable<Result<'find'>>`).
`app:/data`, `app:/cache` and `app:/tmp` exist on every host before a source
runs, so a file directly in one (`app:/data/notes.json`) needs no `mkdir`; a file
deeper down needs its folder first (`storage.fs.mkdir('app:/data/drafts')`, which
makes the folders above it too), or the write fails with `ENOENT`.
The [human guide's data-module section](contract-for-humans.md#writing-the-data-module)
has a complete `app.ts`: synchronous, `fetch` and SQLite sources, the grants
each needs (one per line: `['sqlite.open app:/data/books.db', 'net.fetch https://…'].join('\n')`;
`net.fetch` takes an `http` or `https` origin, `http://127.0.0.1:8080` too, or a
whole scheme, `net.fetch https:` (any host and port, for a server the person names
at run time; `net.websocket` takes `wss:` or `ws:` the same way); on iOS
cleartext `http` reaches only a local host, and only with `app.json`'s
`host.ios.localNetworking` set, unless ATS is off (below); a device on the local
network the person names or the app finds is `net.fetch local-network` with
`device.local-network purpose.<key>`, whose text is the platform's prompt on macOS
and iOS ([recipe](recipes/local-network.md)); the app's own `http` to any other
host, such as a server the person types in, needs each Apple platform's
`appTransportSecurity` (`host.macos`, `host.ios`) set to
`{ "allowsArbitraryLoads": true }`, which turns ATS off for every connection the
app makes, frames included, and which App Store review asks the app to justify;
an `iframe` of `http://` from a named host, or such a sub-resource of the app's
own `assets/` page, needs `host.macos.appTransportSecurity` or
`host.ios.appTransportSecurity` set to `{ "allowsArbitraryLoadsInWebContent": true }`,
which relaxes web views only (ATS lets an IP address or a `.local` name through
without it, LLP 1020.001 §4 Q6, though on an iPhone those also ask for Local
Network access), and how to drive it with storage. The macOS driver launches the assembled app
bundle, so its ATS policy also applies during drives. A path grant covers its path and what is below
it, by whole names: `sqlite.open app:/data` covers `app:/data/inbox-amy.sqlite`,
`sqlite.open app:/data/inbox` does not. A refused open names the file it wanted
and the grant line that would admit it.
A line of `app.ts`'s grants that begins with the word `development` is a development
build's only: a production bake (`EXACT_UPDATE_TRUST=production`, which an Apple
`--archive` or `--release` and a deploy set) leaves it out, and any other keeps it
without the word (`development device.calls purpose.calls`, Harmony's test call,
LLP 1133.033 §13 question 4). Only the build's copy of `app.ts` changes, so the
module's own `grants` constant is the build's, and the app can hide what the line
serves (Harmony's `testCallBuild` source). A Rust data crate's `grants()` can't mark
a line yet: the bake refuses one (`grant-development`). The bake's own trailing word
on a development build's `local-network` line (LLP 1133.036 D5) is another thing.
A token, a password or a key the module keeps is a secret, not a file: grant
`secret.keep <name>` (one line per name, `secret.keep signal.token`) and use
`store.set(name, value)`, `store.get(name)` (a string, or `null`) and
`store.forget(name)` in an answer (LLP 1018). It is the Keychain on Apple; on the
web, the page's `localStorage`, readable by any script on that origin; on Linux,
unencrypted files in the app's data directory, retained across launches (an app
without an ID keeps them in memory). A drive keeps it, by default,
only in a named `--storage` store (`EXACT_STORE=real` gives an Apple drive, on a Mac, a simulator or a phone, the Keychain a normal launch reads).

**Web refresh and session tokens belong in an HttpOnly, Secure cookie set by the
app's server, which the page never reads.** Follow [web sign-in tokens and
sessions](recipes/web-sessions.md) for SameSite, credentialed fetch, CSRF, rotation
and logout, sharing the flow with native `secret.keep`. Inside an answer,
`exactHost()` returns `'web'` or `'native'` at run time; no per-build transport
setting is needed. This string store does not protect a web token from
injected scripts. See the browser's [cookie attributes](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie).

A secret that only the person in front of the device may read (an account's token
behind Face ID) carries a policy on its line, `secret.keep token user-presence`
(`biometry-any`, `biometry-current-set`, `device-credential`), then `optional` when
the person chooses whether it is locked; the app grants `device.local-auth <purposeKey>`
and depends on `@exact/local-auth` (LLP 1133.010). Its value is never in the store:
`store.get` throws `InvalidAccessError` while it is locked, `await store.unlock(name,
{ reason })` shows the platform's sheet and answers the value, `store.has` and
`store.locked` say what is kept. `await store.protect(name)` turns an `optional` lock
on without a prompt on iOS, iPadOS, Android, and macOS builds provisioned for the
data-protection Keychain. It rejects without a prompt when locked secrets are unavailable.
For a switch that asks for confirmation, call `authenticate(reason=)` from
`@exact/local-auth` first and wait for it to succeed before calling `store.protect`.
The store's presence policy controls access to the secret.
`await store.unprotect(name, { reason })` shows the platform's sheet before turning
the lock off. `store.set` keeps every write in the name's state, a rotated token's too.
Keep the value out of every answer: an answer is kept and shown. The web keeps no
locked secret yet: a name that must be locked is refused there, and
`localAuthStatus().presenceSecrets` is false. Under the agent the sheet is a
`presence` ticket (`tap @presence allow`, `cancel`, `fail`,
`lockout`, `invalidated`), after Face ID's `permission` ticket while it is `prompt`.
The compiler accepting a source call does not provide its implementation. Check
its arguments, declared result, grants, storage access, and bake-time behavior.
Keep generated output out of version control. Use app-local sources for domain
formatting or algorithms beyond the finite standard roster.

A bake runs initial data work and packages first-frame values. A first-frame
value is not the answer: every host asks the TypeScript module again at launch,
natively once it loads after first pixel, even a source with no arguments
(`logs`: `<resource> shows its build-time answer until its source answers`, then
`<resource> answered: …`). A resource with an argument (one after `with` too) made
from `exactTime()`, through a derive, another resource or a state an action
assigns it (or writes under a condition on the date), gets no first-frame value, since the bake's date is unknown: it shows
its placeholder until the module answers with the launch's date, so a task waiting
on `lib.loaded` never adopts an answer stamped at 1970. Don't pass the clock into a
source whose answer doesn't depend on it, or its first frame waits for the module:
format times in Contract (`formatTime(at, time.utcOffset, "short")`). A `send` an action makes before the module loads (a
document opened at launch) waits for it, `pending` meanwhile (`logs`: `send <name>:
waits until the data source is ready`). Live requests run after that under host scheduling. Do not assume a secret store, disk database,
or authenticated network session is available while baking. See
[the data-module reference](reference.md#generate-typescript-data-source-types).

### Sign-in providers (`@exact/oauth`)

The `oauth` recipe is the code; this is each provider's configuration, as its
documentation requires (LLP 1146; the IDs are `packages/oauth/test/PROVIDERS.md`'s).

| Provider | Configuration |
|---|---|
| Google | `issuer: 'https://accounts.google.com'`, `issuerAliases: ['accounts.google.com']` (GOOG-2), `authorizationParams: { access_type: 'offline', prompt: 'consent' }` for a refresh token (GOOG-8). Grant `net.fetch https://oauth2.googleapis.com`, its token endpoint's origin (GOOG-1). An iOS client's callback is its reversed client ID, `com.googleusercontent.apps.<id>:/oauth2redirect` (GOOG-5); a desktop or web client needs its `clientSecret`, which is not secret (GOOG-4) |
| Microsoft Entra ID | `issuer: 'https://login.microsoftonline.com/common/v2.0'`, `issuerTemplate: true` (ENTRA-1), `offline_access` in `scopes`. Register the callback as "Mobile and desktop"; send no `clientSecret` (ENTRA-3). A web build's callback is registered as "Single-page application" (ENTRA-4) |
| Auth0 | `issuer: 'https://<tenant>.auth0.com/'`, the tenant's domain with a trailing slash (AUTH0-1); `offline_access`, with "Allow Offline Access" on the API (AUTH0-3); `authorizationParams: { audience }` for an API's token. With rotation's overlap period on, `retryLostRefresh: true` |
| Okta | `issuer`: the authorization server's issuer, `https://<org>.okta.com/oauth2/default` (OKTA-1); `offline_access` (OKTA-4); `retryLostRefresh: true` while refresh tokens rotate with a grace period (OKTA-6) |
| OneLogin | `issuer: 'https://<subdomain>.onelogin.com/oidc/2'` (ONELOGIN-1); no `offline_access`, which it refuses (ONELOGIN-2); `endedErrors: [{ error: 'invalid_request', status: 401 }]` (ONELOGIN-4) |
| GitHub | `endpoints: { authorization: 'https://github.com/login/oauth/authorize', token: 'https://github.com/login/oauth/access_token' }`, no `openid` (GH-1), `clientSecret` (GH-7), `commaScopes: true` (GH-5), `endedErrors: ['bad_refresh_token']` (GH-4). Its token endpoint does not answer a browser, so a web build needs a server (GH-8) |
| Salesforce | `issuer: 'https://login.salesforce.com'`, the `refresh_token` scope (SF-14); it sends no `expires_in`, so the expiry is learned from a 401 (SF-1) |
| Amazon Cognito | the endpoints are on the user pool's domain, not the issuer's: grant both origins (COGNITO-1) |
| ForgeRock (PingAM) | `issuer`: the realm's, `…/oauth2/realms/root/realms/<realm>`, as its discovery document names it. An app with its own sign-in screens answers the authentication tree (FR-1), then passes `signIn` an `authorize` that POSTs the authorization request with the SSO token (FR-2; below), natively only. A gateway that answers 403 for an expired access token: `expiredStatus: [401, 403]` (FR-4) |
| Sign in with Apple | not supported: its token exchange needs a server-signed client secret |

`signOut` forgets and revokes the session, but the browser keeps the
provider's own sign-in, so the next sign-in returns as the same account
without showing a page. To let the person choose another account,
`signOut(store, { endSession: true })` also opens the provider's end-session
endpoint (OpenID Connect RP-Initiated Logout 1.0): send it from a press,
register the callback as a post-logout redirect URI, and set
`logoutRedirectParam` for a provider that does not call that parameter
`post_logout_redirect_uri`. A browser step that does not finish still signs
out, answered `ok` with code `end_session` and a sentence in `error`. A later
end-session press retries that provider logout. The web reserves a blank
provider window synchronously at the press, then navigates it after sign-out
takes the session under its store lease and constructs the logout URL. Its
ID-token hint and revocation therefore always name the same login without
losing the press's browser activation. Until the provider logout succeeds or
a new sign-in replaces it, the store retains its endpoint and ID token under
the same secret lease; the app remains signed out. Entra and Cognito may
return from logout without `state`. A successful logout return carries no
`code`, `error`, `id_token` or `access_token` in its query or fragment. Its
state, when present, must occur once and match the request.
On Android and in the runner, a matching state addresses the return; a
sign-in takes a sign-in answer before a logout sharing its state. A return
without state and without a sign-in answer goes to the oldest logout at that
callback, whoever else waits; one without state that carries a sign-in answer
goes to nobody. On the web, the
logout's own popup addresses its return through its opener. Before navigating,
the glue also puts a fresh random identity in that popup's `sessionStorage`;
the callback reads and removes it. After severance, that identity addresses
the return to the live logout holding it. An addressed logout return carrying
a sign-in answer fails immediately with 502. An unaddressed broadcast carrying
a sign-in answer completes no logout, even if its state matches. No tab takes
a shared lock or reads another tab's state.
A request still held at the ten-minute bound answers 499 once, whether its
timer or a late return arrives first.

Revocation runs inside the same lease that removes the credential, before the
provider browser opens. Each token request has a 60-second host deadline, so
sign-in, refresh and another sign-out for the same key wait behind it. No reply,
a timeout, 429 or 5xx leaves the token for the next sign-out in the same data
module runtime; any other reply clears it. This is best effort: there is no
background retry and the debt is not stored in the secret, so a reload, restart
or runtime replacement ends those retries. Another browser tab may revoke the
same token again; RFC 7009 treats that as success. Recovery from an interrupted
secret update remains fail-closed and removes the whole secret, which can also
discard a retained provider-logout hint.

Every option, with what it does, is in `@exact/oauth`'s `index.d.ts`.

**Tokens outside HTTP.**

Use `provider.accessToken(store)` in data code when the intended API takes
its access token somewhere `provider.fetch` cannot reach: a third-party SDK,
or a message after a WebSocket opens, on a host whose mapper receives a
socket handle. Prefer `provider.fetch` for HTTP requests. An OpenID Connect
service such as Convex instead takes `provider.idToken(store)`, with that
service configured to trust the issuer and audience. An ID token is not an
API access token.

Like `idToken`, `accessToken` synchronously answers what the store holds now,
or `null` when signed out. It reads no clock and makes no refresh of its own.
When the service reports expiry (a socket's authentication failure or an SDK's
error), await `provider.refresh(store)`, check its `ok`, and ask for the token
again. This uses the same refresh as `provider.fetch` after an API refusal;
concurrent callers refreshing the same held token make one refresh. A failed
refresh that keeps the session leaves the old token; one that ends it leaves
`null`. Handle the failure before attempting authentication again.

Both tokens are credentials. Keep them in data code; never return them in
a Contract answer, put them in state, or log them. Answers are kept and
shown by the agent. The return types warn about this in their documentation;
they are ordinary strings, with no compiler warning or runtime filter that
prevents a credential from entering an answer (LLP 1018 D5, LLP 1157 D7).

With a token in hand, the caller owns where and how it is sent:

- Send it only to the API it was issued for, over an encrypted connection.
  `provider.fetch` checks the destination against `apis`; the getter cannot
  check what the caller or SDK does with the returned string.
- Never put it in a URL. [RFC 9700 §4.3.2](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.3.2)
  prohibits passing access tokens by the URI query method described in
  [RFC 6750 §2.3](https://www.rfc-editor.org/rfc/rfc6750.html#section-2.3).
- Ask for the token again when authenticating or reconnecting. Do not hold
  a copy across a refresh or sign-out. The caller handles the service's
  authentication failure and reconnect protocol. `provider.fetch` still refuses
  `ws:` and `wss:`.

If the API requires a credential on the handshake URL, it refuses an
unauthenticated upgrade before `open`; a first message cannot authenticate it.
Use the ticket exchange:
call `provider.fetch(store, url, { method: "POST" })` over HTTPS for a short-lived,
single-use ticket, then open
`fetch('wss://…?ticket=…', { exactStream })` synchronously in a fresh source call.
Keep the ticket in data code and obtain a new one for each reconnect. See the
[socket-ticket recipe](recipes/socket-ticket.md).

The following excerpt is only for an API that accepts the upgrade without a
credential, then accepts an `authenticate` message and reports expiry with
`token_expired`. It needs a mapper with a usable socket handle: worker-placed
TypeScript sources and the web's wasm target do not support this send path
(see `docs/reference.md`, socket sending). The app has checked that
`wss://api.example.com/live` is the intended API and granted `net.websocket`
for it. Add these sources to the app's `Sources` object:

```ts
live: (_args, store) => {
  let expired = false;
  return fetch('wss://api.example.com/live', {
    exactStream(event, socket) {
      if (event.type === 'open') {
        const token = provider.accessToken(store);
        if (token) socket.send(JSON.stringify({ type: 'authenticate', token }));
      }
      if (event.type === 'message' && event.data === 'token_expired') expired = true;
      return { expired };
    },
  });
},
renew: async (_args, store) => await provider.refresh(store),
```

When `live.expired` becomes true, the app sends the `renew` mutation. After
its successful answer, it refreshes the `live` resource to reconnect; the
next `open` asks for the token again and sends it as the first message.
The token never enters the URL, subprotocols or a Contract answer. Adapt the
message and refusal handling to the service; do not retry indefinitely.

**Development identity servers.** `issuer`, every `endpoints` URL and each
`apis` origin require HTTPS, except loopback (`localhost`, `127.0.0.1` or
`[::1]`). A refusal names the option, such as `endpoints.token` or `apis[2]`.
For a phone reaching your Mac over the LAN, serve the identity server and API
through HTTPS with a certificate the phone trusts, or an HTTPS tunnel. A
simulator can use the Mac's loopback server. Development builds keep this
HTTPS rule; they have no insecure-origin switch.

**Parameters chosen at the press.** Keep provider defaults in
`authorizationParams`; pass a login hint, connection or account-choice prompt
with the sign-in itself:

```ts
provider.signIn(store, { params: { login_hint: email, prompt: 'select_account' } });
```

`params` overrides those defaults for this request alone, including when
`authorize` sends it. The package still opens a prepared browser request
before awaiting anything. Both parameter maps refuse `response_type`,
`client_id`, `redirect_uri`, `scope`, `state`, `code_challenge`,
`code_challenge_method`, `nonce` and `max_age` with `TypeError`; the package
sets those fields or cannot validate what they ask for.

With `openid`, `signIn(store, { nonce })` sends an app-chosen, non-empty string
and checks the ID token against that exact value. Use a fresh value for each
sign-in. Omit it to let the package generate one. The option applies only to
that press, for a browser sign-in or `authorize`; raw parameter maps still
refuse `nonce`.

For flows the package does not cover, data code may use `openAuthSession`
and `authCallback` directly; their public declarations and rejection statuses
are in the generated `app.d.ts`. A callback must carry exactly one `state`
across its query and fragment, equal to the nonempty value supplied to
`openAuthSession`; duplicates are refused, including one in each. The full URL
returns to data code unchanged, including fragment parameters such as
`id_token` ([OIDC fragment responses](https://openid.net/specs/openid-connect-core-1_0.html#ImplicitAuthResponse)). Keep callback URLs
in data code: they can contain credentials.

On the web, closing the sign-in popup is not reported as cancellation:
a provider's Cross-Origin-Opener-Policy can make an open popup appear closed.
The session waits for its callback, supersession, or a ten-minute bound (499
`cancelled`). Returning focus to the app does not end it. A new sign-in on the
same request target supersedes the previous one. Offer an enabled Cancel while
sign-in is pending: assigning `signed = none` to the mutation holding the
sign-in releases its request and clears `pending(signed)`, so the person can
retry at once. See `apps/auth-fixture/app.contract`.

**An app's own sign-in screens.** A provider that lets an app sign the person
in with its own screens (ForgeRock's authentication trees) still issues an
authorization code. The app sends the authorization request as the provider
documents and answers where it was redirected; the package checks it and does
the rest. The source that answers the tree's last step keeps its SSO token in
the module and answers only that the tree is done: an answer is kept and
shown, and whoever holds the token can sign in as the person.

```ts
let sso = '';  // the tree's SSO token, set by the source that answers its last step
const sources: Sources = {
  signIn: (_a, store) => {
    const token = sso;
    sso = '';
    return provider.signIn(store, {
      authorize: async ({ endpoint, params, fetch }) => {
        const reply = await fetch(endpoint, { method: 'POST', redirect: 'manual',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded', Cookie: `iPlanetDirectoryPro=${token}` },
          body: new URLSearchParams({ ...params, csrf: token, decision: 'allow' }).toString() });
        return reply.headers.get('Location') ?? '';
      },
    });
  },
};
```

This sign-in runs natively. On the web, the browser answers `redirect:
'manual'` with no status or headers (Fetch's opaque redirect) and drops a
`Cookie` an app sets, so a web build needs a server to send the authorization
request, as GitHub's needs one for its token exchange (GH-8). Grant no
`auth.session` or `auth.callback`, and set `callback` to the redirect URI the
provider registered. Send with the `fetch` that `authorize` is given, the
package's transport, and throw an `OAuthError` to say why sign-in failed,
giving it the failing reply's status. `claims(store)` reads the ID token's
claims, for an API that wants one of them in a header; the `log` option hears
what the session did, with no token in it, for a diagnostics screen; the
`fetch` option carries every request the package makes, for a demo whose
server is in the app.

**Testing against a fake provider.** Name `<exact2>/packages/oauth/test/fixture.ts`
as the app's server in `app.json` (`"commands": {"server": ["bun", "<exact2>/packages/oauth/test/fixture.ts", "--port", "4400"]}`,
`"server": {"database": false}`). Its profiles (`spec`, `google`, `entra`, `auth0`,
`okta`, `onelogin`, `github`, `salesforce`, `forgerock`) behave as those providers do. The
issuer is `http://127.0.0.1:4400/p/<profile>`, except Entra's
`…/p/entra/common/v2.0`, Auth0's `…/p/auth0/` and OneLogin's
`…/p/onelogin/oidc/2`. Google's token endpoint is on the next port, `4401`:
grant it too. GitHub has no discovery: its `endpoints` are `…/p/github/authorize`
and `…/p/github/token`. `GET /p/<profile>/api/me` answers the person.
ForgeRock's tree is `POST /p/forgerock/json/authenticate` (the person is
`ada@example.com`, password `lovelace`), and its API answers 403 for an expired
access token.
`authorize` completes the held sign-in against the fixture;
`authorize cancel` cancels it. `request POST "/control"` changes what the
fixture does next: `{"action": "expire-access", "profile": "spec"}`, and
`revoke`, `fail-token`, `hold-token` (then `{"action": "await", "held": 1}`
returns once the app's token request is held, and `release-token` lets it
go), `deny-authorize`; `fixture.ts`'s `control` lists them all.

To open a fresh test already signed in, configure the app's provider with the
local fixture's `/p/spec` issuer. Give `signed-in` the provider's plain
`secret.keep` name, its `clientId`, and the test person's `sub`. The fixture
completes the code exchange before launch. The first data read, a token refresh
and a relaunch all see the same person. Signing out clears the test's session.

```contract-test
test "opens the reader's profile"
  signed-in "oauth.spec" {"clientId":"client-spec","sub":"reader-2","name":"Grace Hopper"}
  answer fetch "http://127.0.0.1:4400/p/spec/api/me" with 200 {"name":"Offline reader"}
  expect text "status" == "Signed in"
  expect text "name" == "Offline reader"
```

`name` defaults to `sub`, and `email` defaults to the empty string. The
fixture generates a bearer token. Give `token` a test value to choose the
first bearer, for example `"token":"reader-2-test"`. The fixture refuses a
value it already issued until reset. To exercise the sign-in button and the
callback itself, start signed out, tap the button and use `authorize`.

#### HTTPS callbacks

For a backend that accepts only HTTPS return URLs, declare
`auth.callback https://app.example/auth/return` instead of the custom-scheme
callback. Register that URL with the backend. Apple uses
[`ASWebAuthenticationSession.Callback.https`](https://developer.apple.com/documentation/authenticationservices/aswebauthenticationsession/callback/https(host:path:));
it requires iOS 17.4 or macOS 14.4. Android uses an
[Auth Tab](https://developer.chrome.com/docs/android/custom-tabs/guide-auth-tab)
when the selected browser supports it, otherwise a browser returning through
a verified App Link on exactly `/auth/return`. HTTP loopback remains unavailable
on Android.

Serve the domain associations over HTTPS at the origin root, without redirects:

- `/.well-known/apple-app-site-association` needs
  `{"webcredentials":{"apps":["TEAM.com.example.app"]}}`. `TEAM` is the signing
  team and `com.example.app` the bundle id. The bake derives the matching
  `webcredentials:app.example` entitlement from the callback grant. On macOS,
  signing it requires a provisioning profile that allows Associated Domains.
  A development build without one reports the missing association; a release
  build refuses to ship without it.
  Debuggable builds append `?mode=developer` to each derived `webcredentials:`
  entry so Apple fetches the file from the domain directly. Enable Associated
  Domains Development on the test device ([Apple's setup](https://developer.apple.com/documentation/xcode/supporting-associated-domains)).
- `/.well-known/assetlinks.json` needs a `delegate_permission/common.handle_all_urls`
  statement naming the Android package and its signing certificates' SHA-256
  fingerprints. Put the fingerprints in `host.android.certificates` in `app.json`;
  include each signing identity used to install the app, including Play App Signing.

With `app.origin` set to this origin and `host.ios.team` set, the web build writes
these files into `.well-known/`, and deploy publishes them with the web root.
For a callback on another origin, the build prints the files that domain must
serve, merging any passkey and navigation entries into the same documents.
Keep `.well-known/` at the origin root when the site has a base path.
`handle_links` controls ordinary navigation and is not needed for auth callbacks.
On Android 11 and earlier, one unverified navigation or callback host prevents
verification of every App Link; the build names the hosts when more than one is registered.
Reserve `/.exact/auth/callback` for the web popup; use a different path for the
native callback so an installed app does not intercept the web sign-in.
Apple returns `502` with its reason when the app is not associated with the
callback domain. A session that starts has not yet proved callback delivery;
check the return in a signed app with the domain's association file published.

### Web sign-in callback

Grant the deployed site's exact `auth.callback https://app.example/.exact/auth/callback`
and register it with the provider. A loopback development page grants its own
callback automatically; deployment has no such exception.

Serve the generated `dist/.exact/auth/callback` at `/.exact/auth/callback`,
without a router fallback or trailing-slash redirect, and its script at
`/.exact/auth/callback.js`. Include the hidden `.exact/` directory in the upload.
With Wrangler or another static-assets uploader, verify that both URLs return
the generated files after deploying; if the uploader omits them, serve them
explicitly from the Worker. Keep the callback's `Cache-Control: no-store` and
`Referrer-Policy: no-referrer` headers.

### Native sign-in (`@exact/sign-in`)

The [sign-in recipe](recipes/sign-in.md) gives the call and server flow.

On iOS and macOS, an Apple-only request opens AuthenticationServices in the
calling session's window. The answer has an empty `id`, a `handle`,
`configURL`, `isAutoSelected: false`, Apple's `user` and authorized scopes
under `apple`, and `token.authorization` containing `id_token`, `code` and
`state`. First-authorization name and email, when supplied, are in
`token.user`. A dismissal returns `NetworkError`, as the web does; runtime
cancellation returns `AbortError`.
Backgrounding returns `NotAllowedError` for an open or waiting sheet.

Pass the raw server nonce in `params.nonce`. Both the native request and
the web authorization URL carry it unchanged. Apple's
[nonce property](https://developer.apple.com/documentation/authenticationservices/asauthorizationopenidrequest/nonce)
and [ID-token claims](https://developer.apple.com/documentation/signinwithapplejs/authorizationi/id_token)
describe checking the supplied value in the token. Exact does not hash it
on either host. Construction and fixture tests cover this rule; comparing
real Apple tokens from both hosts remains a device/provider qualification.

The `auth.apple` grant derives `com.apple.developer.applesignin`; there is
no separate app.json switch. Enable Sign in with Apple for the app's
Apple Developer App ID and use a provisioning profile that grants it.
The build signs the entitlement on a simulator or when the signing profile
allows it. On macOS, a build without the signed entitlement reports Apple
unavailable and requests return `NotSupportedError`. iOS leaves entitlement
validation to AuthenticationServices; an authorization failure returns `IdentityCredentialError` with Apple's code. A simulator run
does not establish successful sign-in on a signed device.

Pass both `publicKey` and passive `identity` to `get` for one system sheet
offering a passkey and Sign in with Apple. Passkeys owns the controller and
the call's lifetime; sign-in builds the Apple request and decodes its result
(LLP 1133.014 D10). Apple `withPublicKey` follows its entitlement availability.
A newer credential ceremony ends a pending passive request with `AbortError`.
Other credential sheets wait in session order, across passkeys and sign-in.
Active identity requests and `silent` or `conditional` mediation cannot share
the sheet. Google on Apple and `getCredentialState` remain unimplemented.
Native capabilities report Google unavailable; use `@exact/oauth` through
the system browser for Google.

### Convex live queries

The `convex` recipe is the code for `@exact/convex`; this is what it leaves
out (LLP 1157).

- **The deployment's address.** `convex(address, …)` takes its `CONVEX_URL`
  (`https:`) or the same origin as a socket's (`wss:`), so one build-time
  setting can name both it and the `net.websocket` grant.
- **`live.status`.** `connecting`, `open`, `ready` (results have arrived),
  `closed` (the socket ended or never opened: refresh `live`, as the recipe's
  `retry` does) or `failed` (Convex said something this client cannot follow;
  `live.error` says what, and a new connection would meet it again, so nothing
  retries it). A frame over the module's ceiling is one: the first frame of
  a connection carries every query's result, 8 MiB together unless
  `convex(url, functions, { maxMessageBytes })` says another.
- **A quiet socket.** Convex sends a frame every 15 seconds. The module asks
  its socket with `exactIdleTimeout`, so the host ends one that carries nothing
  for a minute (`convex(url, functions, { idleTimeoutMs })` says another), in
  real time, whatever the driver's clock. It ends as `closed`, which the
  recipe's `reconnect` opens again. So does a connection with no results by
  Convex's second ping, about half a minute after it opened, whatever
  `idleTimeoutMs` says: the module cannot tell opening frames that were lost
  from a first answer that late.
- **A query's lifetime.** Its identity is its function and arguments. One a
  screen no longer asks for is dropped at the second change after its last
  ask. One asked while the socket is closed, with nothing delivered for it,
  fails and is asked again when results arrive.
- **Values.** A Contract shape is closed (every field, no others) and holds no
  `bigint` or bytes: map a result with `cx.query`'s fourth argument. A value
  outside its shape fails that resource; the socket goes on.
- **Failures.** A failed query throws `ConvexFailure`: the resource fails and
  keeps what it showed. A mutation or an action rejects with one when Convex
  refuses it (`message` is Convex's, `data` a `ConvexError`'s payload), when
  the socket is not open (nothing was sent), and when the socket closes before
  the answer. That last one may have run, and the module sends nothing again
  on the next connection by itself. When
  Convex refuses a token it closes the socket. A request sent behind the
  refused `setAuth` rejects with Convex's words, as `live.authError` has them:
  it did not run. When a token expires, Convex refuses it at whichever request
  meets it first, and one sent earlier (an action still running) may have run,
  so each rejects with "Convex refused the token before it answered (…); the
  mutation may have run".
- **Sending a mutation again.** The failure of a mutation whose connection
  ended before Convex answered carries `again`, a handle (none if you signed
  out after sending it). Once `live.status`
  is `ready` again, `cx.again(handle)` in a mutation's source sends the same
  request: Convex answers one it has seen with its first answer, so the write
  happens once, and runs one it never saw as the person signed in now. Keep
  the handle in state (`catch (e) { return { ok: false, again: e.again ?? -1 } }`)
  and offer the retry. A handle is good once (a mutation sent again and left
  unanswered again fails with the next), the module keeps the last hundred,
  and `cx.setAuth(undefined)` forgets them all, so a mutation is never sent
  again as the next person. An action has no handle: one that must not repeat
  carries its own key.
- **An action's reply** arrives as soon as Convex has it, and the queries may
  show what it wrote a moment later: a mutation's reply waits for them, an
  action's does not. A view that reads both can show them out of step.
- **A person's token.** `cx.setAuth(jwt)` in a mutation's source, with a JWT
  the deployment accepts (its `auth.config.ts`); `cx.setAuth(undefined)` signs
  out. In an `async` source it may follow an `await`; in a plain `.then`
  callback its send throws on the web. With `@exact/oauth`, signing in is one
  source: `async (_a, store) => { const r = await provider.signIn(store);
  cx.setAuth(provider.idToken(store) ?? undefined); return r; }`. When
  `live.authError` says the token expired, a source that awaits
  `provider.refresh(store)` and calls `setAuth` the same way renews it. The token stays in data code.
- **A source's synchronous part may not both send and `fetch`.** `setAuth`
  sends on the socket, and the runner refuses a mutation whose source does
  both before its first `await` (`a mutation cannot both send on a socket and
  fetch`): nothing is sent and its `then` does not run. Fetch first in an `async` source, await
  it, then call `setAuth`: `async () => { await fetch(signOutUrl, …);
  cx.setAuth(undefined); return … }`. If a call's frame never left anyway,
  the module still gives no answer for the wrong person and sends each
  mutation behind the token again, so Convex runs none as the person before;
  calling `setAuth` again sends it again.
- **An upload to a URL Convex signed** is one `async` source, since a
  mutation whose source sent may fetch after an `await`: `const url = await
  cx.mutation(api.files.uploadUrl, {}); await fetch(url, { method: 'PUT',
  exactBodyFrom: path }); return cx.mutation(api.files.saved, { … })` (a
  component's signed URL, such as R2's; Convex's own file storage takes a
  `POST`).
- **After a token change** no answer computed for another token is given,
  across a reconnect too, until Convex delivers it for the new one. The screen
  keeps what it showed until the new answers land: gate a signed-in screen on
  your own session, as with any backend. A refused token is `live.authError`;
  Convex closes the socket, and the next connection is signed out.
- **Renew a token before it expires.** Convex refuses an expired token only
  when a request or a query's rerun meets it, not on a quiet socket. Renew it
  from a Contract task (every 10 minutes for a token that lives an hour) with
  the same `refresh`-then-`setAuth` source.
- **Tests** run with no Convex. `server.ts` beside `app.ts` gives
  `@exact/convex/test/server.ts` the app's functions as plain JavaScript over
  memory, and `test` and `verify` start it (`apps/mosaic/server.ts`); its
  `app.json` needs `"server": { "database": false }`, since the stand-in keeps
  no database. A function's second argument is who asks, `{ identity }`: the
  `sub` of the JWT the client sent, unchecked, or `null` signed out. Convex's
  HTTP API answers too (`POST /api/mutation`, `/api/query`, `/api/action`,
  `Authorization: Bearer <jwt>`), so a test's `request` can write as another
  client; `http: (request) => Response | undefined` answers the app's other
  routes (an auth endpoint), with CORS headers added. Keep the
  rules both run in a file with no Convex import (`convex/tiles.ts`). Each
  test starts an empty stand-in, including with `database: false`. Remote
  servers stay shared. Only `test` and `agent --test` share an ancestor's server,
  saying so; `verify` refuses an answering local server, including an ancestor's.
- **Driving a native host against a real deployment.** The driver seeds a
  native host's randomness (`EXACT_AGENT_SEED`, 1 by default), so two launches
  draw the same session id and Convex answers the second's writes with the
  first's answers, without running them. Give each launch a new seed: `agent
  macos --seed 7 …`. The tests of one file launch at the file's seed too, on
  every host: give each test that writes its own `seed` line, first in the test.
- **Hosts.** The web, macOS, iOS, Linux and Windows. A worker-placed source
  and any other host have no socket to send on. There, and for a read that
  need not be live, use Convex's HTTP client under `net.fetch`: `new
  ConvexHttpClient(CONVEX, { logger: false })` from `convex/browser` (its
  default logger calls `Math.random()`, which a data module refuses), then
  `await http.query(api.messages.list, { channel })`.

#### Convex Auth

With Convex Auth (`@convex-dev/auth`) the deployment runs the OAuth flow, and
the client calls three of its mutations: one that starts a sign-in (`{
redirectTo }` to `{ redirect, state }`), one that completes it (`{ code, state
}` to tokens) and `refreshSession` (`{ refreshToken }` to tokens). Its React
hooks and browser session manager do not run in a data module. Call the
mutations with `cx.mutation`, open `redirect` with `openAuthSession` (it and
`authCallback` are declared for every app in `app.contract.d.ts`), keep the
refresh token with `store.set` under a `secret.keep` grant on native hosts, and
give the access token to `cx.setAuth`. This direct token flow exposes web tokens
to page scripts. For [web sessions](recipes/web-sessions.md), put the flow behind
your server's HttpOnly cookie; `cx.setAuth` does not turn a bearer token into a
cookie. The mutations' names are the deployment's:

```ts
const SESSION = 'example.session';
export const grants = [`net.websocket ${CONVEX}`, 'auth.session https://accounts.google.com', `secret.keep ${SESSION}`].join('\n');

// a source of `sources`
signIn: async (_a, store) => {
  const callback = authCallback(), mine = crypto.randomUUID();
  const started = await cx.mutation(api.auth.startSignInWithGoogle, { redirectTo: `${callback}?state=${mine}` });
  const back = new URL(await openAuthSession(started.redirect, { callback, state: mine })).searchParams;
  const failed = back.get('convexAuthError');
  if (failed !== null) return { ok: false, error: failed };
  const done = await cx.mutation(api.auth.completeSignInWithGoogle, { code: back.get('convexAuthCode') ?? '', state: started.state });
  if (done.status !== 'complete') return { ok: false, error: done.userError.error };
  store.set(SESSION, done.tokens.refreshToken);
  cx.setAuth(done.tokens.accessToken);
  return { ok: true, error: '' };
},
```

- **The state goes in `redirectTo`.** `openAuthSession` completes only on a
  callback that carries the `state` it was given, and Convex Auth sends back
  only `convexAuthCode` or `convexAuthError`. It keeps `redirectTo`'s query, so
  a state drawn in data code and written there comes back.
- **The code is `convexAuthCode`,** not `code`: an empty code is
  `INVALID_CODE`.
- **Resuming and signing out.** At launch, once `live.status` is `ready`, a
  source sends `refreshSession` with the kept token, keeps the new refresh
  token and calls `cx.setAuth`; a reply with no new tokens means the session
  ended, so it forgets the kept one (`store.forget`). Renew the same way when
  `live.authError` says the token expired. To sign out, forget the kept token,
  call the deployment's sign-out mutation, then `cx.setAuth(undefined)`.
- **A drive needs a fixed origin.** Convex Auth checks `redirectTo`'s origin
  against its allowed redirect origins, and the driver picks a new port each
  run: drive with `agent web --port 3000` (or `EXACT_PORT`) and allow
  `http://127.0.0.1:3000`.
- **Native callbacks.** Convex Auth takes only HTTP or HTTPS `redirectTo` URLs.
  Declare `auth.callback https://app.example/auth/return` and allow its origin
  in Convex. Use only an HTTPS callback for this flow: `authCallback()` prefers
  a declared custom scheme when one is present. Apple requires iOS 17.4 or
  macOS 14.4 and the callback host's `webcredentials` association. Android uses
  Auth Tab when supported, otherwise the callback's verified App Link; HTTP
  loopback still answers 501. The [HTTPS callback setup](#https-callbacks) describes
  the signing identities and association files. Native Google approval and
  token rotation still need a check against the app's real Convex deployment.

### Supabase

The `supabase` recipe wires the session and a table; this is the rest. It was
tried against a loopback stand-in for Supabase's APIs, not a live project.

#### Supabase session

Copy `supabase.ts` whole, this section's code and the next one's. The origin and the publishable key are settings:
`"env": ["SUPABASE_URL", "SUPABASE_KEY"]` in app.json, their values in
`.env.local`. Each Auth call keeps its reply under the `secret.keep` name; a
change to the session holds that name's lease (`store.exclusive`, LLP 1146 D5),
whose work must not throw after a request, or the secret is forgotten.

```ts
import { PostgrestClient } from '@supabase/postgrest-js';
import { StorageClient } from '@supabase/storage-js';
import { FunctionsClient } from '@supabase/functions-js';
import type { Store } from './app.contract.d.ts';

// The project's URL and publishable key, settings in app.json's `env`: neither is a secret.
export const PROJECT = import.meta.env.SUPABASE_URL ?? 'http://127.0.0.1:54321';
const KEY = import.meta.env.SUPABASE_KEY ?? 'anon-key';
export const SESSION = 'supabase.session';

type Session = { project: string; access_token: string; refresh_token: string; expires_in: number; expires_at: number; user: { id: string; is_anonymous: boolean } };
export class AuthError extends Error {
  readonly reason: string; // what a failed resource or mutation shows as its code
  constructor(readonly status: number, readonly code: string, message: string) { super(message); this.reason = code; }
}
// Auth's answers to a refresh token it will never take again.
const ENDED = ['refresh_token_not_found', 'refresh_token_already_used', 'session_not_found', 'session_expired'];

// The session's changes, one at a time: a refresh, a sign-in and a sign-out each
// read what is kept inside the lease, so none writes over another's result. A
// lease whose work throws after a POST forgets the secret (LLP 1146 D5), so the
// work's error is carried out of the lease and thrown there.
async function exclusive<T>(store: Store, work: () => Promise<T>): Promise<T> {
  type Done = { value: T } | { error: unknown };
  const leased = store as Store & { exclusive(name: string, work: () => Promise<Done>): Promise<Done> };
  const done = await leased.exclusive(SESSION, () => work().then((value) => ({ value }), (error: unknown) => ({ error })));
  if ('error' in done) throw done.error;
  return done.value;
}
// A session kept for another project's URL is not this one's.
const kept = (store: Store): Session | null => {
  const s: Session | null = JSON.parse(store.get(SESSION) ?? 'null');
  return s?.project === PROJECT ? s : null;
};

// One Auth (GoTrue) call that answers a session, kept with its expiry on the app's clock.
async function auth(store: Store, now: number, path: string, body: object, jwt = KEY): Promise<Session> {
  const res = await fetch(`${PROJECT}/auth/v1/${path}`, { method: 'POST', body: JSON.stringify(body),
    headers: { apikey: KEY, Authorization: `Bearer ${jwt}`, 'Content-Type': 'application/json' } });
  const out = await res.json();
  if (!res.ok) throw new AuthError(res.status, typeof out.error_code === 'string' ? out.error_code : '', out.msg ?? out.message ?? `Auth answered ${res.status}`);
  if (typeof out.access_token !== 'string' || typeof out.refresh_token !== 'string' || typeof out.user?.id !== 'string' || !(out.expires_in > 0))
    throw new AuthError(res.status, 'bad_session', 'Auth answered no session');
  const s = { ...out, project: PROJECT, expires_at: Math.floor(now / 1000) + out.expires_in };
  store.set(SESSION, JSON.stringify(s));
  return s;
}

/** A new anonymous user, unless a session is kept. */
export const signInAnonymously = (store: Store, now: number) =>
  exclusive(store, async () => kept(store) ?? auth(store, now, 'signup', { data: {} }));

// Due for a refresh at `now` (`time.epochAtZero + performanceNow()`): a quarter of its life left, at most five
// minutes, so the minute tick reaches a five-minute token in time.
const due = (s: Session | null, now?: number): s is Session => s !== null && now !== undefined
  && s.expires_at * 1000 - now <= Math.min(300_000, s.expires_in * 250);
// The refreshed session, or `null` when Auth has ended it. A reply lost on the way is
// asked for again at once, inside Auth's reuse interval, which answers it as before.
async function refresh(store: Store, s: Session, now: number): Promise<Session | null> {
  const ask = () => auth(store, now, 'token?grant_type=refresh_token', { refresh_token: s.refresh_token });
  try {
    return await ask().catch((e: unknown) => { if (e instanceof AuthError) throw e; return ask(); });
  } catch (e) {
    if (!(e instanceof AuthError && ENDED.includes(e.code))) throw e;
    store.forget(SESSION);
    return null;
  }
}

/** The kept session, or `null` when signed out; given `now`, refreshed first when due. */
export async function session(store: Store, now?: number): Promise<Session | null> {
  if (!due(kept(store), now)) return kept(store);
  return exclusive(store, async () => {
    const s = kept(store);
    return due(s, now) ? refresh(store, s, now!) : s;
  });
}

/** Supabase's own clients, sending the session's access token; given `user`, only that user's. */
export async function supabase(store: Store, now?: number, user?: string) {
  const s = await session(store, now);
  if (s === null || (user !== undefined && s.user.id !== user)) throw new AuthError(401, 'signed_out', 'Signed out');
  const headers = { apikey: KEY, Authorization: `Bearer ${s.access_token}` };
  return {
    user: s.user, headers,
    db: new PostgrestClient(`${PROJECT}/rest/v1`, { headers }),
    storage: new StorageClient(`${PROJECT}/storage/v1`, headers),
    functions: new FunctionsClient(`${PROJECT}/functions/v1`, { headers }),
  };
}
```

#### Supabase sign-out and Apple sign-in

The rest of `supabase.ts`:

```ts
/** Signs this device out. Revoking its refresh token is best effort: offline, the token stays valid on the server. */
export const signOut = (store: Store, now: number) => exclusive(store, async () => {
  let s = kept(store);
  if (due(s, now)) s = await refresh(store, s, now).catch(() => s);
  store.forget(SESSION);
  if (s !== null) await fetch(`${PROJECT}/auth/v1/logout?scope=local`, { method: 'POST',
    headers: { apikey: KEY, Authorization: `Bearer ${s.access_token}` } }).catch(() => undefined);
});

/** Sign in with Apple's ID token, linked to `user`, the anonymous user the sheet
 * opened for. An Apple ID that already has an account throws `identity_already_exists`
 * and keeps the anonymous session: switching leaves its data behind, so it is the
 * app's choice (`true`). */
export const signInWithApple = (store: Store, idToken: string, nonce: string, now: number, user: string, switching = false) =>
  exclusive(store, async () => {
    let s = kept(store);
    if (s?.user.id !== user) throw new AuthError(401, 'signed_out', 'Signed out');
    if (due(s, now)) s = await refresh(store, s, now);
    if (s === null) throw new AuthError(401, 'signed_out', 'Signed out');
    const body = { provider: 'apple', id_token: idToken, nonce };
    if (!s.user.is_anonymous || switching) return auth(store, now, 'token?grant_type=id_token', body);
    return auth(store, now, 'token?grant_type=id_token', { ...body, link_identity: true }, s.access_token);
  });

// Auth checks that an ID token's nonce is the hex SHA-256 of the nonce it is given: ask Apple for this.
export async function sha256(text: string) {
  const bytes = new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text)));
  return [...bytes].map(b => b.toString(16).padStart(2, '0')).join('');
}
```

#### Supabase accounts

- **The project:** enable anonymous sign-ins; give each table a `user_id uuid
  default auth.uid()` column and row-level security policies on it for the
  `authenticated` role (an anonymous user has it too), and the bucket policies
  on the first folder of an object's name, which is the user's id here. Row
  security that refuses a read answers an empty list, not an error.
- **Sign out** from a mutation that refreshes `session`, passing the press's
  time: `signOut: async ([now], store) => { await signOut(store, now); return {
  ok: true, error: '' }; }`. The recipe's `anonymous` task then signs in a new
  anonymous user. Until a read of the new user lands, and while a `when` is
  false, a resource shows its last answer: put the user in the answer and show
  rows only when it is `session.userId`, as [your own REST
  server](recipes/rest-backend.md) does with `owner`.
- **A refused refresh** (`refresh_token_not_found` and the other ended
  sessions) forgets the session, and `session` answers `null`; a 429, a 5xx or
  no network keeps it for the next tick, and a reply lost on the way is asked
  for again at once. Auth takes a refresh token again within its reuse interval (10 s
  by default), or when it is the active token's parent, and revokes the session
  for any other reuse ([Supabase
  sessions](https://supabase.com/docs/guides/auth/sessions)); the lease keeps
  two answers from spending one token. A session kept for another
  `SUPABASE_URL` is ignored, and its token never sent there.
- **Sign in with Apple** links the Apple ID to the anonymous user. The project
  enables manual linking, and its Apple provider lists the app's bundle ID.
  `@exact/sign-in` passes `params.nonce` to Apple unchanged, and Auth compares
  the token's nonce with the hex SHA-256 of the nonce it is given: before the
  press, draw `raw = crypto.randomUUID()` and ask Apple for `await sha256(raw)`,
  then call `signInWithApple(store, credential.token.authorization.id_token,
  raw, now, user)`, `user` being the `session.userId` the sheet opened for. That
  is the native credential. On the web the ID token stays with the app's server
  ([sign-in recipe](recipes/sign-in.md)), whose token's audience is the
  Services ID, which the Apple provider then lists too; that server makes the
  exchange, with the anonymous session to link, and hands back the session.
  An Apple ID with an account of its own throws `identity_already_exists`
  (`failure(x).code`, from `AuthError`'s `reason`) and keeps the anonymous
  user; switching (`signInWithApple(…, true)`) leaves that user's rows behind.
- **The tokens:** on the web `secret.keep` is the page's `localStorage`, where
  supabase-js keeps them in a browser, readable by any script on the origin;
  on Linux an unencrypted file. A web refresh token out of scripts' reach needs
  the exchange behind your server and an HttpOnly cookie ([web
  sessions](recipes/web-sessions.md)).

#### Supabase Storage and Functions

From a test app's sources, after `const { db, storage, functions, user,
headers } = await supabase(store, now, owner)` and `images =
storage.from('images')`, `owner` being the `session.userId` the file was chosen
under, so a write queued across a sign-out is refused:

```ts
ok(await images.upload(`${user.id}/a.png`, new Uint8Array([137, 80, 78, 71]), { contentType: 'image/png', upsert: true }));
(ok(await storage.from('images').list(user.id)) ?? []).map(f => f.name)
ok(await images.remove([`${user.id}/a.png`]));
ok(await functions.invoke<{ greeting: string }>('hello', { body: { name: 'Ada' } }))?.greeting ?? ''
```

A native request holds at most 4 MiB, so a photo goes up from its file
(`fs.read` of its folder) with `fetch`, which streams it from disk:

```ts
const res = await fetch(`${PROJECT}/storage/v1/object/images/${user.id}/photo.jpg`, { method: 'POST', exactBodyFrom: 'app:/tmp/photo.jpg',
  headers: { ...headers, 'Content-Type': 'image/jpeg', 'x-upsert': 'true' } });
if (!res.ok) throw new Error((await res.json()).message);
```

#### Supabase limits

- **`@supabase/auth-js`, and so `@supabase/supabase-js`.** The build refuses
  both for their uses of the DOM's and Node's globals: the Auth client reads
  `window`, `location`, `localStorage` and `navigator.locks` on browser-only
  paths, and Realtime's socket factory reads `process`. With that refusal
  lifted, Auth still fails: each session call reads `Date.now()`, which every
  executor refuses (LLP 1027.000). The umbrella client with `accessToken`
  would serve tables, Storage and Functions, at 403 KB of web JavaScript to
  these three packages' 92 KB.
- **Realtime.** Its socket outlives an answer; `QUEUE.md`, "A stream that can
  send: Supabase Realtime without polling".

### Child processes (`@exact/child-process`)

A desktop app's data code starts a program with Node's names (LLP 1133.007).
A resource answers `spawn(program, args, {exactStream})`: the child lives
while the resource holds it, and its mapper hears `spawn`, `stdout`,
`stderr`, `exit`, `close`, `drain`, `ready` and `ready-timeout`, with the
incarnation's handle (`write`, `end`, `kill`). An answer awaits `execFile`
for a one-shot run. Each program needs a grant line naming it
(`process.spawn path:ssh`, `process.spawn /usr/bin/tar`); a call with none is
`grant-missing`. Only a person's input or the app's own code starts or
drives a child: a tool's call, a wake or a device event is refused
`person-required`, and its commit lets no child go. Under the agent nothing
runs unless the drive names it: `--process <program>=script:<file.json>`
answers from a script on the agent's clock. macOS and Linux run it; every
other target answers `NotSupportedError`, which `processStatus()` says
ahead. The options, events and limits are `docs/reference.md`'s "Child
processes".

### Tools: what a call answers

A `tool` line offers a root action to callers outside the app: the agent
driver, and, through its face in `app.json` `intents.tools`, Siri and
Shortcuts (App Intents) and a browser's agent
([WebMCP](https://webmachinelearning.github.io/webmcp/)). Every door
answers one envelope (LLP 1111 D4), and `state.intents.calls` keeps the
last twenty with their `door`:

- `outcome`: `ok`, `failed`, `superseded`, `timeout`, `interrupted` or
  `refused`. `certainty`: `none` (it did not run), `applied` (it ran and
  its work settled) or `uncertain` (it ran, and its effect may or may not
  have landed: read before calling again).
- `reason`, one word where there is one: `unavailable` (not offered on that
  door, `available when` false, a face that changed), `arguments`, `busy`
  (5 s behind another call or a wake, what it reports still being answered,
  storage still writing), `cancelled`, `refused-commit`, `background`,
  `person-required`.
- `value` is what `returns` reads (null when refused), and `message` a
  sentence for the caller. The record adds `cancelled`.

A refresh does not make a call `busy`: `returns` and `fails when` read a
resource's standing value (its last answer, its placeholder, a family's
gathered list), and `pending`, `failed`, `failure` and `current` answer at
once.

**A cancelled call.** A browser's agent aborting `execute`, or the system
cancelling an intent's task, cancels the call (the door sends
`{"op":"cancel","id":<call>}`). One still waiting never runs: `refused`,
`none`, `cancelled`. One already dispatched runs to its end, since its
commit has landed and what it started is the app's to finish; its record
says `cancelled: true`, and the journal that the caller left.

**On the web**, each faced tool is registered with `document.modelContext`:

- `readOnlyHint` for a `reads` tool whose reach (its action, the actions
  it calls, the `then`s of what it sends) has no `send`; `consequentialHint`
  for `destroys` and for every `writes` except `writes additive closed`,
  which only adds to the app's own data; `untrustedContentHint` when
  `returns` is a string, whose text may not be the author's (a note, a
  review).
- `outputSchema` is the envelope, its `value` typed from `returns`, and
  `execute` resolves with it for every outcome. It never rejects: an agent
  learns nothing of a rejection but "OperationError".
- A face parameter's `title` and `description` are its input schema's
  (`"page": { "title": "Page", "description": "The page you have read up
  to.", "type": "integer" }`); App Intents shows the same two words.
- A route with no `activate=` that would wait for the reader is `idle` in
  an app with a faced tool on the web door, so the agent finds the tools
  once the page has loaded. A declared `activate` is kept.
- `document.modelContext` exists only in a secure context: on `https`, on
  `localhost` and on `127.0.0.1`, not on a LAN address over `http`
  (Chrome 157 with `--enable-features=WebMCP`: none at
  `http://10.0.0.4:<port>`). To try a LAN host, serve it over HTTPS or
  start Chrome with `--unsafely-treat-insecure-origin-as-secure=<origin>`.

An app may also declare its own `AppIntent` in `modules/apple/*.swift` or a module's
`apple/launch/*.swift`. Apple builds extract those modules' constants and combine their
metadata with generated faces in the app's `Metadata.appintents`, even with no faced tools.
An app with native intents links its module library at launch so the system can resolve the
intent before any view opens. Launch-part extraction uses the target's compilation conditions,
including `DEBUG` for its debug configuration. On Linux, authored `openAppWhenRun`, `isDiscoverable`,
introduced iOS/macOS availability and scalar parameter defaults are preserved; defaults that
cannot be represented exactly and unsupported declarations fail the build.
Linux builds also refuse more than one App Shortcuts provider across modules and generated faces.
A launch part can import ExactToolDoor, which a plan with a tool links, and call the public `ExactTools.call` door (LLP 1133.023 D15).
The `--extensions` selection applies to native and generated intents alike.
On Linux, native `IntentFile` content types may use `UTType(importedAs:)` or the constants
`.image`, `.png`, `.jpeg`, `.heic`, `.heif` and `.data`.

### File parameters on an Apple face

On a string parameter, declare `"type": "file", "accept": ["image/png", "image/jpeg", "image/heic"]`
and set the face's `"doors": ["apple"]`. Accepted MIME types come from `scripts/document-utis.json`,
excluding directories. Set `host.ios.minimumOS` to `"18.0"` or later and `host.macos.minimumOS`
to `"15.0"` or later: App Intents' `IntentFile` initializer with `supportedContentTypes` needs them.

The accepted MIME types map to UTI identifiers in `scripts/document-utis.json`. A type the
supported Apple platforms do not declare (Markdown and Ogg audio) is refused at build time.
The generated action copies the file before calling the tool; only the path crosses the door.

The tool receives an `app:/tmp/picked/…` path, as a file input does. Each file is at most 64 MiB;
the copy lasts until the next process launch, including after the caller cancels. Read it under
`fs.read app:/tmp/picked`, or copy it to durable app storage before returning if it must survive.
WebMCP takes JSON arguments with no file handoff, and AppFunctions' door is not built;
a file face naming either is refused at build time. The call's existing time bounds stay the same.

## Views, layout, and interaction

For an attachment of any type, use `input type="file"` with no `accept` (or
`accept="*/*"`) and `multiple` when needed. `showPicker("attach")` opens the
input with `id="attach"`; `change` receives `Picked[]` with copies under
`app:/tmp/picked/`. A copy's file name is not the file's: `path` is
`app:/tmp/picked/<id>.<ext>`, `<id>` from the launch and a counter, and `name` is the
original name. Read the bytes at `path` (grant `fs.read app:/tmp/picked`), and show or
upload them under `name`. A specific `accept` list remains bounded to image/video
types and the app's `file_handlers`. For audio, declare `audio/mpeg`, `audio/wav`,
`audio/mp4` or `audio/ogg` in `file_handlers` and name those exact types in `accept`;
`audio/*` remains outside the admitted wildcard types. XHTML uses
`application/xhtml+xml` in the same way. On Linux the agent can answer a held
picker, but the live picker is unavailable (LLP 1069.002, "Any-type input").
Android uses the system Photo Picker for `image/*`, `video/*`, or both wildcards,
when the picker is available. Specific MIME types and other lists
use the document picker; unknown extensions leave its filter unrestricted, with
`accept` checked after import. Both support `multiple` and return copies; no
photo-library or storage permission prompt is needed. Back fires `cancel`.
HEIC/HEIF stays original for an empty `accept`, `*/*`, `image/*` or an explicit
HEIC/HEIF type or extension. A specific list containing `image/jpeg`, `.jpg` or
`.jpeg` also offers HEIC/HEIF in the document picker, then converts it to upright
JPEG at quality 0.9 without location metadata. The delivered type
must still match `accept`; conversion failures fire `cancel` with a journal reason.

The view forms are element, component use, `if`/`else` (and `else if`),
keyed `for`, exhaustive option `match`, and `children`. Wrap root regions in a
stable element.
A literal `role` must name a concrete ARIA role, or it is refused.
`aria-activedescendant` names the active option while a field keeps focus
([combobox recipe](recipes/combobox.md)); the web exposes it, and other hosts carry it
without using it.

Content pages can use `p`, `strong`, `em`, `blockquote`, `ul`, `ol` and `li`.
The web emits the corresponding HTML elements in the built page and when the
app updates it. A paragraph holds a string or text runs. Lists hold `li`
elements, including items from a keyed `for`. Write `li "First"` for a simple
item, or nest paragraphs and another list inside it. Use `list` for a scrolling
app list. Tables and description lists also emit their HTML elements.

Contract spells a few HTML names differently. Write `view` for a plain
`div`, or `column` and `row` for flex layout. A plain `text` becomes a
`div` on the web and a `span` inside another text. Write `h1` through `h6`
for headings, `image` for `img`, and `link` for `a`. Give a field an
`aria-label`, or use `aria-labelledby` with the ID of its visible label.
Set the document title and description with `head title=… description=…`.
A field's Enter runs its `submit` action. For a disclosure, keep `open` in
state, show the body `if open`, and toggle it from a button.
A view chooses with `if`, as an action does; a count is
`xs.length`, never `count`, `len` or `.size`; a resource's placeholder is `else
empty()` (`[]` for a list), never `else []`. A component
call uses parentheses; a built-in element uses space-separated attributes.
`button "Save" press=save` is text-child sugar; an explicit text child is useful
when that label needs its own styling or driver id. A `button` is the web's
`<button>`: a block whose content is centred in its height and whose text is
centred (`text-align: center`). Give a sized button no alignment rows; write
`display="flex"` (a row) for an icon and a label side by side, and
`text-align="start"` on a list row or card made of a button.

Use the CSS and HTML vocabulary. Defaults matter: a bare box is block and
content-box, while `row` and `column` supply flex styles. Do not emit React Native
property names, invented CSS aliases, or unknown HTML tags. Read the schema and
tag mapping for admitted properties and units. CSS standard behavior does not
imply that every browser API is implemented on every host.

For a control that should remain a Tab stop but be announced as unavailable,
write `aria-disabled=true`. It does not stop `press`; check the unavailable
state in the action. Descendant controls and inline links inherit this
accessibility state unless they explicitly say `aria-disabled=false`. HTML
`disabled=true` removes a form control from focus.
Projected Apple segments, UIKit bar items and titles, and AppKit menu items keep
that accessibility state on the native item people encounter. UIKit menu rows
and alert actions stay selectable but read as enabled; UIKit's public disabled
states would stop their input.

`class=Style` applies literal style values; node attributes win. Conditional
classes select two named styles with `class=(condition ? A : B)`. Computed values
go on nodes. Styles can also carry `backgroundMaterial`, `glassGroup` and
`buttonStyle`. In a conditional class, each prop must be set in both styles or
neither, unless the node supplies its own value. `backgroundMaterial="none"`
explicitly removes a material. There is no selector cascade, dynamic style
object, or arbitrary class-string composition. A style branch can mix a number
and a CSS keyword in a property's admitted value space; this does not add
general union types.
On a number-only row such as `opacity`, a computed branch known to be a string
is refused at build time even when another branch is numeric.

`image-rendering` controls a raster image's sampling and inherits through its
ancestors. `auto` smooths; `crisp-edges` uses nearest-neighbour sampling when
shrinking or enlarging; `pixelated` enlarges to the nearest integer multiple
with nearest-neighbour sampling, then smooths to the requested size. At a
fractional scale, `pixelated` and `crisp-edges` can differ. Use
`image "assets/sprite.png" width=96 height=96 image-rendering="pixelated"`
for enlarged pixel art. The value may be bound to state or supplied by a named
style. It changes paint, not the image's natural size or layout.
Native hosts preserve the source samples when the decode fits their image
budget; a larger image uses a smaller decode so it can still be displayed.

`background-image` takes up to four linear, radial, conic or repeating linear
gradients; `mask-image` takes one. Stops take percentages, or px lengths for
linear/radial gradients (unitless zero is allowed). For example,
`background-image="repeating-linear-gradient(-45deg, transparent 0 4.243px, #e6e6e6 4.243px 5.657px)"`.
A repeating gradient’s px period stays fixed as a box resizes; mixed px/%
and omitted stops use CSS fix-up after layout. A zero period paints its last
stop, as Chrome does. `background-attachment="local"` is still
refused: separate row backgrounds restart at each row, and repetition alone
does not make a virtualized list’s background follow its content.
`contract vocab background-image` gives the grammar and limits. AppKit’s, UIKit’s,
Linux’s and the web’s hatches match Chrome
([closed issue](../issues/closed/20261011-repeating-hatch-across-list-rows.md)). On UIKit
a repeating background with `background-attachment="fixed"` draws unfixed, as
any drawn gradient does there.

For scrolling, provide a bound and inspect measured layout. For virtualized
lists, use `list virtualized=true` (no other tag takes it), one direct keyed
`for`, and one flow root per row. A vertical list with `appearance="none"`
may also hold fixed content before and after that `for`; a second direct `for`
needs an element around it (all its rows mount). Fixed content cannot hold a
virtualized list. Fixed roots require nonnegative margin lengths, including percentages and
font-relative units, and static or relative positioning without vertical offsets
(`lower-collection-fixed-flow`).
Fixed content keeps its authored sibling layout and stays mounted even when the `for` is empty. It has no data-row key, cell styling,
`keep-mounted` entry or place in `aria-setsize`; give it an `id` for element
`scrollIntoView(id)`. Authored padding stays before the lead and after the trail.
A `for` inside a fixed element is eager and still receives
the development `long-list` notice at 200 rows.
The list's `reachstart` and `reachend` fire when its content edges (the lead's
start and the trail's end, when present) come within `reach-margin` of the port
(one port by default),
and further ahead while a fling travels toward it; page from them rather than from
`scroll` (`docs/reference.md`, "Intersection visibility"). A vertical list needs `height`, `max-height`
or a growing `flex`, which may be computed (`max-height=(narrow ? "320px" : "640px")`).
A percentage bounds it only against a definite containing-block height (a plain
`column` is content-sized), and the bake refuses (`bake-scroll-unbounded`) only a
list with no bound at all, in its first 390×844 frame, so look at the list in each
layout it takes. It takes `estimated-item-height`. A horizontal one needs a
literal `display="flex"` and a positive `height` in points, literal or computed
(`height=(cardW / 0.68)`), takes `estimated-item-width` and CSS's `gap` (or
`column-gap`) between its cards, a nonnegative number, literal or computed, and
refuses wrapping, reversed or right-to-left flow, `justify-content` other than
`flex-start`, and `reorderdrop`. A vertical list is a block, where CSS lays out
no gap: space its rows with a `margin-bottom` on the row root. An estimate may be
computed (`estimated-item-width=cardW`): when it changes, every row the list has
not mounted takes it until measured again, and the rows on screen stay where
they are; a changed gap moves every card by it.
Main-axis padding (`padding-top`/`-bottom`, a row list's `-left`/`-right`) is
CSS's room before the first row and after the last, inside the scroll content:
room under a header laid over the list, or over a tab bar
(`padding-bottom="calc(env(safe-area-inset-bottom) + 49px)"`). It takes a
number, an `env()` length or its `calc()`, or a computed number, not a
percentage; the end that `reachend`, `scrollFollowEnd` and `scroll-start="end"`
reach is past it, and on iOS the pull-to-refresh spinner draws below
`padding-top` (a padding-bottom taller than the port has limits: LLP 1010
§6.9). `scroll-padding` (`-top`/`-bottom`, a row list's `-left`/`-right`, the
same forms) is where its `scrollIntoView` aligns a row, as CSS's snapport:
with `scroll-padding-top` equal to a header's height, the first row's
`block="start"` is `scrollTop` 0. Any scroll container takes it, not only a
virtualized list.
For snapping (LLP 1008 §9), put `scroll-snap-type` on the scroller: `"x
mandatory"`, `"y mandatory"` or `"both mandatory"`, or the axis alone (`"x"`,
which is CSS's `x proximity`: it snaps only near an area). Put
`scroll-snap-align` on its areas: `"start"`, `"center"` or `"end"`, or two
values, the block axis's (y) then the inline axis's (x), as `"none center"`.
`scroll-snap-stop="always"` on an area keeps a scroll from passing over it, so a
fling moves one card or one page. `scroll-padding` insets the snapport and
`scroll-margin` grows an area. The web executes CSS. The native hosts share one
rule: the area position nearest where the platform would rest the scroll; for a
flick (a release faster than 50 points a second, or a mouse wheel's notch) at
least the next position its way from where the drag began, so a short quick
flick moves one card; short of the first `always` position past where it was
released; and for proximity only within a third of the snapport. iOS applies it
to UIKit's projected rest when a drag ends; macOS eases there when a trackpad
lifts (from where its momentum would carry the scroll) or once a mouse wheel
stops, on a scroller that has its `scroll-snap-type` when it first shows (one
that gains it later, or a reused list row that lacked it, glides as AppKit
flings it); Android aims its fling there; a Linux or Windows wheel moves one position
a notch, finer ticks adding up to a notch first. On iOS, areas whose snap
positions (alignment and `scroll-margin` applied) are each multiple of the
scrollport, every one `scroll-snap-stop="always"` with nothing insetting the
snapport, page as UIKit's `isPagingEnabled` (SwiftUI's `.paging`) does. An oversized area stays freely scrollable while it covers the snapport.
Nested scroll containers own their descendants' snap areas: `hidden` is a
boundary; `clip` is not. When a commit moves the area a mandatory scroller rests
on, the scroller follows it (iOS, macOS, Linux and Windows; on Android the
reader keeps its offset). A virtualized list also reads at most 64 unmounted row
roots near the projection (iOS, macOS; on Android around its offset when a touch
lands and around the projection at the release), from their measured or
estimated sizes, and an unmounted `always` row between the release and the rest
holds it; descendants and layout-dependent displacement need mounted rows.

To let a person pinch to zoom a photo, a diagram or a board, put it alone in a
`scroll` and write `-exact-max-zoom` (and `-exact-min-zoom` below 1 to zoom a
large child out; each defaults to 1) on the scroll: `scroll
-exact-max-zoom=4` holding one `image width="100%" height="100%"
object-fit="contain"` is a photo viewer (LLP 1057.001.000). The child keeps its
layout and is drawn scaled; the scroll's extent grows with it and centres it
while it is smaller than the port, so do not align it (`align-items`,
`justify-content`, an `auto` margin and the like are `lower-zoom-align`). The
pinch, the bounce at the limits and the double tap are the platform's;
`touch-action="manipulation"` turns the double tap off, and on a double-tap path
a `press` on content that is not a control waits for the second tap. `scrollZoom`
is the zoom as `scrollTop` is the offset: write it for Fit and 2× buttons (with
`scroll-behavior="smooth"` it animates), and read it as `e.scrollZoom` on the
`scroll` event's `ScrollEvent`; mirroring every `scroll` into the state you
bind is safe. Linux has no touch pinch or double tap, and a pinch on Windows
has no bounce: `contract vocab -exact-max-zoom` says what each host lacks.

`reorderdrop` belongs only on a vertical `list virtualized=true`
(each row's handle names it with `reorderFor`); the compiler refuses it on any
other element, where no host could drag. Lists that share a `reorderGroup`
(each with a `reorderdrop`, an `id` and string keys) exchange rows: the drop
fires once, on the list the row lands in, with the dragged row's key, then the
key it lands before (`none` at the end) and, for an action taking one more
parameter, `ReorderEvent { from, to }`. The action decides where the row goes: a board whose cards join the bottom
of the column they are dropped on ignores that key when `from != to`. A board
has columns in a horizontal `scroll`, each with a header, a grouped list and
a quick-add. Give the list `flex=1 min-height=…` so its scroll box fills the
available height. The finger or pointer chooses the list under it, even with
a grip at the edge of a wide card. The lifted card's centre chooses the gap
within that list. Releasing over a header or gutter uses the last gap the
drag reached. To highlight that column, bind
`reordertargetchange=highlight(col)` on each grouped list. Its action takes
the bound column and a boolean. The source becomes the target on lift. When
another list takes over, the old list receives `false` and the new list
receives `true`. Drop and cancel clear the highlight. Keyboard and
accessibility moves use the same signal. Give each grip `touch-action="none"`
and no `press`, `pan`, `pointerdown` or `key` of its own, so the host's keys
(Space, the arrows, Enter, Escape) work on it. The host draws the lifted row,
holds the drop until the move shows (a second at most) and scrolls the lists
near their edges; do not build card drags by hand. Lists nest one level deep; an inner vertical list needs a literal
`height` or `max-height`. Do not revive the removed legacy `item-height`
windowing mechanism. Rows inserted, removed or resized above what the reader
sees keep the reader's place, as CSS scroll anchoring does; a list at its start
stays there, so rows inserted on top show, and one following its end
(`scrollFollowEnd`) follows it. A scroll the app asks for (`scrollIntoView`,
either form, or an ordinary scroller's `scrollTop` write) owns its scroller
until it settles: while a smooth one runs, `scrollFollowEnd` moves nothing, even
when the same action turns it on, and once it rests the end is followed only if
it rests there, as after the reader's own scroll. Its `scroll` events report the
port as it moves, the first maybe still where it was, and its `scrollend` comes
once, where it rests, after a last `scroll` whose `scrollHeight - scrollTop -
clientHeight` is under a pixel at the end. A jump-to-latest control needs
nothing more than `scrollIntoView("transcript", lastId, block="end",
behavior="smooth")` with `scrollFollowEnd=true` left on. `block="end"` aligns
the row, not the list's end: a list with `padding-bottom` (room under a composer
or a status line) takes the same `scroll-padding-bottom`, and the last row's
`block="end"` is then the end. A row root may move where it paints
(`translate`, `rotate`, `scale`, a relative `top`/`left`, `z-index`: a lifted
row being dragged) and keeps its place in the list.

A virtualized list mounts only the rows near its port, and a screen reader
moves only through mounted rows. `keep-mounted=[first(ids), ids.at(-1)]`
names rows by key that stay mounted wherever the window is: a transcript's
first and last message, so VoiceOver moves from one to the other, or a row that
is playing. It takes up to 64 keys of the `for`'s key type (LLP 1010 §6.11).

A `button` is the platform's own control by default. A background, border or
radius, rich children, or a row/attribute/context the native control cannot
support makes it your bare box instead. Classes count after merging, and a
row or incompatible child on any conditional arm keeps the whole button bare.
Grouped-list row buttons and their detail accessories stay bare by default so
UIKit can read the cell's face; explicit `appearance="auto"` carries a custom native control.
`appearance="none"` explicitly asks for your box. `appearance="auto"` explicitly
asks for the platform's button and refuses unsupported rows or children. The
switch must resolve to a literal after class merging; use `if` with two
buttons to switch. Its first `text` is the title, its second is the subtitle, and
its one `image "symbol:…"` is the symbol. Raster faces need
`appearance="none"`: the graphical plan is shared, and macOS and Linux do not
paint a native raster face yet. These are semantic face data, not layout
children. An image-only face needs a nonempty `aria-label`. With
`flex-direction="row"` (or absent), an image before/after the texts is
leading/trailing; with `flex-direction="column"` it is top/bottom.

Native buttons admit `font-size`, `font-weight`, `color`, `white-space`,
`line-clamp` and `text-align` on the button or its texts; a text's own row wins.
Apple uses only rows authored there, including classes; absent rows leave the
platform's font and colour alone. The web inherits the page's font and colour.
Tab/menu projections keep their existing ancestor `text-transform` on the
projected title, which the face query reads as painted.
`white-space="nowrap"` asks for one truncated line, `line-clamp=N` caps wrapping,
and `text-align="start"` places the face at the start of its box. The symbol
admits its own `-exact-tint-color`, `font-size`, `font-weight` and
`-exact-symbol-rendering`; a non-template icon keeps its colours under `multicolor`.
Absent symbol rows follow the title. Its `width`, `height` and `object-fit` are refused.
Apple rasterizes non-template icon SVGs into its asset catalog, so editing one
requires a native rebuild. Linux-hosted Apple builds use the installed Chromium
(`CHROME` can select it).

`gap` (or `column-gap` for a row, `row-gap` for a column) sets image-to-title
spacing. Absent means the platform's spacing, not zero; title-to-subtitle
spacing stays the platform's. `align-items` and `justify-content` accept only
`center`. `-exact-control-size` takes `mini`, `small`, `medium`, `large`;
`-exact-corner-style` takes `dynamic`, `small`, `medium`, `large`, `capsule`.
Both are style rows admitted only on native buttons, including in a class.
With explicit `appearance="auto"`, `border-radius` sets the authored radius
and wins over the named corner style; under the default it makes the button bare.
`padding` and its longhands set content insets. Absent leaves the native
control's insets/corners alone.
On the Mac, an image-only `plain` or `borderless` button uses AppKit's push
button height at its control size as its default minimum width and height.
Authored dimensions and padding take precedence; its focus ring follows the image.
In a window toolbar or route header, either style gives the toolbar item a borderless button.

`pointer-events` takes `none` or `auto`. A disabled native button retains
its authored colours; bind `opacity` for authored dimming. It can open a
dialog or popover with `commandfor` and `command="show-modal"`,
`"show-popover"`, `"toggle-popover"`, or with `popovertarget`; targets may be
bound, and an empty target is no target. `href`, `action` and swipe attributes
remain refused. `-exact-enabled` transitions are not available.

**Exact's menu rule** (LLP 1021): a row of a menu (`popover role="menu"`)
whose `popovertarget` (or `commandfor` with `command="show-popover"` or `"toggle-popover"`) names a
popover that is no menu opens it as if by the menu's own invoker (the button
that opened the menu, or the node its context menu opened on), as
`showPopover(id, source=)` does: not nested in the menu, so it stays open as the
menu closes on choose, beside where the menu came from. Write that popover
outside the menu: one written inside it is nested by where it is. A row naming
another menu opens its submenu, nested. The build notes each such row it can
see (literal ids, roles and actions: `lower-menu-row-popover`). Every other
row (a dialog's command, a custom one) keeps HTML's own activation. On the web
the rule takes the row as it is at the click: a press that changes a row from
naming a popover to naming something else still opens that popover, and one
changing it the other way leaves the popover nested.

On macOS, iOS and the web, an action can call `showModal(id)` or `close(id)`
for a `dialog`. macOS and the web follow the invoker's modal focus path. On
iOS a confirmation opens as an alert. Bare `close()` closes the window.

On the Mac a `dialog` that is not a confirmation is a sheet on its window, as
iOS's is a sheet (LLP 1115.002 D7.2): AppKit centres it, dims the window under
it and keeps clicks from it. It is as large as its content, at most the
window's content area, and laid out as written (an authored `width` is used,
`width="100%"` is all of that area; its insets are not, since AppKit places
it). Escape and ⌘. close it, the one `buttonStyle="filled"` button answers
Return, `closedby="none"` leaves only its own buttons, and `closedby="any"`
also takes a click on the window behind it. A popover that is not a menu is
AppKit's popover beside its invoker (D7.3), with AppKit's arrow and material,
on the side its `position-area` names (`top …` above, `right …` beside, else
below): `popover="auto"` closes on Escape, a click elsewhere or another window,
`popover="manual"` only by its own buttons. Write no surface for either: the
sheet and the popover are the platform's. A popover of a dialog's is written
inside it, as in HTML (outside it, it is inert while the dialog is open). The Mac
driver paints both in the page by default. Launch with `--chrome platform`
to show the AppKit sheet or popover. In an open dialog, `state dialog`
reports `presentation` as `sheet` or `painted`. Use `screenshot sheet.png
window` to include the sheet and the dimmed parent window. The screenshot
reply lists the windows it drew in `attached`. Both presentations support
`tap`, `type` and `tree`.

`popover` alone also accepts an optional string. For example,
`popover=(expanded ? some("manual") : none)` keeps the same mounted box:
`some("manual")` adds the property, and `none` removes it. An empty string
still declares a popover; it does not restore inline layout. After setting
`expanded = true`, call `showPopover(id)` in the same action. Setting it back
to false closes the presentation and restores the box to its inline parent.
An iframe kept in that box, with the same `src`, retains its mounted guest.
Use explicit viewport bounds and admitted absolute positioning for an expanded
box; this does not add `position="fixed"`. The
[resident iframe fixture](../scripts/fixtures/iframe-resident-popover.contract)
shows the complete enter/exit action and layout.

For an invoker popover with an authored CSS area, use
`position-try-fallbacks="flip-block, flip-inline, flip-block flip-inline"`.
A fresh opening takes the first fitting attempt, retaining the base if none fits.
An open popover keeps its chosen option while it fits, including across resize;
if none fits after movement, it keeps the current one. Commas separate
attempts; spaces combine flips, applied in the order written. `flip-start`
mirrors across the top-left/bottom-right diagonal (`bottom span-right` becomes
`right span-bottom`), so `flip-start flip-block` (right, ending at the
invoker's bottom) and `flip-block flip-start` (left) differ. `none` disables
fallbacks. In a right-to-left document the diagonal runs from the top-right
corner. With no fitting option, CSS shifts the box back into the window. A
popover with an authored `position` is absolute in the top layer and is held
only at the page's start edges (the top, and the left, or the right in a
right-to-left document), so it can run past the others. The web, painted Apple
layers, Linux, Windows and the full-screen terminal implement these flips; an
authored fallback row selects painted placement on macOS for a non-menu
popover, including when added while open. Removing the row restores platform
presentation; focus and text selection survive either change. Native menus and confirmations keep platform placement. Physical areas also accept
`top span-left`, `bottom span-left`, `left`/`right` with `span-top`,
`span-bottom` or `span-all`, and the four `top|bottom left|right` corners.
Logical areas, `flip-x`, `flip-y`, named rules and area-dependent sizing are
not implemented. Use zero insets and default self-alignment for native parity.
Painted Apple, Linux, Windows and full-screen terminal invoker popovers are
sized in each option's region. An authored `position-area="none"` keeps the
viewport block; an authored `position="sticky"` keeps its ordinary layout.
Scrolling moves the selected option without changing its sizing region until
another option wins. Flips transform margins before percentages resolve there;
`flip-start` also exchanges `width`/`height`,
`min-width`/`min-height` and `max-width`/`max-height`.

A route with `navigationPresentation="modal"` is the same sheet on the Mac
(LLP 1115.002 D7.4), as large as its content (an authored `width` or `height`
is used; its `inset` is not), under the window's content area. Its `header`
is the sheet's own: the heading a title row at the top, and at the bottom
right the `id="back"` control (Cancel) and then the one prominent button
(`buttonStyle="filled"` or `"bordered-prominent"`), which answers Return.
Anything else in the header (a search field, a menu's button) stays in it, at
the top of the sheet. Escape and ⌘. go back as the `id="back"` control does;
`closedby` is a dialog's. A heading inside a pressable keeps the route painted
in the window, and `logs` says so. A `dialog`'s first-child `header` is its
sheet's rows the same way (LLP 1141.004 D6), the button that closes the dialog
(`command="close"`) as the Cancel; Escape stays the dialog's close request,
which fires `cancel`. `--chrome platform` shows the sheet: `tap
"add-save"` presses its button (`native: "NSButton"`), `back` dismisses it,
and `state navigation.sheet` names its title, its buttons and its size.

With no `buttonStyle` a button is the platform's default: plain tinted text on
iOS, a push button on macOS, bordered on the web. Say what the action is:
`buttonStyle="filled"` on the screen's main action, `"bordered"` on one that
must read as a button, nothing on a secondary or inline one; and
`destructive=true` on one that deletes, which iOS draws in the system red.
A round control that floats over content (a search or compose button in a corner, a
bookmark over a card) is `buttonStyle="glass"`, or `"prominent-glass"` filled with
the accent, with its symbol coloured by `-exact-tint-color`: iOS 26's own glass, not a
custom circle.
`buttonStyle` needs a native button; on a default button that comes out bare,
`lower-button-style` names the first reason and says to remove it or write
`appearance="none"` without `buttonStyle`. It is a styleable host-policy prop;
its names and allowable branches
are checked against `schema.json`'s `buttonStyles`. Backgrounds, borders,
shadows, filters, `font-family`, other typography or inner layout and
`-exact-press-scale` are refused with `lower-button-style-attr`, naming a custom
`button` as the alternative. A native button still takes size, place, opacity
and transform rows. Hosts implementing the measure hook supply its fitting
size before the first frame, including height-for-width; hosts without it
keep the existing intrinsic-size report. Test the actual
platform look; macOS can report stand-ins for gap, subtitle and wrapping.
See [`controls.rs`](../contract/lower/src/controls.rs) for the checks.

**Prefer native controls.** Write the Contract form and each host draws its own
control; a hand-built lookalike (a painted switch, a row of buttons for tabs, a
drawn title bar) is a bug. [`apps/shelf`](../apps/shelf/app.contract) is the recipe
to copy for an iPhone app: tabs, a large title with a bar button and header search, a
segmented filter, rows that push, a long-press menu, swipe to delete, the system
alert, a sheet with Cancel and Save, pull to refresh, grouped lists, persistence and
fetch, with its tests, in roles and text styles only. `bun scripts/no-tells.mjs <app>`
lists any literal colour, font size or weight an app's `.contract` files still write. On iOS:

| Write | iOS draws |
| --- | --- |
| `button` (`buttonStyle`, native by default) | `UIButton` |
| `list appearance="auto" listStyle="inset-grouped"` of `section`s (`header`, rows, `footer`) | `UICollectionView` list, as Settings ([human guide](contract-for-humans.md#choosing-a-native-button)); a row of your own parts starts and ends at the cell's margin, as UIKit's own rows do, so write no padding at its ends. Android shows Material's list items, with no line between rows |
| the same, `listStyle="plain"`, each `section`'s `header` one letter (Contacts) | the list with UIKit's A–Z section index at its trailing edge, when every section has a row; sections above the letters (Favorites) stay out of it; a name that starts with no letter A–Z goes under `#`, not nowhere. Its headers pin as the list scrolls, over UIKit's blur; a `grouped` list's scroll away |
| a `section`'s `header` holding boxes of your own, not one `text`: `header` › `row gap=8 align-items="center"` › a sized `image` (or a box around one) and a `text` | UIKit's header holding your views (an avatar beside a name), laid out as you wrote them and starting at the cell's margin, so write no padding at its ends; a `padding-left` or `padding-right` you write is from the card's edge, as UIKit's header view spans the card (16 for a hand-built view pinned 16 inside it). Above and below them is UIKit's header inset, 10 points; `padding-top` and `padding-bottom` on the `header` replace it (12 for a hand-built header view's). Pinned in a plain list as a one-text header is, and one VoiceOver heading named by its texts (a header that holds a button keeps each part its own element). Several bare `text`s are refused: put them in a `row` or a `column`. A `footer` holds one `text` |
| `list` with no `appearance`, its rows down the page (each child is a row) | a plain list's cells: UIKit's separators, the cell's grey ground while a row is pressed, the row selected under the screen it pushed. Draw none of it, and no inset either: a row's content stands at the cell's margin (16 at each end; UIKit's, 20 in an iPhone's landscape) where you write no padding, the row `border-box` (LLP 1138); write a padding to choose your own. The row is your box; a border you write on a row is yours and replaces the separator there (`border-bottom="none"`: none), and `appearance="none"` keeps rows of your own (a transcript, a feed of cards). The Mac shows a table, the web and the Linux desktop one line between rows, Android none (LLP 1115.000 D16) |
| `delete=remove(id)` on either list's row (`disabled` beside it: no delete) | the cell's swipe action Delete, which a full swipe performs (`contract vocab swipe`) |
| `input type="checkbox" switch` | `UISwitch`; in a grouped list's row, the row and its switch are one VoiceOver element, as a Settings row is, named by the row's `aria-label`, else the switch's, else the row's text |
| `input type="radio"` (the radios of one `name` are a group) | the SF Symbols circle, filled at its centre when checked, at body size; in a grouped list's row no circle but the row's checkmark, which a tap on the row moves (LLP 1115.000 D13.2) |
| `input type="range"` (as wide as what holds it; in a `row`, `flex=1 min-width=0` beside its label) | `UISlider`. VoiceOver's swipe up or down moves it by a written `step`; with no `step`, by a twentieth of the span and at least 1; under `step="any"`, by a twentieth of the span. For a move of your own and a free drag (a seek bar: 10 s a swipe), write `step="any"` and a `key` handler that changes the value on the arrows and calls `preventDefault()`. The swipe is that keydown first, as a screen reader's step is on macOS, Android and Windows and as a keyboard's arrow is: ArrowRight raises and ArrowLeft lowers (swapped where the range's `direction` is `rtl`), and a prevented one leaves the move to the handler. In a test on iOS, `type "seek" key "ArrowRight"` (or `"ArrowLeft"`) is the swipe: the slider's own accessibility increment or decrement, whichever sends that key, with the slider's step where nothing prevents it. Its `down` and `up` halves are that step's keydown with the move, then its keyup, and `for <ms>` repeats the move as a held key does. A range with `appearance="none"` shows no `UISlider`, and the arrow at it is a keyboard's: its `key` handlers hear it and the host moves nothing. On the other hosts the same line is a keyboard's arrow, which runs the same handler; there a screen reader's step is covered by the host's own tests, and on a Mac with keyboard navigation off an arrow nothing prevents does not move the slider. On the web, VoiceOver in iOS Safari steps a range itself and sends no key |
| a box you draw with `role="slider"` or `"spinbutton"`, `aria-valuenow` (and `aria-valuemin`, `aria-valuemax`, `aria-valuetext`), `tabindex=0` and a `key` handler for the arrows | one adjustable element. VoiceOver's swipe up and down (and TalkBack's and Windows' adjust) are a keydown and keyup at it, as Safari sends them to an ARIA slider: ArrowUp and ArrowDown for a spin button or an `aria-orientation="vertical"` slider, else ArrowRight and ArrowLeft (swapped where the slider's `direction` is `rtl`). The handler changes the value, so a screen reader needs no − and + buttons |
| `input type="date"`, `"time"`, `"datetime-local"` | `UIDatePicker`; with an empty `value` its pill shows your `placeholder` (`placeholder="Add a date"`) or, with none, no words (LLP 1115.000 D13.1). Opening it accepts the current local date or time within `min`/`max`. On macOS, typing, an arrow or Return accepts the field's displayed value; focus alone leaves it empty. The web shows the browser's own empty field. Android shows Material's filled field, the value as the phone writes dates and `placeholder` its floating label, and Material 3's picker, in the control's `accent-color` where it has one (LLP 1115.001.000) |
| `select` of `option`s (no wider than what holds it, however long its options) | a pop-up button with its menu |
| `progress value=done max=goal` (a goal, an upload; as wide as what holds it) | `UIProgressView`, tinted by `accent-color` (LLP 1116 D8); never a drawn track and fill |
| `progress` (no `value`) | `UIActivityIndicatorView`, `.large` from a 37-point box (LLP 1069.001); on Android Material's circular indicator, turning (LLP 1115.001 D9) |
| `popover="auto" role="menu"` of `button`s, opened by `popovertarget` (a row whose `popovertarget` names another menu: its submenu; one naming any other popover opens it from the menu's own invoker, Exact's menu rule; a `role="group"` box of rows with an `aria-label`: a titled section) | `UIMenu`, nested; a section an inline `UIMenu(title:)` (LLP 1021) |
| `dialog role="alertdialog" aria-label="Remove book?"` of a `text`, its action(s) and one Cancel; or a `text` and one closing button alone | `UIAlertController(.alert)`: the label its title, the text its message; the one-button alert when it has no action (LLP 1115 D6; recipe [below](#a-confirmation)) |
| a `dialog` whose own `background-color` or `background-image` you write (your panel, your colours), of any rows | HTML's top layer (LLP 1035.001 D11): presented full screen over whatever is up, a sheet included, with no transition of UIKit's, laid out as written and centred in the window, over UIKit's 20% dimming (a `::backdrop` block's `background-color` in its place, below). Its `animation` is its entrance; removed by an `if`, it stays until its `-exact-exit-animation` ends. `closedby="any"` closes it on a tap outside its box, Escape and VoiceOver's escape are its close request, and a field in it is centred above the keyboard. A box in the page with `z-index` stays under every sheet: use this |
| any other `dialog` (a field, Save, Cancel), opened by `showModal(id)` or a button's `command="show-modal"` | a sheet as tall as its content, up to the large detent, its content laid out as written and centred across it. The `dialog` has `fit-content` width, as in HTML. Put `width="100%" box-sizing="border-box"` on the `dialog` itself to fill the sheet between its side insets; a percentage width on a child alone resolves against the dialog's width. `close(id)`, its `command="close"` button or a swipe down closes it. `closedby="none"` refuses the swipe, `"any"` also takes a tap on the page behind, and a dismissal the person made fires `cancel` (LLP 1115.000 D11.5). A `header` as its first child is the sheet's bar, below |
| a `dialog` of that kind whose first child is a `header` of one heading, the text after it and buttons | a sheet in a navigation controller, as a hand-built form sheet is (LLP 1141.004 D6): the heading the bar's title (large at `aria-level=1`), the text after it the subtitle, its buttons before the heading leading and those after it trailing, a `buttonStyle="filled"` one prominent; a button that only closes the dialog (`commandfor` naming it, `command="close"`) is an item, its Cancel. A header that also holds a search field, a tablist, a readout or anything the bar has no item for stays in the dialog as written |
| `role="tablist"`: each tab a symbol over a label / one text or image | `UITabBar` / `UISegmentedControl`, the tablist at least its native height unless `min-height` says otherwise (LLP 1059) |
| `role="radiogroup"` of `button role="radio" aria-checked=…`, each one text or image (a value chosen from a few) | `UISegmentedControl`, as a tablist's; never a tab bar (LLP 1116.004 D5.2). Android draws Material's segmented buttons (two or more radios: equal shares of the row, the checked one filled with a check), and the Linux desktop and Windows the web's track with the checked radio raised on it. Keys of your own design: a radio that paints itself (its own background, border or radius, LLP 1104 D2), or `appearance="none"` on the `radiogroup`, keeps its buttons as you paint them; on the web, iOS, the Mac and the painted hosts (Linux, Windows and Android’s reader), the group keeps one Tab stop (the checked eligible radio, else the first) and arrow keys that press the next or previous eligible radio (Left and Right swapped right to left); on tvOS the focus engine moves between them |
| a route whose first child is a `header` holding one heading (`text role="heading"`; an `aria-level` alone is not one), a text after it and its buttons | the navigation bar; a level-1 heading (`aria-level=1`) is a large title, in a sheet too, and any other level an inline one; the text after it the subtitle |
| a readout of that header, outside a box around the heading (`role="status"`: words, symbols, dots and one `progress`) | an item on its side of the heading: `UIBarButtonItem(customView:)` holding the element's own views, laid out as written and live (LLP 1141.004), in the bar's type and ink where it says none, with no glass (`backgroundMaterial` its own) and in UIKit's "⋯" menu its name, dimmed. Any other element there (a spinner or a picture alone, a second text, a switch) fails the build of an app that ships iOS (`lower-header-item`), naming the repair. Android's top app bar and tvOS's bar leave a readout out, and the Mac draws the header in the page for one holding a spinner (warning `header-item-left-out` for the targets an app ships); an automatic size at most a third of the bar wide and 44 points high; desktop Linux and the web draw the header as written |
| a route whose first child is a `header` holding buttons and no heading (readouts, a search field, a tablist and plain boxes around them too, nothing else) | the navigation bar with no title, its buttons trailing and the root's `navigationBack` control leading; a text or an image in it keeps the whole header in the page (`lower-header-no-bar`) |
| a pushed route with no such `header` | a bar holding the system Back button alone (LLP 1115.000 D5.2) |
| a route with `navigationBar="none"` | no navigation bar: a full-bleed screen that draws its own way back; the edge swipe still goes back unless the route's control named by `navigationBack` is disabled; [disable that control during a run](recipes/navigation-stack.md) to refuse Back |
| a route with `navigationPresentation="modal"` | a sheet |
| a route pushed while a sheet is on top, with no `navigationPresentation` of its own | pushed inside the sheet, with the system Back; the sheet's swipe down, or a tap behind a `closedby="any"` one, closes the sheet with every route pushed inside it |

On iOS, a system dialog sheet can use the same resting heights as a modal route.
For a form that opens tall and can be dragged down to half height, write
`navigationDetent="medium large" navigationInitialDetent="large"` on the
`dialog`. A sheet with several heights shows a grabber by default.
`navigationGrabber="visible"` or `"hidden"` chooses its visibility.
A dialog that omits `navigationDetent` fits its content.

A native `select` fits its widest option by default, as on the web
(LLP 1069.001 D3). Write `field-sizing="content"` to fit the shown value, as a
UIKit pop-up button does, on either Apple host or the web. These are intrinsic sizes; authored widths and
minimum or maximum widths still size the control's layout box.

What lies over the page beneath a modal `dialog` is the platform's dim:
UIKit's beneath a sheet or the top layer, the web's and the painted hosts'
scrim. To colour it, write CSS's `::backdrop` as a block under the `dialog`
(its value may compute) or in a `style`; it admits `background-color` alone,
with its own alpha, so `transparent` lays nothing over the page and a
full-window panel that paints its own dim is not dimmed twice. The dim fades
in as the dialog opens and out as it leaves (LLP 1035.001 D11; `contract
vocab ::backdrop`):

```contract
component App
  view
    column
      button command="show-modal" commandfor="panel"
        text "Open"
      dialog id="panel" width="100%" height="100%" background-color="#0000008C"
        ::backdrop
          background-color="transparent"
        text "Panel" background-color="#14171D" color="#FFFFFF" padding=24
```

A grouped list section's `box-shadow`, including class and state values, paints
around its card, excluding the header and footer. A section without a card
(`background-color="transparent"`) refuses a shadow. Shadow changes support
`transition`; keyframe shadow animation on a grouped section is not supported
yet and fails the build. If shadow keyframes are declared, a grouped section's
animation must name its keyframes with a literal or a choice of literals so the
compiler can check them; their timing may be computed, and an animation may
be absent. Animations that do not set `box-shadow` remain on the section. On
iPad, pointer hover over a grouped section does not activate its `:hover` shadow;
forwarding from UIKit’s visible cells, header and footer is not implemented yet.

On macOS, date, time and datetime-local controls are default Tab stops whatever
the system's Keyboard navigation setting, as `NSDatePicker` is; a select is one
only with the setting on, as `NSPopUpButton` is. `tabindex=0` makes any of them a
stop in both modes. Their native control owns focus and editing keys;
`focus`/`blur` and cancellable `key` handlers still address the Contract node.
Disabled, inert and hidden controls stay out of the loop; `tabindex=-1` allows
explicit focus while excluding sequential focus.

`contract vocab <name>` lists each one's props. A route does not scroll by
itself: its content goes in a `scroll`, `list` or `overflow-y="auto"` box, which is
the bar's scroller with nothing written (`navigationScroll` names another; "Routes
and web documents"). On Apple, `logs` journals `route-overflow` when the active
route's in-flow content, with the route's bottom padding, passes the route's padding
edge and nothing scrolls it into view. The route's own scroller does; so does the
navigation root's or the native document scroll, for content past the route's border
box, unless the route clips it or sits in a native block of its own. It checks after
first pixel once native layout settles at the window's layout size, leaving smaller
sheet and keyboard offers out. Full-window modals are checked too. A fitting layout
clears suppression so later overflow is reported again.

A sheet's swipe down and a pushed screen's edge swipe press the
route's enabled control whose `id` is the root's `navigationBack`, or, when the route
has none, one outside every route (a header the screens share).
`navigationBack` names that control's `id`, not its action: for example,
`navigationBack="close"` with `button id="close" press=back`. A literal name that
no element in the expanded view has as its `id` earns warning
`lower-navigation-back-id`. A computed name, any computed element id or an
unexpanded component prevents this check. A
matching id somewhere in the view does not prove that Back can reach it: a
control only on an inactive route is not pressed from the active one.
Naming a control is optional:
the navigation root is the app view's first element, and `navigationKey` alone there names none. The agent's
`back` presses it too, as Escape does on a Mac sheet. When a sheet's swipe down should not do what its
Cancel does (Cancel reverts the edits, a swipe down keeps them), give Cancel another `id`:
a swipe down then dismisses without pressing it, and on the Mac Cancel stays in the header,
out of the sheet's Cancel slot. A route with no such
control still has the platform's back button, edge swipe and swipe down (and the
browser's Back): they go back through the root's `navigate` handler with the location
beneath (`nav = go(nav, url)` pops to it), or, with no handler, the runner pops the
router itself, as `nav = back(nav)` would (LLP 1115 D5). On the web the browser's
Forward and a `link` to a declared route do the same with no handler: the runner goes
there itself, as `nav = go(nav, url)` would (LLP 1115.000 D4.3); a native host's link
still needs the handler. A web page reloaded on a deep route launches with the page's
earlier visits beneath it, as the runner's own `go` to each in turn leaves them, so the
browser's Back still pops, and the app bar's steps back to the entry beneath rather than
pushing one (games, on the wasm target, launch with the URL's chain alone). A declared but disabled
control refuses them, as does `closedby="none"` on a sheet. Where the app's own Back and
Forward go less far than its router (a palette open over the page, a history capped at
so many visits), bind the root's `navigationHistory` to the ways it offers now (`"back
forward"`, the default, `"back"`, `"forward"` or `"none"`): on the Mac a way withheld is
no swipe, menu item or mouse button (`contract vocab navigationHistory`). A route whose scroller opens
with a full-bleed picture says `navigationBar="transparent"`: iOS draws its bar clear at
the scroller's top and insets nothing, so the picture sits under the bar as under a fixed
transparent header on the web, and the bar's material comes as the content scrolls.
At the clear edge, the iOS bar's empty areas let touches reach the page; its title,
Back and bar items keep their touches. Once scrolled, its material's band takes touches,
as does a visible background supplied by a native hatch.
To keep foreground controls below it, use `top="var(--exact-safe-area-inset-top)"`
or that value as padding on the hero's content. This reads the route's safe area,
including its bar; `env(safe-area-inset-top)` stays 0 under `viewport-fit="auto"`.
A route that does not scroll (a game's canvas at `inset=0`, `width="100%" height="100%"`) is
likewise drawn from the screen's top edge, its bar items floating over it. On a pushed iOS
route under the platform bar, the declared Back control's text becomes the bar's back button title
beside the bar's own chevron (no text shows the chevron alone), so label it `Recipes`,
not `‹ Recipes`. With no such control the back button is UIKit's default, which since
iOS 26 is the chevron alone, the previous title unshown, as in a hand-built app; a title
there is the control's text, said.

Inside a route's `header`, wrap related bar buttons in a `row role="group"`.
iOS 26 keeps each explicit group's buttons on one shared glass background and
separates adjacent groups with UIKit's fixed-space item. Ungrouped buttons keep
the existing platform default. `buttonStyle="glass"` on a bar button gives it a
glass capsule of its own (`sharesBackground = false`), one for each button so styled. The wrappers remain ordinary layout on the web;
this adds no spacing value or custom button face.

A heading of only white space is no title on iOS, and a large title that starts empty
takes no height. To keep that height while the words load, write the heading as
`text "\u{200B}" role="heading" aria-level=1 color="transparent"`, the colour on the
heading itself (UIKit draws U+200B alone as quotation marks), and drop the colour when
the words arrive. Before iOS 26 a route pushed over it with no back control of its own
shows that title on its Back button.

The bar shows a header's heading, the first text after it (its subtitle), one search
field, one tablist and its buttons, and each other element as an item of its own, drawn
as written; of a box around the heading it shows only the title. A
menu or `dialog` written in the header is its opener's, as on the web, not bar buttons.
On iOS the search field scrolls away with the content while it is empty, as UIKit's
does; `position="sticky" top=0` on the `input` keeps it in view (React Navigation's
`hideWhenScrolling={false}`). The route's scroller runs under the bars wherever its
menus, popovers and `dialog`s are written: they take no place in its flow.
The agent paints the authored header by default, but marks such a node in `tree` and
`screenshot`, and an `expect` or `tap` on it fails, saying so. A `testId` on a bar
button, the search field, a segment or a tab is its `accessibilityIdentifier`, which
XCUITest and Maestro address.

Say what a thing is and leave how it looks unsaid: no `background-color`, `height`,
`font-size` or `padding` on these, or the host draws your box instead of its
control (LLP 1115). Worked forms, each compiled:

```text
// The navigation bar: buttons before the heading are leading items, after it
// trailing; a popovertarget button is a menu item (⋯), a search input the bar's
// search field, a tablist the segmented title.
column navigationKey=`${e.id}` navigationScroll="inbox-list" position="absolute" inset=0 display="flex" flex-direction="column"
  header
    text "Inbox" role="heading" aria-level=1
    row role="tablist"
      button role="tab" aria-selected=(filter == "all") press=show("all")
        text "All"
      button role="tab" aria-selected=(filter == "unread") press=show("unread")
        text "Unread"
    input type="search" value=query input=search placeholder="Search"
    button popovertarget="inbox-menu" aria-label="More"
      image "symbol:more"
    button press=compose aria-label="Compose"
      image "symbol:compose"
  column id="inbox-menu" popover="auto" role="menu"
    button press=compose
      text "New Message"
  // Pull to refresh: a `refresh` handler on the scroller; `refreshing` holds the
  // spinner until the reload settles. A row's `delete=` is its swipe to delete.
  // A first load is a `progress` beside the list while it has no rows, never in a
  // row: a spinner inside a cell is something a hand-built iOS list never shows.
  list id="inbox-list" refresh=reload refreshing=pending(items) flex=1 min-height=0
    for m in items key=m.id
      button press=open(m.id) delete=remove(m.id)
        text m.title

// A subtitle: the first text after the heading (UIKit's own on iOS 26).
column navigationKey=`${e.id}` navigationScroll="list" position="absolute" inset=0 display="flex" flex-direction="column"
  header
    text "Shopping List" role="heading" aria-level=1
    text `${left} ${left == 1 ? "item" : "items"} left`
    button press=add aria-label="Add Item"
      image "symbol:add"
  scroll id="list" flex=1 min-height=0
    …

// A sheet's bar: Cancel is the root's navigationBack control, which a swipe down and the
// agent's `back` press; Done trails.
column navigationKey=`${e.id}` navigationPresentation="modal" navigationScroll="form" position="absolute" inset=0 display="flex" flex-direction="column"
  header
    button id="back" press=back
      text "Cancel"
    text "New Message" role="heading"
    button press=save
      text "Done"
  scroll id="form" flex=1 min-height=0
    …
```

`action reload` is `refresh items`; `pending(items)` is true while it runs.
A segmented control in the content chooses a value or switches panels: a value
from two to five (a preset, units, a period, a filter) is a `row role="radiogroup"` of
`button role="radio" aria-checked=…`, each pressing its value (the web and painted hosts give it
arrow keys and one Tab stop); switching panels, a `row role="tablist"` of text tabs
outside the header. A list row's `delete=action` is its swipe to delete, UIKit's on
iOS (`contract vocab swipe`). Other swipe actions are a hand-written swipe row, a
`scroll` laid out as a row (`display="flex"`) whose content iOS needs exactly the
scroll's size ([pitfalls](agent-pitfalls.md), "A hand-written swipe row shows its
buttons under the row"). Its swipe actions
are UIKit's, drawn by UIKit (rounded on iOS 26 and later) whatever the buttons
paint; when every control the row names says `appearance="none"`, the paint is
kept instead, in a grouped list too: the row swipes by its own scroll, as on
the web, with one row open at a time (any other swipe row's opening closes it,
and its drag closes the other's, a grouped list's row included; a touch elsewhere
in the scroll that holds it, or that scroll's drag, closes it, and the tap that
closes it presses nothing, as in a UIKit table),
a full swipe past half the row pressing the outermost action
(a haptic as it arms), and each action a VoiceOver custom action of the row's
content.

An `image` source is the same string on every host: a path under the app's
`assets/`, an `http(s)` URL, `symbol:<role>` (the roles are
[`schema.json`](../kernel/tables/schema.json)'s `symbols`; a player's are `play`,
`pause`, their `-fill`s, `skip-back-15`, `skip-forward-15`, `skip-back-30`,
`skip-forward-30`, `speaker`, `speaker-mute` and `moon`; an unknown role is refused
with the list), `symbol:sf/<name>` (an SF Symbol by its Apple name: drawn on Apple
only, blank on the web and Linux, where `layout` reports `reason: "platform"`; a
build of an app that ships either warns, its first use with the repair and each
later one in a line; a tab or button that must show everywhere takes a role or an
icon token), an `app:/data|cache|tmp/…` file
(a picked photo, or one the data module kept with `storage.fs`; it shows after a
relaunch too), or a `data:` URL of at most 1 MiB, past which every host shows
nothing (the web and Apple journal `image refused`).

An `iframe` of a page under `assets/` or `deck/` is given an `app:/cache` or
`app:/tmp` file its `src` names in its query, encoded as any query value is:
`` src=`assets/viewer/index.html?file=${encodeURIComponent(path)}` ``. The page
reads `new URLSearchParams(location.search).get('file')` and gets a URL it can
fetch, of any size, until the frame's `src` changes. A value naming `app:/data`, a
link or a missing file is emptied (`file=`), the frame loads as usual, and the host
logs why; copy the file into `app:/cache` first. An HTML, SVG or XML file is given
as `application/octet-stream`, so it never runs as the app: read its bytes, into a
Blob of your own for an SVG `img`, or into a `srcdoc` frame under a `sandbox`
without `allow-same-origin` for HTML. The web, macOS and iOS give files; Android
does not (LLP 1020.001 D7). Shrink a picked photo for an upload limit with
`storage.fs.compressImage(path, to, {maxDimension, maxBytes})`, which writes an
upright JPEG with no location metadata (`quality`, 0 to 1, sets its quality, with
or without `maxBytes`; `type: "image/png"` writes a PNG that keeps transparency; [Exact developer reference](reference.md#shrink-a-picked-image-for-upload-storagefscompressimage));
Linux answers `unsupported`. Save an edit (a turn, a flip, a crop, a size) with
`storage.fs.transformImage(path, to, {rotate, crop})`, which keeps the photo's colour,
HDR and metadata where the host can and says what it wrote
([Exact developer reference](reference.md#save-an-edit-of-a-photo-storagefstransformimage));
`compressImage` takes `rotate`, `flip` and `crop` too. Upload a file with `fetch(url, {method: "POST", headers:
{"content-type": "image/jpeg"}, exactBodyFrom: path})`, not `readFile` then `body`: the host
streams the file from disk as it sends (under `fs.read`, with no size bound except 64 MiB on the
web for a file the app wrote itself), so a 2 MB photo never passes through the answer's 100 ms step
and a long video costs no memory; a missing or denied file rejects the fetch, naming why. To show an
upload's percentage while the person goes on, make it a resource's stream with `exactUploadProgress: true`:
its mapper gets `progress` (`loaded` of `total` bytes sent), then `load` with the reply's `status`, and no
answer waits for it. It is asked once for its arguments while the app runs, across a reload too, though not the
web JS target's development reload (a refresh or store write never sends it again, and one cut off after it began
comes back failed, `interrupted`), so a retry puts an attempt number in its arguments and in its URL or a header:
new arguments that make the same request keep the running upload. A launch starts afresh and sends again an upload the view still shows, so record what it answered (the
server's id for the attachment) in the store and stop showing the resource once you have it
([Exact developer reference](reference.md), "An upload that tells its progress"). Download a file with
`fetch(url, {exactBodyTo: path})`, not `arrayBuffer()` then `writeFile`: the host writes a 2xx body into the
file as it arrives (under `fs.write`, at most 1 GiB), so a large file never enters the engine's 64 MiB heap;
`response.exactBodyTo.bytes` is its size, and a reply that is not 2xx leaves the file untouched and reads as
usual. Keep a picked photo by copying it to
`app:/data` and answering that path; never tell hosts apart in the data module
(`HermesInternal`) to choose a source
(LLP 1069.002 D7, LLP 1011 §2).

An `image` hears HTML `<img>`'s two events, once per source, on every host: `load=` when the picture
is ready (no payload), and `error=` when it does not
load, with a `message` that says why (`error=failed` runs `action failed(message:
string)`; `error=failed("icon")` passes `"icon"` before it). Natively the
message is the reason (`HTTP 404`, `Could not connect to the server.`, `not an image
format this host decodes`), and a link it quotes has no query; the browser gives none, so the web says `the image did
not load`. On every native host a download that asking again may mend (the network, a lookup, TLS, a timeout, the spool,
a 5xx, 408 or 429, an empty body) is asked again while the picture is on screen, after 1, 2, 4, 8, 16 and 30 s
and never sooner than `Retry-After`, so its `error` comes once those are spent (about a minute of
waits, plus the downloads and any `Retry-After`), and one that fails off screen errs only after it has shown (LLP 1011 §4). A symbol fires neither. What each host's fetch of an `http(s)` source
sends, follows and accepts is LLP 1011's: macOS and iOS send no cookie and no
`Referer`, follow redirects to any host (within App Transport Security), take a
body up to 64 MiB whatever its `Content-Type` and its status, as a browser's `<img>` does
(a 404 that is an image draws; one that is not is `HTTP 404`), and keep an HTTP cache on disk;
the web is the browser's `<img>` (its cookies and `Referer` rules, no size cap). Apple
and Linux also draw SVG images, with scripts, external references and animation dropped.
The compiler checks literal `assets/…svg` and `deck/…svg` image sources with that parser:
a dropped construct warns at the source; a refused file fails a build that ships a native
target. Editing or creating a named file rebuilds the plan. Raster bytes keep their format
even under an `.svg` name.
Files unavailable at build, and remote, `app:/` or computed SVG sources, are checked at
run time. A tinted remote image on the web
needs CORS headers, or it is an `error` too. A terminal entry reads a PNG from a path
beneath its own directory, an absolute path or a `data:` URL, a regular file of at most
64 MiB; it downloads nothing and has no app directories yet, so an `http(s)` or `app:/`
source is an `error` there. A drive sees both after `clock +<ms>
real`; `clock settle` can return just before them.

### Announcing a change

There is no announce call. A screen reader reads a live region as its text changes: a
`text role="status"` (or `aria-live="polite"`) is polite, asking to wait for current
speech, and a `text role="alert"` (or `aria-live="assertive"`) asks to interrupt it.
`aria-live` wins over the role's, and `aria-live="off"` silences a `status`. To announce
a result, a form's error or "3 selected", keep such a `text` mounted, empty until there is
something to say, and change its text ([recipe](recipes/announce.md)):

- Put the role on the `text` itself. Apple hosts read the text under a live box, but on
  Windows a box is read by its own accessible name, which it takes only from an
  `aria-label`, so a `row role="status"` whose child text changes says nothing there;
  Android sends the change from the box, and what TalkBack reads of it is untested.
- Leave `aria-label` off the region: on Android and Windows it is what is read, and a
  change of the text alone is not.
- Text the region already had is not read again. One that arrives holding its text is read
  on some hosts and not others (below), so mount it first.

| Host | What it does |
| --- | --- |
| iOS, iPadOS, tvOS | `UIAccessibility.post(notification: .announcement)` with the region's text, queued behind current speech when polite. An `alert` that arrives is also announced, except one already there when the app launches. A region under `aria-hidden` says nothing |
| macOS | `NSAccessibility.post(… .announcementRequested)` with the text, at low priority when polite and high when assertive; an arriving `alert` as on iOS. VoiceOver may still finish the focused control's speech first |
| Web | the browser: the element carries its `role` and any `aria-live`, and the screen reader treats it as any page's live region |
| Android | the node is a polite or assertive live region (`AccessibilityNodeInfo.setLiveRegion`), and a content change is sent from it when its accessible name changes and, usually, when it arrives; that is what TalkBack reads a live region by. TalkBack itself has not been heard reading one |
| Windows | AccessKit's UI Automation adapter: the node's `LiveSetting`, and a `LiveRegionChanged` event when its name changes or it arrives with one. Not yet heard from Narrator |
| Linux | nothing yet: the Linux host serves no screen reader (no AT-SPI) |
| watchOS | nothing: the watch entry refuses `aria-live`, and its SwiftUI driver posts no announcement for a `status` or `alert` |

### Sound, MIDI and media

A sound effect is a declared WAV that an action plays (LLP
1096): `sound "assets/…wav"` at the
top level (16-bit or float PCM, one or two channels, at most 10 s; the compiler
reads it), then `playSound(src, at=, gain=, group=, playbackRate=, pan=)` from any
action. A path built at run time (`` `assets/kits/${kit}/kick.wav` ``) plays only the
files a `sound` line declares, so declare each one it can name. Every call is a new
voice, so a retrigger is another call. `at=` is the runner's clock
(`performanceNow()`'s milliseconds; the past means now), `gain=` a linear 0–1, and a
`group=` is monophonic by start time: a voice ends where the next one in its
group starts, as a drum machine's choke does. `playbackRate=` (0.25–4, Web Audio's
name) moves pitch and length together, as tape does: at 2 a voice is an octave up and
lasts half as long. `pan=` places it from −1 (left) to 1 (right) by Web Audio's
equal-power law (LLP 1125
D10). `stopSounds()` (or `stopSounds(group=…)`) ends what
sounds and cancels what waits. `setSounds(group=, gain=, playbackRate=, pan=, ramp=)`
changes what sounds and waits, in the group when one is given, without starting it again:
each value given moves in a straight line from where it is to the new one over `ramp=`
milliseconds (20 if left out, so a value set at every `pointermove` glides; 0 is a step),
and a voice not yet started takes it outright (LLP 1096 D14). A sound that follows a
drag starts with `playSound(…, group="drag")` on `pointerdown` and changes with
`setSounds(group="drag", …)` on each move. A rate change moves the voice's end by
what is left of its file, and a voice that had no `pan=` is panned from then on,
so give one that will pan its starting pan in `playSound` (a mono sound is 3 dB
quieter once panned, even centred). The web plays the first sound after the page's first
tap or key. Android plays through an arm loaded at the first sound, sharing Apple's
mixer. Its `audio_session` defaults to `"ambient"`: mixing, no audio focus, and
silent while the ringer is silent or vibrating. `"playback"` takes focus;
`"playback-mix"` mixes without ducking other audio. Android stops effects in the
background and when headphones are unplugged; the next foreground effects open
the output again. Linux and Windows keep the record and play nothing.

```contract
sound "assets/ding.wav"

component Ding
  action ding
    playSound("assets/ding.wav")
  view
    button "Ding" press=ding
```

To keep time (a metronome, a drum machine, an interval timer, a rhythm game), declare
what should play with a `sequence` and let the runner play it (LLP
1125 D2–D5); do not
start hits from a timer ([a step sequencer](recipes/sequencer.md): lanes of their own
lengths, a kit, pause and resume, MIDI). It has a task's gate, and one `play(tracks,
bpm=…)`. A track loops at its own `beats`, `loops` times or forever (0), so tracks of
different lengths run against each other; a step is at a `beat` (a sixteenth is
`0.25`) and plays a sample, a haptic, or both. The runner issues the steps into
the voice table a second ahead on its own clock, so no action runs per step and a
busy frame does not move a beat. The arguments are read again after every commit:
change the pattern or the tempo in state and it plays from that commit, the tempo
without a jump in phase. `from=-4` with `countIn=` is a count-in; `ended=` runs an
action when a finite take ends, at that time:

```text
shape Step
  beat: number
  src: string
  gain: number
  group: string
  note: number
  velocity: number
  channel: number
  duration: number
  haptic: string

shape Track
  beats: number
  loops: number
  offset: number
  output: string
  steps: Step[]

fn click(beat: number, gain: number): Step = Step(beat=beat, src="assets/click.wav", gain=gain, group="click", note=-1, velocity=100, channel=0, duration=0.1, haptic="")

component Metronome
  state playing = false
  state bpm = 120
  resource transport = exactTransport() as shape Transport
  derive bar = [Track(beats=4, loops=0, offset=0, output="", steps=[click(0, 1), click(1, 0.5), click(2, 0.5), click(3, 0.5)])]
  sequence beat when playing
    play(bar, bpm=bpm, from=-4, countIn=[click(0, 1), click(1, 1), click(2, 1), click(3, 1)])
```

The shapes are flat and positional, as `playSounds`'s hit is: "none" is `""` or
`-1`. Keep the tracks in a `derive`, so a commit that changes nothing else
compares them at once. `exactTransport()` says what plays (`state`, `reason`,
`bpm`, `anchorAt`, `anchorBeat`); the beat now is `anchorBeat + (performanceNow() -
anchorAt) * bpm / 60000`. A playhead or beat light follows it as an animation, not a
frame task: a loop whose opacity is a computed `linear()` curve on a clock timeline,
re-read by a task keyed on the anchor ("Following the transport", below), so no app
code runs while it plays. A frame task is for text that changes
with the beat, gated on it being shown, whose action writes the beat, not the time, so a
frame that moves no beat commits nothing. The gate turning
false stops it; `stopSounds()` silences what sounds and the next steps still play.
For one or two scheduled hits, `playSounds(hits)` with `at=` is enough (a list of a
shape whose fields are, in order, `src`, `at`, `gain` and `group`, then optionally
`playbackRate` and then `pan`).

**Following the transport.** A playhead or a beat light that follows `exactTransport()`
is an animation, not a frame task: each light loops over its cycle (a bar, a lane) and
Core Animation or the browser plays it, so no app code runs while the music plays.
`level` passes a `linear()` curve's output through as the opacity, so a computed curve
says when in the cycle the light is lit; a negative delay puts the cycle where the
transport is; a clock timeline keeps a light mounted later in the same phase.

```contract
timeline Transport
keyframes level
  from opacity=0
  to opacity=1

shape Transport
  state: string
  reason: string
  bpm: number
  anchorAt: number
  anchorBeat: number

shape Step
  beat: number
  src: string
  gain: number
  group: string
  note: number
  velocity: number
  channel: number
  duration: number
  haptic: string

shape Track
  beats: number
  loops: number
  offset: number
  output: string
  steps: Step[]

fn pct(t: number, cycle: number): string = `${round(t / cycle * 10000) / 100}%`
// Lit through the first `span` ms of each `cycle`, a cycle starting `start` ms along the
// beat grid; `pos` is where the transport was when the loop mounted.
fn lit(span: number, cycle: number, start: number, pos: number): string = `level ${cycle}ms linear(1 0% ${pct(span, cycle)}, 0 ${pct(span, cycle)} 100%) ${0 - (((pos - start) % cycle) + cycle) % cycle}ms infinite`

component Beats
  state playing = false
  state pos = 0
  state synced = ""
  resource transport = exactTransport() as shape Transport
  derive anchor = `${transport.anchorAt} ${transport.anchorBeat} ${transport.bpm}`
  // `transport.bpm` is 0 before anything has played; a derive may not be Infinity.
  derive beat = 60000 / max(1, transport.bpm)
  sequence click when playing
    play([Track(beats=1, loops=0, offset=0, output="", steps=[])], bpm=120)
  // Where the transport is, read once per play, seek or tempo change.
  task follow when transport.state == "playing" key=anchor
    after(1, sync)
  action sync
    pos = transport.anchorBeat * beat + performanceNow() - transport.anchorAt
    synced = anchor
  action toggle
    playing = not playing
  view
    column padding=16 gap=8
      button press=toggle testId="toggle"
        text (playing ? "Stop" : "Play")
      row gap=8 role="img" aria-label="Beat"
        for i in range(4) key=i
          view flex=1 padding=4 border-radius=4 position="relative" background-color="-exact-tertiary-fill"
            if transport.state == "playing" and synced == anchor
              for g in [synced] key=g
                view position="absolute" inset=0 border-radius=4 background-color="AccentColor" opacity=0
                  animation=lit(beat, 4 * beat, i * beat, pos) animation-timeline=Transport
```

- **What stays a frame task:** text that changes with the beat ("Count-in 3"), gated on the
  transport states that show it. Its action writes the beat, `floor(anchorBeat +
  (performanceNow() - anchorAt) * bpm / 60000)`, never the time: the runner compares state
  after an action, so a frame on which the beat has not changed commits nothing and draws
  nothing (`apps/metronome/app.contract`'s `readout`). Read the position in an action (a
  pause's) as `anchorBeat + (performanceNow() - anchorAt) * bpm / 60000` beats.
- **A light that is not just on or off** puts every stop in the curve. Over a four-step cycle,
  a trail ember lit the step after the head and fading from 0.4 to 0.18 through it is
  `linear(0 0% 25%, 0.4 25%, 0.18 50%, 0 50% 100%)`; a fade after the beat is a few stops
  sampled from its easing.
- **A one-off change at a beat** (dim two beats after a touch) is a finite animation whose delay
  is that beat's distance from now; a second, identical `keyframes` name restarts it, as CSS
  restarts an animation only when its name changes.

Tests read the runner's record: `expect sound has "assets/ding.wav" at 0`, with any
of `gain`, `ends`, `playbackRate`, `pan` and `by end|group|cut|stop|cancelled|edit`, or
`expect sound missing …`; a sequence's steps are voices like any other, so `clock
+2000` then `expect sound has "assets/click.wav" at 1500` works with no audio, and
`expect state transport.state == "playing"` reads the transport.
`state sounds` lists the last 64 voices, `state transport` the transport.

MIDI is Web MIDI's, and the runner keeps its queue as it keeps the voices (LLP
1125 D6–D8). Grant `device.midi purpose.midi`, a line of the data module's grants
(`export const grants` in `app.ts`, `grants()` in Rust) whose second word is a key of
`strings/en.json`: `"purpose.midi": "…"`, the text the person is shown when asked. Read the ports with `resource midi = exactMIDI() as
shape MIDI` (`state`: `prompt`, `granted`, `denied`, `unavailable`; `outputs` and
`inputs`, each `{id, name, manufacturer, state}`; `sent`, the last 128 messages that
went out in time order, each `{output, data, timeStamp}`, Timing Clock left out: an activity log; a track whose `output` is `""`, and `sendMIDI("", …)`, go to no device but are listed with `output: ""`, so the log shows what plays with none connected), ask for them from a press with
`requestMIDIAccess()`, and let the person pick an output (a `select` of
`midi.outputs` by `id`). Then a step's `note` plays on its track's `output` (a
note-on, and a note-off `duration` beats later), `clockOut=output` on `play`
sends clock, Start and Stop, and `sendMIDI(output, [144, 60, 100], at=…)` sends
whole messages from an action. Hear a controller with `midimessage=heard
midiinputs=[id]` (the action may take a `MIDIMessageEvent`: `input`, `data`,
`timeStamp`, `beat`). To keep a device's time instead, `clock=input` on `play`
(a `select` of `midi.inputs`): it waits (`transport.state == "waiting"`) until the
device sends Start, then plays at the device's tempo (`transport.bpm`), and its
Stop leaves it waiting again. Nothing to schedule by hand: a host hands each message to the
device 50 ms before it is due, so an edit or a stop takes back what has not gone.
Test it with no device: `prefer midi.outputs "OP-XY"` then `tap "connect"`, and
`expect midi sent "OP-XY" 99 4c 7f at 0` (bytes in hex) or `expect midi missing …`;
`tap midi "keys" 90 3c 64` is a message from an input, `tap midi "keys" clock 125
1000` a second of its clock at 125 bpm; `state midi` lists the
queue ([Metronome](../apps/metronome/app.contract) is whole).

A video is HTML's `video`, not a native view (LLP 1042):
`video "assets/intro.mp4" poster="assets/intro.jpg" autoplay=true muted=true loop=true
playsinline=true object-fit="cover"` is a looping hero, which Apple draws with an
`AVPlayerLayer`. Give it a `poster` (a still of its first frame): AVPlayer takes 50 ms
to a second to draw its first frame after launch, and the poster shows until it has.
Its box is a replaced element's: with `width` and `height` auto it is the source's
natural size (300×150 until the metadata arrives), even when it is absolutely
positioned with `inset=0`, since opposing insets do not stretch a replaced element
(CSS 2.1 §10.3.8, §10.6.5), and an adaptive stream's box changes with each
rendition. To fill a box, write `width="100%" height="100%"` with `object-fit`.

On Apple, the player normally caps HLS renditions to the video's box.
`preferredMaximumResolution="640x360"` sets an author bound below that.
`startsOnFirstEligibleVariant=true` starts from the first eligible variant in
playlist order, bounded by what the box needs and the author bound; a playlist
without rendition sizes uses AVFoundation's normal selection. Playlist order
plus this flag is the start control; Exact does not reorder variants or offer
a `start-quality` attribute. See Apple's
[`startsOnFirstEligibleVariant`](https://developer.apple.com/documentation/avfoundation/avplayeritem/startsonfirsteligiblevariant).

For custom loading on Apple, mark the video `hatch="clip"` and declare that
element hatch for `ios` and `macos` in `app.json`. The app's Swift in
`modules/apple/` receives `elementConfiguration(_:_:)` synchronously before
each item is made, including source changes and `load(id)`, in list rows too.
The configuration is an `NSMutableDictionary`: `url` is the resolved source
`URL`; set `asset` to an `AVURLAsset` with your options or resource loader.
Leaving `asset` unset keeps normal loading. A cold launch waits for hatch
registration. The supplied asset goes through the same rendition bounds and
start preference. Do not load its tracks to warm it: that can select a rendition
before the item exists. At `built`, or `changed` if the player was deferred,
`element.platform` is the `AVPlayer`. Exact owns its item, playback and authored
preferences; configure loading here and add observations afterward.

For example, an app that implements a `PlaylistLoader` delegate can fetch a
multivariant playlist, put its chosen start first, make the remaining URIs
absolute, and serve it under its own scheme:

```swift
import AVFoundation

// In the app's ExactModule subclass; PlaylistLoader is the app's delegate.
private var loaders: [ObjectIdentifier: PlaylistLoader] = [:]

override func elementConfiguration(_ element: ExactElement, _ configuration: AnyObject) {
    guard element.hatch == .clip, let config = configuration as? NSMutableDictionary,
          let source = config["url"] as? URL else { return }
    let loader = PlaylistLoader(source: source)
    let asset = AVURLAsset(url: loader.url)
    asset.resourceLoader.setDelegate(loader, queue: loader.queue)
    loaders[ObjectIdentifier(element)] = loader
    config["asset"] = asset
}

override func elementEnded(_ element: ExactElement) {
    loaders.removeValue(forKey: ObjectIdentifier(element))
}
```

Keep the delegate alive while its asset is in use: the resource loader holds it
[weakly](https://developer.apple.com/documentation/avfoundation/avassetresourceloader/delegate).
A private scheme changes AirPlay and FairPlay loading behaviour; the app must
test those paths with its loader. This video hook is Apple only. Android has no
video player hook yet; the web uses its own media element.

`audio` stays HTML's player for long media
(a song, a podcast: `video`'s props and events with no picture, LLP 1042 §8).

A media source is the string an `image` takes, `app:/data` files included: an
episode the data module downloaded with `storage.fs` plays on every host but
headless Linux, which has no player. An empty `src` fails (`error`
`src-not-supported`), as HTML's does, so render the element once it has a
source. On Apple, a bundled or `app:/` HLS playlist fails with
`src-not-supported`; use a bundled MP4 or serve the HLS playlist over HTTP(S).
A bound `currentTime` seeks when its value changes; to seek to the same
time again (a skip back, "start over", a scrubber let go where it was grabbed)
call `fastSeek(id, seconds)`, which seeks each time it runs. `load(id)` loads the
source again: a retry after an `error`, or a file written since. Both name the
element by its `id`, as `focus` does.

Pictures that are not a video (a page a server streams as JPEG frames, a
camera's stills) float in the platform's picture-in-picture window with
`requestPictureInPicture("id")` on the `image` that shows them: the window
shows each picture its `src` names next, and the image hears
`enterpictureinpicture` and `leavepictureinpicture`;
`exitPictureInPicture()` closes the window. A request that fails is the
image's `pictureinpictureerror(message)`, on every host that cannot float it
too, so handle it where a press asked: a toast, the control unpressed. The
image may be a hidden 1-point one that a stream of its own paints; removing it
closes the window. macOS and the web's JS target float it
(LLP 1042 §9). Leave the window to the platform
unless the app must look like another: `presentation="window"` asks for a
browser's picture-in-picture window as Chromium draws it on the Mac, and
`presentation="panel"` for a floating panel with a title bar, each with
`title=`, `width=` and `height=` (LLP 1042 §9.1); the web shows either in
Document Picture-in-Picture's window where the browser has one. On macOS
each image asked for in a panel gets a panel of its own, while the
picture-in-picture window (the platform's or `"window"`) holds one image
at a time, as a browser's does, and moves to the next that asks
(LLP 1042 §9.2).

To be the system's Now Playing app (the media keys, the lock screen, Control
Center, a headset's buttons, the browser's media hub), give the player the Media
Session's `metadata=MediaMetadata(…)` and bind its actions as the element's events
(LLP 1098):

```contract
component Episode
  state paused = true
  state seek = 0
  state at = 0
  state part = 1
  action ticked(t: number)
    at = t
  action skipBy(sign: number, d: MediaSessionActionDetails)
    seek = max(0, at + sign * d.seekOffset)
  action seekAt(d: MediaSessionActionDetails)
    seek = d.seekTime
  action restart
    seek = 0
  action nextPart
    part = part + 1
  action hostPaused
    paused = true
  action hostPlaying
    paused = false
  view
    audio "assets/episode.mp3" paused=paused currentTime=seek timeupdate=ticked
      metadata=MediaMetadata(title=`Episode 12, part ${part}`, artist="The Show", album="", artwork="assets/art.png")
      seekbackwardOffset=15 seekforwardOffset=30
      seekbackward=skipBy(-1) seekforward=skipBy(1) seekto=seekAt
      previoustrack=restart nexttrack=nextPart
      pause=hostPaused playing=hostPlaying
```

- Every field is named (`album=""` when there is none); `artwork` is one image,
  an `https:` URL or an app asset path.
- The six actions are `setActionHandler`'s names: `seekbackward`, `seekforward`,
  `seekto`, `previoustrack`, `nexttrack`, `stop`. Each offers a last
  `MediaSessionActionDetails` (`action`, `seekOffset`, `seekTime`, `fastSeek`) to
  take or leave. A bound action is an offered control; the lock screen shows no
  skip button for an app that does not bind one.
- `seekbackwardOffset`/`seekforwardOffset` are what the lock screen shows and
  the `seekOffset` when the platform gives none (default 10). Keep an action that
  ignores the record (`seekforward=skip(30)`) at the offset it declares.
- Play and pause are the element's own: the platform plays and pauses the player
  and the app hears `play`, `playing` and `pause`. With `paused` bound, mirror
  them, as above.
- A Mac's previous and next keys send `previoustrack` and `nexttrack`, not the
  seeks: bind both.
- One element owns the session: the claimant that most recently started playing,
  kept while it is paused.
- On iOS the build refuses a claimant without `"audio_session": "playback"` and
  `"audio"` in `host.ios.backgroundModes` in `app.json`. `"playback-mix"` does
  not qualify: a mixable session is never Now Playing.

Tests trigger an action as the platform would and read what is published:
`tap "audio" mediasession "seekto" 600` (the seconds are a seek's `seekOffset`,
`seekto`'s time), `expect mediasession title == "Episode 12, part 1"`, `expect
mediasession has "nexttrack"`; `state mediaSession` is the whole record.

### Markdown editing and colour

A Markdown editor is a `textarea` with `markup="markdown"`; a `text` with it is
the reader (one node; the value stays the source string). Its toolbar is
`retainFocus` buttons calling `format(id, command[, argument])`, and its
`select` event hands an action the formats at the selection, for active states
([Markdown](contract-grammar.md#markdown-markup-format-select): the commands and
the `MarkdownSelection` fields):

```contract
component Editor
  state body = "# Title"
  state bold = false
  action edit(v: string)
    body = v
  action selected(s: MarkdownSelection)
    bold = ` ${s.formats} `.includes(" bold ")
  action embolden
    format("editor", "bold")
  view
    column
      button (bold ? "Bold (on)" : "Bold") press=embolden retainFocus=true
      textarea id="editor" value=body input=edit select=selected markup="markdown"
      text body markup="markdown"
```

A document longer than a screen is one reader per block, never one reader for
the whole document ([long text](recipes/long-text.md)).

Mentions, tags and citations drawn as pills in that editor and reader are
inline atoms: declare `atom` kinds by their source form, list them in `atoms=`,
give each source a face with `atomFaces=` (its label, icon and tooltip, its
colours when one chip is not coloured as its kind is, and its direction when a
chip must read left to right in right-to-left text, `direction="ltr"`, and its
accessible name when the drawn label is not what a screen reader should say,
`ariaLabel=`) and hear `atompress=`
([inline atoms](contract-grammar.md#inline-atoms-atom-atoms-atomfaces-atompress)).
Unsaid, a pill is the platform's token; when the design draws its chips another
way, write that on the `atom` as CSS (`padding-inline`, `border`,
`border-radius`, an icon's size and the gap after it with `-exact-icon-size` and
`gap`, a ring in a `:checked` block, an underline in a `:hover` block) and leave
the rest to the platform.
Keep them out of your own rules over `setSelectionRange` and `beforeinput`: the
editor keeps a caret out of an atom and deletes it whole. An editor where
Arabic or Hebrew lines sit beside English ones gives each line its base
direction with `paragraphDirs`, HTML's `dir` words, one per source line
([Markdown](contract-grammar.md#markdown-markup-format-select)).

Choose a colour by what it means, not by how it looks. **The accent is for
what the user can act on**: a link, a plain button's title or symbol, a filled
button's fill, a toggle's on state, the icon of a tappable row. Write it as
`AccentColor`, never as the colour the accent happens to be today
(`-exact-system-blue`): the platform keeps the accent dynamic and dims it to
grey behind an alert or a sheet to say the screen under it is inert. **A
colour of its own is for what means something by itself**: status
(`-exact-system-green`, `-exact-system-red`), text hierarchy (`-exact-label`,
`-exact-secondary-label`), data, and marks. A colour of its own never dims,
except on a native button, which greys whole as UIKit greys a button, its
symbol's `-exact-tint-color` too. On iOS the tint dims, whoever set it (the
root's `accent-color` or a node's own), as UIKit greys a view's `tintColor`:
controls, bars, text, symbols, fills, shadows, gradient stops and SVG paints in `AccentColor`.
To keep the tint from dimming on a mark that must follow an accent the app
cannot write as a value (a theme's, or one the person picks), write
`-exact-tint-adjustment="normal"` on it or an ancestor (UIKit's
`tintAdjustmentMode`). On a standard list row write it on the `row`, whose cell
UIKit draws the image in; on the image it does nothing. A bar, its items and a
tab bar keep the mode of the views UIKit holds them in, so they grey with the
screen whatever their nodes say.
Set the app's accent once, on the root:
`accent-color` on the root element is the app's accent, and on iOS it is the
window's tint, as an AccentColor asset is, so every `AccentColor`, bar, sheet
and alert follows it. On Apple hosts only, an app may instead ship its accent
as an `AccentColor` colour set in its asset catalog
(`modules/apple/*.xcassets`, below): the build names it in the app's
Info.plist (`NSAccentColorName`, as Xcode does), and it is the accent wherever
the root's `accent-color` is `auto` or absent (UIKit applies it as every
view's tint; macOS's AppKit uses it where the user's system accent is
Multicolour). The web, Linux and Windows never read it: there the root's
`accent-color` is the accent the controls take, and the web's `AccentColor`
keyword stays the browser's, so an app that wants one accent everywhere writes
it on the root.

Keep `id` and `testId` separate:

- `id`: host command target, geometry, cross-node references.
- `testId`: driver/test target and stable inspection name. On the web it is
  `data-testid`, not the DOM `id`: a brief that asks for element ids wants `id=`
  (and `testId=` too where tests target the node).
- `for` key: data identity for row lifetime, not either node id.

Events bind actions (`press=save`, `input=edit(item.id)`). Captured arguments come
before host payload arguments. Do not add an event object or a JavaScript closure.
`navigate` is special: no captured arguments and zero or one location parameter.
`transformgeometry` and `transformrelease` must be paired. The complete payload
matrix is in [events](contract-grammar.md#events).

Known host commands are statements, not expression-returning functions. Several
commands have specialized argument checks; some generic commands leave validation
to the host. Do not infer that compiler acceptance proves a command's arguments
work. Use the command's working fixture and inspect its host result.

`relaunch()` restarts the standalone macOS process after its orderly quit,
including the storage drain; it works with the development menu off. A
`beforeunload` cancellation cancels the restart. The web reloads the page.
iOS/tvOS, Linux and native agent sessions journal a refusal; for a scripted
restart, start another drive on the same `--storage`. See [host commands](contract-grammar.md#host-commands).

### Bar buttons, glass and the search field

- iOS 26 puts adjacent ungrouped bar buttons on one glass capsule. `buttonStyle="glass"`
  on each gives it a separate capsule of its own; a `row role="group"` keeps some on one.
- A `popovertarget` button is the item's pull-down menu. Write the menu beside the header;
  one written in it is still its button's menu, never more bar buttons.
- The header's `input type="search"` hides when scrolling while it is empty, as UIKit's
  does; `position="sticky" top=0` on it keeps the search field visible.
- A header's buttons that press, and its disabled ones (dimmed, as UIKit's), are bar
  items; any other element is an item drawn as written ("Tabs and stacks"). Inside a box
  around the heading, what the title does not show is hidden on iOS
  (`lower-header-unplaced`).

### A horizontal strip in a sheet

`scrollIntoView(id)` scrolls every scroller above the element, as the web's does, so
scrolling a strip of choices to one moves the sheet's form too. `block="nearest"` leaves a
scroller that already shows it alone and `inline` places it across the strip:
``scrollIntoView(`icon-${i}`, block="nearest", inline="start", behavior="instant")``.

### Native views: touch input

On Android, a hosted module's `View` receives `dispatchTouchEvent` in its own
coordinates. Returning `true` for `ACTION_DOWN` accepts delivery. A drag past
touch slop on an enclosing scroller's axis cancels that delivery and scrolls the
page, as with a button inside an
[Android ViewGroup](https://developer.android.com/develop/ui/views/touch-and-input/gestures/viewgroup).
A view or descendant that needs its own pan calls
`parent.requestDisallowInterceptTouchEvent(true)` before the crossing move;
request it on down to reserve the pan, as the maps package does. Passing `false`
allows interception again. The request clears on up or cancel. An unscrollable
axis and a second pointer before interception stay with the hosted view.
A `modal` route's sheet takes an up-and-down drag the same way: the view keeps
it only if it made the request before the crossing move, so a map in a sheet
pans every way and a button there lets the sheet be dragged by it.
See the [Android module seam](../host/android/README.md).

### Native views: accessibility

A native view ([recipe](recipes/native-views.md)) that says nothing to VoiceOver is one
element under its box's `aria-label`, with the traits of its `role`: `button`, `link`,
`img`, `switch` or `checkbox` (`aria-checked`), or `slider` (`aria-valuenow`). A view with
parts sets `accessibilityElements` (`UIAccessibilityElement`s with a label, `.button` and
`accessibilityActivate`); the box is then a group under the label.

### Native views: a package's

A package serves a tag by declaring it in its `exact.json` `views` (LLP 1133.001 D1): an
app that depends on the package places it with no `app.json` entry. Its implementation
makes it in each session, the same instance that answers its names: Swift
`view(_:id:props:events:)` on the `ExactPackage` class, a `roster` of view classes from the
web implementation, Kotlin `ExactExtension.view`; `id` is the node's. A view declared
`"displayed": true` is told `displayed(id, shown, origin)` when it shows or hides, and a
prop marked `"binds": true` `bound(id, prop, origin)` when it changes. Each report carries
the origin of the commit that changed it. On Apple, a track view attaches its presentation through
`context.tracks.present`. The first matching prop and committed binding subscribe under
the less privileged of the display and binding origins. A replacement native view reads
the held reader and its waveform history when the presentation registers. A track prop
declares `"type": "handle<track>"` and `"binds": true`. For a development
`EXACT_MODULES` override, rebuild the host and its module together.
Hiding, removing or rebinding the runner node
releases its reader. A commit can hand a running track directly to another node.
Props and events must be declared;
an undeclared attribute is `lower-native-attr`. Standard box attributes and the nine
common module events remain available without a declaration. A declared prop may take
a known non-layout attribute's name where it means what CSS means by it (`exact-camera`'s
`object-fit`): written on the tag it goes to the view. A declaration that names a
layout attribute (`width`, `padding`, `flex`) is refused with `bake-view-prop-layout`,
naming the package, view and prop; give the view prop another name. A class's rows
style only the box: `object-fit` in a class never configures the viewfinder, so write
it on `exact-camera` for that. A renamed spelling (`fontSize`) is refused whatever is
declared. `packages/fixtures/swatch` is a whole one.

A view placed by a Contract library counts too, including through a re-export. The app
still declares the view's package as a direct dependency. A component the app never
instantiates does not link its views' implementations.

### A package's GPU surfaces

To show a static glTF model, add `@exact/model-viewer` from
`packages/model-viewer` in this checkout. Its `modelViewer` surface supplies
the camera, picking, node styles, faded-behind rendering, clipping and
hotspot labels as Exact children. See [the model viewer recipe](recipes/model-viewer.md)
for a runnable example, file sources and the surface record (LLP 1133.039).

Any package may declare `gpu` beside `views` in its main `exact.json` (LLP 1153.003 D5):

```json
"gpu": {
  "crate": "rust",
  "exactGpu": "0.4",
  "surfaces": { "sky": { "arity": 1, "factory": "sky_surface::surface" } },
  "targets": ["web", "macos", "ios", "linux"]
}
```

`crate` names a directory inside the package containing a standalone `Cargo.toml` and
`Cargo.lock`. Its dependency is `exact-gpu = "0.4"`, without a path or Git source. It uses
`exact_gpu::wgpu` and depends on no other Exact crate. Each factory returns
`Box<dyn exact_gpu::Surface>`. The build generates the registry and `module!` exports,
including the web dependencies; the package writes neither. `packages/fixtures/sky` is
a complete example.

For compute or larger resources, `gpu.limits` and `gpu.features` each take `required`
and `optional` members. Limits are objects of WebGPU camelCase names to nonnegative
integers; features are arrays of WebGPU feature names. For example,
`"limits":{"required":{"maxStorageBufferBindingSize":268435456},"optional":{"maxBufferSize":1073741824}}`
and `"features":{"optional":["float32-filterable","subgroups"]}`. The build checks names,
ranges and duplicate declarations. Each package's device requests its own requirements,
clamping optional entries to the adapter and raising paired limits as needed. An unavailable required entry refuses only that package's surfaces with a
diagnostic; apps that declare nothing keep the existing grant. `Surface::device_ready`
receives the granted `wgpu::Features` and `wgpu::Limits` before preparation, including after
recovery. See [the GPU recipe](recipes/gpu-module.md) for the request and fixture APIs.

The app adds the package with `bun exact.mjs add <directory or @scope/name>` and writes
`canvas surface=sky(false)`. The package's full name supplies the module name:
`@fixture/sky` becomes `fixture-sky`. Duplicate module or surface names are refused,
including conflicts with `gpu.modules`. A placed surface may take up to `arity` positional
arguments; the surface checks named arguments and defaults.

A surface may add `"wgsl": "src"` to name the argument carrying an app shader
(LLP 1181 D9). A string literal there must name a file under `assets/`. The compiler
validates it as plain WGSL without optional WebGPU features, with one fragment
entry, built-in inputs only, one `@location(0) vec4f` output and at most one
resource: a uniform struct at group 0, binding 0. Members may be `f32`, `i32`,
`u32` or `vec2f`–`vec4f`, without aliases,
`@align` or `@size`. `time` and `devicePixelRatio` must be `f32`; `resolution` must
be `vec2f`. `src`, `maxFps`, `animationPlayState` and `alphaMode` are reserved surface
arguments, and the compiler refuses a literal value of the last three that the surface would
refuse. `alphaMode` is `"opaque"` (the default) or `"premultiplied"`, whose fragment returns
colour already multiplied by alpha.
Other named arguments bind members, with a CSS colour accepted for a `vec4f`.
The compiler checks literal values and compares the uniform reader's layout with
naga. Failures report `analyze-surface-shader` at the shader declaration or canvas,
with file, line and column. Computed sources and values are checked when they bind;
surfaces without `wgsl` keep their own validation.

`targets` names hosts among `web`, `macos`, `ios`, `tvos`, `android`, `linux` and
`windows`, and defaults to all of them; the watch draws no GPU surface. An omitted host
builds no artifact for the package, and its canvases there are drawn as they would be
without it: by the app's own `gpu/` module when that registers the name, else by the data
source on Canvas 2D. With neither, the canvas shows only its own styles, and a native host
logs that the module is absent.

Each package resolves in its own generated workspace under `.exact/package-gpu/`.
The checkout supplies `exact-gpu`. The build tries three whole dependency graphs in order:
a merge that prefers newer compatible shared versions while preserving the generated
manifest's exact pins; every package-lock entry except `exact-gpu` and competing exact
pins, plus the checkout's `exact-gpu` and the pinned dependencies; then a merge that
prefers the SDK's versions. The package-first attempt retains multiple versions in one
semver series and adds none of the SDK's other dependency blocks. Cargo resolves each
attempt; a crate Cargo picks that neither lock holds is replaced by the newest version of
the same name and series that one does, if Cargo accepts it. The first attempt whose
versions and checksums all occur in either input lock wins.
Missing Cargo sources are fetched before the locked build. If every attempt fails, the
error names each attempt's failure. An incompatible `exactGpu` requirement stops the
build before resolution. Neither input lock changes.
The app commits no Rust files for a package. Modules load only when their surfaces mount;
package WGSL is compiled into the crate, rather than shipped through `gpu.shaderRoots`.

### Model viewer

`cameraOrbit` is azimuth, polar angle from the top, then radius. Angles take
`deg` or `rad`; lengths take `m`, `cm` or `mm`. A radius of `100%` fits a
sphere around the target; `105%` leaves a margin. Unsaid, the orbit is
`"0deg 75deg 105%"`, `cameraTarget="auto auto auto"` centres the model, and
`fieldOfView="auto"` frames at 30°. A narrow canvas widens the vertical
field of view to keep the model in frame. Zoom changes radius.

Changing the orbit, target or field of view animates to that view.
`postMessage("reset", "modelViewer")` returns to those authored values after
a drag, including when a view button writes the same values again. The
record's settled camera values can be saved and bound back as a named view.
`interpolationDecay` is milliseconds, 50 by default; use 0 to jump, including
when the app honours the person's reduced-motion preference.

`cameraControls` is off unless written. On, drag to orbit, pinch or wheel to
zoom, two-finger drag or a modified/right-button drag to pan; focused, use
arrows, Shift + arrows and Page Up/Down. `disablePan` and `disableZoom` stop
those camera moves. The canvas reserves all touches and wheels in its box
at mount, even with zoom disabled. In a scrolling page, leave room beside
it to scroll. To change input mode, mount another canvas, as
[the scrolling fixture](../packages/model-viewer/test/fixture/scrolling.contract)
does. With controls off, that fixture picks from `pointerup` by posting
`pick <offsetX> <offsetY>` to the surface.

Each `ModelNode` styles its named node and descendants. A deeper entry wins;
for the same name, the last wins. The whole entry wins, without merging
fields. `color=""` keeps material colours; otherwise use a CSS colour, with
`opacity` from 0 to 1 and `visibility="visible"` or `"hidden"`. System
colours such as `AccentColor` need platform context and are refused.
Unnamed nodes use `#<index>`; duplicate names share a style.

For the faded-behind style, mark the shell `occluder` and its inner parts
`revealed`. The inner part draws at `occludedOpacity` where the shell hides
it. Occluders are always opaque: use `occlusion="none"` when you want a
translucent shell instead. A tap picks the revealed part through an
occluder, but ordinary opaque geometry in front still blocks it. Hidden and
clipped geometry, and non-occluders at zero opacity, cannot be picked.
`message=pick` receives the deepest mesh node's name, or `""` for a miss.

`clipPosition` is a point on the plane, in model coordinates; its `auto`
axes use the model's centre. `clipNormal` points towards the half that
stays. `""` disables it. Drawing, picking and hotspot anchors share the cut;
the cut faces are not filled.

A `ModelHotspot` anchors a direct canvas child by `testId`. Its position is
in model coordinates; `position=""` uses the node subtree's bounding-box
centre. `normal=""` skips the facing test. Labels follow the camera and dim
to `minHotspotOpacity` (0.25 by default) behind opaque geometry outside
their own subtree, or when their normal faces away. They stay pressable
and accessible while dimmed; off-canvas, behind-camera, hidden-node and
clipped anchors hide them. Write a named group and label on the canvas,
and give hotspot buttons the same picking action as the model. On the web,
a label that writes `filter` overrides its occlusion fade; put that filter
on a child inside the label instead. iOS keyboard focus skips hotspots;
VoiceOver can reach them. View buttons provide a touch alternative to orbit.

`src` also accepts `app:/data/…`, `app:/cache/…` and `app:/tmp/…`. Data code
downloads or imports a GLB, writes it under its `fs.write` grant, and returns
the path to bind. Reading that path needs no surface grant. See
[the fixture's cacheModel source](../packages/model-viewer/test/fixture/app.ts)
and its scrolling route for a file created at runtime. Direct `https:`
sources are refused. Names are ASCII, at most 128 bytes after removing
`assets/`, with no `.` or `..` path part; files are at most 64 MiB.

The reader supports `KHR_mesh_quantization` and `EXT_meshopt_compression`.
It draws static triangles with material base colours. `OPAQUE` (the default)
ignores material alpha; `MASK` keeps or drops the whole primitive at
`alphaCutoff` (0.5 by default). Both use material opacity 1; `BLEND` uses
the base colour factor's alpha. Node style `opacity` multiplies that material
opacity, except for occluders, which stay opaque. Textures, animation, skins
and morph targets are ignored. `.gltf`, sparse accessors and other required
extensions are refused. On the web, parsing blocks the page once; native
parsing runs off the frame thread.

Read `exactSurface("modelViewer")` with the imported `ModelViewerState`:

| Field | Use |
|---|---|
| `presented` | Hide the loader after the current picture is presented. |
| `src`, `loaded`, `error` | The bound source, whether it loaded, and its error sentence (`""` on success). |
| `triangles` | The displayed model's triangle count. |
| `nodes` | Names and parent names (`""` at a root), for a parts list. |
| `cameraOrbit`, `cameraTarget`, `fieldOfView` | The last settled camera, ready to save as a view. |
| `cameraView` | `front`, `right`, `back` or `left`, optionally prefixed `upper-` or `lower-`. |

A replacement keeps the old picture while loading. Failure clears it and
sets `presented=true`, `loaded=false` and `error`. The empty frame ends the
loader; show the error text that remains. The record belongs to the first
live canvas of that surface name; each canvas's picks still reach its own
handler. Import the package's shapes: records entering a GPU surface are
positional, so their declaration order matters.

The generated fixture is the tested model. Web drawing needs WebGPU. tvOS
shows the model and accepts app view buttons, but has no remote orbit or
focusable hotspots. Linux has no screen-reader bridge. Android, Windows and
physical-phone behaviour remain unqualified; watchOS and the terminal have
no GPU surface.

### Native SDK dependencies

An Apple implementation names Swift Package Manager products in its
`apple/exact.json` (LLP 1133.001 D9, expo/exact#751):

```json
{
  "swiftPackages": [{
    "url": "https://github.com/apple/swift-collections.git",
    "exactVersion": "1.1.4",
    "products": ["DequeModule"]
  }]
}
```

For an app's own `modules/apple/*.swift`, put the same `swiftPackages` array
under `host.ios` or `host.macos` in `app.json`. Swift imports the product's
modules normally. All Apple destinations share one `Package.resolved` beside
the app. Run `bun exact.mjs lock` after changing these declarations, bindings
or used interfaces; review and commit the lock. Lock discovery follows the
compiled plan, including views placed through Contract libraries. Builds
refuse a missing or mismatched lock and disable automatic resolution.
Conflicting direct versions name both requesters.
SwiftPM verifies binary targets by checksum. One shared dynamic library links
all declared products used by the app's Swift and its bound implementations,
so SDK singletons and configuration are shared across consumers. It loads with
the first consumer that needs it; an unused implementation stays unloaded.
A dynamic framework a package brings, such as `Sparkle.framework`, goes into
the Mac app's `Contents/Frameworks` whole, with its helpers and XPC services;
`exact release` signs each inside out with the Developer ID and the hardened
runtime ([Mac binary updates](recipes/mac-binary-updates.md)).
App-local `.xcframework`s remain supported. Package resources ship with the
app and are available through `Bundle.module`. SDK privacy manifests and the
requesting implementations' declarations accompany the shared library and
dependency dylibs. CocoaPods is not supported.

Android implementations declare pinned `group:artifact:version` coordinates
in `android/exact.json`'s `maven` array; an app's own Kotlin uses
`host.android.maven`. See [Maven libraries](../host/android/README.md#maven-libraries-one-locked-verified-gradle-graph-llp-1133008-d8-item-5)
for the shared Gradle graph, lock and verification files.

### A package's configuration

A package that needs a value from the app, such as a vendor SDK's API key or a
tile server, declares it per target in its interface's `exact.json` (LLP
1133.021 §9 item 7):

```json
"config": {
  "android": { "apiKey": { "type": "env", "required": "release", "missing": "yandex-mapkit-key-missing" } },
  "ios": { "apiKey": { "type": "env", "required": "release", "missing": "yandex-mapkit-key-missing" } },
  "web": { "tiles": { "type": "string", "required": true }, "maxZoom": "number" }
}
```

A target is `web`, `android`, `ios`, `macos` or `tvos`; each is declared on its
own, since a package binds per target. `type` is `string`, `number`, `boolean`
or `env`, and `"maxZoom": "number"` is short for `{"type": "number"}`.
`required` is `true` or `"release"`, and `missing` is the error code a build
without the key reports (else `bake-config-missing`).

The app gives the values in `app.json`, under the interface's name and the
target. A key typed `env` names the environment variable that holds its value:

```json
"packages": {
  "exact-yandex-mapkit": {
    "android": { "apiKey": { "env": "YANDEX_MAPKIT_API_KEY" } },
    "ios": { "apiKey": { "env": "YANDEX_MAPKIT_API_KEY" } }
  }
}
```

The build reads the values, with each variable's value as the build runs, and
writes them into the code it generates for the app. The package's code reads
the declared keys for its target from its context. A key given no value is
absent.

| Target | What the package reads |
| --- | --- |
| Android | `context.config`, a `JSONObject` on `ExtensionContext` |
| iOS, macOS, tvOS | `context.config`, a `[String: Any]` on `ExactPackageContext` |
| Web | `config`, an object passed to `implementation({ config })` |

```kotlin
val key = context.config.optString("apiKey")   // "" when absent
```

```swift
let key = context.config["apiKey"] as? String
```

Every build refuses these, naming the key and never its value:

- `bake-config-type`: a value of another type than declared, such as a key
  written into `app.json` where `{"env": "…"}` is declared.
- `bake-config-unknown`: a key the interface does not declare for that target.
  The error suggests the nearest declared name.
- The declared `missing` code: a required key with no value. A key that is
  `"required": "release"` is required only in a release build. On Android, that
  is `--release` or production trust. On Apple, it is production trust
  (`--release`, or `EXACT_UPDATE_TRUST=production`) or a distributed build
  (`--distribution`, or an archive that is not `--development`). On the web,
  it is a site to host (`--out`).
  A development build without the key warns and builds.

An `app.json` entry for a package that is not installed is a warning.

**`env` keeps a value out of the repository, not out of the app.** Anything in
`config` ships inside the APK, the IPA or the page, and anyone with the app
can read it, as with a setting an app reads through `import.meta.env` (LLP
1153 D4). A value that must stay secret belongs on a server. Restrict a vendor key to the
app instead: by package name and signing certificate for Google's keys, or by
bundle id where the vendor offers it.

A native app's configuration is part of its binary. Changing a value, or the
value of a variable it names, needs a new binary, and `exact deploy` and
`exact push` classify the change that way. The binary's receipt records an
`env` value only as its SHA-256. Publishing an update therefore needs the same
variables set as the build had.

**A vendor SDK that reads its key from the Android manifest.** An `android` key
may also fill a `<meta-data>` the SDK reads by its own name:
`"metadata": "com.google.android.geo.API_KEY"`. The name must be on Exact's
list (`CONFIG_METADATA` in `scripts/android-manifest.mjs`), which today holds
only the Google Maps SDK's key. Two bound packages may not fill one name. A
package that needs another name asks for a row. An SDK that takes its key in
code, such as Yandex MapKit's `MapKitFactory.setApiKey`, needs no row.

### A package's storage and session

A package's implementation finds the session's directories behind `app:/data`,
`app:/cache` and `app:/tmp` on its context: Swift `ExactPackageContext.data`, `cache` and
`temporary`, Kotlin `ExtensionContext.data`, `cache` and `temporary`, the Linux host's
`Context::data()`, `cache()` and `temporary()`. They are the app's own, and under the
agent the drive's store, which a named store (`EXACT_AGENT_STORAGE`, `open({storage})`)
keeps across the drive's launches. The sessions of one app share them, so what a package
keeps for one session (a local replica) goes under the session's `label`, which is the
same at every launch: the name the embedder gave the session (`makeSession(label:)`; the
sample host's `a` and `b`), and in an app `main` (a later Mac window `window-2`,
`window-3`, in the order they open). `session` is unique only within the process and
`hostSession` new at each launch; key nothing durable by either.

An admitted `app:/` path that a source passes resolves on the same context: Swift
`ExactPackageContext.file` for a file and `directory` for a directory, Kotlin
`ExtensionContext.file` and `directory`. The destination may be missing, and the package
creates it; an existing destination of the other kind is refused, as is a path that
leads through a symbolic link. The result is a path, not an open handle, so a package
opens or empties it promptly (LLP 1133.009 D10).

### A package source on the web

When a package has no web implementation, a source in its `exact.json` can
still answer with `substitute.default`. An optional source can declare
`"default": null` to answer `none`. A source that lists `"web"` in
`substitute.unsupported` fails with `NotSupportedError` even if it has a default.

For a status that stays readable, use `"targets": {"web": {"available": false}}`
inside its `substitute`. The other fields keep their default values. List
`"web"` in `unsupported` on the operations that require the missing platform feature.

### A package's native module, library and web assets

A package fills the data module's one native module (`native.call` in `app.ts`) from the
`exact.json` at its root, whatever its `exports` "." names (LLP 1154 D2, D3):

```json
"native": {
  "rust": "native/lib.rs", "class": "Vault", "link": "vault",
  "library": { "aarch64-apple-darwin": { "url": "https://…/vault.xcframework.zip", "sha256": "…" } }
},
"web": { "assets": ["web/device.wasm"] }
```

`rust` is the shim's root file, relative to `exact.json`, and `class` a type in it that
implements `exact_js::NativeModule` and has `fn new(app: &str) -> Self`; the shim names only
`exact_js` and `std`. A native build joins it to the app's crate and attaches it (an app with
its own `Cargo.toml` writes `.with_native(exact_native)` in `embedded_data`). Its
`configure_storage` gets the app's roots and `sqlite(path)`, which admits `sqlite.open` under
the app's grants and answers a `SqliteFile` guard: while the module holds it, the app's own
`storage.sqlite` is refused the file, so a shim closes its own SQLite before dropping the guard,
on its close operation and in `Drop` (the host drops the module on reload).
`library` pins one `{url, sha256}` per Rust target of a dynamic library named `link`: for
Linux a shared object whose SONAME is `lib<link>.so`, for Apple a zip of one `.xcframework`
whose slice is `<link>.framework`, installed as `@rpath/<link>.framework/…`. The build
fetches it by its hash before Cargo, links it from the shim's `#[link(name = "<link>")]`, and
carries it beside a Linux executable or, signed, in an Apple bundle's `Frameworks`.
`web.assets` are served at `/assets/<package>/<path>`, captured with the package's code when
`app.ts` imports it. Each of these fails the build by name: a wrong hash; a target `library`
does not list; two dependencies declaring `native` (`bake-native-twice`).
`packages/fixtures/vault` is a whole one, its library built by `test/library.mjs`.

### What a package may ask of the Apple host

A package's native code cannot reach three things only the host owns: the app's Info.plist,
its signed entitlements, and the system's calls on the app and scene delegates. Each is a
closed list in exact2 (LLP 1133.001.000). A package opts into entries on a list and never
adds one, so installing a package never changes what the app is trusted with; a package
that needs an entry the lists lack asks exact2 for one. Nothing here reads the package's
scope: a package kept outside this repository takes its author's (`@fixture/invites`, a
whole one under `packages/fixtures/invites`), and `@exact/` names exact2's standard library.

**Info.plist keys.** A package's Apple implementation (`apple/exact.json`) adds to the
Info.plist of every app that uses it, whether or not the app grants a device. `plistKeys`
sets a key to a value, and only a key on the Apple host's allow-list, with the one value the
list gives it (`PLIST_KEYS` in `host/apple/plist.mjs`), for the platforms it names:

```json
"plistKeys": { "SKIncludeConsumableInAppPurchaseHistory": true }
```

`plist` names a dictionary keyed by the strings keys the app passes to one call's
option, each valued by its base-locale text:
`"plist": { "NSLocationTemporaryUsageDescriptionDictionary": "requestPreciseLocation.purposeKey" }`.
The build refuses, naming the package, a key not on the list, another value, a `plist`
entry that is not `"<call>.<option>"`, and any key the build itself writes. An app has
no Info.plist field of its own.

**Entitlements.** A package has no field for one. Its interface's `grants` lists the device
rows (`runner/src/device.rs`) its names need, as alternatives; a name that needs a row of its
own lists it in its own `grants`. An app that uses such a name without the line gets
`grant-missing` naming the line, and at run time the name is refused `NotAllowedError`; the app
writes the line, the build signs the row's entitlements, and `exact deploy`'s reach table shows
it. `device.shareplay`, added for such a package, signs Group Activities'
`com.apple.developer.group-session` on iOS and, under a provisioning profile that allows it,
on the Mac; not on tvOS. A Mac build without such a profile signs none and still ships.
`device.cloudkit` names no purpose and derives the container `iCloud.<bundle id>`;
`device.cloudkit iCloud.<name>` selects another container. A second container is refused.
On iOS and iPadOS, and under a provisioning profile on the Mac, it signs
`com.apple.developer.icloud-services = ["CloudKit"]`,
`com.apple.developer.icloud-container-identifiers = [<container>]` and
`com.apple.developer.icloud-container-environment`: `Development` under a development
signature, `Production` under a distribution signature, including an archive.
The profile must allow CloudKit and the selected container by value. It grants no push,
background notification mode or iCloud Documents entitlement. tvOS, App Clips and extensions
sign none. A Mac release without a suitable profile ships with CloudKit unavailable and
claims no shares. A package receiving shares declares the host event below.
`device.icloud-kv`, for an app module that uses `NSUbiquitousKeyValueStore`, names no purpose
and signs `com.apple.developer.ubiquity-kvstore-identifier` with the app's application
identifier (`<team prefix>.<bundle id>`, `SIMULATORX.<bundle id>` on a simulator) on iOS and
tvOS and, under a profile that allows it, on the Mac. Its App Clip and extensions sign none. A
device build needs a profile with the iCloud capability; a Mac release without one is refused,
since the store would keep its values on that Mac without saying so.
`device.sensitive-content-analysis`, for an app module that uses `SCSensitivityAnalyzer`, names
no purpose (the framework has no usage key; whether it analyses is the person's own setting,
`analysisPolicy`) and signs `com.apple.developer.sensitivecontentanalysis.client` as
`["analysis"]` on iOS and, under a profile that allows it, on the Mac (macOS 14 and later); not
on tvOS, and nothing on Android, Windows, Linux or the web. A device build needs a profile with
the Sensitive Content Analysis capability; a Mac release without one is refused, since the
analyzer would report nothing as sensitive.

**Host events.** A call the system makes on a delegate, held by ExactKit until the package
that takes it drains it, even when it came before the package's module loaded. The list is
`HOST_EVENTS` in `host/apple/plist.mjs`; ExactKit's `HostEvents.swift` names the same events.

| Event | Platforms | Delivered by | Needs | Writes |
| --- | --- | --- | --- | --- |
| `cloudkit-share` | iOS, macOS | iOS: the scene delegate's `windowScene(_:userDidAcceptCloudKitShareWith:)`, or the connection options' `cloudKitShareMetadata` when it launched the app; macOS: the application delegate's `application(_:userDidAcceptCloudKitShareWith:)` | `device.cloudkit`, signed in this build | `CKSharingSupported` |

The Apple implementation declares what it takes, `"hostEvents": ["cloudkit-share"]`, and its
interface `"start": "load"`, so an instance exists once the module loads. The build refuses,
naming the package, an event not on the list and one two bound packages declare. An event is
taken where a bound package declares it and the build signs its grant (on the Mac, under the
profile it chose); a taken event's key and `ExactHostEvents`, the names taken, are written to
Info.plist, and for any other neither. At launch ExactKit adds a taken event's delegate methods
by selector; an app that takes none answers none.

Each payload is the system's object, as an `NSObject` the package casts (ExactKit links neither
CloudKit nor GroupActivities), addressed to a session: in an iOS app its one session; on the Mac
the front window's; from an embedder, the session it passes to `ExactApp.hostEvent(_:_:session:)`.
One for a session that has ended goes to the first live session. At most 16 are held per event,
the oldest dropped beyond that, and one identical or equal to one still held for the session is
held once. The implementation takes what is held for its session with
`context.hostEvents.take("cloudkit-share")` and hears each later one as
`ExactHostEvents.arrived` (`userInfo` `{event, session}`, the session its `context.label`). A
test delivers one by posting `ExactHostEvents.deliver` with `{event, payload, session?}`.

### Arranged panes

On iOS and iPadOS 27.1 or later, an element hatch can call
`element.arrange(using: controller)` with a `UIArrangementViewController` configured
with `UISplitArrangement` or `UIOverlayArrangement`. Exact places the element's two
authored children in the controller and lays each out in the space UIKit gives it.
Give the element a definite size, such as `height="100%"` in a sized route;
allocated children do not give an auto-height parent its height. Pass `nil` to
restore CSS layout.

A `true` result accepts the request; UIKit lays it out when the owning container
is available. `false` means the element is unavailable, it does not have exactly
two valid children, or a new controller is already attached or has populated
content slots. Once accepted, the request uses CSS while the element
does not have exactly two valid children, then resumes native layout when both
return. Child order and replacements follow the authored tree.

The first child is UIKit's primary pane and the second is its secondary pane.
Exact supplies their child controllers and manages the arrangement's content
slots and containment. Supply a detached controller with empty slots; reuse that
controller for configuration changes. Call UIKit's `updateArrangement(_:animated:)`
to change axes, split or overlay behavior, dimensions and placement properties.
Exact does not translate those settings. Navigation stays in the enclosing controller.
UIKit may hide a pane when the allowed axes do not suit the current display.

```swift
let arrangement = UIArrangementViewController()
arrangement.updateArrangement(.split.axes(.horizontal))
element.arrange(using: arrangement)

// A later menu action changes the same controller, keeping the authored panes.
arrangement.updateArrangement(.overlay, animated: true)
```

UIKit's `.intrinsic` dimension measures each pane's authored content under the
proposed constraints, including text wrapping and resolved pane margins.
Content and margin changes update the measure; the allocated pane size remains
UIKit's. Automatic, absolute and fractional dimensions use UIKit's sizing rules.

Each pane is laid out as a root in the space UIKit gives it: an automatic width fills
that space and an automatic height is the content's, so a pane scroller without a
height or a maximum height does not scroll. Give a pane that should fill its
allocation a height such as `height="100%"`.

Use a bounded scroller in each child for independent scrolling. The first visible
vertical scroller is the default title and scroll-to-top target; `navigationScroll`
can name another. UIKit owns bar minimization, safe-area changes, each pane's
scroll-edge effect and navigation transitions. Arrangement adoption leaves the
route's native navigation policy unchanged. The route hatch receives the enclosing
controller, so title and bar settings use UIKit directly:

```swift
let item = route.controller.navigationItem
item.largeTitleDisplayMode = .automatic
item.navigationBarMinimization.minimizationBehavior = .onScrollDown
item.navigationBarMinimization.safeAreaAdjustment = .automatic
```

Set only the native properties the app needs to change. For a fixed header, set
`minimizationBehavior` to `.never`. Select the bar's scroll view with
`route.controller.setContentScrollView(_:for:)`. An explicit native selection
survives authored layout updates. Passing `nil` makes the edge automatic again, as
in UIKit, and Exact selects as on a route the app never touched: for the top edge
the scroller `navigationScroll` names (none when it names no scroller), else the
first visible vertical one, when it sits under the bar (none when content other than
control bars sits above it), and for the bottom edge the scroller at the route's
bottom.
`contentScrollView(for:)` reports Exact's selections too, so it does not tell an
automatic edge from a selected one. UIKit takes a selection for the top and bottom
edges only: it ignores a selection for a side edge. The two edges are independent:
the setter `setContentScrollView(_:for:)` takes `.all` to select both, while the
getter `contentScrollView(for:)` takes only `.top` or `.bottom` and raises an
exception for a side edge or `.all`.
For example, a menu action can select the secondary pane after the arrangement
has mounted:

```swift
if let scroll = arrangement.viewController(for: .secondary)?.contentScrollView(for: .top) {
    route.controller.setContentScrollView(scroll, for: .top)
}
```

A pane's view controller reports its scroll view only while UIKit insets the pane, so it
reports none for a pane UIKit hides. A pane `scroll` with an element hatch always has its
scroller in the hatch's `element.scrollView`.

The route hatch's `contentScrollView` is the route's content scroll view when it is
read, the one a status-bar tap scrolls while the route is active: an app's top-edge
selection, an app-owned `UIScrollView` included; else, where the route has a
`navigationScroll`, the scroll view of the element it names, or nil when no element
carries the name or it does not scroll; else the scroll view of the route's first
vertical scroller; else the route's own viewport, which an unpresented route has
when the navigation root scrolls or native chrome uses
`interactive-widget="resizes-visual"`; else nil. A native list's scroll
view is its collection view. UIKit's
`contentScrollView(for: .top)` reports Exact's scroller only while it sits under the
bar. UIKit's bar follows a selection at once. Exact's own layout of the route follows
it (the scroller its title collapses with, the scroll views UIKit insets, and the
route's clearance) at the route's next layout or as the route appears. A selection made
during a push or a pop on screen, a presentation or a dismissal, on a route that does not
appear in it, follows when it ends, unless a batch meanwhile prepares the route's stack; a
transition that a stack off screen keeps until its tab shows again does not hold it.
Until then, after a nil, `contentScrollView(for:)` returns nil, while the route
hatch's `contentScrollView` already names Exact's automatic choice.

The containing navigation controller still owns one shared navigation bar. UIKit
decides where its large title and each pane's edge effect appear; the panes keep
their own scroll offsets and safe areas.

Declare the hatch for its platforms in `app.json`. Keep navigation outside the
arrangement and avoid nesting it in authored scrollers or other native layout
containers ([Apple's containment guidance](https://developer.apple.com/documentation/uikit/uiarrangementviewcontroller)).
Keep the same children mounted when switching between native and CSS layouts, so
folds and resizes preserve their controls, bindings and state. Older systems and
other hosts use the authored CSS layout. Duo Lab's
[Panes screen](../apps/duo-lab/arrangement.contract) and
[hatch](../apps/duo-lab/modules/apple/ArrangedPanes.swift) use UIKit split and overlay
arrangements and responsive CSS elsewhere. Duo Lab's
[Arrangement screen](../apps/duo-lab/arrangement-view.contract) sets title display, bar
minimization, safe-area adjustment and the bar's scroll view through the
[route hatch](../apps/duo-lab/modules/apple/ArrangementOptions.swift). See Apple's
[arrangement guidance](https://developer.apple.com/design/human-interface-guidelines/designing-for-iphone-duo).

## Routes and web documents

Declare at most one `routes <slot>` in the app's root file. Each row names an
absolute path pattern, optional tab membership, parentage by indentation, and
optional rendering/activation policies. `notfound` is a bare fallback: with none, the
compiler writes it and a "Page not found" screen with a way home (the location keeps its URL);
write it to draw your own. Do not redeclare the implicit router state with `state`.

Assign the results of `open`, `push`, `replace`, `go`, `select`, or `back` to that
router state. Use `path("route", args…)` to construct checked locations. A
template literal as a location is refused (`route-template`) and a string literal
is checked against the table, but any other computed string is not checked, so
always build locations with `path()`. A route's parameter fields are strings, with
empty strings for absent fields. `select` takes a tab name, not a URL.

`open` replaces the destination tab's stack with its declared route chain, from
root through destination, and leaves the other tabs as they were. For a direct
child this is `[root, child]`, so Back returns to the root; deeper destinations
retain their declared intermediate ancestors. Use `push` to retain the visited
screens instead. Signing out of an app with tabs, `open(nav, "/")` alone keeps the last account's
screens in the other tabs: open each tab's root, the tab to land on last,
nested because an action's writes land when it ends
(`nav = open(open(nav, "/history"), "/")`).

`for entry in stack(nav) key=entry.id` gives retained screens their identities.
Bind the host's navigation root and per-entry `navigationKey`s as the
[router fixture](../contract/corpus/routes.contract) demonstrates. Test back,
tab switching, the same-URL push, external navigation, and route parameters.

Exact's web servers use declared routes for file-like deep links: `/profile/:handle`
admits a navigation to `/profile/photos.zip`. Existing public files win; unmatched
file paths stay 404, even with `notfound`.

A route literal may hold Unicode and spaces as they are (`"/item/café"`) or escaped; a pattern may
not end in a space, which a browser trims: drop it, or write it `%20`. A literal spells an escape
as a browser sends it: the router matches a path only as written, while a host serves every
spelling from one file. An escape of a character a URL carries as itself (`%41` for `A`) or one
in lower case (`%c3`) is refused (`route-pattern`), naming the spelling to write. A
`render=build` page is written at its path decoded (`/item/caf%C3%A9` to
`item/café/index.html`), but for what no file name may hold on every system, which stays escaped:
a separator, `%`, `<>:"|?*`, a control character, a trailing dot or space, and a Windows device
name's first letter (`/docs/con` to `docs/%63on/index.html`). `serve` looks pages up by that
name; two pages that would share a file are refused.

**A built site, hosted.** `bun exact.mjs web-build --out dist` writes the site to `dist/`, a
release build as deploy makes one (no agent mode): a page for each `render=build` route
(rendered under Bun unless the app has a render entry of its own), `404.html` for a `notfound`
route rendered so, `shell.html`, and `routes.json`, the route table in the plan's order with each
route's pattern, render policy and `notfound`. A route whose render draws randomness
(`crypto.getRandomValues` or `crypto.randomUUID`, for example) cannot be `render=build`, since
its page would give every reader the same random values: the build fails, naming the route;
declare it `render=request` or `render=client` instead. A `notfound` route declared `render=build`
renders at `/404.html`, so a route whose pattern matches that location, such as `post "/:slug"`,
fails the build, naming the route: give it another pattern or declare the `notfound` route
`render=request` or `render=client`. An app with routes also gets `robots.txt`, which
allows every path, and with `app.json`'s `app.origin` a `sitemap.xml` of the pages made at build,
by the render server's rule: not the `notfound` page, a page whose status is neither 200 nor 503
(a render at its deadline), or one whose `head` says `robots="noindex"` or `"none"`. A page rendered per request is not made at build, so the built
sitemap does not list it. A declared route at `/robots.txt` or `/sitemap.xml` keeps that path
instead of a generated file. Inside the app, `--out` is `dist` (or a path under `dist/` or
`target/`), which no build reads as source; the build it replaces is kept beside it in
`dist.gen` while a page sent from it may still ask for its files, and `--out` refuses a directory
beside which something else holds one of those names (`dist.gen`, `dist.previous`, `dist.lock`).
`bun exact.mjs serve dist` serves it on 127.0.0.1 (`--lan`: every interface), and any static
host serves it by the same rule: a file at the path; else, for a browser's navigation to a path
a route matches, `shell.html` with 200; else `404.html` with 404. A host with a single fallback
page serves `shell.html` for every page it lacks, and the client draws the `notfound` route. For
a route declared `render=request` or `cached`, the home route's too, `serve` renders the page for
the request under Bun from the build it serves (a `cached` page kept 60 s), so it arrives with
its data; so is a miss, with 404, when the `notfound` route is declared so. A static host serves
their shell. `npm start` runs `bun exact.mjs serve`, which with no directory serves the last
`web-build` without `--out`.

On Apple, a launch that no link starts opens at the manifest's `start_url`
(for example `"/today"`, `/` by default), as an installed web app does; Apple
builds bake it in. Linux and Windows open at `/` without a launch location.
A launch that a link starts opens where the link goes.

For a custom scheme, declare `protocol_handlers` in `app.json`, for example
`[{"protocol":"web+notes","url":"/open?u=%s"}]`. The Apple hosts and Android
register that scheme. An incoming `web+notes://auth/callback#done` reaches `/open` with
the complete original URL percent-encoded in `u`; read it with
`searchParam(top(nav), "u")`. The handler needs an `/open` route and a
navigation root with `navigate=`. At cold launch it is the initial location;
when already running, each link fires one `navigate` event in arrival order.
A scheme registered with `host.ios.urlSchemes` or `host.macos.urlSchemes` alone
needs no `/open` route: the link arrives as its path. A non-empty authority is the first
segment and an empty one is none, so `notes://post/42?x=1`, `notes:///post/42?x=1` and
`notes:/post/42?x=1` are all `/post/42?x=1`. Expo's `Linking.createURL("/post/42")`
writes the three-slash form.
For the app's own web addresses, declare `"handle_links": "preferred"` beside
`app.origin` (and `scope`, `scope_extensions` to narrow or widen the claim):
iOS, macOS and Android then open links to them in the app, at the link's path,
once the origin serves the association files the web build writes or prints
(reference.md, "Verified https links").
A drive on macOS, iOS or the Android reader delivers a link as the system would,
without iOS's "Open in …?" alert: `type <root> "web+notes://auth/callback#done"`.
For a packaged-only scheme, use
`[{"protocol":"notes","url":"/open?u=%s","distribution_only":true}]`.
Apple archives and `--distribution` builds register it; local builds omit both
its scheme and template. `--bundle` or `--release` alone does not enable it.
On Android a production build (`--bundle`) registers it and a development
build omits it. Web builds copy this key into the web app manifest, without `http` and
`https`; the browser decides which schemes it supports and whether the
installed app is registered.
A Mac web browser declares `https` and `http` together, each with the same
`url` and the same `distribution_only`: a web link, and an HTML file Finder
opens in the app (as its `file:` URL), reach that template. The person makes
the app the default browser in System Settings. On iOS, `host.ios.webBrowser: true`
registers the pair and signs the managed browser entitlement; Apple approval and
a provisioning profile granting it are required. Without this opt-in iOS registers neither, and
`host.<platform>.urlSchemes` refuses both
([Exact developer reference](reference.md#run-an-app-from-a-terminal-on-macos), its table).

Every route (a node with a `navigationKey` under the root) is a direct child of the
root or of a `role="tabpanel"` named by a direct `role="tab"` child of its first
linked `role="tablist"`, through `aria-controls`. Unlinked tablists do not select
a stack. Routes may pass through `for` and `if`, never another element: the compiler
refuses one behind a wrapper or in an unlinked panel (`lower-route-place`). Make each
route `position="absolute" inset=0`: the hosts hide a covered route, as
`visibility: hidden` does, so an in-flow route still takes its room. A covered
route stays mounted (its nodes are in `tree`, marked `[hidden]`, or `[inactive]`
beneath a sheet, which still shows it), so a `testId` two screens share names
two nodes: give each screen its own (`list-error`, `detail-error`).

Content taller than a native route needs a scroll container. An unpresented iOS
route has a page viewport when its navigation root scrolls, preserving the root's
scroll axes. Under native chrome, `resizes-visual` also supplies a page viewport
that accommodates the keyboard without layout reflow. Otherwise put scrolling
content in a `scroll` or `list` with
`flex=1 min-height=0` right after its `header`. Use an outer `scroll` when a
document contains smaller scrollers, so content outside them remains reachable.

```text
column navigationKey=`${e.id}` position="absolute" inset=0 display="flex" flex-direction="column"
  header
    text "Inbox"
  scroll flex=1 min-height=0
    …
```

That scroller is the route's content scroller with nothing written: the first in the
route that scrolls vertically. On iOS it goes under the bar and collapses a large
title, which happens when it is the route's child right after its `header` (in a route
with no header, its first child); with content between them (a switcher, a filter, an
error line) the title keeps still and that content sits below the bar, large title
included; a tap on the status bar, or on the selected
tab at its root, scrolls it to the top. A route with several scrollers names a
different one with `navigationScroll`, by its `id`. The compiler refuses a literal
name that no element of the route carries, or one whose element is a box that never
scrolls on y (`lower-route-scroll`).

## Tabs and stacks

On iOS, `overflow="auto"` or `"scroll"` on the navigation root keeps the native
container inside that scroller, sized to the root's box. Otherwise the native
container fills the session view.

Tabs with a stack each have one layout that works on web, macOS and iOS
([the tabs fixture](../contract/corpus/tabs.contract), driven by
`host/web-js/conformance/tabs.steps`):

```text
main navigationKey=`${top(nav).id}` navigationBack="close" navigate=follow display="flex" flex-direction="column" height="100%"
  column flex=1 min-height=0 position="relative"
    for t in nav.tabs key=t.name
      column role="tabpanel" id=`panel-${t.name}` position="absolute" inset=0
        for e in t.stack key=e.id
          column navigationKey=`${e.id}` navigationScroll=`content-${e.id}` position="absolute" inset=0 display="flex" flex-direction="column"
            header
              if e.name == "item"
                button id="close" press=back
                  text "Home"
              text e.name role="heading" aria-level=(e.name == "item" ? 2 : 1)
            scroll id=`content-${e.id}` flex=1 min-height=0
              …
  row role="tablist" display=(top(nav).name == "full" ? "none" : "flex")
    button role="tab" aria-controls="panel-home" aria-selected=(nav.tab == "home") press=pick("home")
      image "symbol:home"
      text "Home"
  if toast != ""
    button position="absolute" … // a root overlay: after the tablist, over everything
```

- A tab is built the first time it is selected, as UIKit and SwiftUI load a tab: until
  then its panel is empty (`unopened` in `tree`), and the components in it have no
  state yet. Once built it stays, with its stack, drafts and scroll. Resources and tasks
  are the root component's and run whichever tab shows. A drive or a test taps a tab
  before it reads or presses anything inside it; a target in an unopened tab is
  refused with the tap that opens it (LLP 1075.003 §9.14).
- Each route starts with a `header` (its heading, and on a pushed route the
  `id="close"` control), which iOS makes the navigation bar. Leave the tablist's and the
  routes' backgrounds, heights and colours unsaid: the platform draws the bars and the
  page. For an authored design, put `appearance="none"` on the `header` or root
  `tablist` to keep that bar's layout, colours and type on iOS too. A header
  button with `appearance="none"` stays authored in place while the rest
  of the header still projects. A title button keeps its contents; an authored Back
  replaces UIKit’s Back. Search or a header tablist can opt out on that element
  (a tab button alone does not opt its parent out of segments). Tabs retain their
  panels and stacks; other styling alone does not opt a header out. On iOS 26 and
  later neighbouring bar items share one glass capsule, as UIKit groups them;
  `buttonStyle="glass"` on a header button gives that item a capsule of its own
  (`sharesBackground = false`).
- A header `button` (or `link`) is UIKit's own bar item (LLP 1141.004 D1). Its own children
  are one symbol, one text, or a symbol and a text, and they write only where they sit
  (a size, `flex`, a margin or padding): the symbol is the item's image and the text its
  title, in the bar's type, the platform's; with both, the symbol is the image and the
  text its spoken name, as a hand-built bar's item has them. Write its colour on the
  button (`color`, the item's tint), and on a native button its `font-size` or
  `font-weight` (`buttonStyle="glass" font-size=12` is a small "● Clock" chip, its glass
  fitting the words). The button's own fill, border and padding give way to the bar's
  glass. A button whose one child is a filled box holding one text, one symbol or one
  picture (an avatar, a badge) is UIKit's item too, drawn from the box without the
  capsule. The box writes its fill, `border-radius`, size and layout. Text takes
  `font-size` and `font-weight`, and text and symbols take `color`. On a button that says
  `backgroundMaterial="glass"` (or `"glass-clear"`) the
  badge sits in a glass capsule of its own. A picture (`image url`, an avatar's photo)
  fills the box by its `object-fit`, clipped to its corners; until it loads the face is
  the box's fill. Leave the font unsaid for an ordinary Edit or Done.
- A readout (`role="status"`) of words, symbols, dots and one `progress`, holding nothing
  that presses, is an item on its side of the heading: its own views in a custom view,
  laid out as written and following the state in place, so its `progress` turns there
  (LLP 1141.004 D3). It sits on the bar with no glass; `backgroundMaterial` written on it
  is drawn as its own. VoiceOver speaks a change of it, its texts are never the subtitle,
  and where the bar is too narrow UIKit's "⋯" menu lists it by its accessible name,
  dimmed (give it an `aria-label`). (A `button disabled=true` is a dimmed bar item,
  UIKit's disabled one, which VoiceOver reads as a button that cannot be pressed, not as
  a status.) A row after the heading that is a symbol and words is the subtitle; one
  that changes says `role="status"` to be a readout.
- **The iOS bar holds a closed set, and the compiler refuses the rest.** In an app that
  ships iOS, a route's `header` that is the bar holds its heading; the first text after
  it as the subtitle (or the first text in a plain box after it, or a line of symbols and
  words); an avatar before the heading in its box; buttons, links and badges as above;
  readouts; one search field; and one tablist of titled tabs. Plain boxes around them,
  or empty between them, lay them out. Anything else (a spinner or a picture beside the
  heading, a second text, a switch, a pressed row of more than words, a button with a
  dot beside its label or a coloured text, a button inside a readout) fails the build
  (`lower-header-item`), and the error names the repair: make it one of those, move it
  below the `header` into the route's content, or say `navigationBar="none"` on the
  route's element and draw the header yourself. The compiler reads each arm of an `if`
  or a `match` as if taken, so a branch does not get past it. An app that does not ship
  iOS, and a header that is not the bar (no heading, two headings, `appearance="none"`
  on it), draw the header as written. What the compiler cannot see (a value computed as
  the app runs) iOS keeps out of the bar.
- On the other targets:
  - **Android's top app bar and tvOS's bar leave out a readout**, and Android the title's
    own parts too: the subtitle, the avatar and the tap on the title. The Mac's toolbar
    takes a header only when it has a place for every part: the heading, the first text
    after it (in a box or not) as the window's subtitle, buttons whose face is words,
    symbols, dots and at most one picture, readouts of words, symbols and dots, one
    search field and a tablist of two or more tabs that each show a title alone.
    Anything else (a readout holding a spinner, a pressable title, a tablist of one tab,
    a second search field) sends the whole header to the page. For the targets your app
    ships, the compiler names each readout Android or tvOS leaves out and the first part
    the Mac has no place for, once a header (`header-item-left-out`, information); it
    does not name the title's own parts, and where an `if` or a `match` decides which
    text is the subtitle or which search field is the second, it gives it the benefit.
    The web and desktop Linux draw the header as written. To show something on every
    target, put it below the `header`, in the route's content.
  - **A readout's automatic size is held to a third of the bar's width and 44 points of
    height**, and UIKit shrinks or hides the title before the items; a `width`, `height`
    or minimum you write wins, and the item takes that size. A trailing or bottom
    `margin` on it is not kept; use `padding`.
  - **Inside the heading's own box only the title shows**: the rest is hidden
    (`lower-header-unplaced`). Put an item outside that box.
  - **On iOS, leave a readout's type and colour unsaid for the bar's own**: a bar button
    title's size and weight, the label colour, and UIKit's grey spinner. It grows with the
    text size only as far as the bar's own titles do; at the accessibility sizes a long
    press shows it larger. Write `color="-exact-secondary-label"` for a quiet label. What
    you write on the readout or inside it wins as written; a `color`, font, spacing or
    case on the `header` or the page does not reach it. A `rem` or a text style there
    follows the root font size, so it grows with the person's text size or your
    `setRootFontSize`; `min(-exact-footnote, 20px)` holds one near the bar's size if you
    want that.

  ```
  header display="flex" align-items="center"
    button id="back" press=back
      text "Settings"
    text "Activity Log" role="heading" aria-level=2
    row role="status" aria-label=(live ? "Clock, receiving" : "Clock") display="flex" align-items="center" gap=6
      column width=7 height=7 border-radius=3.5 background-color=(live ? "c.cyan" : "c.dim")
      text "Clock" font-family="Menlo" font-size=12 color=(live ? "c.cyan" : "c.dim")
    if refreshing
      row role="status" aria-label="Getting latest status" align-items="center" gap=6
        progress
        text "Updating" color="-exact-secondary-label"
  ```
- Each tab names its panel with `aria-controls`; the panels are the stacks, and every
  tab's stack stays mounted, so a pushed screen, a draft and a scroll offset survive
  a visit to another tab. A tab is `select(nav, name)`; selecting the shown tab
  again pops it to its first visit. A sheet is a route of the tab's stack, rendered in the
  panel's `for e in t.stack` like the others, and is presented over the tabs; with
  tabs the root's own children are no routes, and one with a `navigationKey` is refused.
- On iOS the panels become a `UITabBarController`: a tab of one symbol over its label
  is its bar item, a filled box holding a text is the item's badge (the tab stays the
  platform's tab; the web draws the box as a badge on the symbol), and the
  tablist's `accent-color` (inherited, as in CSS) tints the selected item; the unselected
  colour, the label's font and the badge's colour and size are UIKit's, and nothing
  authored reaches them. `tap tab-…` works on every host. An app's `bun exact.mjs agent ios` and `test ios` drive what a
  person sees (`--chrome platform`; `--chrome agent` paints the authored tablist and
  header instead, as `scripts/agent.mjs` does by default): UIKit's tab bar, navigation bars and sheets show,
  `screenshot "file" window` draws them (`display` reads the simulator's screen as it
  is, a system alert over the app included), and a tap naming a tab or a header control
  presses it through its bar (`native: "tab-bar-item"`, `"bar-button-item"`). The
  bars' own motion runs on real time (`clock +300 real` before a screenshot).
  The software keyboard belongs to the system and is outside the app's drawing.
- On the Mac a root tablist placed as above (the root column's last child) is the
  window's sidebar: AppKit's source list, one row a tab, with its button in the
  toolbar and View ▸ Show Sidebar. Under the agent on `macos` the authored tablist
  paints (on iOS only when `scripts/agent.mjs` runs alone: `exact.mjs` shows UIKit's
  bars); `--chrome platform` on `macos` shows the sidebar, and a tap naming a tab
  presses its row (`native: "sidebar-row"`).
- For a footer pinned below the scrolling tabs, put the root tablist and a `footer`
  together as the only children of a `nav` directly inside the navigation root. The Mac,
  iPadOS 18 and later, and the wide web layout show the footer in the sidebar. It can
  contain an account button or other controls. The bottom bar hides the footer on iPhone,
  Android and the narrow web layout. An app-drawn tablist keeps its footer in the page.
- On an iPad (iPadOS 18 and later) the same root tablist, placed as above, is UIKit's
  sidebar-adaptable tab bar (`UITabBarController.mode = .tabSidebar`): a sidebar of the
  tabs where the window is regular-width and wide enough (an iPad in landscape), the
  floating tab bar with its sidebar button otherwise, and the bottom tab bar in a compact
  width. UIKit switches between them as the window changes; the routes clear the
  sidebar as they clear any bar, so their content is laid out beside it. A tablist the
  author places elsewhere, or draws (`appearance="none"`), stays a tab bar, and so,
  for now, does a bar of six or more tabs.
- **The search tab**: the root tab that says `rel="search"` (HTML's link type for "an
  interface for searching"; keep `role="tab"`) is the app's search tab, and is written
  last. On iOS 18 and later, in a bar of five tabs or fewer, it is a `UISearchTab` with
  the tab's own symbol and label (none: UIKit's magnifying glass and "Search"); the first `input type="search"` in its
  root route's header is its field, which UIKit moves into the bar when the tab is
  selected. `autofocus` on that field is `automaticallyActivatesSearch`: iOS 26 sets the
  tab apart at the trailing end of the bar and opens the field on every selection;
  without it the tab is one among the others. The web, the Mac's sidebar, Android and
  the other hosts keep it where it is written. A header field on a list is that list's
  filter; a search tab searches the app. `contract vocab search` has the details;
  Expo's `role="search"` trigger and SwiftUI's `Tab(role: .search)` are `rel="search"`.
- **A mini player above the tab bar** (docs/recipes/mini-player.md):
  `navigationTabAccessory="now-playing"` on the root tablist names an element, written
  after every tab panel the tablist links (usually just before the tablist), that iOS 26
  and later show as UIKit's tab bar accessory (`bottomAccessory`): a glass capsule on
  every tab, on an iPhone above the bar and inline where the bar was while it hides, on
  an iPad at the bottom of the page. UIKit places and sizes it (48 points high on an
  iPhone 18 Pro with iOS 27), the element's box is UIKit's whatever `height` it says,
  and the routes clear it. Show and remove it with `if` or `display`. The web, the Mac
  and Android lay the element out where CSS puts it, above the tab strip; iOS 17 to 25,
  which have no accessory, leave it there, hidden under the shown tab bar. Where the
  compiler can prove it, a literal id that names nothing, or an element inside the
  tablist, a tab panel, a route, a `list`, a popover or a `dialog`, a run of text or a
  part of an `svg` or a `select`, is refused (`lower-tab-accessory`, for example
  "`navigationTabAccessory="player"` names an element inside a tab panel, but the tab
  bar's accessory is shared by every tab"); its message says what to change. On the
  live tree, where the compiler cannot tell (an element written before a panel the
  tablist links, in the `footer` the sidebar shows, or a radio or a tab of its group), a
  development build on iOS 26 and later writes the reason to `logs` while Exact's own
  tab container shows the tabs. To see it, drive iOS with `--chrome platform`.
- A tab bar that minimizes as content scrolls (`tabBarMinimizeBehavior`) is not a
  Contract word: unsaid, the bar is UIKit's default, and an app that wants it sets it
  from the `tabs` hatch, which hands it the `UITabBarController` (docs/reference.md,
  "Access hatches"). The routes follow the safe area a minimizing bar changes, and an
  accessory goes inline beside the minimized bar. The hatch owns `bottomAccessory`
  while Exact shows no accessory.
- A box written beside the routes in the navigation root (a column the routes are offset
  past) shows on iOS as on the web: the native container that stands for the routes
  paints under a child no route's box meets, and UIKit lays the navigation bar's title
  and buttons out past such a column. For a sidebar of sections beside the content,
  prefer the root tablist above, which an iPad shows as its sidebar.
- On the Mac a route's `header` is the window's title bar and toolbar: its heading the
  window's title (under a `head title`, over the app's name; there is no large title),
  the first text after the heading the window's subtitle, its buttons toolbar items, a
  button whose `popovertarget` is a menu a menu item, a tablist of text tabs the
  centred segments, its `input type="search"` the toolbar's search field, and the
  `id="back"` control the bar's own Back, which is the back/forward pair where the
  navigation root offers Forward (its `navigationHistory`, both when it says nothing).
  A header holding anything else (a picture, a text before the heading or a second one
  after the subtitle, a pressable title, a button holding a spinner or a second picture) stays in the page as
  written, whole, and `logs` says what it was. Under the agent
  the authored header paints; `--chrome platform` on `macos` shows the bar, `tap` and
  `type` naming a header's button, tab or search input go through it
  (`native: "NSToolbarItem"`), and `state window.toolbar.header` names its title, its
  items by test id and which are in the overflow menu. Its search field arrives with
  AppKit's own animation (`clock +1200 real` before a screenshot). In a list beside its
  detail the window's title and subtitle are the detail's, as in System Settings, and
  the list's heading stays over its rows; beside a sidebar (three columns) the list's
  heading is the title and the detail's stays in its pane, as in Mail.
- In the Mac's list column, a sectioned list uses an inset AppKit table.
  Each section header is a group row. A link in the section body selects its
  row and opens the detail. The footer has no table row. When a virtualized
  list repeats sections, AppKit can address every row while Exact keeps a
  window of sections mounted.
- On the Mac a window toolbar the app declares (`row role="toolbar"
  toolbarPlacement="window"` of action buttons and at most one heading) has Customize
  Toolbar when every button in it has an `id` of its own: the ids are the items'
  identities, the written order the default, and AppKit keeps the person's arrangement
  across launches and in every window. A button hidden there keeps its place, hidden.
  A route's `header` is not customizable: its buttons change with the route, as a
  hand-built window's that changes with its content are.
- Hide the tab bar on a route with `display="none"` on the tablist. Never remove the
  tablist with `if`: without it the root has no tabs and the panels' routes are
  found by no host.
- Root children after the panels box (a toast, a timer strip, a full-screen menu) paint
  over the routes and the native bars on every host, as later siblings do in CSS.
- A modal route (`navigationPresentation="modal"`) is opaque: its unsaid
  `background-color` is `-exact-background` (the grouped one when it scrolls a grouped
  list), so the route under it, dimmed, never shows through; write one to say another.
  On iOS that unsaid ground is the sheet's: below its large detent the sheet is
  UIKit's glass, as a hand-built one is, and at the large detent the system
  background. A `background-color="transparent"` is clear at every detent.
  Its corners are UIKit's: on iOS 26 and later a large sheet is rounded like the
  display and inset from the bottom edge, as Apple's own are. A `border-radius` on the
  route is the sheet's corner radius (its top-left one; `preferredCornerRadius`), which
  also seats a large sheet flat on the bottom edge.
  A modal route opened over a sheet (a confirmation over a dialog)
  is a sheet over that sheet, on iOS presented from it; Back returns to the sheet
  under it, and popping both closes both. An ordinary route pushed while a sheet
  is on top is shown inside that sheet, which keeps its detents (on iOS and macOS
  in its own navigation stack, on the web and Android in the sheet's box, on
  desktop Linux in the box the sheet's route was laid out in); Back returns to
  the route before it in the sheet, and on iOS and Android dismissing the sheet
  closes them all. Over a `fullscreen` route a push is a page of its own. On Android a
  modal route is Material's bottom sheet at its detents, over a scrim, dragged
  between them or down to dismiss; pushes and pops cross on Material's shared
  axis, and Back is previewed from Android 14 (`host/android/README.md`, "Sheets
  and route motion"). Both move on the agent's clock there: `clock settle` before
  reading where a sheet or a route is. Under the agent's frozen timing (the
  default) on iOS, a `tap`, `back` or `type` that opens or closes a sheet, or pushes
  or pops a route, waits up to two seconds for UIKit to finish it, its answer's
  `then` included. A reply with `settled: false, reason: "transition"` says the
  transition had not ended, or Exact had not followed it yet. `--timing platform`
  leaves that to `clock settle`.
- A full-screen route (`navigationPresentation="fullscreen"`) that paints nothing
  stands on the platform's background. One whose author wrote a `background-color`
  (`transparent` included) or a `background-image` stands on nothing else: the page it
  covers stays under it, inert, and shows through where its background is translucent
  or while it fades out, as in CSS (a photo viewer's black fading to the chat it came
  from).
- A sheet's heights are `navigationDetent`, space-separated words: `large`,
  `medium`, a point height, a percentage of the sheet's maximum height (`"45%"`,
  React Native's `sheetAllowedDetents: [0.45]`; above 0% and at most 100%) or
  `fit-content` (the route's content height; a menu or
  a short dialog), which goes alone or as `"fit-content large"`. Write a share of
  the screen as a percentage, never points computed from a measure. Heights go
  shortest to tallest on every screen, as UIKit takes them: before `large`, which
  goes last, use one kind (points, or percentages with `medium` as half), each
  taller than the one before; `"300 45%"` crosses on a small screen. `"100%"` is
  `large`, and a point height a screen's maximum reaches, with a taller detent after
  it, is left out there. A percentage, like a point height, leaves out the bottom
  safe area, which UIKit adds below. A literal with
  another word or order is refused; a bound one iOS cannot use is journaled (`logs`)
  with the route, and the sheet keeps its other detents or opens `large`.
  `fit-content` measures the route laid out alone, its
  height left to its children, so nothing sized from the sheet counts; on iOS every
  viewport unit (`vw`, `vh`, `vmin`, `vmax`, and kin) is the window's in every sheet, never the
  sheet's, so `min-height: 100vh` opens the sheet at its maximum. A scrolling
  route is measured the same way; its rows can keep the default `flex-shrink: 1`.
  The route does not pad `env(safe-area-inset-bottom)`: UIKit adds that band
  below the detent.
  iOS sizes a sheet, and so does the web on a window narrower than 900 px: it opens
  at the same height, its foot on the window's, and stays there (no grabber, no
  drag between heights), with the bottom safe area below, and a route pushed inside
  it stays in it at that height. There it is the tallest sheet in a window under 480 px tall
  (UIKit's compact height) and on a touch screen while a text field in it has the
  focus, as UIKit raises one over the keyboard; a wider window draws the card. macOS
  and Linux ignore the detent (LLP 1075.003 §9.11, LLP 1115.000 D4.8).
- A sheet opens at its first height, the shortest, as UIKit opens one, unless the
  route says `navigationInitialDetent`, one of its `navigationDetent` words:
  `navigationDetent="medium large" navigationInitialDetent="large"` opens tall with a
  half-height stop (React Navigation's `sheetInitialDetentIndex`). It is read when
  the detents are set, so changing it alone does not move an open sheet.
- A sheet's grabber is the platform's unless the route says `navigationGrabber`:
  iOS draws one on a sheet with several detents, which it resizes, and none on a
  sheet of one height; `"visible"` or `"hidden"` says otherwise (UIKit's
  `prefersGrabberVisible`). Draw no grabber of your own.
- Without tabs, the routes are the root's own children, laid out the same way.
- Tests reach a tab by `tap`, or deliver a location as `type <root> "/saved"` (LLP
  1038 D11), which calls the root's `navigate`. `back`, an operation and a test
  step, is the platform's Back on every host (LLP 1116.006 D4): an open dialog's
  cancel, else the top sheet dismissed, else the pushed route popped, as iOS's Back
  button, the browser's Back or ⌘[ goes; with nothing to go back from it fails. On
  the web `tap <root> history -1` is the browser's history alone (native hosts
  refuse it).

A `head` node supplies document metadata. The innermost active value wins for
each field. `head edited=dirty` marks a document with unsaved changes (the dot in
a Mac window's close button; nothing elsewhere). Before a window closes or the app
quits, every element with a `beforeunload` handler hears it, DOM's event: an action
that calls `preventDefault()` keeps the window open — the browser asks "Leave
site?", a Mac or Windows app asks its own question — and `close()` closes the window once
it is answered (studio diary R17). Any other element's `title` is HTML's tooltip. `scroll document` declares page scrolling. Route `render`/`activate`
policy belongs to the site's renderer/build pipeline; follow
[the document fixture](../contract/corpus/document.contract) and
LLP 1048.003. Do not claim SEO,
streaming, or deployment correctness from a client-only screenshot.

### Head tags on the web

`app.json`'s `host.web.head` lists tags the web build writes into the `<head>` of every
document: the shell, each page rendered at build or per request, and the 404 page. It is
for what a site says once to the browser: an analytics script, a preconnect, site-wide
JSON-LD. A page's own title and metas stay its `head` element's.

```json
"host": { "web": { "head": [
  { "link": { "rel": "preconnect", "href": "https://www.google-analytics.com" } },
  { "script": { "async": true, "src": "https://www.googletagmanager.com/gtag/js?id=G-XXXXXXXXXX" } },
  { "script": { "text": "window.dataLayer = window.dataLayer || []; function gtag() { dataLayer.push(arguments); } gtag('js', new Date()); gtag('config', 'G-XXXXXXXXXX');" } },
  { "script": { "type": "application/ld+json", "text": { "@context": "https://schema.org", "@type": "WebSite", "name": "Euxy", "url": "https://euxy.example/" } } }
] } }
```

- An entry is one `script`, `link` or `meta`, whose value is its attributes: a string, or
  `true` for a bare one such as `async`. A script's `text` is its content; an object or a
  list is written as JSON, with `<!--`, and `</script` before whitespace, `/` or `>`,
  written `\u003C…`, which JSON reads alike. A `text` that is a string is written as it is,
  and one holding either sequence is refused, since the page's HTML parser reads it there:
  write `"\x3C!--"` or `'<' + '!--'` inside a string, or a space after `<` in code.
- The build refuses another tag, a malformed entry or attribute name, and what Exact
  writes or reads itself (the charset, the viewport, the `head` element's fields, the icon,
  manifest, theme colour and colour scheme, a `meta` named `exact-…`, a script or preload
  of its own `app.js`, `glue.js` or `app.wasm` however its URL is spelt, and a script or
  link of an `application/vnd.exact.…` type such as its checkpoint), naming the entry. A
  `rel` is read as its words, so `"shortcut icon"` is an icon.
- A script runs once per document. When the runtime changes the route in place, the tags
  stay and do not run again, so an analytics tag counts that change only if it watches the
  browser history. A page that loads no runtime (an `interaction` page with no handlers)
  follows a link as a new document, which runs them again.
- A script does not write inside `#exact-root`: the runtime owns that tree, and the page's
  sheet keeps the rules there only for what Exact writes (LLP 1047.003 D1). What a script
  adds elsewhere, such as a banner on `body`, meets every rule of the sheet.
- The dev loop and `serve` send them too, so a tag there reports local visits. `serve` and a
  static host send no Content-Security-Policy, so the scripts run as on any page. The Rust
  render server (`<app>-render --serve`) sends one. It admits each inline script here by its
  hash and each script `src` by its origin; a JSON-LD block needs neither.
- What those scripts load or contact in turn is the app's to allow there, in
  `host.web.csp`. A list adds sources to Exact's for that directive. A string replaces
  Exact's value, and the word `exact` in it stands for Exact's sources. `false` sends no
  policy. `host.web.cspReportOnly` takes the same shape and is sent as
  `Content-Security-Policy-Report-Only`. Google's tag, for example, loads its script and
  sends its hits from other origins:

  ```json
  "host": { "web": { "csp": {
    "script-src": ["https://www.googletagmanager.com"],
    "connect-src": ["https://*.google-analytics.com", "https://*.analytics.google.com", "https://*.googletagmanager.com"]
  } } }
  ```

  A directive Exact does not write starts from the one CSP falls back to (`'self'`, through
  `default-src`), so a list never refuses what the page could load before. Directive names
  are lower case. The build refuses a `,`, a `;`, a line break, or a list item with a space
  in it, since each would end a source, a directive or the policy; the browser reads the rest.
- Two keywords do not add. Exact's `script-src` holds hashes, and beside a hash the browser
  ignores `'unsafe-inline'`; `'strict-dynamic'` makes it ignore `'self'` and every host
  source, so Exact's own `app.js` stops loading. Either goes in a string without `exact`
  (`"script-src": "'self' 'unsafe-inline' https://www.googletagmanager.com"`), which also
  admits Exact's inline scripts.
- A report-only policy reports to a `report-uri`. `report-to` needs a `Reporting-Endpoints`
  header, which the server does not send, so its reports reach only the console.
- A static host sends no CSP. A page that wants one there says it in `host.web.head` as
  `{"meta": {"http-equiv": "Content-Security-Policy", "content": "…"}}`, or the host sends
  it as a header. The runtime and the page module (`modules/web/index.js`) load from the
  page's origin and inject no inline script, so `script-src 'self'` admits them. The
  policy must still admit what the render server's does: `'unsafe-inline'` in
  `style-src` (without it the runtime's style sheets are refused and the app does not
  start), `data:` and `blob:` in `img-src`, and `'wasm-unsafe-eval'` in `script-src` when
  the page runs WebAssembly (a Rust data module, Markdown, text flow and motion do). A
  page rendered at build carries inline scripts, the one that activates it among them:
  under `script-src 'self'` add each one's hash, which the browser's refusal names
  (`'sha256-…'`), or the page stays as rendered.

## Time, motion, graphics, and platform facts

`clip-path` clips a box and its contents. Use `polygon(0% 25%, 100% 0%, 50% 100%)`
for an outline that follows the box's size, or `inset(4px round 8px)` for an
inset with rounded corners. Coordinates and radii take `px`, `%`, or unitless
zero. Insets take one to four edges; `round` takes one to four radii and an
optional `/` followed by vertical radii. `circle()` and `ellipse()` take lengths
or `closest-side`/`farthest-side`, with an optional `at` position (one or two
lengths or position keywords). The default reference is the border box, or the
fill box on a web SVG element. Percentages follow its current size; a circle's
percentage radius uses its diagonal divided by √2. `polygon(evenodd, …)` uses
the even-odd fill rule. `path('…')` accepts SVG path data in fixed CSS pixels;
`url(#id)` references a `clipPath` on an SVG element. Other reference boxes,
calculations, relative length units and three/four-value positions are refused.
Basic shapes on SVG children (including groups, text and nested `svg`) are
web-only; a build targeting native hosts refuses a known basic-shape value.
Computed `url(#id)`, `path()` and `none` remain supported. A computed basic
shape on native SVG is ignored with a host note. Use a basic shape on a
surrounding box, or a `url(#id)` clip on the SVG element in `userSpaceOnUse`
coordinates without percentages instead.
See [CSS Masking 1 §5](https://www.w3.org/TR/css-masking-1/#the-clip-path).

A task has one `every(ms, action)`, `after(ms, action)`,
`every(frame, action)`, or keyed `change(action)` entry, on the line under it:

```text
  task ticker mount
    every(1000, tick)
```
 The action is parameterless. Millisecond intervals
are whole-number literals of at least 1. The frame form has no delta-time argument and does not
catch up missed display frames. For deterministic tests, use the driver's clock.
The clock moves in advances, and one advance commits at most 4,096 timer actions
and `then`s (`TIMER_FIRE_LIMIT`), those that change nothing included; the rest is
refused, what committed is kept, and the clock stays at the last one's time. A
fast `every` under a long `clock +N` can reach it: tick slower or move the clock in steps.

Use `change(action)` for work that must follow an input change, such as reading
layout or issuing a host command:

```text
  task reveal when visible key=selectedId
    change(reveal)
```

`key=` is required. The action runs once after the first committed state with an
open gate, then once after each changed key or reopened gate. It runs in a separate
commit after the host has applied the preceding commit, with no elapsed-time wait.
Pending key changes coalesce: the action reads the latest committed state. Closing
the gate cancels pending work. A refused action is spent until its key changes or
its gate reopens; a refused input commit queues nothing. On the agent's clock,
`clock settle` runs this work without advancing time. A reload starts fresh.
The compiler rejects an action that can write or refresh its own trigger's inputs,
including through derives, resource arguments, called actions, mutations or other
change tasks (`analyze-change-cycle`). Keep pure computed values in `derive`.
`change` takes no timer options; existing `after` and `every` schedules keep their
elapsed-time behavior.

On Android, these tasks can run while the activity is in the background and its
process can execute; they need no drawn frame. `every(frame, …)` pauses there.
Timers do not keep the process alive or bypass Android's
[Doze and App Standby](https://developer.android.com/training/monitoring-device-state/doze-standby)
restrictions. Use the background packages for work the system schedules;
an in-process poll cannot guarantee a timely alert while the app is suspended.

A task with `when` (a gated task), such as
`task hide when toast != "" key=toastUntil` with `after(5000, expire)`, has its
timer only while the gate holds, as an `if` arm has its nodes, and a new key
restarts it, as a new `for` key makes a new row
(LLP 1092). Nothing
runs when the gate changes: turning true arms the timer from that commit's time,
turning false drops it, and an idle task keeps no host awake and commits
nothing at rest. `key=expr` alone means `when true key=expr`. An `after` fires
at its deadline, its action seeing `performanceNow()` equal to it under a test's or the agent's
clock and no earlier on a device: clear without re-testing the time (a strict
`performanceNow() > until` can do nothing there). A repeat that missed ticks while the app was
suspended fires once on a device, so measure time with `performanceNow()`, never by ticks. The
gate is a bool and the key a string, number or bool; neither may read `performanceNow()`
(`analyze-task-gate-clock`): gate on state and let the timer measure time. A
toast, a debounce (`when draft != saved key=draft` with `after(800, save)`), a
round's tick (`when screen == "play"`), a flight's frames
(`when flying` with `every(frame, step)`) and a sheet's opening
(`when top(nav).name == "icons" key=top(nav).id` with `change(reveal)`) are each
one gated task. A child at a bounded use may declare the same forms: each instance
starts its tasks when it mounts and their gates hold, and stops them when a gate
fails or the instance is destroyed. Actions run in that instance's frame. A
covered screen or hidden tab can stay alive, so pass its activity as a prop and
gate work on it; visibility alone grants no permission. Each instance of an
`every(frame, …)` task runs an action per frame while armed. Reload carries
compatible state and restarts timers, as it does for root tasks.
On the deterministic clock, equal-deadline tasks run in declaration order;
instances of one declaration run in creation order. A gate or key change keeps
that order; destroying and recreating an instance gives it a new place.

**Retry backoff.** For a bounded ladder of delays, use one gated task per step.
Only the active step has a timer, which wakes once at its deadline:

```text
  state waiting = false
  state attempt = 1
  task retry1 when waiting and attempt == 1
    after(250, again)
  task retry2 when waiting and attempt == 2
    after(500, again)
  task retry3 when waiting and attempt >= 3
    after(1000, again)
```

Have `again` clear `waiting` and send the request. On failure, increment `attempt`
and set `waiting` again; reset `attempt` to 1 on success. A new attempt then arms
its step's timer. If the step stays active between retries, use `key=attempt` to
restart its timer. Computed intervals remain refused. A wait that must track a
continuous value can use a gated `every(frame, check)` and compare
`performanceNow()` with its deadline; see "Following the transport" for tempo.

**Repeating while a condition holds.** Use a task with `when` and `every`. The
timer runs only while the condition is true. Any action that makes it false
stops the timer.

```text
  state pulsing = false
  state dim = false
  task pulse when pulsing
    every(1200, step)
  action step
    if dim
      dim = false
    else
      if busy
        dim = true
      else
        pulsing = false
```

The first `step` runs one interval after the condition turns true. Do not build
a loop from mutations: a `then` cannot send its own mutation
(`analyze-then-self-send`). For a purely visual loop, use a CSS `animation`
instead.

`performanceNow()` is the runner's clock in milliseconds since this launch of the app (it
restarts at 0 on every launch, and a window opened later reads the same clock; the driver's clock under the agent, from 0), as the web's
`performance.now()`, not a date: a deadline of `performanceNow() + ms` sent to a server is
in 1970, and one stored is meaningless after a relaunch. Anything stored or sent takes
`time.epochAtZero + performanceNow()` ([timer recipe](recipes/timer-that-survives-relaunch.md)). For the date, read the reserved
`exactTime` source and add `time.epochAtZero + performanceNow()`. There is no
`now()`: it is refused (`type-now-renamed`) with those two repairs. Its fields, which a shape declares as it reads them:
`epochAtZero` (Unix milliseconds when `performanceNow()` read zero), `utcOffset` (minutes east
of UTC), `locale` (BCP 47, the first preferred language), `formatLocale` (BCP 47, the
regional format the platform's own formatters use: macOS and iOS's Region, so English
with a Korean region is `en-US` and `en-KR`, with the hour cycle the user forces as
`-u-hc-h23` or `-u-hc-h12`, so `Intl.DateTimeFormat(time.formatLocale)` writes the time in
the device's 12- or 24-hour cycle; Android's locale with that hour cycle; `locale` where a host keeps none apart),
`timeZone` (IANA), `resolvedLocale` (the language of
the string table the app shows, `""` with no tables) and `seed` (a whole number
below 2^32, drawn once per launch, so `%` and `>>> 0` take all of it; mix a round
into it in 32-bit steps, as the [random recipe](recipes/random-and-shuffle.md) does). A read does not itself schedule a future render, and a
derive that reads `performanceNow()` is not read again as time passes, and when it is read
again differs by host. For a displayed value that must follow the clock, keep the
time in state that a timer's action (`task … every`) writes. Prefer `clock settle` to waiting for a transition in real time.
`time.utcOffset` is the zone's offset *now*: every host answers it again when the
offset at the clock's instant changes (a DST change, a new zone), checked before a
timer fires, so a midnight timer after the clocks change reads the new offset.
Under the agent it is the drive's zone at the virtual date, checked after each
`clock` (the JS target also before each timer inside one). It is not the offset
of an arbitrary timestamp: to show a past or future instant across a DST change
in the viewer's zone, format it in TypeScript with
`new Intl.DateTimeFormat(time.locale, { timeZone: time.timeZone })`.

Use admitted CSS transitions and keyframes. Check which properties animate and
which require optional capabilities. `-exact-spring(…)` (a `transition` timing
function), `-exact-exit-animation`, `-exact-layout-transition`, and presentation timelines have
specific documented behavior;
they do not admit arbitrary frame callbacks or a second app-state graph.

A native list (`list appearance="auto"`) applies a change at once, as UIKit
does, unless a row carries a layout transition or an exit animation. Then a
change in which rows it holds, or in their order, animates on iOS: rows slide
to their new places and rows that come or go fade, with UIKit's own timing,
not the written duration. Its first rows, a restyle and a list out of a window
still apply at once.

macOS and iOS play eligible box `translate`, `scale`, `rotate` and `opacity`
animations and transitions in Core Animation. A transform stays sampled when
the box takes input, contains an interactive view, uses a noncentral origin,
is sticky or has a layout transition. Filtered pictures also need sampling.
Mounting an interactive descendant returns a running transform to sampling.
On macOS, a box with `backdrop-filter` keeps its transform sampled so the
blur's sampling region follows it. Rounded background colours stay sampled
because AppKit can paint their fill on a separate layer or into an image.
The agent's clock still seeks these animations; taking the clock returns
transitions to the engine.

As in CSS, a `transition` or `animation` value's units, keywords and function
names read in any ASCII case (`0.3S EASE` is `0.3s ease`), its numbers are CSS's
(`1.s` and `infs` are no times), and a keyframes name keeps its case: in
`animation="Spin 1s EASE"` the name is `Spin` and `EASE` is the easing.

A `rotate` keyframe accepts the node's rotation syntax: an angle (degrees when
unitless), optionally with `x`, `y`, `z`, or a three-number axis. For example,
`to rotate="-90deg x"` folds a card edge-on. An angle-only keyframe uses the
node's declared axis, so `to rotate=-90` on `rotate="0deg x"` also folds about X.
An explicit keyframe axis overrides the node's; differing axes interpolate by
spherical interpolation, while a common axis preserves full turns. SVG elements
and route-transition pictures refuse spatial keyframes because their native
transforms are 2D; animate a surrounding box or a box inside the route instead.

In a virtualized list, `-exact-animation-trigger="view"` (the default) holds a
prefetched row's animations until the row first overlaps its list's port.
`"none"` starts them on insertion. Reveal preserves authored pauses; leaving
and returning to the port replays only if the row was retired and mounted again.

### Motion that follows a scroll

A value that follows a scroller is a scroll-driven animation, CSS's own, not a `scroll` action
that writes state (that runs a round trip behind the scroll):

```contract
keyframes follow
  to translate="-1000px 0px"
keyframes fade
  from opacity=0
  to opacity=1

component App
  view
    column timeline-scope="--cols"
      row overflow="hidden" width="12rem"
        row flex-shrink=0 animation="follow 1s linear both" animation-timeline="--cols" animation-range="0px 1000px"
          text "Names"
      row overflow-x="auto" width="12rem" scroll-timeline="--cols x"
        text "Columns"
      column overflow-y="auto" height=300
        for i in range(20) key=i
          view height=60 flex-shrink=0 animation="fade 1s linear both" animation-timeline="view()" animation-range="entry"
```

- **Timelines:** `scroll-timeline` (or `-name` and `-axis`) on a scroller is its offset;
  `view-timeline` (`-name`, `-axis`, `-inset`) on any node is that node crossing its nearest
  scroller. `animation-timeline` names one, or is `scroll()` or `view()`; a sibling's name
  needs `timeline-scope` on a shared ancestor. One timeline per node and kind.
- **Ranges:** `animation-range` takes lengths and percentages of the timeline, and on a view
  timeline `cover`, `contain`, `entry`, `exit`, `entry-crossing` and `exit-crossing`, each
  with an optional length or percentage.
- **Paint rows only:** keyframes under a timeline animate opacity, transforms, colours and
  `box-shadow`. Every host moves them in the scroll's own frame; the web leaves them to the
  browser, and a browser without scroll-driven animations shows the nodes' own values.

Transform springs retain velocity when interrupted. An SVG path's `d` instead
plays the spring as an easing curve: retargeting starts from the displayed path
with no inherited velocity, including when toggling back (LLP 1055.000 D15,
LLP 1062 D3). This is the same on the web and native hosts. Use a transform for
a rigid path move that must keep its momentum.

To seed a translate spring from an authored pan, write
`-exact-translate-velocity=` followed by a string such as `"800 -600"`: finite x and y
velocities in logical CSS pixels per second. In `panrelease(vx, vy)`, clamp or adjust
each axis, then update this row and the translate target in the same action.
`"0 0"` starts from rest, including on cancellation; omission or Contract `none`
(without quotes) preserves an interrupted spring's velocity. For example, bind
``-exact-translate-velocity=(released ? `${vx} ${vy}` : none)``. Changing only velocity
never restarts motion. Enabling a translate spring after `transition="none"` can
start from nonzero velocity even when the endpoint equals the current position.
The row persists: clear it when a new drag begins or before an unrelated target
change that should inherit velocity. It has no effect on an easing transition or
with `transition="none"`.

Computed animation longhands cannot accompany a literal list in `animation` or
another longhand. For a list that changes with state, compute the complete `animation`
shorthand, including each animation's play state or duration.

For drawing and pointer-tracking, `pointerdown`, `pointermove` and `pointerup`
hand an action that takes it a `PointerEvent` (`offsetX`/`offsetY` from the
node's content box, `button` and `buttons`, `pressure`, `pointerType`,
`pointerId`, and `clientX`/`clientY` from the viewport, `frame()`'s space), on
any node, a canvas included; set `touch-action="none"` on a drawing surface.
The web browser fixes `touch-action` when the touch starts. On iOS, the host
reads it when UIKit asks whether the pan may begin, so a write in that touch's
`pointerdown` applies there. For the same behavior on every host, set `none` before the
touch starts (LLP 1057.001).
Any button goes down, numbered as the DOM's: `button` 0 primary, 1 middle, 2
secondary, 3 back, 4 forward (-1 on a move), `buttons` bits 1, 4, 2, 8, 16. A
`contextmenu` action may take the same record, where the click was. The back
and forward buttons go Back and Forward at their release, as a browser's do,
unless a `pointerup` handler calls `preventDefault()`. `wheel` hands a `WheelEvent` (deltas,
modifiers; a trackpad pinch is a Control-held wheel) and `preventDefault()` keeps
the scroll from happening; `drop` hands a `DragEvent` whose `files` are `doc:`
handles of any type, a folder's included, with the drag's `text` and `url`,
and takes a file drag over a field inside its node; an app can show a drop
overlay at `dragenter`, which the `drop` action clears when the drop lands
and `dragleave` clears otherwise; a `paste`'s `ClipboardEvent` carries `files` too. See
[Pointer](contract-grammar.md#pointer) and [Clipboard](contract-grammar.md#clipboard).

Use `keycapture=shortcut` on a container when its shortcut must run before
the focused field's `key=` handler. Capture runs outermost first, then the
target's capture and `key` handlers, then `key` bubbles outward. A capture
handler can call `stopPropagation()` to stop later handlers, including the
target's `key` handler when called there. `preventDefault()` keeps the field
from editing. The container gains no Tab stop from `keycapture` alone.

`key` is the DOM's keydown and `keyup` its keyup; an action that takes it hears
a `KeyboardEvent` (`metaKey`, `code`, `repeat`, `isComposing`, …). An Enter or
Escape handler at a field skips `e.isComposing`, as web code does: on the web
the Enter that picks an input method's candidate arrives composing. Track a held modifier as a
page does: set it from the event on every keydown and keyup, and forget it when
the window loses the focus, since a key let go in another app sends no keyup
(the page's `window` `blur`). `exactPage().hasFocus` turning false gates the task
that forgets it, and the hint shows only while the window has the focus:

```contract
shape Page
  hasFocus: bool

component Hints
  resource page = exactPage() as shape Page
  state meta = false
  action held(k: string, e: KeyboardEvent)
    meta = e.metaKey
  action forget()
    meta = false
  task release when meta and not page.hasFocus
    after(1, forget)
  view
    column key=held keyup=held testId="list"
      if meta and page.hasFocus
        text "⌘ held"
```

Drive it with `type "list" key "Meta" down`, `prefer has-focus false` and
`clock +1`. While an input method composes text in a field on macOS or iOS, a
modifier's own keys still reach these handlers, and a ⌘ chord commits the
composed text first (its `input`, then the chord's `key`), so a composer's
⌘Enter `key` action sends what was typed; a button that declares ⌘Enter is not
pressed then. See [Keys](contract-grammar.md#keys).

A long-press or right-click menu is a `popover` the node names with
`contextPopover="<id>"`: its `button` rows (with `popovertarget="<id>"
popovertargetaction="hide"`), `hr` separators and `role="group"` sections of rows
(`contract vocab menu`) are the menu, and one row with
`contextPreview=true` is the preview, whose `press` is what tapping the preview
does (open the conversation). iOS presents UIKit's context menu, with the row
lifting and the preview popping into the screen its press pushes; macOS an
`NSMenu` without the preview; the web and the default agent open the popover anchored to
the node. The menu needs the popover menu-shaped (LLP 1021
D3): each row its text and at most one `image`, with `aria-checked` for a checkmark;
a row with anything else beside its label (an icon box) makes macOS open the
painted popover anchored to the node instead. The node's own `contextmenu` action runs first, so one popover can
serve every row of a list. A node inside an open popover opens its menu over
that popover, which stays open, as HTML nests them. The default agent opens it with
`tap <node> contextmenu` (LLP 1021 §5.1). On an iOS simulator under `--touch platform`,
use a [real long press](recipes/ios-driving.md).

The order of a menu's rows is the platform's unless the menu's trigger says otherwise.
UIKit's own order puts the first row nearest the finger, so a menu that opens upward
from a button near the bottom of the screen shows its rows last first. `menuOrder` on
the trigger (the `popovertarget` button, a `header` button that opens a menu, or the
node with `contextPopover`) says it: `"fixed"` keeps the rows as written wherever the
menu opens (a list of presets), `"priority"` always puts the first row nearest the
finger, and `"automatic"`, the default, is the platform's. iOS and tvOS set it as
`preferredMenuElementOrder`; the web, the Mac and the other hosts always show the rows
as written, so they have nothing to apply.

#### Tips

`tip="<title>"` on an element is a tip that teaches it (LLP 1035.006.000), with
`tipMessage` under the title and `tipImage` a symbol beside it (`symbol:<role>` or
`symbol:sf/<name>`). The element's `id` names the tip, which the platform remembers across
launches, so the build refuses a `tip` without an `id` (`lower-tip-id`), and a `tipMessage`,
`tipImage` or `tipDone` without a `tip` (`lower-tip-parts`). `tipDone=<bool>` says the person
has done what it teaches; once true the tip is retired for good. iOS, iPadOS and macOS show
it as TipKit's popover over the element, or over the navigation bar or toolbar item drawn
for a header's button, when TipKit says it may: one at a time in document order, while the
element is on screen and uncovered, never again after the person closes it. An empty title
is no tip, so a computed one offers it only when it applies. The web, Android, Linux,
Windows and the terminal have no tip system and show none: put nothing essential in a tip.
A plan that binds `tip` links the `tips` capability (TipKit and SwiftUI on Apple). The
driver's `state.tips` lists each tip's TipKit status, whether it shows and what it points
at. Recipe: `tips`.

#### Swipe to reply

`swiperight` moves its node with the finger: it follows up to 64 points, then a fifth of
the finger's travel, and the action runs if the node is at or past 64 points when the
finger lifts. A finger reaches it only with `touch-action="pan-y"`, which leaves vertical
drags to the list. A `transition` on `translate` springs the node back after the lift;
without one it jumps back. The node's first child with `swipeIndicator=true` goes from its
own `opacity` and `scale` to 1 as the swipe reaches 64 points, and back by its own
`transition` after the lift. Give the node `position="relative"`, so an absolute indicator
is placed against the node and not against the row around it. `symbol:reply` is a speech
bubble (SF Symbols' `bubble.left`); Messages' left-turning arrow, `arrowshape.turn.up.left`,
has no named symbol, so an app declares it as an icon token. The iOS host plays a selection
haptic as the swipe crosses 64 points, so the action plays none. `apps/messages`' bubble
is a whole example.

`swipeleft` is the mirror, for Discord's reply or a bubble of your own on the right: the
node follows a leftward drag and the action runs at or past 64 points to the left. A node
may have both, each direction running its own action. The directions are the screen's,
in a right-to-left layout too. Neither reveals buttons: a row's Delete is `delete=`
(`contract vocab swipe`).

```contract
component Thread
  state replyTo = ""
  action reply(id: string)
    replyTo = id
  view
    column align-items="flex-start" padding=12 gap=8
      box position="relative" swiperight=reply("1") touch-action="pan-y"
        transition="translate -exact-spring(300, 30, 1)" testId="bubble-1"
        image "symbol:reply" swipeIndicator=true position="absolute" left=-32 top=8 opacity=0 scale=0.5
          transition="opacity -exact-spring(300, 30, 1), scale -exact-spring(300, 30, 1)" aria-hidden=true
        text "Lunch tomorrow?" padding="8px 12px" border-radius=18 background-color="-exact-fill"
      if replyTo != ""
        text "Replying to “Lunch tomorrow?”" testId="reply-bar"
```

```contract-test
test "a swipe right answers the message"
  tap "bubble-1" drag 100 0 from 10 10
  clock settle
  expect tree has "reply-bar"
```

A test's drag is a real touch on an iOS simulator: start it where 100 points stay on the
screen, and `clock settle` after it, so the spring back has ended when the screen is read.

#### A confirmation

"Remove book?" with a message, Cancel and a destructive Remove (React Native's
`Alert.alert`) is a `dialog role="alertdialog"`: its `aria-label` is the
title, a `text` the message, each action a `button` with `press` that closes
it, and one Cancel, a closing `button` without `press`. Open it from a button
(`commandfor="<id>" command="show-modal"`) or from any action with
`showModal("<id>")` — a context-menu row's `press` asks before it deletes:

```
action askRemove(id: string)
  pending = id
  showModal("remove")
…
dialog id="remove" role="alertdialog" aria-label="Remove book?"
  text (title + " will be removed from your library.")
  button press=remove(pending) destructive=true commandfor="remove" command="close"
    text "Remove"
  button commandfor="remove" command="close"
    text "Cancel"
```

iOS shows the centred alert, its Cancel kept; the web its modal `dialog`;
macOS `NSAlert`, a sheet on the window, its Cancel on Escape and Return on the
one `buttonStyle="filled"` button that is not destructive, when there is exactly
one (LLP 1115.000 D7.6). Text, a cancel and at most three buttons make an
alert; several actions with no text (an "Open in…" chooser) make an action
sheet, unanchored on an iPhone (its Cancel drawn) when it has a Cancel and a popover
at its invoker when it has none, so a tap outside can dismiss it (LLP 1115 D6).
An alert has no outside, on every host (LLP 1115.000 D13.4): Escape and Back
cancel it (the Mac's one-button alert excepted, below) and a tap or click beside it does nothing, which is `closedby="closerequest"`,
implied for the alert's shape, so leave `closedby` unwritten. Write
`closedby="any"` only to ask for the platform's dismissible form in its place:
an action sheet on an iPhone (a popover on an iPad), a dialog a click outside
closes on the web, a sheet a click on the window behind it closes on the Mac, a
Material dialog a tap outside cancels on Android.
`close("<id>")` from an action closes it as its Cancel does.
Escape (and Command-period on the Mac), Back and a dismissal outside it are
HTML's close request: the `dialog` hears `cancel`, then `close`. Its Cancel, an action
and `close("<id>")` fire `close` alone, as a `command="close"` button does in a
browser. iOS reports a hardware Escape at its alert as the Cancel's tap, so
there it fires `close` alone.

An alert that only tells ("Couldn't save the note." and OK) is a `text` and its
one closing button, with no action: iOS's alert with that one button, macOS's
`NSAlert` with it as the default (Return presses it and Escape does nothing, as
on AppKit's own one-button alert), the web's modal `dialog`, Material's dialog
on Android. Under `closedby="any"` it is the dialog's own sheet on the Mac, which a
click outside dismisses; iOS keeps the alert, since an action sheet drops a
lone Cancel in an iPad's popover. The action that learns of the failure sets the state its `text`
reads and calls `showModal("<id>")` ([tell the person something failed](recipes/error-alert.md)).

An alert's own `font-family`, written on the `dialog` or its class, applies to
its title, message and button labels on web, Linux, Android and Windows.
Android and Windows keep the platform face when the family is only inherited.
iOS and macOS keep the system face: `UIAlertController` and `NSAlert` have no
public font API, so the compiler warns when an alert requests a family there.

A prompt (a rename's, `Alert.prompt`) is the alert with a text field: an
`input` among its rows, which counts as its message when it has no `text`:

```
dialog id="rename" role="alertdialog" aria-label="Rename Pattern"
  input value=draft input=editDraft placeholder="Name"
  button commandfor="rename" command="close"
    text "Cancel"
  button press=rename disabled=(draft == "") commandfor="rename" command="close"
    text "Save"
```

iOS shows `UIAlertController` with the field (`addTextField`), macOS `NSAlert`
with it as the accessory view, the web the `dialog` with the field on a line of
its own. The field is the authored `input`'s, going through its machinery: the
input's value writes reach it (what is typed stands until the page writes the
value), its `placeholder`, keyboard, `disabled`, `readonly`, `maxlength` and
`beforeinput` are its, each keystroke is its `input` and the end of editing
its `change`, so the action's `press` reads what was typed. Return is one
rule on every host: the input's `key` handlers hear it first (and may prevent
it); a field with a `submit` handler then commits and runs it, and that is
all, as a hand-built field whose delegate handles Return; any other commits
its edit and presses the first action that is not destructive,
whatever `buttonStyle` says; while that action is disabled Return does nothing, so a Save with `disabled=(draft == "")`
opens dimmed and enables as a name is typed. iOS draws no action bold, as a
hand-built alert does not, unless the action Return presses says
`buttonStyle="filled"`: then it is the alert's preferred action, bold. (An alert
with no field prefers its one filled action that is not destructive, as the Mac
makes it the default button.) Only the alert's shape takes
one field at a time: with it, a cancel and at most three buttons, no `for`
(`lower-alertdialog` says which row breaks it).

A menu row's second `text` is its item's grey subtitle on iOS when it is a line
under the first (two texts in a block `button`, or a `flex-direction="column"`
one), with or without an `if` around it; two texts side by side in a row, or one
positioned `absolute` (a count at the corner), stay one title.

A submenu is a row whose `popovertarget` names another menu popover (`Copy ▸
path / link`): a submenu `NSMenuItem` on macOS, a nested `UIMenu` on iOS, and
on the web and under the agent the nested popover, opened by `tap <row>`. Place
it beside its row with `position-area="right span-bottom"`; give its items
`popovertarget="<outer menu id>" popovertargetaction="hide"` so a choice closes
the whole menu; write no `press` on the row that opens it (a native menu never
runs it); and draw the web's `›` as an `aria-hidden` text, since the native
menus draw their own arrow (LLP 1021 §5.2).

For the Mac menu bar, put `-exact-menu="app"` on an ordinary button to add
an item without a shortcut. Literal destinations are `about` (directly under
About, where Check for Updates… goes), `app`, `file`, `edit`, `view`, `go`,
`window`, `help`, and `none` (no item: the chord stays a shortcut);
explicit placement overrides the usual `aria-keyshortcuts` chord filing. A
placed item shows and takes the button's chord holding ⌘, or else one holding ⌃
(`aria-keyshortcuts="Control+Space"`); a chord of ⌥ or ⇧ alone stays out of
the menu, since those type. ⌃Space is macOS's default shortcut for Select the
Previous Input Source, and where it is on the system takes it first. Its
accessible label or text titles it, and `disabled` disables it. An item in the
app menu or Help is the app's, so it stays enabled while no window is key and
under a sheet or a Contract modal dialog. Keep the button reachable: the agent taps it,
and other hosts show that ordinary button, with no application menu.
On the root, `-exact-menu-remove="show-tab-bar close-all"` removes
selected standard items. Its whitespace-separated list must be literal; unknown
ids and non-root use are refused. The fixed ids are `close-all`, `show-tab-bar`, `show-all-tabs`, `enter-full-screen`,
`minimize`, `zoom`, `bring-all-to-front`, `show-previous-tab`, `show-next-tab`,
`move-tab-to-new-window`, `merge-all-windows`, `back` (Go ▸ Back), `forward` (Go ▸ Forward), `develop` (the
Develop menu; its chords stay), `paste-and-match-style`, `find`,
`spelling-and-grammar`, `substitutions`, `transformations`, `app-help` (Help ▸
*App* Help), `actual-size`, `zoom-in`, `zoom-out`.
Other standard items remain; other hosts ignore removal.
`-exact-menu-keys="close-window=none"` on the root keeps File ▸ Close Window
with no chord, so ⌘W reaches the page; declare `-exact-menu-remove="close-all"`
with it, or AppKit's Close All ⌥⌘W shows as a row of its own.
`close-window=Meta+Shift+W` gives it ⇧⌘W instead: one `aria-keyshortcuts`
chord with `Meta` that is not the host's or the system's own. View has Actual Size
⌘0, Zoom In ⌘+ and Zoom Out ⌘-, a page zoom as a browser's: the viewport,
`px` and `rem` all follow it; an app's own zoom commands set it with
`setPageZoom(factor)`. See the grammar’s “The Mac’s menu bar”.

Two more presentations of a button reach past the window
(LLP 1133.025 D4, D5). `dock` beside a menu
id, or alone, also files the button in the app's Dock menu:
`-exact-menu="file dock"` is New Conversation in File and in the Dock menu.
`-exact-menu="status"`, alone, makes the button the app's status item: a menu
bar item on macOS, a notification-area icon on Windows. Its text or
`aria-label` is the item's title and tooltip, one `img` or `svg` child its
image, and its `popovertarget` menu the item's menu. On macOS the image is
a template. On Windows a `symbol:` uses the taskbar's text colour, an `svg`
is drawn as authored, and a file-backed `img` is contained at the current
display's small-icon size and reloaded when its DPI changes. The app's icon
is the fallback when the image is absent, still loading or fails to load.
On macOS a click opens that menu, or
runs `press` when there is none; on Windows a left click runs `press`, or
opens the menu when there is none, and a right click opens the menu. On both
the button takes no room in the page; elsewhere it is an ordinary button. An
accessory app's status menu ends with the host's Quit, and a Windows status
menu with Exit; `-exact-menu-remove="status-quit"` removes either. On Windows
a placed status item leaves the close button closing the app, as Windows'
guidelines advise; only `keepRunning` makes it hide the window. Bare `focus()` brings the
window forward on macOS and Windows, showing it if it was hidden (Linux logs
it as unsupported); `hideWindow()` puts it away again with the app still
running, as the person's close does under `keepRunning`, or on macOS while a
status item is placed (refused in the journal elsewhere, and where
the app would end without its window); the agent lists the items as
`state.statusItems` and `state.dockMenu`.


`frame(id)` and `measure("literal-id")` are action-only geometry reads returning
`Geometry` (`x`, `y`, `width`, `height`, `provisional`, `unavailable`). Handle `unavailable` and `provisional`.
Native answers are provisional while text in the subtree has estimated geometry,
or an image or video awaits its natural size. Accepted text measurements clear
the estimate at the next layout; unrelated subtrees keep their settled answers.
The id is an element's `id`, not its `testId`: a literal id no element in the view
has answers `unavailable` on every host, and the build warns (`lower-geometry-id`).
`frame` reads the last layout's border box
where the viewer sees it, as `getBoundingClientRect` does: in the viewport, through
every `translate`, `rotate` and `scale` on it and above it (the bounding box of a
turned box), with every scroll offset above it (the page's too) applied, so a drop
target needs no scroll bookkeeping and a dragged card is where it shows. Natively a
transform in flight counts at its end value; the web reads it mid-flight. `measure`
reads the border box as if its `height` were `auto`, at the same origin, with every
other style kept: its in-flow content, plus its padding and border, within its
`min-height` and `max-height`, and the height opposing `top` and `bottom` insets
give an absolutely positioned box. So a navigation root whose children are all
absolutely positioned, with no padding, border or `min-height` of its own,
measures 0 however tall it is laid out. Motion in flight does not count on
any host: `measure` reads a `height` transition at its end and a
`-exact-layout-transition` at the laid-out box, and the motion plays on, so a box
mounted under a parent that is still growing reads its settled size in its first
`resize`. Read a laid-out
size, the screen's included, with `frame`, and size a sheet as a share of the
screen with `navigationDetent="45%"`. Before the first layout both answer
`unavailable` with zeros, as on the web until the page's geometry code loads after
first paint; natively a mount task's `after(1, …)` runs after that layout.
`measure` is also `unavailable` for a box with no single size (an inline run, or
one under `display: none`); on the web while a `keyframes` animation of a font's
weight, stretch or variation settings plays in the box or above it, with no
event when it ends; and, natively, while the tree has a change not yet laid
out or a Mac entry has content regions registered.
Neither is a computed style binding to run every render.
`elementFromPoint(x, y)` names the front-most of the same boxes at a viewport
point by its nearest `id` (`option<string>`), through the same transforms. For a
point read that sees through an overlay, `elementsFromPoint(x, y)` lists every
hit node's own id, front to back, including hit ancestors; id-less nodes are
skipped (except an id-less hit outside its named ancestor's box includes that
ancestor once). `querySelectorAll("[data-paint~=\"accent\"]")` lists the own ids of
mounted, displayed, non-inert nodes bearing the declared `data-paint` word in
tree order. Covered routes and unselected tabs do not answer; a split list
answers with its detail where the host shows them side by side (the web at desktop
widths, and iOS where UIKit's split view shows both columns; over the detail only the
list answers, LLP 1127 D8, and `state window.split` says which shows).
Closed popovers and dialogs do not answer; open ones do. Point reads skip inert
subtrees and use the presenter's placement for open top-layer content. Point reads
also name the front modal dialog or presented route where its native backdrop accepts input,
beneath any later popover. Native sheet chrome outside authored content is
not part of the backdrop. The backdrop's hit region is independent of the
content's clip and pointer events; it never exposes the page or a lower
modal. Both reads run only in actions and give one snapshot. Use `frame(id)`
for each returned id when drawing outlines.
For a drag still built by hand, test the dragged node's visual centre (the middle of
its `frame(id)`, which includes the `translate` the drag gave it), not the
pointer: the box under a lifted card is the card.

SVG uses SVG names. Native SVG geometry has incomplete support for `calc()`,
`min()`, `max()` and `clamp()`, including nested SVG viewports and `<use>`
placement and sizing. Use numeric lengths or percentages there; CSS layout math
on the outer SVG element does not imply support inside its scene.
SVG attribute-only lengths accept user units (unitless), `px`, `in`, `cm`, `mm`,
`pt`, `pc` and `%`: line/linearGradient `x1`/`y1`/`x2`/`y2`, radialGradient
`fx`/`fy`/`fr`, marker `markerWidth`/`markerHeight`/`refX`/`refY`, and each item
of SVG text/tspan `x`/`y`/`dx`/`dy`. Relative units and function text such as
`calc()`, `env()` and `var()` are refused in literals and literal template parts
(LLP 1055.000 D4). Literal concatenations are checked together; an incomplete
fragment that could finish an accepted unit is left to runtime. Literal choices in
conditionals, matches and inlined derives are checked too. Bound values use the
same grammar: an unsupported computed value clears the attribute and journals
`runtime-svg-length` on every target. Marker `refX` also accepts
`left`/`center`/`right`, and `refY` accepts `top`/`center`/`bottom`; native hosts
currently place these keywords at 0.
`foreignObject` renders directly on the web, but draws nothing through SVG `use`.
The compiler warns at a `use` whose unambiguous, unconditional literal target contains
one (`lower-svg-use-foreign-object`). Native hosts refuse `foreignObject` at run
time; for content on every host, position an ordinary box over the `svg`. Canvas 2D calls
live in a data module, and GPU/game surfaces in their optional module. A
hyphenated native tag must be declared by a dependency's `exact.json` `views` or
listed in `app.json`'s `modules` (the bake refuses others). An app module's unknown
attributes pass through unchecked as its props. A package view refuses an attribute
declared as neither a prop nor an event with `lower-native-attr`, apart from standard
box attributes and common module events. A known attribute binds to the module's
box, which takes layout, box and paint rows, handlers, `testId`, `id`, `role`, ARIA,
`disabled` and `inert`; any other known name (`color`, `value`, `command`, `href`) is refused,
so give the module prop another name. A package view's declared non-layout prop
(`exact-camera`'s `object-fit`) goes to the view when written on the tag, even where
the name is a known attribute. Declaring a layout attribute as a view prop is
`bake-view-prop-layout`. A class's rows style only
the box and set no view props; write `object-fit` on the camera tag to configure its
viewfinder. A renamed spelling (`fontSize`) is refused whatever is declared. Do not
turn a missing widget or canvas operation into invented Contract syntax.

In an agent drive (a `*.test.contract` cannot set an extension's substitute with
`prefer` yet), `@exact/camera` accepts
`prefer extension camera.picture={"path":"assets/test/card.jpg"}` for a still,
or `prefer extension camera.clip={"path":"assets/test/walk.mp4"}` for a clip.
Both name app assets. A clip is decoded once and loops on the session clock,
from the track's start: `clock +100` advances its frame by 100 ms. The
viewfinder, a package reading the track's picture and `takePhoto` receive
the same frame. A screenshot sequence follows that clock too. The journal
records the clip's SHA-256, not its frames. Clear either preference with
`=clear` before setting the other; setting both is refused in the journal.
This works on Android, iOS, macOS and web; Linux has no camera substitute.
For a fixture shared across these hosts use H.264 MP4 and a web browser with
H.264 WebCodecs support. See the [camera package's guide](../packages/camera/test/checklist.md#agent-pictures-and-clips).

On Apple, each package's Swift library loads at its first call, view, picture,
authentication or wake. Permission, preference and foreground updates are kept
until then; they do not load an unused package. A package declaring `"start":
"load"` starts after first pixel. An app's own Swift that imports a package's
Swift module loads that library with the app module. Under the agent,
`state.extensions.<name>.implementation.loaded` reports whether it is mapped
(LLP 1067.000.001).

Put vendor `.xcframework` bundles beside an Apple native module's Swift files
(`modules/apple`, or its platform-local replacement). The Apple build selects
by platform, device or simulator, and architecture from each bundle's `Info.plist`.
Static libraries are linked into the module. Dynamic frameworks are copied with
their resources into the app's `Frameworks` (`Contents/Frameworks` on macOS) and
signed with the app's identity before the app is signed, in development and
production, including versioned macOS frameworks (`Versions/A`). Their install
names must use `@rpath/<Name>.framework/…`. An xcframework may carry macOS and
iOS slices together: source capture skips the binary bundle, including its
version links. Changing a selected binary requires a new native build. Framework
filenames must be unique across package libraries, module dependencies and the
host runtime (`ExactCore.framework`); a collision fails the build, naming both sources.

Apple native modules can include asset catalogs beside their Swift sources:
`modules/apple/*.xcassets`, or `ios/modules/*.xcassets` / `macos/modules/*.xcassets`
when the app has a platform-local module directory. The local directory replaces
`modules/apple` for that platform; tvOS uses the iOS directory. The Apple build
compiles these catalogs together with its generated assets into the main bundle.
On macOS, a catalog makes the dev build and agent launch the assembled `.app`.
Load custom symbols with `UIImage(named: "donut", in: .main, with: configuration)`
or `NSImage(named: "donut")`; SwiftUI uses `Image("donut", bundle: .main)`.
`symbol:sf/...` still looks up system symbols only. A colour set named
`AccentColor` is the app's accent on Apple hosts (`NSAccentColorName`; the
accent section above). Catalog changes require a
new native build; they are not web assets or live update payloads.

An Icon Composer project is selected explicitly by `host.ios.icon` or
`host.macos.icon` in `app.json`, as an app-relative `.icon` directory; tvOS
does not inherit it. It compiles with the catalogs and replaces the native
raster icon from `icons`, which still supplies the web. See the
[icon inputs in the reference](reference.md) for conflicts and fallback.

An iOS native module that uses `canOpenURL` to check for another app lists
its schemes in `app.json`: `"host": { "ios": { "queriesSchemes":
["comgooglemaps", "waze"] } }`. The build writes `LSApplicationQueriesSchemes`.
List at most 50 schemes, without `:` or `://`; changing the list requires a
native rebuild.

An app that was renamed, or whose name Siri mishears, lists the other names Siri
and App Shortcuts' `${applicationName}` should answer to: `"host": { "ios": {
"alternativeNames": [{ "name": "Oldname", "pronunciationHint": "old name" }] } }`,
and `host.macos.alternativeNames` for the Mac. The build writes
`INAlternativeAppNames`, and copies the iPhone app's into its watch app, which
inherits none and must list the same. At most three; the hint, optional, is
written as the name sounds. Apple admits names based on the app's own or ones its
users commonly call it, never another app's. Changing them requires a native
rebuild.

Write pressed `background-color`, `color` and absolute `opacity` in a
`:active` block. `-exact-press-opacity` multiplies the node's presented opacity;
`-exact-press-scale` multiplies its presented scale. The host applies these
values while a finger or mouse holds the innermost pressable inside its box.
Sliding off, cancellation or a pan releases it. Custom nodes ease over 120 ms;
native controls keep the platform's timing. The agent applies the target at
once, so `"tap row down" "screenshot x.png" "tap up"` shows it. Pressing requires
no runner round trip. Active `color` reaches text that inherits the node's
colour; descendants with their own keep it. On custom nodes, authored feedback
replaces the platform's unsaid press. Native buttons and native list rows keep
unwritten fields of the platform's highlighted state. Leave feedback unwritten
for the platform default. A press that changes layout uses app state.

`-exact-hit-slop` grows where a pointer reaches a node without moving or
painting anything, as React Native's `hitSlop` and a UIKit `point(inside:with:)`
override do (Charlie, 2026-11-10). It takes one to four nonnegative lengths, as
`padding` does, with no percentage; `-exact-hit-slop-top` and its siblings set
one side. `-exact-hit-slop="12px"` on a 24px icon button takes a tap 12px
outside it. Where boxes overlap, the front one still takes the pointer, slop
and all, so a button written after a slopped one keeps its own box. An
`overflow` other than `visible` on the node or an ancestor clips the slop.
`tap <target>` still aims at the box's middle, and `elementFromPoint` counts the
slop. On the web the slop is a positioned pseudo-element, so it reaches from the
padding edge (a border's width comes off it) and a node that says no `position`
becomes `position: relative`. An `image`, `input` or other replaced element
refuses it with `lower-hit-slop`; give it to the box or button that holds one.

Haptics are already there (LLP 1077 D14). `-exact-press-haptic` (`selection`,
`impact-light|medium|heavy|soft|rigid`) plays at touch-down without a round
trip, as `-exact-press-scale` does. `haptic("selection" | "impact-…" | "success" |
"warning" | "error")` is a host command an action runs, for example when a
drag crosses a threshold. iOS uses the feedback generators; the web vibrates
where it can; Linux does nothing.

An interface size setting is `rem` plus `setRootFontSize(px)`, CSS's `:root {
font-size }` (LLP 1069.000 D3). Size what should scale in `rem` (text, control
heights, paddings) and what should not in `px`; an action calling
`setRootFontSize(size)` re-lays every `rem` out in its own commit, on every host.
The app's size stands over the host's (the browser's setting, iOS Dynamic Type's
body, 13 on macOS, 16 on Linux, and 16 times Windows' Text size factor), as an author's `html { font-size: 20px }` stands over a
browser's font-size setting; `setRootFontSize("medium")` hands it back, so a
"Default" choice that follows Dynamic Type calls that. Smaller and Larger that
should scale Dynamic Type instead of replacing it are percentages:
`setRootFontSize("87.5%")`, `("125%")`, CSS's percentage of `medium`, so the root
is that share of the host's size and follows it when Dynamic Type or the browser's
setting changes. A size of 0 or less is refused (a literal at compile time, a computed one in `logs`). It is not kept
across a launch: a root `task restore mount` with `after(1, applySize)` sets the
stored size again. Do not multiply a scale factor into every size instead.

Platform facts are reserved sources (`exactViewport`, `exactPage`, `exactDelivery`,
`exactSurface`, `exactCanvasPerformance`, `exactTime`, `exactGeometry`, `exactNativeChain`,
`exactSafeAreaUnsettled`); the bake refuses a declared field the source does not have.
`exactDelivery()` includes the string fields `version` and `buildNumber`: the
installed binary's `EXACT_VERSION` and `EXACT_BUILD_NUMBER` at bake time (defaults
`0.1.0` and `1` only when unset). Over-the-air updates leave them unchanged; an
older bake without them answers `""`. The web JS build uses the same variables
at build time, with `""` for each unset variable. Rust-rendered pages read those
values from the web build, including when the server has neither variable set.
`state.delivery` mirrors them. Its boolean `production` is whether the
installed binary was baked with production trust (`EXACT_UPDATE_TRUST=production`:
Apple's `--release`, `--distribution` and `exact release`, Android's `--bundle`,
`host/web/build.mjs --out`; other builds are development ones unless the variable
is set). It is the answer the runner uses to keep agent mode and development
overrides out of a shipped binary, and an over-the-air update cannot change it.
Gate a developer-only control on it, and gate what that control reads as well, so
a production build never asks for it:
`resource overrides = loadOverrides() when not build.production as shape Overrides`.

`exactGeometry(id[, space[, node[, regions]]])` reads a node's geometry from the layout
just committed (LLP 1123 D6). `space` is `"owner"` (the default, the document's outside
every content owner), `"window"`, or `"node"` with the `id` of the node whose border box
is the space; `regions` is UIKit's region query options, `"inactive"`, `"mirrored"` or
both. Its declared shape chooses what it reads: `available`, `generation`, `size`,
`frame`, `observations` and `margins`. `generation` is the number of the layout the
answer reads, 0 before the first. `observations` has an inset set (`top`, `right`,
`bottom`, `left`) per family: `safeArea`, `keyboard`, `keyboardUndocked`,
`safeAreaCorners`, `layoutMargins`, `layoutMarginsCorners`, `readableContent`,
`readableContentCorners`, `readableContentGuide` and `systemMinimumLayoutMargins`.
`margins` has the `layoutMargins`, `readableContent` and `readableContentGuide`
rectangles (`x`, `y`, `width`, `height`). Declare every field but `available` and
`generation` an `option`, and each family and rectangle in them too: each is `none`
before the node is laid out and wherever the host does not have it, and the bake refuses
a shape that does not say so. `frame` is the border box as laid out, before the scroll
offsets and transforms above it; it is `none` in `"window"`, which only the host can
place, and in a `"node"` space in another owner. Until hosts report an owner's band,
keyboard and margins and the session view's band (LLP 1123 stage 3), a node inside an
owner reads `none` for its safe area and margins and 0 for the keyboard, and one outside
every owner reads 0 for each; on the web every quantity is `none`, and the terminal host
does not deliver the facts. A later layout that changes what it reads answers it again in
a commit of its own, so a branch on it lays out after the layout it read; a chain of such
commits stops after eight, holding the last answer until a layout with another cause, and
a development build alerts `geometry-feedback`. The bake refuses a path from a geometry
fact to what moves the geometry it reads (`bake-geometry-feedback`): any route's
`navigation*` attributes, its bar (its first `header`) and a branch before that header,
the root's tab bar, a toolbar the host places, the system bars, a write of the router's
slot, a popover, a sheet's detents, a picker, full screen, picture in picture or sharing,
a scroll position or snap, `scrollIntoView`, focus, and the window's fitting or
orientation. The path may be direct or run through a branch, a `display` or a class that
hides the node, a `for` key, a derive, a resource, state, a mutation, a child's prop, or
an action that runs without the person: a task's, a sequence's `ended=`, that of an event
the person does not make, such as an image's `load`, or that of a layout event (`resize`,
`intersect`, `reachstart`, `reachend`, `transformgeometry`), which runs on the fact
whenever the view reads one. An action the person runs, by a press, a key, a scroll or
focus, may read a fact, as a UIKit app chooses a popover or a sheet by its width when a
button is pressed: it runs once per event, so it cannot feed back. A fact may decide
content: sizes, text and which content shows. A report that owes no layout, such as a
band no style reads, reaches the facts at the next layout. `exactNativeChain(id)` answers
`available` false, its `entries` `none`, until hosts report their native chains.
`exactSafeAreaUnsettled()` answers `available`, `count`, the boxes the last layout left
unsettled, and `ids`, the `id`s of those that have one.

A TypeScript-only update currently removes the plan's source-shape requirement
when it pairs the plan with its module. If that update first reads a field of a
reserved source that the installed binary predates, delivery can admit the plan
although the binary cannot answer the field. Release a new native binary and
cohort before pushing the first use of a new reserved-source field. This applies
to every reserved source; a C ABI bump alone does not make the paired plan
require that ABI.

`exactViewport().displayMode` is a string: the current CSS display
mode on the web, `fullscreen`, `window-controls-overlay` (the tall title row
`display_override` asks for) or `standalone` on macOS, `standalone` on other
native hosts. Declare it on the resource's shape; `state.device.displayMode`
reports it too. `prefer display-mode standalone` substitutes it on Apple and
in Chrome, from a page's boot and through a reload, so a test on the web
reaches an app's standalone branch; the browser's own `display-mode` media
query does not change, and Firefox and WebKit refuse the override. This reads the window, not a fullscreen navigation route.
`exactViewport().devicePixelRatio` is `option<number>`: display pixels per
logical point, updated when the display scale changes. It is `none` before a
host reports its scale, including a server render and the terminal. It is the
display's ratio; a game's render scale does not change it.
`safeAreaTop`, `safeAreaRight`, `safeAreaBottom` and `safeAreaLeft` are numeric
logical points with the same document-inset meaning as `env(safe-area-inset-*)`.
`segments` is a list of rectangles with numeric `x`, `y`, `width` and `height`
fields in the layout viewport's coordinates, ordered by row then column. With
no fold it contains the whole viewport. Declare the fields your app uses in a
segment shape and `segments: Segment[]` in its viewport shape. Insets and
rectangles update when the host's document geometry changes, in the same commit
as the size a rotation brings.
`keyboardHeight` is how much of the layout viewport the software keyboard covers,
in logical points, before the page avoids it, whatever `interactive-widget` says:
CSS's `env(keyboard-inset-height)`. It is the keyboard's layout guide on iOS (a
dialog sheet's field included, though the page under the sheet is not avoided and
the agent's `layout.env` then says 0), the IME's inset on Android, and on the web
`navigator.virtualKeyboard.boundingRect` where the page overlays the keys, else how
far the visual viewport falls short of the layout viewport's bottom
(`innerHeight - visualViewport.height - visualViewport.offsetTop`). Under
`resizes-content` without the VirtualKeyboard API the keys shrink the page itself
(its `height` is the shorter one) and nothing covers it: 0. It is 0 with no
keyboard, and on macOS, desktop Linux and Windows. It changes in the commit
that shrinks the viewport, so `height + keyboardHeight` holds still while the keys
move. With a hardware keyboard iOS still shows a short bar of its own, and this is
its height: a row of the app's own on the keys that suits only a software keyboard
tests a height (`when viewport.keyboardHeight >= 120`). A `row role="toolbar"
toolbarPlacement="keyboard"` at the bottom of the page or of a sheet route sits on
the keys under every `interactive-widget`: a sheet's layout ends at the keys while
its field has the focus, whatever the root says; on the page, `overlays-content`
lifts the toolbar onto them and `resizes-content` ends the page there.

`exactViewport().pointer` and `.hover` are CSS's `pointer` (`fine`, `coarse`,
`none`) and `hover` (`hover`, `none`) for the primary input: the web's media
queries; `fine` and `hover` on macOS, Linux and Windows, and `coarse` and `none`
on Android's touch screen and on a Linux one with no mouse or touchpad, read once
at launch; on iOS `coarse` and `none`, or `fine` and
`hover` while an iPad has a trackpad or mouse attached; `none` and `none` on tvOS
and on Android TV.
They change while the app runs. Show a game's on-screen stick under `coarse`
(the starter does, `game/recipes/touch-controls.md`), never by platform. The web's
neutral look sizes its own controls for a touch device instead (`any-pointer:
coarse`: any coarse input, normally a touch screen), so they keep their size
while an iPad's trackpad comes and goes. Under the agent each is the device
class's from launch; `prefer pointer coarse` and
`prefer hover none` substitute them. Chrome emulates a touch screen, so it takes
`coarse` with `none` and `fine` with `hover`, and refuses `pointer none`; the
Firefox and WebKit carriers refuse both.
`exactViewport().accessibilityTextSize` is true while the person's text size is one
of iOS's accessibility sizes (AX1 to AX5, UIKit's `isAccessibilityCategory`), where
UIKit's own views drop decoration and stack what sat side by side; it is an Exact
fact, not a CSS media feature, read wherever Exact draws with UIKit's metrics (iOS,
iPadOS, tvOS, visionOS, watchOS), and also true on Android at `Configuration.fontScale >= 1.3`, Exact's convention
for Android's large-text range. It reads the system setting independently of any
root font the app authors. It is false on the Mac, Linux, Windows and the web.
`EmptyState` drops its symbol then. Under the agent on iOS, `prefer root-font-size
28` (Dynamic Type's AX1 body) or more sets it. That `prefer` sets only the size of the
text Exact draws: UIKit's own text, a native field's among them, keeps the simulator's
text size. To see a screen at the largest size, as `verify` does, set the simulator's:
`xcrun simctl ui <udid> content_size accessibility-extra-extra-extra-large`. With no
size, `content_size` prints the current one: set it back after.
`exactViewport().screenReader` is a string: `"on"`, `"off"` or `"unknown"`, live
as the system reading changes and refreshed on resume (LLP 1039 D10). Apple reads
VoiceOver; Android reads touch exploration, a proxy for TalkBack or another screen
reader; Windows reads `SPI_GETSCREENREADER`. The web, the terminal, the renderer
and Linux without detection report `"unknown"`. `"off"` means the platform reported
it off; it never means detection was unavailable.
Use this fact to pause disruptive self-updating content, such as rolling text or
auto-advance, never to select a separate reduced UI. Pause on `"on"`, and treat
`"unknown"` as possibly on for optional rotation. Keep the same content accessible
and offer manual advance. Reduced motion is an independent reason to pause.
Declare `screenReader: string` on the viewport shape. The agent's
`prefer screen-reader on|off|unknown` substitutes it;
`state.device.screenReader` reports it even without a resource.
See [a rolling stat](recipes/rolling-stat.md).

`exactViewport().gpuTier` is `low`, `mid` or `high`: how much GPU work a frame can
afford, chosen once before the GPU device is made from what the adapter reports
(a phone GPU of the Adreno 600 class is `low`), else from the screen (a phone-sized
one `low`, a computer's `high`), never from a measured frame time; a phone-sized
screen is never `high`, nor is any iOS or Android device natively. A game's renderer follows it (`Game::quality`); a HUD can
read it to offer a quality setting. A native host answers its GPU module's tier
once the module loads (`high` before it). Under the agent it is `high` from launch, and
`prefer gpu-tier low` substitutes it and remakes each world's renderer at its rung.
`exactPage().platform` identifies the host from the first frame. Declare
`platform: string` or an enum over the following values in the page resource's shape.
The native hosts report
`ios`, `macos`, `tvos`, `watchos`, `android`, `linux` or `windows`.
A browser or server-rendered web page reports `web`, a terminal app reports
`terminal`, and a runner booted without a host reports `headless`. A browser
on an Android phone therefore reports `web`.

`exactPage` answers `visibilityState`, `frozen`, `onLine`, `canShare`,
`canShareFiles` and `canSharePictures` (whether `share(files=…)` takes
`app:/` files and an element's `"#id"`; the web never takes a picture),
`canOpenFiles`, `name` (the window's name: `main` in the app's first window on
every host, a link's `target` in the window it opened, empty in a `_blank` one;
every window runs the root's tasks, so `task t when page.name == "main"` runs
once however many are open), and `hasFocus` (`document.hasFocus()`: the app's window has the
system's focus; false while another app or window is in front, so an app can
choose an in-window message over a system notification), `persisted` and
`restores` (the back/forward cache: `persisted` is true while the page is put
away, from a `pagehide` with `persisted` until the `pageshow`; `restores`
counts those `pageshow` events, so `task t when page.restores > 0
key=page.restores` runs once for each return, never at launch; a Mac or Windows
window a `keepRunning` close hid is put away, a minimized one only hidden), and
`presented` (below). A reminder that should
open a route uses `showNotification(title="Done", navigate="/thread/42")`
(`@exact/notifications`); its click reaches the navigation root as a warm
`navigate` event or a cold launch location, and without `navigate=` it is
`notificationclick` on the root. Under the agent `state.notifications` lists
each post, and `tap @ref` clicks one. Under the agent each
is the drive's (`prefer has-focus false`; `state.device`). Use dimensions,
media preferences, page facts,
and capability state rather than suffixing files by platform. Preference facts
inform authored policy; the engine does not automatically remove all motion.

`frozen: bool` means the host has entered a lifecycle state in which ordinary
work is subject to stopping until execution resumes. The runner immediately
applies the transitions defined for that fact. Gate a feed with `when not
page.frozen`: its resource or stream goes in the commit that turns `frozen`
true, without waiting for an authored cleanup handler. A sequence stops in that
commit too (LLP 1125 D4). The fact is not itself a process pause, and it is not a
phone or desktop label: `visibilityState` can be hidden while `frozen` stays
false.

On both web targets, the transition into `frozen` also settles physical output
still waiting for presentation, whether it came from a sequence or a standalone
command. Sound plays, haptics, note-ons, MIDI Start, Continue and Timing Clock
are discarded. Sound stops, note-offs, all-notes-off and MIDI Stop are delivered
in order before suspension. Returning does not restore a discarded start.

| Host | `frozen` producer |
|---|---|
| Web (both targets) | Page Lifecycle `freeze`/`resume` on `document`, where fired; false elsewhere |
| Android | Stopped activity without a promoted foreground service carrying a live claim or playing media |
| Linux desktop | Constant false |
| Windows desktop | Constant false |
| iOS and tvOS | Background without active audio eligible for background execution; an idle playback category grants nothing |
| macOS | Constant false |
| watchOS | Constant false: the live watch driver has no page lifecycle producer yet |
| Terminal and render hosts | Constant false |

A browser promises nothing after `hidden`: it may stop or discard a page without
a `freeze` event. False does not promise continued execution. Persist data as it
changes; do not depend on a final asynchronous cleanup. The bake and an agent
launch answer false; `prefer frozen true` substitutes the fact and its defined
transitions, and `prefer frozen false` resumes them. The drive's clock and host
pump continue: timers, replies, hatch callbacks and native announcements can
still run. Qualify process suspension on a real host, outside a drive. See the
[feed recipe](recipes/suspending-feed.md).

`presented: bool` is true from the present of the page's first picture: the
moment a person can first see it, and the moment the animations its first commit
began start, since every host starts a commit's new animations at the present of
the first frame that shows them. Time anything else from it, such as a launch
splash's minimum display time (`task hide when page.presented` running
`after(600, done)`), not from mount: on an Android cold launch the first frame
can reach the screen hundreds of milliseconds after the first commit. It stays
true for the page's world; a dev reload or an activated update starts it false
again, and hiding, freezing or the back/forward cache never clear it. It is true
at the first commit under the agent, headless and in the render host, where
nothing presents; `prefer presented false` holds it false so a drive can watch
what waits on it, and moves no animation. The bake answers false.

Leave navigation presentation to the host first: a root `tablist` becomes UIKit's
tab bar on iOS and a sidebar on macOS, and `navigationPresentation` chooses a
route's presentation. Neither pointer coarseness nor `frozen` identifies a phone.

`exactCanvasPerformance("world")` is an opt-in, read-only metrics resource for
that named canvas. Declare only the fields your HUD needs, each as an option:

```contract
shape Metrics
  frames: option<number>
  cpuEncodeMs: option<number>
  gpuMs: option<number>
component App
  resource metrics = exactCanvasPerformance("world") as shape Metrics
  view
    canvas surface=world
```

A canvas without an active metrics resource does no HUD reporting work. A resource
whose `when` condition is false collects nothing. Snapshots arrive at most
once per second while the canvas is visible, including while it is idle. Before
its first snapshot, after disposal, and for unsupported measurements, fields are `none`. Values
never enter kept answers, world saves, replay or simulation hashes.

Numeric fields are `pixelWidth`, `pixelHeight`, `renderScale`, `devicePixelRatio`,
`effectivePixelRatio`, `samples`, `draws`, `triangles`, `instances`, `frames`,
`fps`, `windowMs`, `lateFrames`, `missedFrames`, `longestGapMs`, `maxFps`,
`cpuTickMs`, `cpuFeedMs`,
`cpuEncodeMs`, `cpuSubmitMs`, `gpuMs`, and each CPU/GPU duration's `MaxMs`
variant (`cpuTickMaxMs`, for example). Durations and gaps are milliseconds;
frame counters cover the sampling window and draw counters describe its latest
submitted frame. `fps` is the submitted-frame count over the monotonic wall time
between snapshots (`windowMs`); an idle window reports zero. The first snapshot
and a window driven by the agent's virtual clock leave FPS unavailable.
`processMemoryBytes`, `gameHeapBytes`, `textureMemoryBytes` and
`geometryMemoryBytes` are optional byte counts. `audioState`, `thermalState`
and `quality` are optional strings; `idle` is an optional boolean. Hosts and
surfaces report only measurements they actually support. The game renderer
reports render dimensions, scale, samples, draw counts, frame pacing, CPU
tick/feed/encode durations, and on every host: `quality`, the resolved rung
(`low`, `mid`, `high`) once a binding resolved it; `maxFps`, the frame cap in
force, `none` when the canvas draws at the display's rate; and `idle`, true
while the canvas asks for no frames until something changes (a world at rest
or paused; `none` under the agent's virtual clock). On Apple and Android,
`thermalState` is the last drawn live frame's `nominal`, `fair`, `serious` or
`critical` (Android moderate is `serious`). It is `none` without a reading in
the snapshot window, and always `none` on the web, Linux and Windows. Memory, in bytes:
`processMemoryBytes` is the process's footprint (`phys_footprint` on Apple, the
resident set on Linux and Android), `none` on the web and Windows;
`gameHeapBytes` is the GPU module's linear memory on the web, the simulation's
and renderer's heap, and `none` natively, where that heap is the process's;
`textureMemoryBytes` counts the delivered textures the renderer holds on the
device, retired ones awaiting compaction included (render targets are not
counted) and `geometryMemoryBytes` the renderer's
vertex and index buffers, on every host once the renderer exists.
`audioState` is a game's audio output where its host gives it one (the web,
Apple, Windows): `running`, `suspended` (hidden, or waiting for a gesture),
`interrupted` (suspended by the host's interruption: a call or alarm on iOS,
the window losing focus on Windows) or `closed` (no device: under the
agent's virtual clock, or not yet opened); `none` on the Linux family and for
a game without audio. `gpuMs` and `gpuMaxMs` are the mean and longest GPU
execution of the window's frames, from the first pass's start to the last
pass's end by GPU timestamps, read back a frame or two late: on Metal natively
(macOS, iOS, tvOS), on the web only where the browser grants
`timestamp-query`, and on the Linux family and Windows where the adapter has
timestamp queries; `none` elsewhere and for a window with no sample. A canvas
writes timestamps only while a plan reads the resource (or the agent's `perf`
is armed). `cpuSubmitMs` and
`cpuSubmitMaxMs` are the mean and longest wall time of the queue submission of
the frames the canvas drew in the window, on every host (on Apple the
submission also schedules the frame's presentation); a frame shared by several
canvases is one submission, which each of them reports. The 16 KiB snapshot limit is separate from the public surface
record's limit.

A GPU HUD can show its last successful publication while its world loads:
`resource hud = exactSurface("world") as shape Hud keep`. Only a successful
live publication is kept; defaults, refused publications and disposal do not
replace it. The saved value stands until the first live publication. A changed
surface name or incompatible declared shape discards it. Removing `keep` starts
from the shape's defaults again. Each kept answer has the data-resource limit
of 256 KiB, including its identity and shape, and shares the bounded launch
cache. An app whose device grants forbid kept answers stores none. Other host
facts, including canvas performance, cannot opt in.

`exactSurface(name)` is the host's channel for read-only facts about where
the app is running, not only for a GPU surface's published record. The
terminal host publishes `exactSurface("terminal")` (`mode`, `images`,
`colors`, `log`, which counts the `role="log"` nodes it has seen, and
`printed`, the `id` of that log's last child it wrote to scrollback), and an inline app retires printed entries from it (LLP 1101.001
P1; `apps/harness/terminal.contract` shows the task). Declare its shape
(`resource terminal = exactSurface("terminal") as shape Terminal`); on any
other host it stays unloaded. A new host fact uses this channel before
anyone proposes a new reserved source (LLP 1101.002 §0 P11).

A GPU canvas's record (`canvas surface=world()`, read as `exactSurface("world")`)
carries one host fact beside what its module publishes: `presented`, true once a
complete frame of the canvas's current picture has been handed to the compositor,
on every host. It is false until then, and again after a restore, a restarted
world, a lost GPU device or a new target, until the next such frame. It changes
only at those moments, never per frame. Keep a loader over the canvas until it is
true; a record that says the world is set up is not proof that a frame has
drawn. A module cannot publish a field of that name. A surface may have more
than one live canvas, on two routes at once: each runs a world of its own, and
the record is the first live canvas's.

```contract
shape Table
  presented: bool

component App
  resource table = exactSurface("world") as shape Table
  view
    main width="100%" height="100%"
      canvas surface=world() width="100%" height="100%" position="relative"
        if !table.presented
          text "Setting the table" position="absolute" inset=0 aria-busy=true
```

The terminal's `colors` is the selected output bit depth: `0` (monochrome),
`4` (16 ANSI colours), `8` (256 colours), or `24` (RGB). The host reduces
authored colours to that palette; at zero it emits no foreground/background
colour codes, while preserving bold, underline, italic and reverse-video
focus. Images use alternate text in monochrome. Authors can choose different
content, symbols or emphasis rather than relying on colour alone:

```contract
shape Terminal
  colors: number
component App
  resource terminal = exactSurface("terminal") as shape Terminal
  view
    column
      if terminal.colors == 0
        text "! Attention required" font-weight=700
      else
        text "Attention required" color="#d78700"
```

Interactive and headless CLI runs detect the palette from `TERM` and
`COLORTERM`. Nonempty `NO_COLOR` disables colour (including `NO_COLOR=0`);
empty `NO_COLOR` does not. `TERM=dumb`, `unknown`, an absent terminal type
without a colour hint, and unrecognized terminal types fall back to monochrome.
This is conservative environment detection, not a terminal capability query.
Use `--color auto|never|16|256|truecolor` on any terminal app or `exact-terminal`
to override it (`always` means `truecolor`; `--color=never` also works).
An explicit choice overrides `NO_COLOR`. The `--drive` verbs `print` and
`document` always write text; the selected palette also applies to `.ans`
screenshots. Run once from a shell (`--print`), `auto` writes SGR only to a
terminal, and `never` writes plain text, a picture as its alt text. A Rust
embedder may call `Host::set_colors(ColorDepth::Monochrome)` before starting
the loop or emulator; direct painting and `Vt` otherwise default to RGB for
deterministic tests.

A `text` inside a paragraph can carry `press=` (a `span`'s `onclick`) and
`href=` (an `a`). In the terminal a click on the run runs its
handler; without one (and on every host for `href`) it follows the link: a path naming one of the app's routes is `navigate` on the
navigation root, and an `http`, `https`, `mailto` or `tel` URL leaves the app,
as `openURL` does. An app without routes that wants a run to act in the app
(the LLP reader's cross-references) gives the run `press=`. In a terminal the
mouse is the app's only full screen or while a dialog is open; inline, ⌘-click
on the run's link is the terminal's own.

Localized strings use `t("key", name=value)` and app `strings/<locale>.json`
files. Compile against the files to check keys and placeholders. A language
setting calls `setLocale("de")` (`setLocale("")` for System) in its action; the
choice is kept across launches and read before the first frame. Formatting
functions accept a narrow set of literal formats; app wording is an app `fn`.
On Apple platforms the tables are the bundle's localizations, base first, so the
system's own words follow the table it picks. On the Mac a direction you leave
unsaid is AppKit's process direction: an Arabic table on an English Mac leaves
the window and panes left to right, and `state` reports `language.dir` as `ltr`.
Set `host.macos.layoutDirection: "rtl"` in `app.json` to mirror the whole window,
including the split view, toolbar and sidebar, even with an English table.
Omission or `"auto"` keeps AppKit's direction; a launch argument naming either
`AppleTextDirection` or `NSForceRightToLeftWritingDirection` takes precedence.
Elsewhere the direction is the table's. A `direction` you write
wins on every host. `language.dir` is the layout direction: on the Mac, text
under no written `direction` takes each paragraph's base direction from its
first letter, as an `NSTextField` does, so an Arabic sentence in a left-to-right
window keeps its full stop at its own end and stays aligned to the start.
Writing `direction` fixes the base of all text under it, so do not write it to
move a full stop (Markdown text keeps the layout direction). Because the Apple
binary lists its languages, adding or dropping a table other than an app's only
one is a new binary there; editing a translation still ships as an update.

#### Folds on iOS

`exactViewport().devicePosture` and the viewport segments are the web's
([foldables](recipes/foldables.md)). On the iOS 27.1 simulator UIKit moves an alert
(an `alertdialog` with text) on either fold, and a page or form sheet (a sheet route
or a modal `dialog`) on a book fold: folded like a book, an alert to the middle of
the left segment and a page or form sheet to that segment, its right edge about 12
points into the band (measured in left-to-right text); folded like a laptop, an
alert to the bottom segment, while a page or form sheet still spans the fold. The
tab bar stays clear of either fold. [Apple's adaptive-layout
talk](https://developer.apple.com/videos/play/tech-talks/111463/) says instead that
alerts move to the trailing side of a book fold and can move to the top of a laptop
fold, and that action sheets, menus and popovers move clear too, which was not
measured.

A route moved off a window edge is still padded as if it reached it: its top on a
laptop fold, its right on a book fold's segment 0.

## Inspection and testing

The native agent's `tree` includes declared SVG geometry in each shape's
`props`: a circle's `cx`, `cy` and `r`, an ellipse's radii, and a rectangle's
position, size and radii. These are model values, not animation samples.
Lengths in user units are numbers, including resolved font-relative lengths.
Percentages, viewport lengths and unresolved `calc()` and `env()` values
remain CSS text. `layout <target>` includes the same properties.

On Apple development builds, an `iframe`'s web content is inspectable from
Safari's Develop menu. Enable Safari's web developer features, launch the app,
and select its web view under Develop. This inspects the embedded page; use the
agent operations below for Exact's native tree. Production builds
(`EXACT_UPDATE_TRUST=production`), `exact release`, and IPA archives leave
web-view inspection disabled.

An Apple build's dev menu (a four-finger tap, or ⌘D on a hardware keyboard or a simulator's; on a Mac, Develop › App Info…, ⌘D, in a development build: a Mac build with production trust, `EXACT_UPDATE_TRUST=production`, or made by `exact release` has no Develop menu and takes none of its chords unless it is built or launched with `EXACT_DEV_MENU=1`, or a person runs `defaults write <bundle id> ExactDevMenu -bool YES` and the build did not say `0`; an iOS build, a release one too, has the menu unless it is built or launched with `EXACT_DEV_MENU=0`, LLP 1115 D9) uses a native grouped navigation sheet on iOS. Project commands and diagnostics have full-width rows; Build Information opens a separate, selectable text screen. On macOS, App Info opens with the
build it is in: the app and version, when, where and with which Xcode it was
built, the Exact commit and the app's own (with branch and a dirty flag), the
build kind, the device, and the app's release notes. `host/apple/build.mjs`
stamps these into the bundle's Info.plist on every build, `--archive` included,
so a shipped binary carries the build machine's host name and the commits too.
Release notes are `release-notes.md` beside `app.contract`: UTF-8 text, shown as
written, up to 16 KB; no file, no section. A deploy script that publishes the
build under a revision sets `EXACT_DISTRIBUTION_REVISION` for the build to show
it. Copy (iOS and macOS) takes all of it as text.

On iOS, the developer menu's **Frame Health** row opens a native card in a window of its
own above the app's, so above every sheet, alert and cover, whichever opened first, and
in the app's appearance; the Mac's Develop menu has the same item. Row and item are
checked while the card shows, and follow it when it closes itself. The card starts hidden
and takes the touches within its bounds: drag it from anywhere, even past the window's
edges, and on release it springs to the point along the edges of an area 8 points inside
the window, past the safe area and above any keyboard, that is nearest where it would
stop sliding at a constant 2,000 points/s²: a gentle release stays by the nearest edge,
and a fling reaches another. Accessibility actions move it to an edge, keeping its position
along the other axis. Close, at its top left, or the checked **Frame Health** dismisses
it. On iOS, Minimize, beside Close, leaves a small status and frame rate at the same edge,
8 points below the status bar, while measurement continues; tap it to restore the card.
In a screen corner, the small status moves along the edge until its corner keeps 8 points
from the screen's. Dragging and accessibility actions move either form, and rotation and
keyboard changes keep it within the window. Touches outside the visible form reach the app. LLP 1079.000 §7.2 describes its rows: Live, Paused
or Idle; the requested rate and any system limit; frame rate
and missed frames for the last 0.5 s and 5 s; pacing; and charts of frame rate and
frame time, or a table of work time, late frames, maximum frame time and pacing
changes. A tap on a chart expands it; a tap on the title defines each figure; Share,
at its top right, asks what to share, Agent Report, Summary or Image, and opens the
system's share sheet with that one item; where the card measures the screen, it offers
two agent reports, the screen's and Exact's frames'. The card states no pass or fail of its own;
Details, at the end of the line under the header, swaps the figures and charts for
`perf frames health`'s three results over the last 5 s with their reasons and the counts
behind them, in rows that scroll within the card; Done in its place returns. These are timing proxies; actual presentation and
native-only animation cadence are unmeasured. While the card is shown, bounded hitch
capture also records matching asynchronous owner and apply timestamps. Idle time never
becomes a frame time.

The card's charts take each frame and show the last 30 seconds: the frame rate's moving
average over the last half second against the request, and each frame's frame time and
work, the longest in each tenth of a second, against that frame's deadline. Within a run
of frames the lines reach across tenths of a second without frames; an idle app, a pause
and a reset leave gaps. The charts start no display link and store no file. Measuring
Exact's frames, they take the frames the app's clock already runs and move with the time
on a tenth-of-a-second timer of their own, which stops once their newest mark has left
them. Their figures update every half second, and at once when the source stops, on the
existing bounded publication pass in common run-loop modes, so touch tracking does not
suspend them. Measuring the screen, the charts take every UI update and glide on Core
Animation, and the card reads on a half-second timer of its own in place of
publications, which stops as it hides. The reader creates no display link, reads nothing
while the card is hidden, paused or detached, and keeps at most 60 half-second samples;
the first read after a pause covers the paused span. During native scrolling, a card
measuring Exact's frames stays on the app callback source; in development builds on iOS
18 and later the separate experimental scroll source reaches `perf frames` and the
report.

`perf frames health` exposes the metrics. `state.frameHealthUI` contains only the
card's visibility and window-local bounds, marked as host controls outside app nodes.
On iOS 18 and later the card's face shows every UI update of the app on its screen and
the main thread's work, which it measures only while it shows; `perf frames` reports
Exact's own frames (LLP 1079.000 §7.4).
Embedders opt in by passing a host view as `DevMenu.install`'s `frameHealthContainer`.

`app.json` may set `"frame_health": { "mode": "off" }` or `"core"`. Without a mode, a
build with development trust collects from launch, and a production bake on demand:
only while the card shows, and while it is hidden every collection hook returns at
its first check. `"core"` collects from launch in any build, and `"off"` never collects. Where
collection is off, the developer menu's row reads **Frame Health Disabled**, dimmed,
and a tap on it names the setting and says to remove it; on the Mac, choosing the item
does. The core has fixed
storage for up to 16 collectors per UI thread and does no per-callback allocation;
these bounds do not substitute for measuring whole-hook overhead on real devices.

### Startup marks of any build: `EXACT_OBSERVE_LOG=1`

An Apple app launched with `EXACT_OBSERVE_LOG=1` prints its startup measurement to stderr once launch
has settled, in a release build too. The one `observe: startup` line has the launch type (`cold`,
`warm`), how the first tree booted (`baked`), how time to interactive ended, Observe's metrics, and each
mark in ms from process start (`main`, `didFinishLaunching`, `boot`, `commit`, `present`, `interactive`
…). The metrics are `timeToFirstRender` (the first vsync after the first tree's commit) and
`timeToInteractive`, both from `didFinishLaunching`, and `coldLaunchTime` or `warmLaunchTime`. On a
simulator, give the variable to the app through `simctl`'s prefix and read its console:

```sh
SIMCTL_CHILD_EXACT_OBSERVE_LOG=1 xcrun simctl launch --console-pty <udid> <bundle id>
```

On macOS, set it in the executable's environment. The painted hosts (Linux) print a line of their own
marks under it. These are the host's clocks, in process: quick to repeat when comparing two builds, but
no screen's measurement (a video, `XCTApplicationLaunchMetric`). A simulator's numbers move by about
10 ms between batches with the Mac's load, so compare builds in interleaved rounds of several launches.

### Compact frame-health reports on physical devices

Run an Apple app on a connected phone with `bun host/apple/build.mjs --device --run`.
Open the four-finger menu and choose **Frame Health**, which in a production bake starts
collection, reproduce the hitch, then tap Share on the card and choose Agent Report; the share sheet takes
the JSON text. Exact creates no report file or automatic upload. A person can send
that text to an agent. The system share destination controls any storage it creates.
On iOS 18 and later the card measures the screen, and Share offers **Agent Report:
Screen** (`ui-update`) and **Agent Report: Exact's Frames** (`exact-app-display-link`),
each a report of its own within 4 KB. The screen's has no frame times; an app drawn by
GPU surfaces, whose canvases Exact's frame clock ticks, has its frame times and their
work in Exact's, its GPU execution and presentation unobserved.

For controlled device checks, `scripts/fixtures/frame-health-app` is a minimal fixture:
one Start/Stop button, one moving canvas tile and 32 static rows. It starts paused;
the drawing function asks for another frame only while running. There are no app
resources, storage/network grants, widgets or content assets; the only image is
the required app icon. It reuses the Canvas Gallery native host with its own source. Build it without
installing or launching it on the phone:

```sh
EXACT_APP_DIR="$PWD/scripts/fixtures/frame-health-app" bun host/apple/build.mjs canvas-gallery-apple --device --phone "<phone name or UDID>" --no-install
```

The development build supports the experimental scroll observer. Use matching
optimized collector-on/off archives for overhead comparisons; development sampling
is excluded from production-overhead results. The prepared pair uses optimized Swift
and Rust `host-dev`; full production LTO is a separate configuration. The fixture
requires iOS 27.0 or later.

Each source retains its latest 16 timing events in fixed memory. A report includes
only events from the last 120 seconds. A burst can overwrite the ring sooner; the
report names overwrite, expiry and byte-budget omissions separately. Hiding the card
stops detail capture unless an agent lease still owns it. Reload
preserves retained evidence and advances the generation; explicit clear or session
teardown removes it. The core creates no expiry timer, extra display link, stack
sampling, screenshots or log collection. The optional scroll observer is described below.

An attached agent can use the existing device carrier (`ios --device`, optionally
`--phone <name|udid>`), or a simulator, with `--timing platform` or, on the
simulator, `--live` ([reference](reference.md#inspect-work-and-native-presentation)).
Once a `clock` operation has taken the agent's clock, a capture stays paused
(`virtual-clock`). A collector that collects on demand (`ExactFrameHealthMode` is
`demand`) is started by `capture start` and `track start` as by the card's showing, in
a new generation, and stops when their lease ends or they stop, unless the card shows;
between leases `perf frames health` reads nothing (`collection-disabled`). Where
collection is off, both are refused with `collection-disabled`. A drive runs these
operations
(`sky` is a canvas):

```text
perf frames capture start 60000
perf frames health
perf frames health during "tap sky drag 0 -100 over 500"
perf frames report
perf frames capture stop
perf frames capture clear
perf frames live 5000
```

While the lease holds, an input's landing is allowed and takes nothing: its
answers land on the platform's clock. Every other `clock` operation is refused,
and `perf frames live` takes a lease of its own, so it comes after `capture
stop`. The `scroll` source (`health scroll`, `report scroll`) reads native
scrolling. On iOS, an agent's drag without real touches moves a list's offset
over the requested `over` duration, allowing intermediate frames to be observed.
The recognized drag does not start the native-scroll observer, which relies on
UIKit's dragging lifecycle. Without an observed native drag, the source answers
`native-scroll-unobserved`.

A capture lease lasts 1–120000 ms, default 60000. The card and the agent
lease own capture independently. Existing callbacks, applies and reads check expiry;
an idle app gets no expiry wakeup. Power/thermal notification observers can remain until that next
check. Reports bypass the legacy sampler and cost at most 4 KB (4000 UTF-8 bytes,
including the agent carrier's newline). There is one encoder in flight per process;
agent reports are admitted once per 15 seconds. Encoding runs on a utility queue after
a bounded main-thread copy. Repeated busy/rate-limited requests receive small refusal
packets. No report is automatically persisted, queued for retry or sent periodically.

A report covers one source. The default selects the source with the most recent
retained hitch; `report app` or `report scroll` selects explicitly. `source` identifies
it, `otherSources` names retained evidence in other sources, and `selection` says how
to get another: an agent's request, or, for a report the card's Share gave, its other
agent report. Counts and generations
belong to their named source. Forwarded apply context does not count an app apply or
its deadline again as a native-scroll event.

`perf frames health [app|scroll]` reads `exact.frame-health.metrics/2` without starting
the legacy sampler. It reports frames as LLP 1079.000 §4.6 defines them: a display-link
callback that started frame work is a frame, and one held by earlier work in flight, or
with no frame work due, is not. `frames` gives callbacks, held callbacks, frame
intervals, their total time and rate, frame-time median, p99 and maximum bounds, pacing
and schedule changes, intervals across a schedule change, callbacks whose timestamp went
backwards (`outOfOrder`; each ends a run), and the requested slots and missed frames on
the requested-rate grid.
`work` gives one record per frame, from its timestamp until its callback returned and the
owner work it sent had landed, with late frames (finished after `targetTimestamp`),
median, p99 and maximum bounds, and the frames whose work is still in flight (`pending`)
and already past their deadline (`overdue`). Where the deadline carries no information,
as on the iOS simulator, `late` and `overdue` are null and `latenessUnknown` says why.
The cursor carries the frame-time and work bins. The reply also carries requested
and scheduled periods, coverage, generation and an explicit cursor. Send that cursor
unchanged as `cursor` in a subsequent health request to read a delta. A
producer/generation mismatch, malformed cursor or impossible histogram population
returns `baseline-required`; retry without it. Read timestamps differ from the
timestamps through which intervals and work are finalized. Each completed interval or
work record enters the next finalized window, even if it began before that window's
initial read. Never subtract lifetime maxima or assume a wall-time clipping rule.

`assessments` judges the same window under `exact-frame-health-policy/2`, with one
result for each question and no overall pass:

| Assessment | Question | Decided from |
|---|---|---|
| `cadence` | Did frames keep to the platform's schedule? | `frameIntervals` and `skippedRefreshes`, the scheduled periods frame intervals spanned beyond each interval's first; `lateFrames`, work that finished after its frame's `targetTimestamp`, and `overdueFrames`, work still in flight past it; `unclassifiedIntervals`, frame intervals across a schedule change, which have no refresh count |
| `target` | Did frames arrive at the requested rate? | `requestedSlots` and `missedFrames` on a grid at the requested period (spatial motion and canvases ask for the display's maximum, paint-only motion for 60 Hz); `restrictedIntervals`, frame intervals with either end scheduled more than 0.1 ms slower than the request that governed it. `context` lists conditions at the read (`simulator`, `low-power-mode`, `thermal-serious`, `thermal-critical`) without naming them as the cause |
| `contentLatency` | Did content appear when it was meant to? | Unobserved on Apple |

Each result is one of `fail`, `restricted`, `proxy-pass`, `insufficient-evidence` or
`not-applicable`, and `reason` names the predicate that decided it. An observed
skipped refresh, late frame (or frame still in flight past its deadline) or missed
frame is `fail` even when the rest of the window is unobserved; a restricted schedule
makes `target` `restricted` instead. `proxy-pass` means frames kept to schedule; it is
never a presentation pass, and without presentation feedback target delivery is at
best `insufficient-evidence` (`presentation-unobserved`). Other insufficient reasons
are `virtual-clock`, the collector's unavailable code (`collection-disabled`,
`collector-capacity`, `wrong-thread`), `schedule-unknown` (the scroll source),
`no-frame-intervals`, `no-callbacks` (the source runs but sent none in the window),
`stale` (the source runs but has sent no callback for over a second, as through a
blocked main thread; `status` is `stale` too), `out-of-order` (a callback's timestamp
went backwards and ended a run), `schedule-changed` (an interval across a schedule
change has no single period, so no refresh count; it counts in the window where it
finalizes) and `simulator-deadlines`. The iOS simulator dispatches display-link
callbacks after their own `targetTimestamp`, so there lateness is unknown and only
skipped refreshes can fail cadence. `not-applicable` is `idle`, or for `target`,
`no-rate-request`: timers and frame tasks run at the platform's default rate without a
request, so they have a schedule to keep but no requested rate.

Frame times and work times use 64 saturating 32-bit bins: 48 of 500000 ns over [0,24
ms), 15 doubling tails from 24 ms, and an open last bin. Quantiles are half-open
bounds; a null upper endpoint means the open bin, and null quantiles mean no samples
or invalid distribution accounting. Saturation is explicit. `histogram.valid`
describes accounting, not health. All metric responses fit the same 4000-byte wire
envelope.

Apple `perf frames live <1–30000 ms>` takes a bounded capture lease, reads its
baseline once the lease holds, waits in the driver, then reads a cursor delta and
releases its lease. During that lease the host refuses
all virtual-clock requests, including implicit takeover from held-clock operations.
Use real input. Platform timing now preserves frame tasks, timer wakeups, async fills
and macOS ordinary-wheel/reorder timing; frozen-clock agents retain their deterministic path.
Media, permissions, context-menu adapters, touch diagnostics and appearance overrides
can still differ under an agent. Qualification uses a normally running app or an
audited journey; these clock changes do not certify every agent workload. Plain frame
reads under platform timing use the bounded metrics path. Plan-site `perf <target>`
can enable its requested counters without starting the old frame sampler.

On Android (`agent.mjs android`), the same operations read the reader's own frames in the
same formats, from two sources: `app` (`exact-app-choreographer`), each Choreographer turn
on exact2's thread, whose work runs from its vsync until the main thread has ingested what
it published, against Choreographer's deadline (Android 13 and later; before it the next
vsync), where a turn whose stream waits behind eight others has its work lost and counted
(`work.lost`, which keeps cadence from passing); and `window` (`android-frame-metrics`), the window's `FrameMetrics`, whose frames
have work against their `DEADLINE` (Android 12 and later) and no frame times, as the
screen's source on iOS has none. `health window` and `report window` name the second;
`scroll` is refused there, since the reader's scrolling is the window's. Neither observes
presentation, and a GPU canvas's `SurfaceView` is outside the window. A drive owns the
clock, so a frame is virtual until `perf frames capture start` lends the display the clock
for its lease: meanwhile a `clock` request is refused (an input's landing is not), and the
capture's replies carry `clock`, from which the driver goes on once `capture stop` or the
lease's end returns it. `perf frames live <1–30000 ms>` is such a lease, which reads both
sources from its start to its end. `perf frames track` answers unavailable. Only a driver's launch collects.

### A scroller's offset per frame: `perf frames track`

`perf frames track start <node> [1–120000 ms]` leases a scroll track on one node's native scroller (default
30000 ms). `perf frames track read` returns what it recorded, and `perf frames track stop` reads the rest and
releases it. The format is `exact.frame-health.track/1`. A fling's top hits and prepends are counted from it, which
the bounded metrics and reports don't carry.

- **Where its rows come from:** each row is one committed offset change, recorded in the host's existing scroll
  callback: iOS `scrollViewDidScroll`, or the Mac clip's bounds change. The track adds no display link, timer or
  observer, so a tracked fling is scheduled as an untracked one is.
- **At rest:** a still scroller writes no rows. On the Mac, a bounds change that leaves the origin where it was (a
  resize at rest, including the first after `start`, whose origin is the baseline) writes none either.
- **A bounded read:** like `perf frames health`, `report` and `capture`, a track request never waits for a slice in
  flight to land. `perf frames track start <view id>` sends only the track request. A testId is resolved with one
  `tree` first, which does land slices in flight, so resolve it before the glide, or start with the id `tree`
  lists to start or replace a track mid-glide.
- **Several rows in one frame:** a frame can write more than one row when the host also moves the offset in it (a
  prepend's anchor correction). These are separate offsets, and their short intervals aren't frames.
- **Leases:** one track a session. A second `start` replaces the first (its unread rows go, and the reply says
  `replaced: true`). A `start` that fails (`no-scroller`, a bad lease) leaves the current track as it was.
- **Where it may be leased:** exactly where `perf frames capture` may: frame health enabled (a development build
  by default, or `"core"`) and no virtual clock. An agent cannot attach to a production bake. Elsewhere it answers
  `available: false` with capture's reason.
- **Memory:** 4096 rows of 16 bytes, allocated at `start` and freed by the read that ends a stopped track, a
  replacing `start`, a reload or clock change, the session's teardown, or, once the lease ends, the next track
  request (or, while the track is still recording, the next feed). No timer ends it.
- **Reads:** each reply fits 4000 bytes with the carrier's newline. It holds the oldest unread rows that fit; `more`
  says there are others, and `cursor` (a row count, as a decimal string) continues from where it stopped. A read
  with no cursor is from row 0, the track's first. The driver keeps each session's cursor, so consecutive `read`s
  don't repeat rows, and starts again from 0 after a successful `start`.
- **Cursors:** the driver keeps one cursor per app session it drives (`s.session`), so switching sessions doesn't
  carry a cursor across.
- **Stop and ends:** `stop` freezes the track: no row is written after it. Its reply is the first page of what is
  left (`stopped: true`, `more` if there is more), the rest is read by cursor, and the read that answers
  `more: false` releases the ring. A frozen track nobody reads to the end is released when its lease ends or a new
  `start` replaces it. The driver's `stop` reads to the end and joins every page. A
  read after the lease ended answers `track: false, reason: no-track`; the driver says so instead of returning a
  partial page.
- **Malformed nodes:** a node id that names no view (negative, too large, not an integer) is refused as
  `no-scroller` and leaves the current track.
- **`dropped`:** the rows the ring overwrote before the read reached them, which happens past 4096 unread rows.

| Field | Meaning |
|---|---|
| `t_us` | Arrival of the scroll callback, `CACurrentMediaTime`, microseconds since `start` |
| `interval_us` | Spacing from the previous row. 0 (a new run, no comparable predecessor) for the first retained row, after a gap of 0.5 s or more, and after a gap of more than two periods following a `t` or `-` row: a held finger or a scroller at rest moves nothing and writes no row, so that gap is a pause, not lost frames |
| `missed` | A **proxy**: `max(0, round(interval / period) − 1)`, with `period_us` = 1 / the screen's `maximumFramesPerSecond` at `start`. It counts callback spacing, not presentation: a display running below its maximum rate reads as missed frames, and an unchanged offset in a frame writes no row |
| `offset_cpt` | The scroller's committed offset along its scrolling axis (y unless it scrolls only sideways: on the Mac, its overflow says so when only one axis scrolls, whatever its content's height; otherwise, as on iOS, content wider than the scroller that fits its height), in hundredths of a point, clamped to JSON's safe integers (±(2^53 − 1)) |
| `phase` | `t` a finger dragging, `d` decelerating, `b` past its range, `-` neither. On the Mac, `t`/`d` come from the event the scroller's own wheel handler moved it with (`phase` or `momentumPhase` began or changed; an ended phase is `-`), a plain wheel is `-`, and a move outside a wheel event (a layout's) is `-` |

On macOS, phased trackpad replay (`tap … wheel … gesture`) is refused under platform
timing with `platform-trackpad-replay-unqualified`. AppKit has no validated completion
acknowledgement for this synthetic sequence, so an immediate layout read may see
zero or partial movement. Use a plain wheel event in an
agent drive, or reproduce the trackpad gesture in the normally running app. Frozen-clock
replay remains available for functional tests and does not establish live timing.

The iOS/tvOS 18 native-scroll prototype observes one mounted active scroller with a
passive `UIUpdateLink`: it leaves continuous-update, rate, latency and presentation
preferences at their defaults. It is disabled in release/production builds pending
device qualification. A hidden, zero-size host lifecycle view ends observation as soon
as the scroller leaves its window; the normal end/deceleration callbacks close it too.
This view has no hit target or accessibility element. Reads never create the observer.
A missing earlier update phase increments `nativeUpdateSamplesLost` and skips that
sample. The observer reports completion-deadline overruns. Its cadence is unknown:
passive updates can pause while a finger rests, so they contribute no FPS, interval
histogram or maximum-gap population. This observer is not presentation feedback.

On macOS, Develop offers Frame Health, and the card's Details and Share work as on iOS.
Actions follow the responder chain to the eligible window; an embedder opts in by
passing `frameHealthContainer` to `DevMenu.install`. The standalone adapter supplies its
host slot. The card takes the clicks within its bounds and moves without changing the
app's viewport. Details shows within the card on both platforms, with the app's frames.
Active reader updates are coalesced at half-second intervals across the process.
Lifecycle changes may publish immediately; there is no idle publication timer, and only
an iOS card measuring the screen reads on a timer of its own, twice a second while it
shows. macOS spatial motion, canvases and press feedback request the current display
maximum, including above 120 Hz; press clocks are per window and update when its screen
changes.

`exact.frame-health/2` is a versioned JSON schema. Field explanations stay here to
leave bytes for evidence. `coreSinceReload` counts every frame since the collector's
last reset, not only the retained rows: callbacks, frame intervals, missed and late
frames, and `frameTimeBoundsNs` and `workBoundsNs`, the bounds of the bins holding the
median, p99 and maximum frame time and work (`perf frames health`'s bins; a null upper
bound is the open last bin). A field is null where its source does not measure it, as
the screen's frame times, or where its bins do not hold every frame. `columns` names each field in a `hitches` row. Export keeps
the newest rows that fit, in chronological order, with `retention.omittedForBudget`;
it never reduces timing precision to fit. Sequences, captures and generations are
decimal strings; other numbers are safe JSON integers. `time.clamped` invalidates
precision claims for values beyond that range. `capturedAt` is wall-clock export
context only. All intervals use `CACurrentMediaTime`, in microseconds: a retained
hitch keeps one absolute timestamp and stores its other times as 32-bit microsecond
offsets (35 minutes either way) and durations (71 minutes).

| Row fields | Meaning |
|---|---|
| `sequence`, `capture`, `generation` | Event sequence since clear, capture ID, and reload/clock generation. Identity names the installed binary, not the source of every hot reload |
| `age_us` | Age at the export snapshot, measured from callback arrival, late apply end or late pending-work finish |
| `flags` | Evidence, workload and power bits below |
| `target_offset_us` | This callback/tick's platform prediction relative to the event; it is not intended content presentation |
| `period_us`, `requested_period_us` | Scheduled callback interval and this session's preferred host interval at the event; zero is unknown/default. Other owners may request a higher rate |
| `gap_us`, `callback_us` | Callback-arrival gap and synchronous callback duration. Gap requires a stable predecessor; zero means no comparable evidence. Apply and pending-work events have no callback duration |
| `before_gap_us`, `before_callback_us` | Previous callback's observed gap/duration in the same segment; zero without evidence |
| `after_offset_us`, `after_gap_us`, `after_callback_us` | First observed following callback's arrival offset, gap and duration; null if unavailable. At most two pending events receive this context |
| `owner_queued_offset_us`, `owner_began_offset_us`, `owner_finished_offset_us` | Matching asynchronous frame-update submission, owner start and owner completion, including decoding and host callbacks |
| `main_landed_offset_us`, `apply_began_offset_us`, `apply_finished_offset_us` | Main arrival and outermost presenter-apply boundaries. Apply excludes legacy sampler bookkeeping |
| `work_target_offset_us`, `operations` | Captured frame update's target and batch operation count; an apply without a frame target has no work target |
| `frame_start_offset_us` | Pending frame's start relative to its finish; null for callback and apply events. Its deadline is `target_offset_us` |

A pending frame that finishes after its deadline records a work event even when its
owner batch changes nothing and is skipped. Its origin is the frame's finish: the
latest of callback return, owner landing and any deferred canvas draw. The row has
no apply boundaries or workload/power flags. A late apply may also have an apply
event; these are separate observations, and the core late-frame count counts the
frame once. Synchronous work already has its callback event.

A report whose `source` is `ui-update`, the card's measure of the screen's UI updates on
iOS 18 and later, has no frame times and none of Exact's owner or apply boundaries, so
its rows have only `sequence`, `capture`, `generation`, `age_us`, `flags`,
`target_offset_us`, `callback_us`, `before_callback_us`, `after_offset_us`,
`after_callback_us`, `waited_us`, `exact_us`, `card_us` and `layouts`. There
`callback_us` is the update's work: the main thread's time from its scheduling until its
last Core Animation commit, less `waited_us`, the time it waited between them, as UIKit
lets it for input. The commit is as the card sees it, in an action just after Core
Animation's, behind any action ahead of its own in that phase, so a commit that ends
just before the deadline can count as late; an update with no commit seen ends at its
completion. The work is placed to end at the commit, so its effective start, the commit
less the work, is when it began only where its waits all came first. `target_offset_us`
is UIKit's completion deadline from that effective start, negative when the deadline
came before it; work longer than it is late. `before_callback_us` is the previous
update's work, and `after_offset_us` and `after_callback_us` the next update's effective
start and its work. `exact_us` is the part of the work in Exact's host phases (applying
batches, rows, text, scroll callbacks, pictures and waits on the runtime), `card_us` the
card's own, its drawing included, and `layouts` how many nodes' views UIKit laid out;
the rest is UIKit's, the system's and Core Animation's commit, and Exact's work outside
its phases, such as laying out those views. Null is unmeasured. Updates that follow each
other as one finishes are the main thread working without a pause; a single long one
between short ones is one stall, such as a new screen.

Offsets are relative to the event's origin; negative means before it. Null boundaries
are unavailable, never zero-cost work. Callback events attach the largest apply since
the preceding callback, otherwise the most recent apply only if it ended within two
scheduled periods. This is temporal evidence, not causal attribution. Queue delay,
owner elapsed time, main-thread landing delay and apply duration can be derived by
subtracting their matching boundaries. Elapsed time includes descheduling; it is not
CPU time. The report cannot attribute unobserved main-thread work, GPU execution,
presentation, input latency or native-only animations. Zero counts or an empty report do not establish healthy rendering. Live agent scheduler/apply parity remains unqualified; reproduce
outside an agent run for production behavior.

`flags` bits: 0 callback gap >1.5 stable periods, 1 synchronous callback past target, 2
captured tick apply past its target, 3 following callback known, 4 apply context known,
5 largest apply in the callback interval, 6 packed value clamped, 7 pending frame work
past its deadline; 16 motion, 17 spatial, 18 canvas, 19 frame tasks, 20
fill pending, 21 tick pending, 22 hatch frames, 23 workload flags known (otherwise bits
16–22 are unavailable); bits 32–33 thermal state (0 nominal, 1 fair, 2 serious, 3
critical), 34 Low Power Mode, 35 card shown, 36 card dragging. A `ui-update` row's bits
16–23 are the app session's and bits 35–36 the card's as its update was scheduled, and
bits 40–46 the users of Exact's frame clock, in any session of the process, asking for
frames then: 40 the session's clock, 41 a list's offset correction, 42 a navigation
reveal, 43 the scroll pump, 44 deferred heavy leaves, 45 a live SVG filter, 46 a
textarea's resize grip; with none of them, UIKit or the system asked. These cached
observations do not establish which activity caused the hitch.

Save received text as `report.frame-health.json` if needed, then use
`bun scripts/agent.mjs trace report.frame-health.json` for the expert readout. This
explicit recipient-side save does not create an app-side retention policy. Commit,
presentation and host-phase probes like those explored on `ide/trace-costs` can guide
a later targeted investigation; this bounded monitor does not run those probes.

Build diagnostics include stable ids and original file ranges. Locations are
1-based line/byte-column coordinates, with exclusive end columns; a usage, I/O or
manifest error has no range (line and columns 0). Honor related
locations when an error crosses a child prop, injected action, or imported file.
Do not use a character index as a byte offset in Unicode source.

```sh
bun exact.mjs contract build app.contract --json
bun exact.mjs contract symbols app.contract --name save
bun exact.mjs contract build app.contract -o /tmp/app.plan --map
```

The source map (`<plan>.map.json`, beside the plan) is separate from the plan and
keyed by its digest. Use it only for
the accepted generation it describes. The driver can connect layout to the
original declaration, component call sites, and winning style attribute.
Production artifacts do not need a development source map.

### The driver's operations

The driver has ten operations: `tree`, `screenshot`, `tap`, `type`, `state`,
`layout`, `logs`, `clock`, `prefer`, and `perf`. Variations are arguments, not new
commands; `prefer` takes CSS's media feature names (`"prefer prefers-color-scheme dark"`,
`"prefer prefers-reduced-motion reduce"`). The display's scale, which a hairline is one
device pixel of, is the device's: a simulator's or the Mac's own, and on the web and the
Linux host what `EXACT_SCALE=2` launches them at (1 otherwise; `"prefer display-scale 2"`
changes it under a native host, and `layout` shows the boxes that follow). Targets are `testId`s (or view ids): give every control a `testId` and
drive it on every host, iOS included (`agent ios`), never by screen coordinates.
On a machine with several booted iPhone simulators, name yours: `EXACT_SIM=<udid>`
for `agent ios` and the smokes, `--sim <udid>` for `host/apple/build.mjs`; a
choice among several is refused rather than guessed (another drive's simulator).
A target no `testId` carries resolves by a view's exact accessibility label or
text (`tap "Save draft"`); a name several views share refuses, naming them.
`type` also sets a control's value: `type "persona" "bob"` chooses a
`select`'s option by its `value`, and a date, time, range or checkbox takes
its value the same way, in a drive or a test (a tap does not open a native
`select`'s menu under the driver).
`tree --ax` prints the platform's accessibility properties without claiming
VoiceOver's reading order. An iOS simulator drive containing `tree --ax` enables
UIKit's accessibility runtime before launch and restores the simulator's setting
after launch. Library drives request this with `open({host: 'ios', ax: true})`.
`verify` reads it once at each screen it checks and counts its findings as `check`'s:
a control with no name, an element exposed while hidden, one reachable outside an open modal.
Use `tree` to find targets, `state` for data and delivery, `layout` for
geometry, `perf` for the work a drive cost (`perf <target> during "<op>" …`: per
plan site, evaluations, unchanged results, instances created and retired), and
screenshots for rendered output. `state.tasks` gives each task's next due time
(`null` while a gated task is idle or an `after` is spent) and `state.queued` each
queue mutation's waiting sends. Logs name refused operations and data errors.
Under the driver's clock no frame is presented, so `perf frames` measures a
live window instead: `perf frames live 3000` lets the page's clock follow the
wall for 3 s and reports the frames it presented (p50/p95/p99, late frames)
and each game world's frame, tick, feed and encode times, on the web's wasm
target (a game's). The world ticks on the wall in that window, so a hash
taken after it is not a seeked run's.
For a game canvas, JavaScript `s.tap("world", {mouse:true, at:[x,y]})` sends one
primary mouse click on web, macOS, Windows, and Linux (on macOS any node takes
it, so a click can land on a link inside a paragraph). `{contextmenu:true, at:[x,y]}`
sends a right-click on desktop hosts and the web. On Android and iOS,
`tap <target> contextmenu` delivers a recognized long press to the nearest node
with a `contextmenu` handler or `contextPopover`. Its handler runs and its menu
opens, where present; a handler needs no popover. Coordinates are relative to
the target's top-left; omit `at` for its center. Invalid, covered, or offscreen
points are refused. Mouse clicks also require held contacts to be released.
Windows' headless agent refuses a field's native editing menu before changing
its focus or selection; authored menus and handler cancellation still work.
The CLI forms are `tap world mouse [at <x> <y>]` and `tap world contextmenu
[at <x> <y>]`. `tap <target> auxclick` is the middle button and `clicks 3` a triple
click (each press counting 1, 2, 3); every click form and a wheel take `at <x> <y>` and
`modifiers Shift+Meta`, and `down … modifiers Shift` or `drag … modifiers Shift` holds
them to the lift. Chrome and macOS deliver these as a hand's (on macOS through the
application, so its local event monitors see them). Other carriers answer
`delivery: "unsupported"` for forms they cannot carry; Firefox and WebKit carry
clicks and wheels at a point, including wheels with modifiers. A word a form does
not use is refused by name. `tap stage wheel 0 -20 modifiers Control`
is a pinch's wheel; `tap world drop a.board` drags a file in (web, macOS, Windows). Plain canvas taps and held contacts are
fingers, so their platform pointer identity and retained press history can differ
from a mouse's; use the intended physical input when comparing game saves.

On Firefox and WebKit, a wheel positions the pointer at its delivery point first;
if it moves, the reply's `hovered: [x, y]` reports that input. To compare wheels across engines,
use `session.tap(target, { hover: true, wheel: [dx, dy] })` on every page. It resolves
one viewport point for the hover and wheel, keeping it if hover changes the layout.
The wheel may also name `at: [x, y]` and `modifiers: 'Control'`.
`conform.mjs --browser` does this and reports the comparison as hover followed by wheel.

`tap` and `type` scroll a target whose middle is out of view into it first (its
nearest scroll containers, then the page) and say so in the reply's `scrolled`.
A tap aims at the target's middle, or, where the target is not there (a wrapped
inline run, whose middle can fall between its lines), at the middle of the first
of its lines that is; a tap whose point lands on something else fails, an
ancestor that would take the press itself included.
A tap that names a node presses that node, never a control inside it: a node
with its own `press` (or a link) whose middle holds another — a post row's link
card, its Like — is pressed at the nearest point of its box the search finds
that reaches it (the reply's `avoided` names what the middle holds), or refused
when it finds none; a node without one is refused when its middle holds a control
(`tap post-0 would press card-0 inside it; tap card-0, or tap post-0 at <x> <y>`).
The search tries a grid of about 12 points, then about 3 points and the box's
edges 1 point in, so a reachable strip thinner than that, away from the edges,
can be missed: then name a point. `tap <target> at <x> <y>` presses at a point
from the target's top left, whatever a finger there reaches, on every host. What
a press there would deliver counts as a control: a node with its own press, an
SVG element, an inline run or link, a form control or text field (it toggles,
opens or takes the focus), a grouped list's row, a surface's action button and a
canvas that takes input. A disabled node presses nothing (on the web only a
disabled form control: `disabled` on a box means nothing there). On iOS without
`--touch platform` a refused tap has still dismissed, or begun to open, a
painted popover, as the tap's first step.
`type` on a control sets it as a person choosing would, with `input` then
`change`: a `select` takes an option's value or its label, a date, time or
`datetime-local` input its HTML value (`2026-10-09`, `14:00`,
`2026-10-09T14:30`), a range a number, a checkbox `true` or `false`, a radio
`true` (it is unchecked only by checking another of its group). A
`select`, a range and a date, time or `datetime-local` input keep the person's
choice until their bound `value` changes, as a text field does (LLP 1069.001 D4,
amended): an action that writes nothing, or only sends a mutation, shows the
choice until the bound value moves, on every host, and `type` replies with what
the control shows. A checkbox still snaps back to its bound `checked`.
On a text field or textarea `type` inserts the text, one `input` (in a CLI op the
text is the rest of the op's words, joined by one space, quotes included:
`"type new-task buy milk"`;
in a test file each value is one quoted string, a checkbox's `"true"` too); `change` comes as a
person's would, when the field commits (`type <id> key Enter` on a field, or the
focus leaving it), so an edit saved on `change` needs one of those (on macOS a
`tap` on a button leaves the focus in the field, and commits the field's `change`
before the button's `press` runs). A CLI drive's
input does not wait for a request its action sends; `clock data` lands it.
`tap <target> drag <dx> <dy> … during "<op>" …` runs the quoted reads after the
move, with the finger still down, before the hold. Under `--touch platform` the
lift is scripted, so the reads run inside the hold and `during` needs one
(`hold 300 during "state"`). Under the agent's clock the gesture moves the clock by its
`press`, `over` and `hold` as the finger makes them (under `--touch platform` on real
time, as a `clock +<ms> real` would), so the app's `every` and `after` run while it is
held and `performanceNow()` in its pointer actions reads how long it was pressed. `tap <target> drag to <other> [at <x> <y>]`
ends the drag on another node (refused by name when it is not mounted or off
screen); a drag that should autoscroll a list or a board is `drag dx dy hold ms`. On iOS
without `--touch platform` a drag scrolls the scroll or list it starts on with the
finger, no fling; any other drag there is refused, naming the flag.
On the Android APK reader, a named scroll or list receives deterministic scroll
samples; the reply labels native touch recognition and fling as unobserved.
Presenter-owned pans keep their contact path. Scroll samples cannot hold a touch
for `press`, `hold` or `during`; use native device input to verify those gestures.

`into keyboard` (library: `drag.into: 'keyboard'`) lets the end pass the
viewport's bottom, down to the screen minus its bottom safe area. It requires
an iOS simulator with `--touch platform` and the software keyboard up at the
press. The start stays in the viewport. For a held dismissal, write
`tap transcript drag 0 200 hold 1000 into keyboard during "state"` and compare
`keyboard.top` with the finger's endpoint.

Append `then dx dy [over ms] [hold ms] [during "op" …]` to script another leg
with the same finger, in a drive or a test file. For example:
`tap handle drag 0 120 over 300 hold 500 during "layout" then 0 -120 over 300 hold 500 during "state"`.
Each delta starts at the previous endpoint. Under `--touch platform` all legs play
in one XCTest synthesizer record, with one down and one up, and reads run inside
their leg's hold; if they outlast it, the drive fails. Elsewhere the carrier's
contact plays them; on iOS without `--touch platform` only a test's drag can. The complete path must stay inside the
viewport (or the extended end bounds with `into keyboard`), last at most
10 seconds, and spend at most 2 seconds moving, across at most eight legs.
Library calls use `drag.path: [{dx, dy, over, hold, during}]`
for the additional legs, with `during` read functions. Separate requests never
keep an iOS contact down.
Under `--touch platform` a tap replies `viewGone: true` when its view left the
window before the finger lifted, so that view had no lift (UIKit rebuilds a
grouped row's detail button in the first seconds after the list appears), and
`downAfterLift: true` when the window had the down only after the finger lifted
(the app busy, or the system slow to deliver it; a UIKit switch in a list cell
takes a down and a lift that arrive together as no tap). Read the state, and tap
again if nothing changed; `touch.dispatched` less `touch.ended` says how late
the down was, and an app that blocks its main thread that long loses a
person's tap too.

#### Drags on iOS

A wheel (`tap <scroller> wheel 0 300`) and, without `--touch platform`, a drag on
the scroll or list it starts on (`tap <scroller> drag 0 -300`, past the screen
too) leave it as a person's scroll by that distance does, with no glide: the
content moves that far, a large title collapses as it goes, and a scroll back to
the top opens it again. A drag moves over `over` milliseconds (250 by default),
including one that ends past the screen; live frame capture observes the motion.
As a person's, none stops with the content partway under
a large title: it eases to the title's open or closed rest, whichever is nearer,
so under one a wheel of 20 from the top moves nothing (with a search field in the
bar it stays where it ends: not measured). Under `--touch platform` a drag whose ends
are both in the viewport is a real one (`delivery: platform`): the content follows the
finger once UIKit's pan begins, some 10 points in. Without a hold it glides on
after the lift, and `clock settle` waits up to 4 s for it to stop (`reason:
"scroll"` past that). Held for `hold 100` or longer it stops with the finger,
which the runner steps a point across the drag and back in the hold's last
150 ms, as a finger does (measured along a list); `hold 50` still glides. With
`into keyboard` and the keyboard up at the press, an end below the viewport
stays `delivery: platform`, down to the screen minus its bottom safe area;
farther ends are refused. Without that option, a drag that ends past the
viewport is the agent's drag above, `delivery: recognized`.

On macOS a wheel is applied at once, with no momentum and no rubber band.
`tap <scroller> wheel 0 600 fling` is a trackpad's fling instead: the driver
posts its events to the app through the window server, AppKit glides and bands
as it does for a person, and the reply comes when the scroller rests, with its
`offset` (on the real clock; the session's clock does not move). `wheel 0 3000
hold` leaves the finger down, so `layout` and `screenshot` see a band pulled
past the end, and `wheel 0 0 release` lifts it; a drive that ends with a hold
open lifts it too. These need the terminal's Accessibility grant (System
Settings › Privacy & Security) and a window that is on screen; without either
the operation fails and says why. A pull that starts at a scroller's end goes to
the scroller around it, so one from the end of the outermost moves nothing.
During a reorder `state.reorder` reads `{ item, from, to, before, phase,
ending }`; a grouped grip also takes `type <grip> key Space` and the arrows. `clock +N` moves the virtual clock without
waiting for a store's or the network's reply on real time (unless a timer fires
first); its reply says what is still in flight (`inflight`, on every host), and `clock settle` lands it.
On the web JS target, a single jump can deliver several timer-driven DOM edits together
to a `MutationObserver`. To observe an intermediate value, advance to that time in a separate clock step.
A request the server holds, marked `exactHeld: true` (a long poll, LLP 1016.000 D9), is the exception: no
clock waits on it, neither a jump nor `clock settle` nor `clock data`, and each of their replies names it
(`held`, `heldTargets`; `state.pending` marks it `held: true`). Its answer must be safe to supersede.
After an action that causes a push you will read (a write the server echoes, a build's report), say so with
`clock held [<target>…] [<n>s]`: real time passes with the clock frozen, so no timer fires, until each held
request in flight (or each named) has had one reply, the server's own timeout answer included; then `clock
data` lands what that reply started. Past its bound (20 s) it answers `settled: false, reason: "held"`,
naming the targets that had none.
A stream counts as landed when its first mapped value commits, so for state it
reaches later (a socket's READY after HELLO),
wait with `clock until state <path> == <literal> within <ms>`: real time with the clock frozen, polling `state`
until it equals the JSON literal; a miss fails naming the last value.
`clock settle` also runs the clock to where the last running finite, unpaused
transition or animation ends, firing the timers due on the way, so a test on a timer's grid moves with it,
and on past a task due a commit later (`after(1, …)`, which an input or an answer armed) until none is.
`clock data` takes clock ownership, including after `--timing platform`: it freezes
platform timing at the current instant and leaves the clock held after the reply.
It lands data without moving that clock: the data module's activation and
every request in flight, each answer's `then` with it, no timer fired (its reply names a task
due a commit later, which a tap next would come before: `clock settle` runs it). A CLI drive's
first operation runs at boot and may come before that has landed (an authored test
lands it before its first step), so a drive that reads or taps data starts with `clock data`
(a `tree`, `layout` or `screenshot` taken before the clock first moves, with a request
in flight, says so on stderr).
A playing `video` or `audio` is on real time too: the clock never seeks or holds it, so
between operations it moves only as far as the drive took. `clock +N real` lets
N ms of real time pass with the clock moving beside it, a step at a time: a
video plays that far (its `timeupdate`s arrive), and a reply that lands in the
span lands (LLP 1042 §3). On Apple's hosts `state media` reads each video's
player: its `renderer`, `paused`, `currentTime`, `muted` and `volume`. Under the
agent a video never takes the audio session, so the app's `audio_session` is not
applied and a drive cannot show that it is heard (AVKit's speaker shows a slash).
On iOS, AVKit's own controls (`controls=true`) show only for a real touch: tap the
video under `--touch platform`.
`clock +N real until <path>` is a test harness's affordance, not something an app
drive needs: the same real time with the clock beside it, ended early once the
file at `<path>` (a local path, not a URL or a glob; its directory must exist)
exists, which the harness driving the drive writes at the event it waits for. The
clock moves by exactly the real time gone, and the reply says `until: "met"` (at
once, `real: 0`, when the file is there already) or `until: "bound"` after N ms.
Under the driver a picker, an export, a share or an auth session opens no panel:
it is held and `state` lists it under `pending` with its capability. Answer it by
the node its answer arrives at (the `id` the command names) or by its capability:
`type @folder-input path/to/folder`, `type @pick photo.jpg`, `tap @open-directory
cancel`. `@N`, its ticket, works too, but a ticket counts every request before it,
so it differs between hosts and runs. On Linux, Windows and Android a tap on a date
or time control holds its picker the same way, by the control's kind: `type @date
2026-10-23`, `type @time 09:30`, `tap @date clear`, `tap @date cancel`.

Authored tests are a smaller language over that API:

```contract-test
test "an action uses its computed next value"
  expect state count == 0
  tap "advance"
  expect text "result" == "1/2"
  expect state doubled == 2
```

This test goes with the complete example above. A test opens with its launch
lines — `size 1200x800`, `epoch "2026-09-21T12:00:00Z"`, `time-zone
"America/New_York"`, `locale "fr-FR"`, `seed 7`, the driver's flags of those
names, and `fresh` (the Snapback dev server emptied first, `fresh
"seed.json"` seeded from a file; [Snapback tests](recipes/snapback-tests.md)) —
written first in the test or, for every test, at the top of the file.

`prefer prefers-color-scheme dark` sets a media preference before the first
observation. It can lead the test or the file, or appear later to change the
preference between steps. Use the driver's CSS media feature names and values
(`prefers-color-scheme light|dark`, `prefers-reduced-motion reduce|no-preference`,
and the other media preferences listed above). `prefer permission.notifications
granted` sets a module's permission the same way, as the driver's `prefer
permission.<name> granted|denied|prompt` does. A test's leading value overrides
its file's default for that name only. Duplicate names at file level or among a
test's leading launch lines are errors; later steps may change the same name.
A bad name, value or unsupported emulation fails that test at its line.
`reload` and verify's relaunch keep the latest applied preferences. On native
hosts they are applied after connection, before the test observes the app;
they do not set its appearance before mount. MIDI's `prefer midi.…` remains a
step inside a test.

`prefer root-font-size "24"` sets the root text size in pixels for a large-text
fixture. Other single-value page facts, such as `prefer online false`, follow
the same launch, override and reload rules and use the drive's parser and name
aliases. Hosts refuse facts they cannot substitute. Folds (`posture`, `segments`,
`gap`), mode pins and extension substitutes are drive-only for now.

Before the first step (and after a `reload`) the driver waits for the app's data
as `clock data` does: its data module activated and every request launch started
(a store's open, a fetch) answered, the clock unmoved and no timer fired, so a
test does not start with `clock settle`. `before data`, a launch line, skips the
wait; what has landed then is the host's (a native app runs on real time before
the driver connects).

To test an error path, fail the fetch: `fail fetch "<url prefix>"` makes every
later fetch whose URL starts with it fail exactly as a refused connection does
on that host (a TypeScript source's `fetch` rejects with `FetchError` kind
`"Network"`; a Rust source's request settles `Failed { kind: Network }`), and it
never goes out. Leading the test it is armed before the first data load, so
"the API is down when the screen opens" is the launch; later it is a step.
To test going offline after the data loaded (a Snapback4 partition, which
cannot open before its first sync, is the usual case), put `clock data` first;
a runner note names a `fail fetch` armed before the data loaded.
`times N` fails only the next N; `pass fetch "<prefix>"` stops it; a counted
fault that never fired fails the test. A fault reaches only fetches that start
after it: a request already in flight with the same arguments is reused by the
runner (one request per resource and arguments), so re-opening the screen while
it is pending gets that request's answer, not the fault. Arm the fault before
the request starts (as the launch line, or before the step that first loads
it), or let the pending one settle (`clock data`) first. The app's own `catch`, error record and
retry run, so this checks the real error handling (LLP 1103). A drive takes
`--fail-fetch <prefix>` at open and the ops `"fail fetch <prefix> [times N]"`
and `"pass fetch <prefix>"`; `state.faults` shows each prefix's hits.

Each HTTP attempt of a long poll can take a matching fault, including an
attempt with its updated cursor. It retries as after a network failure;
`state.streams` shows the backoff in `retrying`. A successful reply resets
the backoff. Other stream types do not take fetch faults.

```contract-test
test "the list shows an error, then retries and loads"
  fail fetch "https://api.example.com/recipes"
  expect text "error" == "Couldn't load recipes."
  pass fetch "https://api.example.com/recipes"
  tap "retry"
  clock data
  expect tree has "recipes"
```

`answer fetch "https://api.example.com/greeting" with 200 {"text":"Hello"}`
supplies a response without contacting that service. Leading a test or the file,
it is installed before the first resource load; later it is a step. The real
source reads its status, headers and body. A quoted string supplies plain text;
JSON supplies `application/json`. Optional `header "name" "value"` pairs before
the body set response headers. Use a text body to test malformed JSON.
An answer replaces a failure or answer at the same prefix; `fail fetch` replaces
it with a connection failure, and `pass fetch` clears it. The longest live prefix
decides. Reload carries the current table, and tests start with independent tables.
On the web, the carrier gives the table only to the top-level document on the
original app origin, excluding child frames and pages on other origins.
Development render servers apply the fixtures while rendering request and cached
pages, preserving unrelated server-fetched answers in the checkpoint. Fixture
renders are not shared or cached between requests. On adoption, only answers that
fetched a URL matching a table prefix are asked again, including a prefix cleared
by `pass fetch`. Other rendered answers stay settled. A delivered response does
not excuse a resource failure from `expect no failures`. An injected connection
failure remains arranged after
its prefix is replaced or carried through reload.
Fixtures keep request validation, file-read grants and upload limits, deadlines,
response limits and native HTTP caching. Native prefetch and its foreground
fallback use the same fixture. A web response and its clones keep the request URL
and read-only headers.
Unmatched URLs still go to the network. These are immediate, buffered responses;
streams, redirects and `exactBodyTo` are not supported. See the
[authored-test grammar](contract-grammar.md#authored-tests) for limits.

To write through the app's own server as another person, `request POST "/items"
{"title": "Bread"} as grace` is the driver calling it (`Authorization: Bearer grace`,
under app.ts's local URL; a status outside 2xx fails the step with the status and
body, unless `… as bob expect 409` says it is the refusal the test wants). The app shows the write when it next reads: `clock +<its poll's ms>`, `clock
data`, then `expect` ([your own REST server](recipes/rest-backend.md),
[authored tests](contract-grammar.md#authored-tests)).

A slow server is bounded in the source, not the view: `fetch(url, {
exactTimeout: 10000 })` cancels the exchange after 10 s (headers and body) and
rejects with a `FetchError` of kind `"Timeout"`, which the source catches and
answers as any failure (a Rust source's request takes `Request::timeout(ms)`).
Without it a stalled fetch waits the platform's limit (60 s without data on
Apple). Test the error state with `fail fetch`, as above; a timeout itself is
tested against a stand-in server that never answers (the reference's
"exactTimeout"). A stream that may go silent (a WebSocket, an event stream,
a byte stream) takes `exactIdleTimeout: ms` beside `exactStream`: the host
ends it, as one
`error` event of kind `"Timeout"`, when nothing has arrived for that long,
and the app refreshes the resource to open it again (the reference's
"exactIdleTimeout"); `fail fetch` does not produce it.

A test whose text depends on the date names its `epoch`; without one it runs at
the driver's 2026-01-01 UTC. A drive or test whose app talks to a live backend
(`snapback4 dev` runs on real time) says `--epoch now` or `epoch now`: the
machine's clock, read once at launch, from which `clock` moves the date as
before, so the app's dates agree with the server's but differ run to run. The steps are `tap "id" [hover|dblclick|contextmenu]`,
`tap "id" modifiers "Shift+Meta"` (a press with keys held),
`tap "list" into "key"` (a virtualized list's row brought into view by its key,
so the next step can tap a row outside the rendered window),
`tap "id" pinch <scale> [at x y]` (two fingers; `scale` greater than 0),
`tap "id" drag dx dy [from x y] [mouse] [press ms] [over ms] [hold ms] [into keyboard] [during "op" …]`
(a finger, `pointerType` touch, where the carrier has one, unless `mouse` names the left button; `during`
is last: quoted reads or `clock` while that contact is down, after the move and
before the hold; `press` and `hold` advance the virtual clock, except under
`--timing platform`, and `over` does not, so on the virtual clock a GPU surface's
samples of one move share its time, `at_ms`: give them times with `--timing platform`,
or with a contact's phases, each hold before the step it times, as `tap sky down`,
`tap hold 16`, `tap move by 0 -20`, `tap hold 16`, `tap move by 0 -20`, `tap hold 16`,
`tap up` (the last hold lifts the finger a frame after its last move); on an iOS simulator it is a real UIKit gesture, so a swipe row's
full swipe performs its action),
`tap "id" drag to "other" [at x y] […]` (it ends on the other node's middle, or
at a point in its box; a card dropped on another list; on iOS a `tap` naming a
native swipe action's control performs it as assistive technology does, with no
swipe, where the web's tap scrolls the row to it, and `--touch platform`, as in
`bun exact.mjs test ios --touch platform`, makes every tap a real touch),
`type "id" "text"` (sets the value), `type "id" "text" append` (after the value
the tree shows, as typing after a prefill), or `type "id" key "Name"`
(`down`, `up` — its `keyup` handlers hear it — or `for <ms>` on the virtual
clock, repeating as a held key does: a keydown with `repeat` true 500 ms after
the down, then every 83 ms),
`type "id" paste "text"` (⌘V on macOS, Ctrl+V elsewhere, then the paste; a `key` handler that `preventDefault()`s that chord keeps it from landing), `type "id" copy`, `type "id" cut`, `pick "id" "path"…` or
`pick "id" cancel` (a held picker or export, by its node or capability as
above; paths are the test file's), `clock settle|data|+ms|+ms real|ms` (`data`:
what is in flight lands, with each answer's `then`, the clock unmoved), `clock held [<resource>…] [<n>s|<n>ms]`
(the driver's: real time with the clock frozen until each held request, or each named, has had a reply, such as a
long poll's after a push the test caused, within the bound, 20 s unless given; then `clock data`), `clock until
state <path> == <literal> within <ms>` (real time with the clock frozen until `expect state`'s path equals a
string, number, `true`, `false` or `null`), `resize
800x600` (the window, mid-test), `reload`
(the app restarts on the store it had, including a `secret.keep`, its state and
clock starting over, so a test shows what persists; the web reloads its current
URL, including its agent launch facts; on Linux, Windows, macOS and iOS a
native app reopens at the drive's path or app scheme `--url`, otherwise at
its no-link cold launch location:
the manifest's `start_url` on Apple, `/` on Linux and Windows; an HTTP(S)
`--url` selects a development server, not a launch location. Native reload
builds a fresh navigation stack; `at` changes only the first launch, so
assert a detail-route launch before reloading. Android's APK reader refuses
these launch URLs and a leading `at` path other than `/`, where it boots;
`agent android --headless` runs
the Linux host on Android and accepts both. See
[reload and native launch tests](contract-grammar.md#authored-tests)),
`close` (the window's close button, as ⌘W: a `beforeunload` that
calls `preventDefault()` keeps it open and the test goes on to the app's "Save
changes?"; macOS and the web), `back` (the platform's Back: a dialog's cancel,
a sheet's dismissal, the pushed route popped; a refusal is a note, nothing to
go back from fails),
`screenshot "file"`, `expect tree has|missing "id"`, `expect disabled|enabled "id"` (a control's
`disabled`; a `tap` on a disabled one fails its step on every host), `expect focused "id"` (the
node holding the keyboard focus), `expect layout "id" width|height ==|<=|>= n` (its laid-out
size in points, from the box `layout <target>` prints: `expect layout "status-mark" height <= 24`
fails for a glyph a column stretched), `expect text "id" == "…"`
(the node's text; a control's value, so a `select` reads its chosen value, not its
options, and a checkbox with a `checked` binding `true` or `false`; else its descendants' — a button's label — else a field's value), and
`expect state name == <literal>`, where `name` may go on into a record's fields
(`board.active.present`) or a list index (`rows.0`). A literal is a number (negative
allowed), string, bool, `none`, `some(literal)`, list (`["a", "b"]`, `[[1, -2], []]`),
or shape with literal fields (`Point(x=1, y=-2)`). `==` and `!=` compare the whole value
structurally (`expect state shown != "hint"`). Numeric bounds use `>=` or `<=`
(`expect state offset >= 90` and `expect state offset <= 110`); they require a number, without coercing strings or bools.
`expect state mode in "idle" "ready"` checks literal membership; alternatives use the same
literals as `==`, separated by spaces. Every operator also accepts a state path on the
right: `expect state pinned == state locked`, `expect state offset >= state minimum`,
or `expect state mode in state allowedModes`. Both paths read the same state reply.
Bounds require two numbers; `in` requires a list whose element type matches the left.
Incompatible path types fail compilation with both types named. A missing path on
either side fails, including with `!=`; a present `none` compares as a value. A
mismatch prints both values and the right-hand path when present.
`expect wakelock held|released` reads `state.wakelock.held`: the runner's record of displayed elements requesting the
lock, not whether the OS or browser granted it. A mismatch names the holding
view ids and their testIds when present, or says nothing holds it.
A failed expect names any resource that
failed, and why; with no input before it, the requests still in flight (the boot's
own, or what a `clock +N` left on real time). An input step ends with what it settled: an answer the data
module gave in the input's turn, and its mutation's `then`, are there for the
next step. Otherwise the clock stands still between steps: a reply on real time
(a store's, the network's) or a transition an input started lands at a `clock`
step, so `clock settle` before the `expect` that depends on it. Storage work that
has answered by the end of a web input may still be pending on iOS, macOS or
Linux: before a later step depends on its reply or its mutation's `then`, use
`clock data` (or `clock settle`, if motion should finish too). A timer
(`after`, `every(ms)`) fires when the clock reaches or passes its time: `clock
settle` fires it only if it reaches it while advancing to a motion's end, so
move to it with `clock +N` (a `task … after(1, restore)` needs `clock +1`).
`type "id" key "Name"`
focuses the target if it takes the focus (else leaves the focus where it is)
and presses the key as a keyboard would on every host: its `key` handlers,
then its default — `"7"` types into a field, `"Enter"` submits it (a
textarea's breaks the line), `"Space"` presses a button, `"r"` reaches an
`aria-keyshortcuts="r"` button — then releases it through the `keyup`
handlers at the focus. A chord holds its modifiers for the key, in
Playwright's spelling: `"Shift+Enter"`, `"Meta+s"`, `"Control+Alt+ArrowLeft"`
([keys](contract-grammar.md#keys)). As a keyboard does, on every host, each
modifier is its own key first: a `key` handler hears `Shift`, then `Enter`
(with its `shiftKey`), so a handler that treats any key as typing must skip
`Shift`, `Control`, `Alt` and `Meta`. A paste is the same (`Control` or `Meta`,
then `v`), and so is a click or a wheel with `tap … modifiers` on Chrome,
Firefox, WebKit and Linux, and a right or double click with modifiers on Chrome.
Each modifier comes up after the key, in reverse, without its own bit, so a
`keyup` handler hears it too. Still flags only, with no modifier key of their
own: Apple's taps, a drag's phased `tap … down`/`move`/`up` (Chrome and Apple;
other carriers refuse modifiers there), and a key a canvas world takes on Apple.
Not every interactive
driver operation is a test-file statement. `contract test` parses and prints JSON;
`agent.mjs <host> --test <file>` actually drives the app.

Test the behavior being changed: a filter retaining row identity, a text edit
sending the new value, an async error retaining its placeholder, a route back
restoring a screen, or an animation at a specific virtual time. An assertion
that restates a constant is weaker evidence than the user's actual sequence.

## Repair common mistakes

| Refused or misleading form | Accepted direction |
| --- | --- |
| `action save writes item` | Delete the clause; effects come from the body |
| `transform` on a box, inline or through `class`, beyond `translate`, `rotate` and `scale` in that order (`lower-attr-tag`) | A box `transform` that is those functions, each at most once, in CSS's individual-property order (translate, then rotate, then scale) lowers to the rows it equals: `transform="translate(0px, 25px) scale(0.8)"` is `translate="0px 25px" scale=0.8`; `scale(x, y)` sets x and y separately, `scaleX(x)` sets `scale="x 1"`, and `scaleY(y)` sets `scale="1 y"`; `none` is no row. The rest (`matrix`, skews, 3D scale, 3D functions, a function repeated or out of order, a bound string) is deferred (LLP 1055.000 D5): write the rows. A `transform` beside a `translate`, `rotate` or `scale` row on the same element is refused; keep one. SVG elements keep the whole property. |
| Component `contract` assertions | Authored `test` blocks exercised by the driver |
| `provide theme = value` around view children | Component `provide` section, wrapper component if needed |
| `items[0]` | `first(items)` or `items.at(0)`, an option to `match` |
| `items.map(x => Row(...))` | Keyed `for` with `Row(...)` in its body |
| `{ title: value }` | Declared record constructor |
| `[...xs, x]` / `xs.push(x)` | `xs.concat([x])` / the same, assigned: `xs = xs.concat([x])` |
| `{...old, title: value}` | `Shape(old, title=value)`, with the record's declared shape |
| `if name` for a string | `if name != ""` |
| `selected.title` when optional | Exhaustive `match selected` |
| `press={() => save()}` | `press=save` or `press=save(captured)` |
| Copying an action's statements into another | Call it: `arrive(path)`; the callee runs in the same commit |
| A helper that reads a slot its caller just assigned | Pass the value: `follow(next)` (`analyze-read-after-assign`) |
| `button press=add` with no child, or only an `image "symbol:…"` (`lower-button-unlabelled`) | Its words in it (`text "Add"`), or `aria-label="Add"` on a symbol-only button |
| A `button` inside a `button`, as a React Native `Pressable` in another (`lower-nested-button`): a press is the inner one's alone, but the outer's accessible name reads both ("Note Listen") | A `row` of the outer's content as one `button` (`flex=1`) and the inner one beside it |
| A heading at the top of a route's content that says its `header` heading again, which shows twice under the iOS bar (`lower-title-twice`) | Drop it; the bar's title names the screen. A heading below names a section: `text "Results" role="heading" aria-level=2` |
| An SVG `use` whose literal `href="#id"` targets a `foreignObject` or a subtree containing one (`lower-svg-use-foreign-object`). Only unconditional literal SVG ids from one source definition resolve. Repeated copies must all contain a `foreignObject`; a conditional copy, a copy without one, ids from different source definitions, dynamic hrefs and dynamic ids stay quiet. References inside the target are not followed | A `foreignObject` draws nothing through `use`. Place it directly in the SVG for the web; native hosts refuse it. For content on every host, position an ordinary box over the SVG |
| A list and its detail (or a form) swapped by `if showingDetail` / `else` or complementary sibling `if`s (also with the same viewport-only `or` term), each with its heading, at the top of the view or inside a route; or pages named by one state, `page == "detail"`, in three arms or sibling `if`s or more, with or without headings (`lower-screen-by-state`). A tab bar's state, a radio group's, and a slot component's choice of how to wrap its `children` (the same content in a canvas or a plain scroll) stay quiet | Each screen a route: `routes`, a row that `push`es its detail, `back` to return; the web gets Back, reload and links, a wide window the split (`contract vocab screen`) |
| A box's border side with a positive written width and no written line style, including an affected arm of a class choice (`lower-border-unstyled`); CSS draws no line. A shorthand that omits its style resets it to `none`; an omitted width resets to 3px, so `border="#ccc"` also warns. Colour longhands alone, zero widths, deliberate styles (including `none` and `hidden`) and native controls stay quiet; bare buttons and fields are checked | Add `border-style="solid"` for all four sides, or the affected side's `border-top-style="solid"` (and likewise right, bottom or left). For a width from a conditional class, add the style inside the class named by the warning, preserving the other arm. A shorthand can say both: `border="1px solid"` before the colour |
| `column padding=24 width="100%"`, as wide as its parent's content plus the padding: it runs past the screen's right side (`lower-width-padding`) | Drop the width (a block or a stretched flex item fills its parent), or `box-sizing="border-box"` |
| An absolute width ceiling containing scalable text stays bounded at accessibility sizes (`lower-text-px-width`). The lint follows `width` and `max-width` through their bounds: a fixed maximum alone caps growth; a minimum alone does not. A fixed minimum can override a smaller cap, but a growing or unknown minimum makes the lint stay silent. Absolute lengths include numbers, px, pt and absolute `calc()`; `em` is fixed against an authored absolute font. Flex can supersede width on a row’s main axis; zero grow with an auto basis keeps its ceiling, and flex does not size a column child’s width. Effective classes, declaration order and parent direction count. Unresolved classes and uncertain sizing stay silent. A computed display leaves the node uncertain but its children are still checked; known-hidden subtrees are skipped. Text in absolute positioning, popovers and dialogs is checked inside that subtree, without counting against enclosing boxes. Text inherits the platform root font. Native Apple controls use the Apple control font, which can scale with Dynamic Type; web controls inherit, and Linux control defaults are fixed. An absolute font size stops scaling within that boundary; `inherit` and `unset` use the parent font even on controls. The warning reads lowering’s grouped-list typography and checks layout boxes containing text runs or text fields, including HTML inside SVG `foreignObject`; an inline run’s own width does not constrain its paragraph | Express the width and its bounds in `rem`, such as `width="3rem"` or `max-width="3rem"`, so they follow the root font, or let the content determine the width |
| `text "Results" aria-level=2`, a level with no role, which is no heading to a screen reader on any platform, gets no heading style and is not the iOS bar's title (`lower-level-without-heading`) | `text "Results" role="heading" aria-level=2`; `role="heading"` alone is level 2 |
| `column role="dialog" aria-modal=true` for a modal you lay out (`lower-dialog-role`) | A sheet, a route with `navigationPresentation="modal"` ([a navigation stack](recipes/navigation-stack.md)); a confirmation is `dialog role="alertdialog"`, opened with `showModal(id)` |
| `image "symbol:heart" -exact-tint-color="-exact-secondary-label"`, which Apple draws in the text colour: a symbol is drawn in its `color` on every host, and `-exact-tint-color` colours a raster image or a native button's face (`lower-symbol-tint`) | `image "symbol:heart" color="-exact-secondary-label"` |
| `` `${n} cards` ``, which reads "1 cards" (`lower-count-plural`) | The noun chosen by the number: `` `${n} ${n == 1 ? "card" : "cards"}` `` (`contract vocab plural`) |
| `for p in posts` with `users.filter(u => u.id == p.userId)` in the row: every update walks posts × users, about 100 ms at 1,000 × 1,000 on a Mac (`lower-row-lookup`) | Join it in the data module: the source gives each post the field it reads (`userName`). A root `derive` evaluates a small view-only join once per settlement; a child derive repeats it at each read. Both remain subject to the list-step bound |
| `for t in tokens` with an offset `tokens.slice(0, tokens.indexOf(t))…` in the row, or a `fn` that walks the list it is passed: each row's work grows with the list, and past 65,536 steps that binding is unset (`lower-row-own-list`) | Count it once in the data module: the source gives each token the field it reads (`at`) |
| `RoundButton(label="Start quiz", …)`, cut to "Start q…" in its 80-point circle (`lower-round-button-label`) | The circle is for a word (Start, Stop, Lap); a screen's main action of more words is `button buttonStyle="filled"` with its `text` (`recipes/content-design.md`) |
| `now = performanceNow()`, then `now` read in the same action (`analyze-read-after-assign`) | Bind it and read the local: `let stamp = performanceNow()`, `now = stamp`, then `stamp` |
| A child owning a `resource`, `mutation` or `task` used under a `for` over data or in a virtualized list (`type-child-resource`) | Put the resource family, mutation or shared task in the enclosing screen and pass its answer or action to the rows |
| `fontSize`, `radius`, `resizeMode` | `font-size`, `border-radius`, `object-fit` |
| `width=20px` | `width=20` or `width="20px"` |
| Unbounded `scroll` | Real height/max-height or flex in a bounded layout |
| `state item = none` with no usable type | Declare it (`state item: option<Item> = none`), or rethink whether it is mutable state |
| `let next = …` where `next` is already an action, state or other name (`type-let-shadow`) | A name of its own: `let revised = …` |
| `let op = ""`, then `op = "tab"` in a branch (`type-let-reassign`) | Choose the value where it is bound: `let op = key == "Tab" ? "tab" : "insert"`; or a `state` |
| `mutation exported` and `action exported` (`type-duplicate-name`) | One name per declaration: a component's props, states, derives, resources, mutations and actions share names (`action fileSaved`) |
| Dynamic navigation template | `path("route", args…)` |
| Unconditional per-frame app work | CSS/presentation motion where possible; a frame task gated on the state that needs it (`task fly when flying`) |
| An always-on `every` that checks whether a toast expired | `task hide when toast != "" key=toastUntil` with `after(ms, clear)` |
| Add a function because it exists in JavaScript | Check the roster or put the operation in the data module; `len`, `xs.reduce(…)`, `xs.find(…)`, `push(xs, x)` and their kind are refused naming what to write |
| `background-color: "Canvas"` in a `style` | `background-color="Canvas"` (or nothing: an unset background is the platform's) |
| `change=flip(t.id)` on a checkbox, `action flip(id: string)` | The event appends its payload: `action flip(id: string, checked: bool)` (the refusal spells it) |
| Two `send`s to one mutation in one action | One combined request, a mutation per request, `mutation … queue` to run both in order, or `mutation … parallel` to run both at once |

What compiles and then misbehaves (an image tile at its intrinsic size, native bars
the agent does not show, a back gesture refused) is in
[the pitfalls list](agent-pitfalls.md), with what to do about each.

Do not “repair” a refusal by adding a compatibility alias to the compiler. A
new language or host feature is a separate, explicit implementation decision.
Repository repair loops are capped at three rounds; report a remaining blocker
with the diagnostic and evidence rather than silently weakening the result.

## Completion criteria

Before reporting an app change done:

- The intended Contract and imported files compile; required data sources exist.
- Complete examples use current syntax, including inferred effects and provider sections.
- The app was built for every affected runtime surface required by repository policy.
- The requested interaction was driven, with state/log/layout evidence as appropriate.
- A reference design is matched in structure, controls and hierarchy, not pixels:
  native controls set their own metrics, so stop when it reads as the same app.
- Authored tests and relevant checks pass, or remaining failures are named accurately.
- Formatting and line caps hold; no generated artifacts or unrelated work entered the commit.
- The result states what changed, how it was verified, and any actual platform limitation.

For learning, continue with the [human guide](contract-for-humans.md). For exact
syntax and vocabulary, use the [grammar reference](contract-grammar.md). For a
new feature, start from the corresponding compiler fixture rather than memory
of JavaScript, React Native, or the predecessor's Contract language.

CSS overflow accepts `visible`, `hidden`, `clip`, `scroll`, and `auto`.
`clip` clips without a scroll container or the zero automatic minimum size of `hidden`. `auto` clips and
permits scrolling, with indicators shown only when content overflows. A visible
axis beside hidden/scroll/auto computes to auto, as on the web. Beside `clip` it
stays visible: `overflow-x="clip"` clips sideways overflow and leaves what
overflows below showing, with square corners.

`filter` on a box or an `image` takes CSS's functions in order (`blur`,
`brightness`, `contrast`, `drop-shadow`, `grayscale`, `hue-rotate`, `invert`,
`opacity`, `saturate`, `sepia`): `image "assets/logo.png" filter="brightness(0.6)"`
dims a logo. The element and its content are drawn, then filtered (Filter
Effects 1 §12); the web, macOS and iOS draw it, and Linux draws it unfiltered
for now (SVG Gallery's `fx-box-filter` and `fx-image-filter`). `url(#id)`
naming an SVG `filter` works on SVG elements everywhere, but on a box or an
image only on the web: Apple does not take it there yet (LLP 1055.000 §0).
In `drop-shadow()`, an omitted colour or `currentcolor` uses the element's
`color`; the colour may precede or follow the two or three lengths.

`backdrop-filter="blur(8px) saturate(1.14)"` filters content behind a box;
the two functions may appear in either order. On macOS it samples only the
parent's subtree: the parent's own paint and earlier siblings. Content behind
a clear wrapper can therefore show through sharp. Make the glass a direct
sibling of the content it covers, including a scrolling list. iOS and tvOS
draw the system blur approximation described in LLP 1053.000 §3.

`-exact-glass-tint` tints the glass of a box with `backgroundMaterial="glass"` or
`"glass-clear"`: `-exact-glass-tint="rgba(232,192,99,0.9)"` is a gold button,
`"rgba(10,12,16,0.55)"` a dark panel. It is a colour resolved in the box's colour
scheme (`light-dark()` and the platform colours work); unset, the glass is untinted. iOS and tvOS 26 set it as `UIGlassEffect`'s
`tintColor` and macOS 26 as `NSGlassEffectView`'s, so the glass itself takes the
colour; a `background-color` is painted behind the glass, not into it. The web and
Linux draw it as the material's background colour in place of the table's; Android
and Windows draw no glass and ignore it. Glass reacts to a touch (iOS's interactive
glass) when its box has a `press` and is not disabled; `glassInteractive=false` turns
that off on a pressable box and `glassInteractive=true` turns it on for one without
a press. Only iOS and tvOS 26 have interactive glass; other hosts ignore it. Neither
applies to a native button, whose glass is its `buttonStyle`'s.

`flex` accepts CSS `none`, `auto`, `initial`, `unset`, a basis, or
`<grow> [<shrink>] [<basis>]` (including `flex="0 1 auto"`). Numeric bindings
keep the `n 1 0%` meaning. `flex="initial"`, `flex="unset"` and a conditional
binding's `none` arm (including a conditional class's absent row) restore
`0 1 auto`; the CSS keyword `flex="none"` means `0 0 auto`.
Shorthands may be literal choices; computed strings are refused. A scroller
with `min-height=0` and positive shrink fits under a bounded flex column.

`display="grid"` takes `grid-template-columns` and `grid-template-rows` as
CSS track lists: px, `rem`, %, fr, `auto`, `min-content`, `max-content`, named lines,
`fit-content(px or %)`, `minmax()` and `repeat()` (fixed counts, `auto-fill`,
or `auto-fit`). Track lengths also accept `calc()`, `min()`, `max()` and
`clamp()` over px, `rem` and %, including inside `minmax()` and valid repetitions:
`grid-template-columns="repeat(auto-fill, minmax(min(100%, 12rem), 1fr))"`.
Percentages use the grid's content box in that axis; when it is indefinite,
they act as `auto` during intrinsic sizing and resolve against the resulting
size. Negative calculated sizes clamp to zero. The wire and web use equivalent
canonical CSS, so whitespace and simplified expressions may differ from the source.
Calculated `fit-content()` limits, other length units (`em` among them), `var()` and `env()`
remain unsupported here; `grid-auto-columns` and `grid-auto-rows` are not yet
Contract attributes. See [CSS Grid's track sizing](https://www.w3.org/TR/css-grid-2/#track-sizes).

Dimension rows (width/height, their min/max, padding/margins, offsets and
border radii) accept `vw`, `vh`, `vmin`, `vmax`, and `svw/svh/lvw/lvh/dvw/dvh`.
On native, all viewport variants follow the window; on web, CSS resolves
small/large/dynamic viewports. Font size, line height, letter spacing, text indent,
gaps, border widths, border spacing and shape margin accept these units too.
Dimension rows also accept the container units `cqw`, `cqh`, `cqi`, `cqb`,
`cqmin` and `cqmax`, alone or in a math function: a share of the nearest
content owner's content root on each axis its host fixes, else of the small
viewport (LLP 1123 D16). The scalar rows above, custom properties and inset
overrides refuse them. The web writes them as CSS's, which resolve against the
small viewport until a content root declares a container there, and leaves an
SVG attribute in one unwritten.
Inherited font sizes and lengths in `em` follow the resolved font on resize.

Dimension rows also take `env(safe-area-inset-top|right|bottom|left)` and CSS's
`calc()`, `min()`, `max()` and `clamp()` over px (and in/cm/mm/q/pt/pc/ch/lh),
percentages, those insets, viewport segments and viewport lengths. Functions
can nest, add or subtract lengths, and multiply or divide by a number:
`padding-bottom="clamp(15px, env(safe-area-inset-bottom), 60px)"`,
`bottom="calc(clamp(15px, env(safe-area-inset-bottom), 60px) + 59px)"`,
`width="min(50%, calc(100vw - 2 * env(safe-area-inset-left)))"`.
`clamp()` takes `none` for either bound (`clamp(none, 20px, 50px)`). CSS's
order is `clamp(MIN, VAL, MAX)`, not React Native's `clamp(value, min, max)`.
Native hosts resolve percentages during layout and update calculations when
insets or viewport dimensions change. The web uses CSS functions directly.
The insets are the document's: on the web the browser's, and on a native host the
device's under `viewport-fit="cover"` and 0 under `auto`, even on iOS when a root
`tablist` or navigation stack takes the whole screen; there a box at the root that
pads with them stays under the status bar
([pitfall](agent-pitfalls.md#native-presentation-and-navigation-ios)).
`var(--exact-safe-area-inset-top|right|bottom|left)` is the safe area where the box
is, a term in the same places as `env()` (LLP 1123 D9). Inside a route, a tab or a
sheet it is that container's own, as UIKit's `safeAreaInsets` has it: the status
bar, the navigation bar and a large title, the tab bar, the home indicator, a
sheet's edges. Outside every route it is what the page itself reaches under: on
iOS the status bar and the home indicator whenever a root stack or `tablist` takes
the whole screen, even where `env()` is 0. On the web and a host that draws no bars
it is `env()`'s, and a web sheet's top is 0. It takes no fallback, and it follows
the bars as the host lays them out (a rotation, a bar shown or hidden). Any box may
set it for its subtree, spelled as it is read:
`--exact-safe-area-inset-bottom="0px"` (a length, no percentage; one that reads
itself is ignored, as a CSS cycle is).

The host clears a route's content of the chrome it did not write (LLP 1123 D3), on
every host with bars: on each edge it pads the route by that inset, or the route's
scroller takes it as its content inset where the scroller starts at the route's top
or ends at its bottom. The clearance adds to the route's own padding
(`padding-top=16` under a 106-point bar puts the content at 122), except where that
padding reads the inset itself (`env(safe-area-inset-top)` or
`var(--exact-safe-area-inset-top)`, alone or in `calc()`, `min()`, `max()`,
`clamp()`): then the edge is the larger of the two, so
`padding-bottom="max(16px, var(--exact-safe-area-inset-bottom))"` is "at least 16, or
the home indicator". Padding on a box inside the route always adds. A header the
bar replaces neither clears nor counts. `-exact-safe-area` on the route (or a
`tabpanel`) turns the clearance off: `none`, or the edges to turn off
(`-exact-safe-area="top"`), the rest staying cleared; anywhere else it is refused.
The route's `background-color` already fills under the bars. For a background or
picture under the bar:

- Where the route is padded, pull a box at its top up and pad it back:
  `margin-top="calc(-1 * var(--exact-safe-area-inset-top))"
  padding-top="var(--exact-safe-area-inset-top)"`. Its content starts below the bar
  and the boxes beside it stay where they were.
- Where the route's scroller starts at its top (a list under a large title), opt the
  top out and pad the scroller's first box: `-exact-safe-area="top"` on the route,
  `padding-top="var(--exact-safe-area-inset-top)"` on the hero. The scroller then
  starts under the bar, at its start the hero's background reaches the screen's top
  and its text rests below the bar. `navigationBar="transparent"` does the same and
  also hides the bar's background until the content scrolls under it.

A box outside every route clears the status bar with
`top="var(--exact-safe-area-inset-top)"`.

Sizes, padding and radii clamp negative results to zero; margins and insets keep
them. `env(titlebar-area-x|y|width|height)` is the title bar beside a Mac window's
traffic lights under `viewport-fit="cover"`. It is zero in full screen or while the
session's toolbar fills the title bar. Once navigation history retains a toolbar,
it stays (LLP 1115.000, retention as built). In a single-location app, a root header
can bring a toolbar before history retains one; it goes away when that header hides
or its route says `navigationBar="none"`. Routes that all withhold their bars before
history retains a toolbar bring none; an authored toolbar or sidebar can still
supply one. With no toolbar of the session's own, `"display_override": ["window-controls-overlay"]`
in `app.json` gives the page the platform's tall title row and its title-bar area,
routes or not. Elsewhere the area is undefined, so give it a fallback (`contract vocab titlebar-area`). Unsupported: `em`, a unitless number where a length goes
(write `0px`, not `0`), a `var()` naming neither a design token nor one of those four, more than 32 values or 32 levels (LLP 1001 §2,
"Comparisons").

`translate` takes one or two lengths, each in px, a percentage of the box's own
border box, or a `calc()` of the two, as CSS's does: `left="50%" top="50%"
translate="-50% -50%"` on an absolute box centres it, a percentage follows the box's
size, and `translate="calc(50% - 12px) 0"` adds a px offset to it. Transitions and
keyframes interpolate the px and the percentage separately, as CSS interpolates a
`calc()`, so any two values interpolate. A `rem`, viewport or `env()` length, or a
`min()`/`max()` that does not simplify to px plus a percentage, is refused.

Transitions animate translate/scale/rotate/opacity, box paint (color,
background-color, border colors, -exact-tint-color, box-shadow), SVG paint/geometry
and pixel heights. On Apple, numeric `height` transitions work with either
`box-sizing`, without `interpolate-size="allow-keywords"`. An `auto` endpoint
requires that opt-in and `box-sizing="border-box"`; automatic height transitions
require one layout root and no registered content region. `width` and other general layout
properties cannot interpolate yet: native layout is not run per frame.
The diagnostic names this engine limit; `-exact-layout-transition` animates a
change in the laid-out box using the existing measured projection.

`cursor` takes CSS cursor keywords (`pointer`, `grab`, `grabbing`, etc.) and
inherits. Web emits CSS; macOS maps to NSCursor with system artwork stand-ins
where needed; iOS/tvOS/Linux ignore the hint (LLP 1001). URLs are refused.

Omit `font-family` for the platform's font. `font-family` accepts literal CSS fallback lists and choices of them, including
`"Inter, system-ui, sans-serif"` and quoted names. A family declared with
`font` uses its bundled faces; other names are local installed families,
whose own italic and bold faces `font-style` and `font-weight` select by CSS's
matching (a family without the face draws its nearest one, never a synthesized
slant or smear: LLP 1019 §5). Web, Apple and the painted hosts try authored
families in order for each glyph, then use platform fallback.
Under Charlie's 2026-10-11 ruling, a string from state or data may also name
an installed or generic family list, as in `font-family=family`. The JS web
target applies it as CSS; macOS, iOS and Linux register session-scoped stacks
and resolve installed faces when the string changes. An invalid list (an
unquoted name that is not CSS identifiers, such as `Arial;` or `1Arial`: quote
it) and `inherit`, `unset` or `revert` leave the family unset, so the text
inherits its parent's family, as the browser does; `initial` is `system-ui`. Runtime strings do
not load font files.
A runtime choice with a bundled-font literal is refused so it cannot silently
select an installed face with the same name; write bundled choices as literals.

A `text` with `selectionchange=act` hears the reader's real text selection
on the web, macOS, iOS, Android and Linux. `action act(s: Selection)` gets `s.text`, `s.start` and
`s.end` in the node's own text ("" at 0, 0 when nothing there is selected).
On web, macOS and Linux, `s.rects` is a list of `Geometry` snapshots for the selected
fragments in viewport space. It is empty when cleared. iOS and Android report an empty list.
`removeAllRanges()` clears the reader selection, and
`setSelectionRange(id, start, end[, direction])` selects a `text` on web and
macOS using source UTF-16 offsets. Both report changed parts through
`selectionchange`; see [text selection](contract-grammar.md#text-selection).
Drive it with `tap <id> drag <dx> <dy> from <x> <y> mouse`; on iOS, where a
long press or a double tap selects a word of text its author made selectable
(`user-select="text"`), with `tap <id> contextmenu [at <x> <y>]` or
`tap <id> dblclick [at <x> <y>]`, whose reply's `selection` says what was
selected (under `--touch platform`, `tap <id> drag 0 0 press 900` is a real
long press).

For a plain text field on the **web, macOS, iOS, Linux, Windows and Android**, `setRangeText(id, text[, start,
end[, selectionMode]])` replaces its selection or a UTF-16 range. Selection
is preserved by default (`select`, `start`, `end` are the other modes).
Exact deliberately fires one `input` so its action can update the bound
value, with `InputEvent.inputType="insertReplacementText"`. On macOS and focused
iOS fields, a changed value is one undo step; web and Linux add none. That input
type also covers hatch replacements and browser spelling
corrections, so it does not uniquely identify this command. `select` follows
input only when the selection changes. URL inputs trim surrounding ASCII
whitespace. Markdown editors on web and Apple use source offsets and add one
undo step. On Linux, Windows and Android, range replacement requires a plain
field. To edit and preview Markdown there, bind a plain textarea and a
`text markup="markdown"` reader to the same string. The terminal host
refuses `setRangeText`. Field
`selectionchange` reports caret moves through `InputEvent`, and
`beforeinput` reports the proposed edit's `inputType` and optional `data`.
Call `preventDefault()` to reject a cancelable edit; composition events
are noncancelable. See the [form controls](contract-grammar.md#form-controls-radio-inputevent-setselectionrange)
section for host limits.

Book typography is CSS's on every host. `text-align="justify"` fills all but
a paragraph's last line. `text-indent` is a length (`text-indent="1.5em"`,
`text-indent=24`; negative with the same `padding-left` hangs the first line).
`hyphens` is `manual` by default: a soft hyphen (U+00AD, written as the character itself or from data) breaks
and shows a hyphen; `none` ignores it; `auto` also hyphenates by the document's
language on the web and Apple (Linux has no dictionary and breaks only at soft
hyphens). Under `overflow-wrap="break-word"` iOS may break a long word with a hyphen it draws, so text that can
hold an address or a code (a URL, an email, a key) takes `hyphens="none"`: a hyphen inside it reads as part of it.

`text-wrap="balance"` (CSS's `text-wrap-style`, inherited) evens a short
paragraph's lines, for a tooltip, a toast or an empty state's message: the
lines before its first forced break keep their count and break where Chrome's
do, on every host. It balances up to six lines (more, a `line-clamp`, or a
word wider than the box, wrap as usual), moves only the breaks (the box measures as
before, and `text-align` places the lines in it), and `auto` turns it off. `pretty` and `stable` are refused; supported values are `auto | balance`.

`-webkit-font-smoothing="antialiased"` (inherited) draws text as Chrome and
Safari on the Mac do under that property: lighter glyphs, without the stem
darkening the Mac's default smoothing adds. Write it on the root only when the
web app being matched sets it; unsaid, text is the platform's. AppKit draws it
and the web writes it. Linux, Windows, a terminal and the widget and watch
drivers accept it and draw as before, as browsers off the Mac ignore it; iOS
is expected to as well, but has not been measured. `none` and
`subpixel-antialiased` are refused; supported values are `auto | antialiased`.

`text-box` cuts a paragraph's line box to its glyphs (CSS Inline Layout 3 §4),
to centre a label in a pill or line a heading's cap height up with an icon
without padding by eye: `text-box="trim-both cap alphabetic"` trims the first
line down to the cap height and the last up to the baseline. Write the trim
(`none`, `trim-start`, `trim-end`, `trim-both`) and the edges in either order,
or `normal`; the edges are over (`text`, `cap`, `ex`) then under (`text`,
`alphabetic`), and `auto` or a lone `text` sets both (a lone `cap` or `ex` is
refused: CSS's one-word edge also names the under edge). The longhands are
`text-box-trim` and `text-box-edge`. The cut uses the paragraph's own font, as
CSS's root inline box: a smaller run inside does not change it.
`text-box-edge` is inherited, so it may sit on a `view` above; the trim is
not, and goes on the `text`. Exact refuses the trim on a `view` or a `column`
(CSS trims a block's first and last lines, which Exact does not reach yet), on
an inline `text` run, on a `text` that is itself in columns, one included (CSS
trims every column), on a `text` laid out as flex or grid (CSS does not trim
it), and the `ideographic` edges. A later declaration, the node's own over its
class's, overrides an earlier one, and a `?:` choice of literals is refused
only when one of its values would be. A paragraph split across a `view`'s
columns is trimmed at its outer ends only. Every host trims: Apple and Linux
measure and paint the cut, the web writes CSS's own `text-box`.

Multi-column layout is CSS's (LLP 1093). A `view` (or a `text`) with
`column-count`, `column-width` or `columns="12em 3"` flows its content through
columns of one width, separated by `column-gap` (unset, 1em) and an optional
`column-rule="1px solid #ccc"`. `column-fill` is `balance` (the initial value:
columns as even as the content allows) or `auto` (each column filled to the
height in turn). Give the box a `height` and its overflow columns continue
sideways, one per page: that is how a reader pages a chapter, moving the
container by `translate` a page width plus a gap at a time, and reading where
the flow ends with `frame()` of an empty `view` after the last paragraph.
`widows` and `orphans` (2 by default, inherited) keep a paragraph's first and
last lines off a column's edge; `break-before`/`break-after` take `column`,
`avoid`, `avoid-column`; `break-inside` takes `avoid`. On native hosts a box
that has its own height, a background, a border or a shadow, a row flexbox or
a grid, is kept whole in one column where Chrome would split it; the journal
says so once per box. Refused, each saying what to write: `column-span`, page
and region breaks, `balance-all`, dashed or dotted rules, and multi-column rows
on `row` or `column` (CSS ignores them on flex and grid; write `view`).

A text field (`input` of type `text`, `email`, `password`, `search`, `tel`,
`url`, `number` or no type, and `textarea` outside the Markdown editor) is the
platform's own field by default (LLP 1104). On the web it inherits the page's
font, as a CSS reset does; other platforms use their control's own text style.
Its text is its own `color`, else the design's `FieldText`, else the
platform's field ink, on every host: a colour set on an ancestor never tints
it (LLP 1115 D4, LLP 1121 D9). A field of your own (`appearance="none"`) and
the Markdown editor inherit `color` as text does. Disabled and placeholder
appearances are the platform's. To colour
the placeholder, write CSS's `::placeholder` as a block under the field (its
value may compute) or in a `style`; it admits `color` alone, with its own
alpha, and without it the placeholder keeps the platform's colour (LLP 1104
D3; `contract vocab ::placeholder`):

```contract
style Field
  ::placeholder
    color="#6b7280"

component App
  state dark = false
  view
    column
      input placeholder="Search" class=Field
      textarea placeholder="Notes"
        ::placeholder
          color=(dark ? "#9ca3af" : "#6b7280")
```

A pressed, hovered, focused, disabled or checked look of your own is a state
part (LLP 1121 D8): a block of that state's paint rows, in a `style` or under
the node before its children.

```contract
style PostButton
  background-color = "#1083fe"
  :active
    background-color = "#0b5fbd"
  :disabled
    background-color = "#9cc8fb"

component App
  state busy = false
  view
    column class=PostButton press=post disabled=busy
      text "Post"
  action post
    busy = true
```

`:disabled` and `:checked` (`checked`, or `aria-checked`, `aria-selected` or
`aria-pressed` true) are the runner's: they take `background-color`, `color`,
`border-color`, `opacity`, `box-shadow` and `-exact-tint-color`, `:checked`
under `:disabled`, and `:disabled` is your own disabled look (no default dim).
`:active` is the host's own press of the innermost node with a `press` or a link's
`href` (`link href=`, `button href=`, as CSS's `a:active`): it takes
`background-color`, `color`, `opacity`, `translate` and `-exact-press-scale`, which become the
node's pressed paint (LLP 1061 D7). Its `opacity` is the opacity it shows
pressed, not a share. On a custom node they replace the host's default press
and move under Reduce Motion too. On the web, Linux, Windows and Android each
property takes its timing from the node's `transition`: a property it does not
cover lands at once, so `transition="none"` lands all five at once. Without a
`transition`, each moves as the platform times a property change: GTK's button
timing on Linux, WinUI's on Windows, Compose's default spring on Android, and
120 ms on the web, which has no platform press timing. On Apple they ease on
the press curve. Native buttons keep their own feedback and its timing beneath
each authored override. On Linux, Windows and Android their authored colours
land at once, as do their authored opacity, scale and translate unless their
`transition` covers them. The `translate` of `:active`
is an offset added to the node’s own translation: `translate="4px 4px"` moves a button onto an offset shadow
plate. It takes x and optional y, in pixel lengths or percentages of the
current border box by default. A third value is refused: a press is
a 2D offset; use transitions for 3D motion. Layout stays unchanged. On the web the hit area moves with the paint, as CSS
does, and the browser’s click rules apply. Native hosts keep the unpressed
hit area and use their press timing. Release or cancellation removes the offset.
Its targets may compute during ordinary commits, including a conditional class
and `none` to clear a target. Pressing needs no runner round trip.
Absolute active opacity and `-exact-press-opacity` are mutually exclusive
(`lower-state-press-paint`), as are two declarations of the press scale. Border
and shadow targets remain unsupported until their host paint is implemented.
`:hover` and `:focus-visible` take `background-color`, `color`,
`border-color`, `opacity` and `box-shadow`, shown at once while they hold (a
`:hover` replaces the platform's hover wash, as `:active` its press; on
the web a `transition` of the node's own on one of those rows still times it), the
focus's over the hover's, under the press. Each host applies them on its own
tracking, over the node's rows (rows a part doesn't name show the base, also a
base that changes while pressed); hover and focus rows are literals, tokens included. A
disabled node takes none. The web (both targets) does this today; Apple, Linux
and Android follow (LLP 1121 stage 8 B). A look's `color` reaches the node's text
as its own `color` does (a descendant that writes its own keeps it), so
`:hover color="ink"` on a link recolours its words, under a `color-scheme`
boundary too; the web and Apple (a Mac's pointer, an iPad's) show it, and
Linux's looks paint the box alone. They also take `scale`, which stands in for
the node's own `scale` while the look holds: like it, it never lays out, and a
`-exact-press-scale` multiplies onto it. It moves there and back on the node's
own `transition` for `scale`, as a `:hover` rule's value moves in a browser, on
every host with a pointer (the web, a Mac's, an iPad's, Linux and Windows); with
none it snaps, and on the web so does one under a `-exact-spring`. An icon
button that grows on hover:
`button scale=1 transition="scale 150ms ease"` with `:hover` `scale = 1.08`.
Native buttons and list rows accept
`:active`; their other states remain the platform's. Active properties keep a
button native, including with `appearance="auto"`. Each written property overrides
that field; unwritten fields keep the native highlighted state. Clearing a target
restores the current native configuration. UIKit and AppKit own native button
tracking, cancellation and accessibility. Native Apple controls add no fixed
press curve to authored properties. A projected bar item, tab segment, menu row or
confirmation action accepts only properties its native API can express. The
host rejects an unsupported combination in the journal and keeps the native
component. A UIKit title-only bar item supports active `color`; image bar items
and per-item segments do not. Remove the unsupported properties.
Other native controls refuse state parts.
A node's own part whose state it never writes (`:disabled` with no
`disabled`) is warned of (`lower-state-dead`).

A native control takes `accent-color` and, a field, the design's `Field`,
`FieldText` and `placeholder` roles, where its host honours them: the
schema's control-role table says which, per host
(`docs/control-roles-measured.md`). For each target the app ships (not the
terminal, which draws no colour, nor tvOS or watchOS, which have no column), a
parameter you set that the host there does not honour is said once, one line
for each declaration that sets it, naming every host and control
(`forma-unreachable: accent-color on macOS switches: NSSwitch draws the
system accent (the first at line 12; LLP 1121 D9)`): the platform's look is
the answer there, or a control of your own (`appearance="none"`, or a styled
pressable for a disabled prominent button). It is information, not a
problem: `verify` and `test` say it as a note and do not count it. A field's
own `color`, `background-color` (or `background`) or `::placeholder color`
wins over the role, so it is not judged.

A style a native host does not draw as written is recorded on its row in the
schema (`hosts`), and `contract vocab <name>` prints it. A style with no such
line has no gap on record, which is not a measurement. For each target the
app ships, an undrawn style is named once by `style-undrawn`, with the
unsupported scope and a supported route where one exists. It is information,
as `forma-unreachable` is. tvOS reads iOS's entries; a host that draws a style
differently (`differs`) is said by `vocab` alone.

For overprint with `mix-blend-mode="multiply"`, put the background and all
crossing shapes inside one `svg`. iOS and tvOS composite that SVG in Core
Graphics; Android rasterizes it with tiny-skia before sending the picture to
its reader. macOS keeps its public Core Animation layer blend, including the
backdrop outside the SVG. The other CSS blend modes are supported there too. Blending with
boxes or a different surface outside that SVG remains unsupported on iOS and
Android. Canvas 2D's `globalCompositeOperation = "multiply"` also draws on both:
draw the backdrop and foreground into the same canvas. It does not blend the
canvas with the page beneath it.

A background, border or radius makes it your own box, as in a browser;
`appearance="none"` says so explicitly. This is decided once after classes
and shorthands: a row on any conditional arm counts, even if its value is
`none` on another arm. `background-clip` and `background-attachment` do not
make it bare. `appearance="auto"` explicitly asks for the native field and
refuses background, border and radius rows, naming each longhand. Appearance
is a literal, resolved from the class then the field's own attribute; to
switch it, write `if` with two fields. Excluded input types and the Markdown
editor keep the bare text-input box or their existing specialised control.

A focused field shows the platform's ring: the OS's on a native field, Exact's
on a bare one (LLP 1104 D6). `outline="none"` (or `outline-style="none"`)
takes it off on every host, for a field whose focus you draw yourself, such as
a ring on the card around a composer. It does the same on any other element
that takes the focus (a `button`, a `link`, a native control, a `tabindex`
box), as CSS's does; tvOS keeps its focus effect, which is how a remote
moves. The card's `focusin` and `focusout`
handlers hear the field's focus on every host without making the card a Tab
stop. `focusout` also fires when focus moves between descendants and when the
focused node is removed; use its `FocusEvent.relatedTarget` to identify a
subtree exit. `stopPropagation()` stops a focus event at its current handler;
autofocus delivers these events too, with IDs read when focus moves.
An element with no visible focus fails WCAG 2.4.7. Tab still reaches it and its accessibility is unchanged. Only
`none` is admitted.

`textarea rows=3` sets its preferred height in lines (default 2); explicit CSS
height and `field-sizing="content"` override it. `maxlength=80` on text inputs
and textareas limits user edits in UTF-16 units; authored `value` updates are
not truncated. It does not apply to `input type="number"`.

`intersect=seen` observes geometric intersection with the nearest ancestor
scroll container (or viewport). `action seen(e: IntersectionObserverEntry)`
reads `e.isIntersecting` and `e.intersectionRatio`; authored arguments precede
the optional record. It fires once on mount, including offscreen, then on
enter/leave at threshold 0. The ratio is a snapshot, not a continuous progress
signal; edge contact may be true with ratio 0. `intersect-margin="200px"`
expands the root for overscan; a literal of one to four `px`/`%` lengths (units
are case-insensitive) has IntersectionObserver's `rootMargin` semantics (percentages use the corresponding
root dimension, as in Chrome).
On iOS, a `resizes-visual` route's viewport also counts as a root. `overflow: clip` remains
an intervening clip, and margins expand only the observer root.
Removing a node stops observation, with no synthetic leave. Both web targets
use the browser. AppKit, UIKit and Linux sample native layout and scrolling;
inline runs, moving presentation geometry, rotated roots and SVG clip references
remain unqualified (LLP 1005, intersection visibility; LLP 1001 records the
percentage-margin spec deviation). Other native hosts remain unqualified.
Container queries and live-position feedback are still deferred.

`resize` is CSS's: `vertical` (or `block`), `horizontal` (`inline`) and `both`
give a textarea the grip a person drags to size it, on the web and on the Mac
and iOS. Only a textarea has a grip on a native host, and no native host other
than the Mac and iOS draws one. The drag writes `width` and `height` as the
browser's inline style does, inside `min-*` and `max-*`. Tailwind's preflight says `textarea { resize: vertical }`;
here a textarea's default is `none`. Given an action instead,
`resize=fit` is the element resize event, `ResizeObserver`'s: `fit` hears the
content box's width and height after the first layout and whenever they change
(one more parameter: its `DOMRectReadOnly`), on every host
([Events](contract-grammar.md#events)); read other boxes there with `frame(id)`
rather than polling with a timer. `user-select="none"` prevents
ordinary text selection; `auto` is the default, and on the Mac it is AppKit's:
a label inside a button, link or other control, or in a `header`, `nav`,
`footer`, toolbar, tablist or menu, is not selectable, while an `article`'s text
and other content text is (so put prose a reader copies in `article`, not a
`header`). On iOS `auto` is UIKit's, where a label selects nothing: write
`user-select="text"` on prose a reader copies (on the text or anything above
it), and a long press or a double tap there selects a word with the system's
handles and its Copy, Look Up, Translate and Share menu, one paragraph (one
`text`) at a time. A label inside a control stays unselectable unless the
`text` is said on it or inside the control. On the web `auto` leaves the
choice to the page's stylesheet, but a control's label (a button, link,
pressable or control role) is not selectable there either; a text's own runs
select with it. `user-select="all"` makes an element's text one unit, as CSS
UI 4 has it (a path, a device code): on the web and macOS one click in it
selects all of it, and a drag, a Shift-click
or Shift with an arrow that reaches into it takes it whole. On iOS a long
press or a double tap in it selects it instead of a word, and a tap selects
nothing, as on a `UITextView`; an element around a paragraph selects that
paragraph (one selection is one paragraph there). An inline run that says
`all` is selectable in text that is not (a label, a `header`), alone, on
all three. Text that presses (`press=`) still presses on each click or
tap; on macOS it does not select, and on iOS a tap while it holds a
selection clears the selection, as in a text view. On Android a long press on
`user-select="text"` selects a word. The handles adjust the selection within
one paragraph, and the floating toolbar offers Copy, Share and Select all.
An inline or enclosing `all` selects its text as a unit. Dragging the paragraph
away from a handle scrolls its container. On Linux and Windows, drag across opted-in text,
double-click a word, or triple-click a paragraph. One click selects a run marked `all`.
Ctrl+C copies the visible text when the host supplies a clipboard.
On tvOS, `text` and `all` leave their content unselected. `contain` is refused by
name: no native host has it. These rows take literals
or choices of literals, so unsupported runtime values cannot bypass the check.

**Colours: say a role, not a value.** Leave a colour unsaid where you can (text,
tint, page and sheet backgrounds, separators and controls are the platform's).
Where you must say one, name a role, which each host resolves to its own colour
for light, dark and Increased Contrast (`labelColor` on iOS, the browser's own on
the web; LLP 1095): CSS's system colours `Canvas`, `CanvasText`, `LinkText`,
`GrayText`, `AccentColor`, `AccentColorText`, `Field`, `FieldText`, `ButtonFace`,
`ButtonText`, `Highlight`, `HighlightText` (`AccentColor` and `LinkText` are the
tint: the nearest `accent-color` a node or an ancestor sets, else the platform's, as
UIKit's inherited `tintColor` is; `AccentColorText` is the ink on that accent, on the web
black or white by its lightness as a filled button's label), and Exact's `-exact-label`,
`-exact-secondary-label`, `-exact-tertiary-label`, `-exact-quaternary-label`,
`-exact-placeholder`, `-exact-separator`, `-exact-opaque-separator`, `-exact-link`,
`-exact-symbol` (what a symbol nobody tinted is, the initial `-exact-tint-color`:
the tint on iOS and the web, the secondary label colour on the Mac),
`-exact-background`, `-exact-secondary-background`, `-exact-tertiary-background`,
`-exact-grouped-background`, `-exact-secondary-grouped-background`,
`-exact-tertiary-grouped-background`, `-exact-fill` (and `secondary-`, `tertiary-`,
`quaternary-`), and the hues `-exact-system-red`, `-orange`, `-yellow`, `-green`,
`-mint`, `-teal`, `-cyan`, `-blue`, `-indigo`, `-purple`, `-pink`, `-brown`,
`-gray` (each `-exact-system-…`). `color="-exact-secondary-label"`, not
`color="#8e8e93"`; `border-color="-exact-separator"`, not a grey hex. Surfaces come
in pairs: a card on a plain page (`-exact-background`, or unsaid) is
`-exact-secondary-background`; on a `-exact-grouped-background` page it is
`-exact-secondary-grouped-background` (white in light mode on iOS and the web, so on a plain white page
it vanishes there; on the Mac both card roles are AppKit's under-page grey); a surface inside either card is its pair's tertiary role. A route whose
scroll is a grouped list (`list appearance="auto"`, `inset-grouped` or `grouped`) is a
`-exact-grouped-background` page with nothing written, bar to bottom edge, so a switcher
or summary between its `header` and the list sits on the list's grey (LLP 1116.007 D5); a
`background-color` on the route, or on that content, wins. One on the list itself is its
ground around the cards, which keep theirs: `transparent` shows what lies behind it, such
as an `aria-hidden` `svg` at `z-index=-1` (LLP 1084 §6.5). A brand colour
the platform has no role for is the one place for a literal.

**Type: say a heading, not a size.** Body text is the platform's body size
already (the root font size: Dynamic Type's body on iOS, 13 on the Mac, the
browser's 16). A `text role="heading"` with no `font-size` or `font-weight` of
its own is the platform's text style for its `aria-level` (LLP 1115 D3): 1
`title1`, 2 (the default) `title2`, 3 `title3`, 4 and on `headline`. Apple's ramp
gives 28/22/20/17 pt at the iOS default, 22/17/15/13 pt on the Mac and
27/21/19/16 px at a 16 px web root. Android's Material look uses 28/24/22/16 sp
at a font scale of 1 (LLP 1115.001 D4). iOS, macOS and Material keep each style's
weight and leading. The web, neutral and terminal looks use bold for levels 1
and 2, semibold for 3. Omit heading sizes and weights unless customizing them.
An authored `font`, `font-size` or `font-weight` wins; an inherited one does not.

Native navigation bars and grouped-list headers own title spacing, and title
typography the author leaves unsaid. A `font-family`, `font-weight`, `font-style`
or `font-size` written on a bar's heading replaces that one part of the
platform's title font: on iOS in the navigation title (a level-1 heading's size
is the large title's, any other's the inline title's), on the Mac in a modal
route's sheet title. The Mac's window title keeps AppKit's font, which no public
API changes. A heading inside an iOS pressable title group takes the same
authored typography over the bar's headline font.
On iOS, a route's level-1 heading gives a large navigation title; other levels
give an inline title. Set UIKit's `largeTitleDisplayMode` through a native access
hatch to change presentation while keeping the semantic rank. Check the host's
implementation for its supported APIs.

Other text takes a style by name with `font`, as WebKit's
`font: -apple-system-headline`: `font="-exact-footnote"` (or
`"-apple-system-footnote"`) sets the style's size and a `normal` font style, and
the style's weight and leading as each platform draws it (LLP 1115.000 D12.2:
the Mac's `headline` bold and its styles' own leading, 13/16 for body; iOS's the
face's own), beating an inherited weight or line height as CSS's shorthand
resets them. The styles are `-exact-large-title`, `-exact-title1`, `-exact-title2`,
`-exact-title3`, `-exact-headline`, `-exact-body`, `-exact-callout`,
`-exact-subheadline`, `-exact-footnote`, `-exact-caption1`, `-exact-caption2`;
`font-size="-exact-caption1"` alone sets the size and keeps an inherited weight
and line height, the style filling only what nothing above says.
`font-size="min(-exact-subheadline, 31px)"` grows with the text size up to its
cap, whether written literally or bound to a string, as UIKit's
`UIFontMetrics.scaledFont(for:maximumPointSize:)` holds one: for a
row of equal cells that must stay one line at the accessibility sizes (a week's
day numbers, which UIKit's calendar caps at 31 points), never to shrink text. A weight or
line height written on the node wins either way, and `layout` names each one's
source (`style`, `heading`, `inherited`, `authored`). `font` takes nothing else
but `inherit`, the common reset's: a control's text then takes every font
longhand from the text around it.
Bold Text (iOS's setting, Android's font weight adjustment) weights the system
font one step heavier, a written weight included, as a hand-built app's text
is, and leaves a family the app names alone: write the weight you mean, not one
you expect the setting to add (LLP 1115.000 D12.3). A drive turns it on, on
Apple, with `prefer bold-text true`.

A colour is any CSS colour the browser paints: hex, `rgb()`, `hsl()`, `hwb()`,
a named colour, `transparent`, `lab()`/`oklch()`/`color()` (clipped to sRGB
natively), or `light-dark(a, b)`; the kernel parses it once for every host.
An app names its own colours once, in a root `theme` block, one
`name="colour"` line an entry: a colour, a role (`card="secondary-grouped-background"`)
or another entry's name. A row writes the name as any colour, in every colour
row and inside shadows and gradients (`color="brand"`,
`box-shadow="0 1px 2px brand"`), and a string from data naming it reads the same
colour. An entry named like a role (`-exact-secondary-label`, or LLP 1095 D4's
bare `secondary-label`, or `Canvas`) overrides that role where a row names it.
Cycles, and a name inside `light-dark()`, are refused (LLP 1095 D4).

An old design is refused with `rename app.tamber to app.forma`. Change its header
to `forma <name>` and its manifest's `tamber` key to `forma` too, if present.
The old CLI verb refuses with the command to use: `bun exact.mjs forma`.

An app with a brand, or modes the person picks, writes its design in
`app.forma` beside `app.contract` (or the file `app.json`'s `"forma"` names)
instead (LLP 1121): a `theme` block of tokens (`surface.card =
light-dark(#f1f3f5, #16181c)`, written as CSS, dotted names allowed), and an
`axes` block of app modes (`dark-style = black | dim  under scheme=dark`), each
token's value in a mode on its own `when dark-style=dim: …` line. Rows name a
token as they name a role, and an expression over tokens
(`color-mix(in oklab, accent.fill 40%, surface.card)`) is folded by the build,
written whole, in a choice's arm or inside a shadow; a mix of colours alone
(`color-mix(in srgb, red 35%, transparent)`) is a token in the `theme` block.
`setMode("dark-style", "dim")` sets the window's mode; `-exact-mode="dark-style=black"`
on a node holds its subtree in one (pins are `axis=value`, by spaces or commas;
`axis:value` is refused), and inherited text inside takes the pin's value. The
agent's `prefer mode dark-style=dim` sets it from a drive (the app's own
`setMode` still wins after it: an app that restores a kept mode at launch
overrides an earlier `prefer`, so prefer after `clock data`; a dev reload keeps
the window's mode unless the edit removed it), `layout <id>` shows
the token beside each row's value, and `bun exact.mjs forma check | eval --mode
… | ir | fmt` checks the design, prints its tokens in a mode, writes its IR, or
puts it in its one layout (`fmt --check` changes nothing). The
rule: a role if the platform has one, a token if it is the brand. A token one
colour in every scheme by design (white ink on a brand fill, a black ground
under media) says so with `scheme=fixed` after its value (`white_100 = #ffffff
scheme=fixed`), and `lower-colour-one-scheme` leaves it alone; an unmarked one
is reported once, at its line in the design. `pair=surface.page` after a
foreground's value names what it is drawn on (a token or a role);
`contract forma check` warns when the two fall below 4.5:1 (`forma-contrast`).
The brand's states come from the theme, not a hatch (LLP 1129): `AccentColor =
accent` is the app's tint, so the keyboard's focus ring is drawn in it (on the web
a solid 2 px line, which reads on a control filled with the accent); `Highlight`
and `HighlightText` colour selected text on the web (iOS draws it in the tint); a
`button` you fill (`background-color="accent"`) hovers in its own ink, as the
filled button does, and a row or card of your own (a `link` or a pressable)
hovers in the brand's colour by a `:hover` part; a sheet's corners are
`border-radius` on its route. A button
in the brand's shape is a face you write on a `button` with no `buttonStyle`; a
`buttonStyle` is the platform's control, which takes the accent and no radius
(`lower-button-style`), and `ButtonText` and `LinkText` are the tint's, not the
theme's (`lower-theme-tint`).
A type token is CSS `font` syntax (`type.body = -exact-body/1.35 "InterVariable"`):
`font="type.body"` writes only the longhands it names, so the text style's own
weight applies where it names none, and a size in px is warned of
(`forma-type-fixed`: it does not follow the person's text size). A family token
is a stack (`family.text = "InterVariable", system-ui`), named by `font-family`.
The checker validates family stacks in both token kinds even when no row reads
them. Unsupported generics such as `cursive` are refused at the token's value.
One that varies by a mode goes on the text, not its container
(`forma-inherited-axis`); on the JS target each row it writes is a property
per mode (`--x-type-body-size`).
A component declares what a designer may tweak in a `params` section (LLP 1121
D15), one param a line: `radius: length = "radius.md"  range="0..40"  step=1
control="slider"  group="Shape"  priority=1  label="Corner radius"`. A param is a
token the component owns: its default is any token value (a literal, a token, or
an expression over tokens and the component's other params), and its rows and
defaults name it `param.radius`; a generated internal token name is refused
(`forma-param-unknown`). Every instance shares it. Its full name is the
component's declaring file and name, `post.contract#PostButton.radius` (an import
alias never changes it; moving the file makes it a new param), and
`PostButton.radius` where no other component has that name (a package's file
is its package's however a `use` reaches it). A param is a row's value in its
component's own rows (a style, shorthand or motion row, an image's `symbol:`
source): a string or a template, through `?:` branches, `match` arms and `+`,
as a motion token is; and its defaults. Passed in a component's argument, it is
the child's row's value. One in a `style` that one component's view alone
applies is that component's (LLP 1139 D3); one in a `style` another
component applies too, a `keyframes` or a state's value,
or in what a row computes (a function's argument, a condition, a comparison,
a callback), or that a child reads otherwise than as a row's value, is refused
(`forma-param-scope`). A param no
row of its component reads (a state part's row counts: `:active`'s
`background-color = "param.pressed"`), directly or through another param, is refused
(`forma-param-unused`); a ranged length is px (`forma-range-unit`), or a base
name or an expression over one, checked in every platform's column, and its
default must be inside the range in every mode; `control` follows the kind
(`color`, `slider`, `stepper`, `token`, `select` with `options="a|b"`, `none`),
and `priority` is 1 to 3. A design token may not be named `param.…`.
`contract forma ir` and the design server's `describe` list them under
`components`; `contract forma check` builds an app with params to admit its
design (a dial only a param reads is read), counts them apart, and charges a
refusal or warning on one to its default. A view that isn't Contract (a native module's, a Rust canvas
component) declares its params in the design instead: `component MapCard`, then
an indented `params` block of lines like a dial's, any kind (`inset: length =
8px  in 0..24  control=slider  priority=2`). Its identity is the design file
and the name (`app.forma#MapCard`); its declaration keeps every param, and
what its default reads, as the code reads them by name; a `when` is not the
component's (the app's design sets a param per mode).
The app sets a param's value in its
design's `params` block (`PostButton.radius = 8px`, `when` lines too), which
every instance shares; the design server's `set-param` writes it,
`unset-param` takes it back to the component's default, and a line naming no
param of the app is warned of.
A package can ship a design (LLP 1121 D11): a file named for the last part of
its `package.json` name (`@exact/reading` → `reading.forma`) beside its entry
file (what `exports` maps `.` to, else `index.contract`), or the file its
`package.json` `"forma"` names inside the package. It starts `forma reading`
and holds one `theme` block of tokens named `reading.…`, which the package's
views name. An app that uses the package gets them. The app's design sets one
by its full name (`reading.card = #abcdef` in `app.forma`), which replaces the
package's, `when` lines and all. A package's design may declare its
components' params (`component Map` with `params`, named
`<package>/<file>#Map`, as its Contract components are), whose values the
app's design sets (`Map.pin = #000000` under `params`, or the full name). It
may declare icons (`icons`, `reading.star = icon(sf "star", svg "star.svg")`),
each named `reading.…` and naming only its own package's icons; the app's
design overrides one by full name under its `icons`, whole. An icon's SVG path
is relative to the `assets/` of the root whose file writes it: the app's
directory for the app's, the package's root for a package's (an icon's, or a
param default's `icon(svg …)` in its design or its Contract), so a package
whose `"forma"` is `theme/palette.forma` still reads `<package>/assets/`; in v1 it must be a one-colour template
(`forma-icon-svg`). It declares no axes, dials, `params` lines or `exports`
(`forma-package-block`; its `when` lines name the app's axes), declares only `reading.…` names and none of the
base names (`forma-package-name`), and a token in it names only tokens the
same file declares, roles, and the base names `space.xs`…`space.xl` and
`radius.sm`…`radius.full`, which the app's design defines until the base scale
ships (`forma-unknown-token`). An app can't use two packages whose names end
alike (`@acme/reading` and `@other/reading`) when both ship a design. A
refusal points at its line in the package's file. `contract forma
check|eval|ir|gen` and the design server fold the app's design with its
packages' tokens, as the build does.
A length token may name a base name bare (`card.pad: length = space.md`) or in
a `calc()`, `min()`, `max()` or `clamp()` with px and numbers
(`calc(space.md * 2 + 4px)`): each host reads it in its own column, and the
compiler checks every column, naming the one a row refuses (a padding that is
negative on mac). A shadow token's lengths may be any of these
(`card.shadow: box-shadow = 0 space.md calc(space.sm * 2) black`), checked in
every column the same way (a blur negative on mac is refused); a literal
`box-shadow` row's lengths stay px, so name a shadow token there. A base name
times a length, and `rem`, `em` or `%` beside one, are refused.
A row's own value may be a `calc()`, `min()`, `max()` or `clamp()` over length
tokens (`margin-top="calc(var(--space-2) * -1)"`, or `calc(space-2 * -1)`):
each token is read where the row is drawn, so a `rem` token stays `rem` and
one that varies by mode follows it. A value computed from state or data
names a token only whole.
A package that is `"private": true` and ships a design is a library (LLP
1139): each component its entry declares or re-exports is held to its rules.
Its spacing, corner and colour rows (a shorthand's each side; `border`'s
colour), and its params' defaults of every kind, name a param, a token, a
role or a base name, or compute over one (`color-mix`, `light-dark`,
`calc`), never a literal (`0`/`0px` and CSS keywords such as `auto` and
`transparent` excepted; `lower-library-literal`); a control's role is the native control's (`input`,
`textarea`, `select`, `progress`, a native `button`) with what its platform
needs (`checked` and `aria-label` or `aria-labelledby`, a range's
`min`/`max`, …;
`lower-library-control`); each of its buttons (a `button`, or a node of
`role="button"`) is named, by `aria-label`, `aria-labelledby` or a text of
its own, and one whose look reads a state prop (a bool named `selected`,
`current`, `pressed`, `checked`, `on`, `active` or `expanded`, in a `?:`,
`match` or class) says it aloud through `aria-selected`, `aria-pressed`,
`aria-current`, `aria-checked` or `aria-expanded`, or a label that reads it
(also `lower-library-control`; any other bool, a size or an ink, is
presentation); and nothing in the package reads the host it runs
on (`exactDelivery`, `exactSurface`, `exactViewport().displayMode` read
from the viewport wherever the package passes it: an alias, a `provide`, a
`fn`; a prop's `displayMode` is the app's data; `lower-library-platform`). Not checked yet: an injected value whose
provider is outside the package, and per-platform host entries (no syntax
for them). A field names its active option with `aria-activedescendant`
([combobox recipe](recipes/combobox.md)). Its names are its API: an app line or a Contract row naming a
name under its namespace that it doesn't define, or a param line naming one
it lacks, fails the build (`forma-package-unknown`; the nine base names are
no package's), and its params are named in full, a short name that names
none too (`forma-param-short-library`). The in-repo `@exact/actions`'
`BarItem` and `HeaderButton` are for an app's own authored bar and header (a
design's, as Bluesky's are); the platform's `tabs` and `header` routes stay
the platform's, and no library component wraps them. A `BarItem` says its
selection as the bar's current item (`aria-current="page"`, iOS's selected
trait) and takes no long press itself: an item held for more puts
`contextmenu` on its slot fill (Bluesky's Profile avatar), so the press lands
on the fill rather than the whole column.
`@exact/actions` also exports `FollowButton` `(id, following, text, label,
tone, size, shape, icon, press)`, a person's Follow button (Bluesky's
FollowButtonInner), an authored pressable under D9's exception as
`PostButton` is. Its variants are Bluesky's own: `tone` `"primary"`,
`"inverted"`, `"subtle"` or `"overlay"`; `size` `"small"` or `"tiny"`;
`shape` `"pill"` or `"round"` (icon only); `icon` `"both"` (plus, then a
check once following), `"follow"`, `"following"` or `"none"`. Its words are
yours: `text` is the visible label (Follow, Follow back, Following) and
`label` the spoken one (`""` speaks `text`). It says following by its label
alone, with no `aria-pressed`, so swap both with the state; a round one shows
no text, so give it a `label` in both states (`"Follow Ada"`, `"Following
Ada"`), which the compiler can't check. Each tone's fill and text, not
following and following, are params (`fill`, `ink`, `followingFill`, …,
`overlayFollowingInk`) over `actions.follow-*` tokens, its sizes are tokens,
and its press scales are params, one for a small pill, a tiny one and a
round one.
The in-repo `@exact/cards` exports PostCard, AtCard, ProfileHeader (and its
ProfileAlert shape), Avatar `(src, size)`, VerifiedBadge `(badge, size, ink,
mark)` and PersonRow; `badge` is `enum<"none", "verified", "verifier">` (as
ProfileHeader's and PostCard's), `"none"` drawing nothing. It was a string
with `""` for none until stage 15 B batch 2: pass `"none"` now, and a data
value of that enum type. `PersonRow` `(id, label, avatar, name, handle,
badge, pill, description, descriptionLines, divider, disabled, press)` is a
person in a list (Bluesky's ProfileCard), its one slot the trailing control
(a `FollowButton`, your own, or nothing). Its one press target is the link,
everything but the slot (`role="link"`, testId `<id>-link`, `label` its
spoken name, such as "View Ada's profile"); the slot is a column beside it at
the trailing edge, never in it, so the trailing control stays its own to a
screen reader, and the pill and description run under the names;
`handle` is the second line as shown (`"@ada.bsky.social"`), `pill` the
pill's text (`""` for none), `divider` `"bottom"`, `"top"` or `"none"`;
`disabled` dims the link (not your trailing control) and says so on it (`aria-disabled`) but doesn't stop `press`, so check in your
action. Avatar's and VerifiedBadge's `size`, and VerifiedBadge's
`ink` and `mark`, have no defaults: pass token or role names
(`size="cards.avatar"`, `ink="cards.badge"`, `mark="cards.badge-mark"`, or
your own), not literal colours, so setting those tokens restyles them.
`@exact/cards` also exports EmptyState `(id, icon, iconSize, tone, title,
message, button, buttonLabel, buttonIcon, buttonTone, press)`, what a list
shows with nothing in it: every text is yours, and an empty `icon`, `title`
or `button` leaves that piece out (`icon` and `buttonIcon` are image sources,
a `symbol:` name or an icon token). The three enums are required, with no
defaults: `iconSize` `"medium"` (48 points, Bluesky's usual), `"small"` (32)
or `"large"` (64) in a 64-point box; `tone` `"high"` (the message in the
label colour), `"medium"` (secondary) or `"label"` (message and icon in the
label colour); `buttonTone` `"primary"` (the brand blue) or `"secondary"`.
`@exact/lists` exports MenuItem `(id, label, icon, destructive,
first, pick)` and MenuGroup (a slot of items): a menu's drawn items in a
sheet, as Bluesky's menu is, not the platform's list cells; the sheet holding
the groups is yours and carries `role="menu"` (the items are `menuitem`s, the
group adds no role). Pass `first=true` on a group's first item, which has no
hairline above it.
The in-repo packages' icons are icon tokens, which an app overrides by full
name: `@exact/cards`' `cards.repost`, `cards.pin`, `cards.corner-down-right`
(PostCard's context and reply lines), `cards.ban`, `cards.eye-slash`,
`cards.warning`, `cards.circle-info` (ProfileHeader's alerts), `cards.attie`
(AtCard's visit button) and `cards.verified`, `cards.verified-check`,
`cards.verifier`, `cards.verifier-check` (VerifiedBadge), and `@exact/actions`'
`actions.reply`, `actions.repost`, `actions.heart`, `actions.heart-filled`,
`actions.bookmark`, `actions.bookmark-filled`, `actions.share`, `actions.dots`
(ActionRow), `actions.gallery`, `actions.camera`, `actions.gif`
(ComposeFrame's toolbar) and `actions.follow-add`, `actions.follow-check`
(FollowButton). Each but FollowButton's two has no `sf`, so Apple draws the
same path as the web; those two name SF Symbols' `plus` and `checkmark`
beside Bluesky's paths, so Apple draws the symbols (as some of the clone's
copies did; a copy that drew a path there changes, a named pixel change). The Attie mark, the toolbar's three and the badges' paths were inline
`svg` with `fill-rule="evenodd"`; each one's holes wind against its outline, so
it fills the same nonzero, and its SVG has no `fill-rule` (an even-odd icon
gives its app a new compat id). A badge is two one-colour templates stacked,
its ground in its `ink` and its check in its `mark`, so both colours stay a
card's tokens and a header's params; an override replaces one layer, its colour
unchanged. Still paths: `BarItem`'s and `HeaderButton`'s icons, which are their
caller's `d` (a prop), not the package's.
Code outside a view names a token by its name too (LLP 1121 D12): a data module
returns a `ColorToken` (or a `LengthToken`, `TypeToken`, … of its kind:
`contract types` declares the design's, so an unknown one fails `tsc`), and `bun exact.mjs forma gen ts|rust|swift|kotlin .` writes bindings into
`.exact/forma/` (never committed): `forma.ts` and `forma.css` (each token's
CSS per mode), `forma.rs` (`pub const SURFACE_CARD: Token<Color>`, and
`Token<Length>`, `Token<Number>`, `Token<Type>` for those kinds, which
`session.forma().get(SURFACE_CARD, place)` resolves in a running session), or
`forma.swift` (`ExactFormaToken.surfaceCard`, for a native view's
`ExactForma(view).color(.surfaceCard)`, `.length(_:)`, `.number(_:)` and
`.font(_:fallback:)`, a font at the person's text size with Bold Text's step),
or `FormaTokens.kt` (`FormaColor.SURFACE_CARD`, `FormaLength`,
`FormaNumber`, for `ExactForma(view).color(…)` in the app's hatches). A
component's param is named by `<Component>.<param>` there (`mapCardInset`,
`MAP_CARD_INSET`); two names that map alike are refused, naming both. The Apple
build writes `forma.swift` and the Android reader's `FormaTokens.kt` before
an app's modules compile.
Recipe:
[a brand](recipes/forma.md).
`color-scheme="dark"` (or `"light"`) on a node makes that subtree resolve
`light-dark()`, platform colours and glass in that scheme, as a sheet that is
always dark does; leave it off to follow the surrounding scheme (LLP 1034 §8).
A Theme setting calls `setScheme("light" | "dark" | "system")` in its action; the
choice is kept across launches and shown from the first frame, and `"system"`
forgets it.
An app that is always one scheme says `"userInterfaceStyle": "light"` or `"dark"` in
`app.json` (`automatic`, the default, follows the device). It is the app's appearance from
launch, as the device's would be: the launch screen, the first frame, sheets, alerts and the
keyboard are in it, `prefers-color-scheme` reads it, `setScheme("system")` returns to it, and
a drive's `prefer prefers-color-scheme` leaves it. A node's `color-scheme` still wins in its
subtree. iOS and tvOS write `UIUserInterfaceStyle`; macOS sets the application's appearance
before its first window; the web page declares it in its `color-scheme` and `<meta
name="color-scheme">` before the first paint. Android, Linux and Windows follow the device.
`status-bar-style="light-content"` (light text, for a dark surface),
`"dark-content"` or `"auto"` on any node, bound to state, sets an iOS or Android phone's
status bar: of what the bar sits over, the declaration painted on top wins.
`auto` follows the winner's `color-scheme`, else the page's scheme; without a valid
declaration the bar follows the page's scheme. On Android, `navigation-bar-style` takes the same three values for the
navigation buttons. It resolves independently in the same scope and defaults to
the page's scheme (LLP 1105 D7). With gesture navigation, Android can choose the
handle colour from what the page actually paints. Android's modal routes are
pages that cover the bar; native sheet detents and transition completion are not
available there (see `host/android/README.md`, Edge to edge). On iOS a flip shows in
its batch's frame after navigation settles, and `status-bar-animation="fade"` fades
it. Android ignores the animation; other hosts ignore both properties.
`system-bars-hidden=true` on any node, bound to state, hides an iOS phone's
status bar and auto-hides its home indicator (it fades after a few seconds
without a touch, and a swipe still goes home): a full-screen game or toy. It
resolves as `status-bar-style` does, apart from it: a `fullscreen` route's
declaration wins over the stack under it, a sheet at `medium` leaves the bars
to the screen behind, and `false` is a declaration too, so a descendant's
`false` shows the bars its ancestor hides. A change waits for a push or a
presentation to land. The status bar hides and shows at once unless the
winning declaration has `status-bar-animation="fade"` (when none wins any more,
the one that went); a style change still fades by its own winner's. On a phone with no notch or Dynamic Island the status bar's 20 pt
leave the safe area, so the content of a route moves up to the screen's top;
elsewhere the top inset stays, as the home indicator's does at the bottom, and
nothing moves: a route whose content must reach the screen's edges (the game's
canvas) takes `-exact-safe-area="none"` and places its controls with
`var(--exact-safe-area-inset-*)`. On Android it hides the
status and navigation bars (immersive mode): a swipe from the screen's edge
shows them over the page for a moment, and the safe area loses the bars'
insets (the display cutout's stay), so content laid out against it moves into
the space they left. It resolves by the same rule, with Android's modal routes
covering pages (`host/android/README.md`, Edge to edge), and changes with
committed state; `status-bar-animation` does not apply there: the system
animates its own bars. Other hosts ignore it.
`wakelock="screen"` on any node, bound to state (`wakelock=(playing ? "screen" :
"none")`), keeps the display awake while that node is displayed; a hidden or
unmounted one holds nothing (LLP 1125 D11; `state.wakelock` is `{held, by}`).
`orientationlock` on any node (`orientationlock=(tilt ? "current" : "none")`)
keeps the window to a Screen Orientation API lock type, or `current`, while
that node is displayed; a sheet's lock wins over what it covers and a
descendant's over its ancestor's. A type only narrows the manifest's
`orientation`, and a lock inside a `dialog` is refused (LLP 1133.029;
`contract vocab orientationlock`).
`currentcolor` takes the node's `color` on borders, `background-color`,
`-exact-tint-color`, text stroke and SVG paint. `unset` clears any row, and `inherit`
an inherited one (`color`, fonts, `fill`…); `inherit` on a row CSS does not
inherit is refused. `order` places flex and grid items. An image's accessible
name is `alt` or `aria-label`; `enterkeyhint` labels a soft keyboard's enter
key on the web, iOS and Android; in a `textarea` that key is Enter, a new line
unless a `key` handler prevents it (on Android only where the hint names an action:
a textarea with no hint, or `enter`, breaks the line itself). A bare number on a length row is pixels (except
`line-height`, where it is CSS's multiple of the font size), and the row takes
CSS's spellings too (`font-size="14px"`, `letter-spacing="-0.5px"`,
`padding="1.5rem"`; `stroke-width="2px"` but no `rem` there), bound or literal
(``font-size=`${size}px` ``). A separator's thickness is `-exact-hairline`, one
device pixel on whatever display the app is on (a third of a point on a phone,
half on a Retina Mac, a pixel on a 1x screen), where `0.5` is one only at 2x:
`border-bottom="-exact-hairline solid -exact-separator"`, or `hr`, which is that
line with no margin of its own (space around it is the `gap` or `margin` you
write). Between the rows of a `list` write neither: its children are rows, and
the separator there is the platform's (LLP 1115.000 D16). A row whose value is a number and no text
(`opacity`, `flex-grow`, `font-weight`, `column-count`,
`column-rule-width`) takes a number where it is computed: a string-typed
expression there is refused, since the native hosts read no text on it (the
literal keywords and `px` of `column-count` and `column-rule-width` compile to
their numbers). `z-index` takes an integer or `"auto"`, its initial value, including in
a conditional (`z-index=(lifted ? 2 : "auto")`). An applicable integer,
including zero, makes a stacking context; `auto` does not. Nonzero values
stay within their parent's paint turn on every host (LLP 1083.000 §3.1).
`max-width` and `max-height` take `none`, CSS's initial maximum, or `auto`, and the web writes
`none` for either. A number field's (`input type="number"`, written so)
`min`, `max` and `step` take numbers, as a range's do; its `value` is its text.
On the Mac, Up/Down and accessibility increment/decrement step the number even with
`appearance="textfield"`. Apple number fields filter typing and paste while keeping signs,
fractions, exponents and input-method composition; `inputmode` alone does not filter text.

`autocomplete` on an `input` or `textarea` is HTML's attribute, written as
HTML writes it (`autocomplete="username"`, `"section-login current-password"`,
`"shipping postal-code"`). The web sets it as written. iOS and macOS read its
last field name (a trailing `webauthn` aside) as the field's AutoFill content
type, over the one `type` implies (iOS: `password`, `email`): `username`,
`current-password`, `new-password`, `one-time-code`, `email`, `tel`, `url`,
`name`, `given-name`, `additional-name`, `family-name`, `honorific-prefix`,
`honorific-suffix`, `nickname`, `organization`, `organization-title`,
`street-address`, `address-line1`, `address-line2`, `address-level1`…`3`,
`postal-code`, `country-name`, `cc-name`, `cc-given-name`,
`cc-additional-name`, `cc-family-name`, `cc-number`, `cc-exp`, `cc-exp-month`,
`cc-exp-year`, `cc-csc`, `cc-type`, `bday`, `bday-day`, `bday-month` and
`bday-year`.
macOS 26 offers verification codes from Messages and Mail in every text field;
`app.json`'s `host.macos.oneTimeCodeAutofill: "marked"` keeps them to fields
marked `one-time-code`.
`off` clears the content type (the web's `autocomplete="off"`); `on` or a
list HTML's grammar refuses (the web's default) leaves `type`'s, and so does
a name the platform has no type for (`country`, `impp`, `sex`), which only the
web can act on.

An `input type="password"`'s value is never agent output, on any host: `tree`,
`layout <field>` and the `type` reply show `value="•••"` for any value that is
not empty, whatever its length (an `expect text` on the field reads `•••`), and
`tree --ax` marks the field `protected` where the host has one (the web's
accessibility tree still shows one bullet a character, as Chrome exposes it).
`state` shows `"•••"` for any string equal to a live password field's text, in
its slots, derives and answers alike; an `expect state` and a drive's `clock until
state` compare against the text and print it masked. What the app stores (its data
module, its store) is the app's own; an `app.test.contract` `type … append` into a
filled password field is refused.

`aria-busy=(not ready)` marks a region as loading for assistive technology.
The launch ledger used by `modules/observe` waits for busy elements before
recording time to interactive. On Apple and the JavaScript web target, a
mounted busy element holds `clock settle`, including inside a closed `dialog`.
An indeterminate `progress` implies `aria-busy`. The settle reply names each
busy element by test ID or view ID. Use `layout <id>` to find its source line in
a development build. Use `clock +1` to run a task due a commit later while
settle is blocked.

`border`, `border-top/right/bottom/left` take CSS width/style/color in any order,
resetting omitted components to medium/none/currentcolor. Widths are px/pt,
unitless zero, or thin/medium/thick (1/3/5 px); styles are none/hidden/solid/inset/dotted/dashed.
The web and native GUI hosts paint dotted and dashed while keeping their widths.
At 1×, AppKit and Linux dot and curved-join pixels have a bounded raster
difference from Chrome. UIKit uses AppKit's painter; its simulator parity is
still unmeasured (LLP 1001, border semantics).
Other CSS border styles are diagnosed as native painter gaps. `text-decoration`
takes none, underline, line-through, or both lines; solid/currentcolor/auto
components retain the supported defaults. Color/style/thickness extensions are
refused by name. Linux paints solid lines with UA metrics and diagnoses its
missing underline skip-ink behavior in the driver log.

A flex shorthand's grow/shrink factors must be adjacent. `width`, `height`,
their minimums and maximums take `min-content`, `max-content` and
`fit-content`, and `flex-basis` (and `flex`'s basis: `flex="max-content"` is
`1 1 max-content`) takes those and `content`, laid out as Chrome lays them out
on every host (LLP 1001.001). `width="fit-content"` is as wide as the content,
up to the space the box has and no narrower than its longest word: a badge or a
card that hugs its text without a flex row around it. `max-content` is the
content on one line, `min-content` as narrow as its longest word, and
`max-width="max-content"` caps a stretched box at its content. A keyword height
is the content's height, but a flex or grid item with one no longer stretches.
`fit-content(<length>)` (Chrome takes it only in a grid track), `stretch`, the
`-webkit-` aliases and `calc-size()` are refused by name.

Font lists distinguish quoted local names from unquoted CSS generics. The eight
mapped generics are system-ui, ui-sans-serif, sans-serif, ui-serif, serif,
ui-monospace, monospace and ui-rounded. Other CSS generics and CSS-wide
font-family values are refused with the native mapping/stack representation
reason, rather than being silently treated as local family names.
CSS escapes and literal double quote characters at the start or end of a family
name are not implemented. Apostrophes and interior double quotes are supported
inside a name quoted with the other quote character.
