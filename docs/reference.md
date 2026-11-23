# Exact developer reference

Operational detail for working in this repository: toolchains, the Contract
CLI, the native and web loops, TypeScript and Rust data modules, delivery,
and the stress examples. The overview and quick start are in the [README](../README.md).

Tooling runs on **Bun 1.4.2**, the version `package.json` pins.
Run `bun install --frozen-lockfile` to install the dependencies in `bun.lock`.
The web artifacts build with a pinned nightly Rust and its std sources (LLP 1047);
everything else uses `rust-toolchain.toml`'s stable. `host/web/build.mjs` prints the
install commands when they're missing:
`rustup toolchain install nightly-2026-08-21 --profile minimal --component rust-src --component llvm-tools`
(its `rust-lld` loads `libLLVM.dylib` from `rustlib/<host>/lib/`, which only `llvm-tools` installs there),
then `cargo +nightly-2026-08-21 fetch` on that toolchain's `library/Cargo.toml`.
Node and npm are not required. Rolldown remains the app bundler; the existing
build, serve, watch, and reload scripts run under Bun. Run tooling unit tests with
`bun test ./scripts/`; the explicit path keeps Bun from searching generated
fixture checkouts. This is the source
tooling installation; a standalone CLI distribution is not packaged yet.

An app with TypeScript sources (`app.ts`) is baked with the `hermesc` paired
with Exact's linked lean Hermes. Install Ibex's pinned, attested bundle once:

```sh
cargo run --manifest-path vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml -- --target <triple>
```

The verified install lives under `~/.cargo/hermes-lean-sys/`. Exact sets
`HERMES_LEAN_SYS_OFFLINE=1` in `.cargo/config.toml`, so an ordinary build
neither downloads a bundle nor compiles Hermes; a missing install fails with
the one-time command, after `exact setup` where an iOS or tvOS build lacks a
bundle setup installs. `HERMES_LEAN_SYS_DIR` remains a development override.
The v6 set supports macOS, Linux, Windows, Android arm64, iOS devices and the
universal iOS Simulator, plus tvOS devices and the arm64 tvOS Simulator. Exact
selects Ibex's English `intl` data tier on Linux and Android, not
`intl-all-locales`; Windows uses OS ICU and Apple keeps Hermes's OS-backed Intl.
Android's receipt enables Intl and binds the English ICU tier. A Rust-only app
needs no engine at run time.
`exact setup` installs the host bundle and, on macOS, the iOS and tvOS Simulator
bundles, and the iOS device bundle where Xcode's iphoneos SDK is installed. The
bundles live under `$HOME/.cargo`, and setup also fetches the locked crates: a
copied or archived checkout, or a fresh `HOME`, needs `exact setup` again.
`exact setup --check` runs Ibex's own offline resolver validation over that
same set. On a CI worker, `bun <exact2>/scripts/exact.mjs setup --check
--platform ios --device` checks the archive's prerequisites; `--platform
android` checks the APK/AAB prerequisites, including JDK 17+ and the NDK's
actual compiler paths. A scoped check fails when a prerequisite is missing
and does not check unrelated web tools. Run from an app's directory, a check
requires a Hermes bundle only when the app links Hermes, as that platform's build
decides: iOS by `app.ts`, Android, macOS and Linux by the cargo graph of the app's
crate. Elsewhere it reports the bundles as needed for TypeScript apps.
Native `--platform` scopes are check-only; install commands are in the platform's CI recipe. `setup --platform web` installs
[web prerequisites](#web-ci-preparation). The general check still reports
optional tools separately (LLP 1086 D6).

For Android, an `Android JDK JIT` row names an AArch64 JDK affected by
[JDK-8335662](https://bugs.openjdk.org/browse/JDK-8335662), which can crash Gradle.
This advisory appears in general and Android-scoped checks and does not block
setup or a first Android build.
Set `JAVA_HOME` to JDK 17.0.16 or later in 17, 21.0.8 or later in 21, or 24 or newer.
Setup does not install or replace a JDK. Android builds warn before Gradle
starts and name matching JVM crash logs after a failure. JVM options stay yours;
the warning reports the affected version even with a JIT workaround enabled.

### Windows painter

An app with no `host.windows.painter` uses ink on a DirectX 11 composition
swap chain, except an app declaring `game`, which keeps the CPU painter so
its module presents directly. Explicit `"ink"` or `"cpu"` selects that painter.
CPU keeps the `softbuffer` window. With development trust, `EXACT_PAINTER`
overrides the choice and `EXACT_INK_DEVICE=wgpu` selects Ink's wgpu device
(DirectX 12 with `WGPU_BACKEND=dx12`). A production bake uses the manifest
choice. With either trust, `EXACT_INK_DEVICE=warp` selects software DirectX 11
for Ink. An Ink build requires the Windows SDK's `fxc`. If it is missing,
the build names the SDK and the CPU option.
`agent windows`, `test windows` and `verify windows` explicitly select CPU
for their headless drives unless `EXACT_PAINTER` is set. Those screenshots
do not qualify the window's composition path.
A direct `EXACT_SMOKE=1` development launch uses the app's painter choice.
`bun exact.mjs windows --release` packages a production binary and omits the
headless driver. To measure an optimized build under the driver, set
`EXACT_UPDATE_TRUST=development` when running the same command.
A generated Windows shell links the data module's network transports when its grant
ceiling includes `net.fetch` or `net.websocket`. Development builds keep both so
a plan can add source origins on reload. Image downloads are independent of data grants. A production
Windows shell with `deploy.store.windows` set to `"0"` omits the downloader when
all image, media, poster and frame sources in its plan are local literals or
the plan has none. Computed sources, development shells and signed updates
keep the downloader.
A hand-written shell gets the same selection by writing
`compat.windows_shell(&baked, &manifest)` from its build script and calling
`exact_windows::run_baked::<AppData, ADMIT_AGENT>(name, window(), PLAN, COMPAT, composed, SERVICES)`.
The TypeScript bake writes the same constants. The heavy-list app shows the
[build step](../bench/heavy-list/exact-heavylist/windows/build.rs) and the
[entry point](../bench/heavy-list/exact-heavylist/windows/src/main.rs).
A Windows login item started with `--login hidden` paints its offscreen launch
frame on the CPU and opens the selected painter when its window first draws.
Linux and Android still opt into ink per platform; Android refuses it until
its Vulkan presenter is implemented. Apple keeps the native host by default;
`host.macos.presenter: "painted"` builds the ink carrier for development,
with distribution refused until its accessibility gate passes (LLP 1149 D12).

### Windows TypeScript

The former private `260318099.0.0` build is superseded. Native Windows
TypeScript uses Ibex's v4 debugger-off lean bundle. `exact-js` selects the
`intl` feature and installs `GROUP_INTL`; Ibex binds the Windows 10 2004+ OS
`icu.dll` from System32 through function pointers, with no ICU linker flags or
bundled locale data. The ordinary app path still requires the Windows
qualification in LLP 1027.006
before release claims. Do not
resurrect Exact's deleted private source builder or use its old
`%LOCALAPPDATA%/Exact/hermes` cache for this snapshot. `EXACT_JS_ENGINE=stub` remains the Hermes-free,
refusing build for CI; it cannot bake a working TypeScript app.

Windows application storage uses the current user's LocalAppData known folder,
under `exact/<app-id>/{data,cache,temporary}` (`app:/tmp` uses `temporary`). It does
not require `HOME`. App and scratch
identities and `app:/` path components must be safe Windows leaves; drive, UNC,
backslash traversal, alternate-stream and reserved-device forms are refused.

Snapback4 consumers use release **0.4.33**: the CLI is pinned in `bun.lock`;
Cargo pins the device and its client to the matching release source commit
`3653cae823145170df3e720f5e321bd1b5b4d79d`. The root `package.json` overrides
the undici 0.4.33 pins with 8.12.2, the version 0.4.43 pins, for the eight
advisories on 8.11.2 (among them
[GHSA-qrxm-fqhp-jwvm](https://github.com/nodejs/undici/security/advisories/GHSA-qrxm-fqhp-jwvm));
a pin at 0.4.43 or later drops the override. The Snapback client is not included in
this repository. Where a checkout has it at `snapback4/`, its accompanying
`snapback4/README.md` covers wiring and device rebuilds. It is one client for an
app's Rust and TypeScript on every host: the protocol in Rust without I/O,
the native device, the web's wasm, and the
TypeScript driver an app mounts with `typescript.sources`. The client is its
own Cargo workspace, `snapback4/`, with its own lock. Direct Cargo commands use
`--manifest-path snapback4/Cargo.toml`; apps depend on `snapback4/` by path.
Harmony's crates (`apps/harmony/app/`) link it, so they are a workspace of their
own (`apps/harmony/app/Cargo.toml`), which the root `exclude`s: the root lock
carries no private source, and root Cargo commands need no Snapback access (LLP
1153 D2.4). Building Harmony or Snapback on a machine without it, a local clone
holding the commit seeds Cargo's cache once: `GIT_CONFIG_COUNT=1
GIT_CONFIG_KEY_0=url.file://<clone>.insteadOf
GIT_CONFIG_VALUE_0=https://github.com/expo/snapback.git
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo fetch --locked --manifest-path
apps/harmony/app/Cargo.toml`.
The driver imports wasm-bindgen glue (`snapback4/ts/generated/`, never
committed). The bake of an app that mounts the driver writes it with
`snapback4/web/build.mjs`, which builds only when the inputs recorded in the
glue's header have changed, and needs the `wasm32-unknown-unknown` target and
`wasm-bindgen` 0.2.130 (`cargo install wasm-bindgen-cli --version 0.2.130`).

The canonical [Messages](../apps/messages/README.md) app is the Exact port of Expo's
chat demo, with model conversations through a local OpenRouter service. It belongs
to the root workspace and does not use Snapback. Run `bun apps/messages/service.ts`
and `bun host/web/dev.mjs --app messages --port 8767`.

### Linux Wayland windows

From a Wayland session, run the binary printed by `bun exact.mjs linux` to open a
window. The compositor owns the display and sends the window its input.
The insertion caret follows the blink cycle and idle timeout the desktop publishes
through the settings portal. With the GTK defaults it blinks every 1.2 seconds and
stays visible after 10 idle seconds.
`EXACT_SIZE` supplies the initial logical window size. Setting `EXACT_DRM` or
requesting VNC selects the DRM/KMS console path. Agent runs are headless.

Wayland windows read the desktop’s light or dark preference from the XDG
Settings portal. A change updates the window decorations and the app’s
`prefers-color-scheme`.

### Linux KMS rotation and VNC

On Linux KMS, `EXACT_ROTATE=0|90|180|270` turns the completed image clockwise.
Without it, the connector's `panel orientation` property chooses the angle; an
absent property means 0. At 90 or 270 degrees layout uses the swapped mode size.
The GPU rotates the image before display. Absolute evdev and VNC coordinates use
the inverse transform; relative mouse movement and scrolling keep the user's axes.
Agent screenshots show the rotated framebuffer, at the panel mode's size. Their
replies give that image's point dimensions and clockwise `rotation` in degrees;
comparison maps layout boxes onto those axes. Agent layout and target coordinates
remain logical. In a headless Linux host, `EXACT_SIZE` supplies the unrotated panel
size in points. On a supported Vulkan display, Ink turns the image while copying
it to the swapchain. With a VNC viewer attached, Ink rotates the image before
readback so the viewer and the panel receive the same pixels. Other painters and
displays without direct Vulkan presentation use KMS dumb buffers.

A Linux development build can serve its screen over VNC: set `EXACT_VNC=1` and
`EXACT_VNC_PASSWORD` to 1–8 bytes. It listens on `127.0.0.1:5900`; `EXACT_VNC=host:port`
selects another address. Production bakes ignore both variables.

## Runtime font families and `aria-disabled`

`font-family` accepts a CSS family-list string from state or data on web,
macOS, iOS and Linux. A change resolves the new installed or generic family
and remeasures affected text. Bundled faces are declared at compile time on
every host; a runtime string does not load a font file.

Contract carries `aria-disabled` on controls and elements with a role, including
groups and inline links. It is a boolean accessibility state: focusable elements
remain in the Tab order, and `press` still runs. The app guards an unavailable action.
`aria-disabled=true` also marks its focusable descendants, including inline
links, as unavailable to accessibility; an explicit false on a descendant
overrides the ancestor. HTML
`disabled` retains its separate focus and input behavior; see
[focus order](contract-grammar.md#focus-order-tabindex).
On Apple hosts, a projected tab or pressable node reports the same accessibility
state through its native segment, UIKit bar item or title, or AppKit menu item.
UIKit menu rows and alert actions remain selectable but read as enabled: their
public disabled states also stop selection.

## Grouped section shadows

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

## Serve RealWorld

RealWorld's public pages can be served by the native renderer:

```sh
EXACT_WEB_DIST=target/realworld-dist bun host/web/build.mjs realworld-web --wasm
cargo run --release -p realworld-web --bin realworld-render -- --serve target/realworld-dist --port 8080 --name Conduit --generations target/realworld-generations
```

The home feed, articles and profiles arrive as anonymous HTML. Their public
responses are cached for 60 seconds by default (`--lifetime` changes this).
Links work without the runtime, which loads when the page is idle; a press
made before it is ready is replayed once it is. Login and editing routes
start the client normally. The renderer compresses what it sends (brotli, else
gzip, by `Accept-Encoding`), and SIGTERM drains it. Reading-page transfer and
the later runtime download are separate costs. Keep the `--generations` directory
across deploys, outside the replaced dist. It retains the newest four builds of
`app.wasm`: an open page's `?v=` URL gets its exact build, or 404 once it expires.
Malformed or repeated versions get 400. A browser holding a retained build can
download another as a delta (LLP 1047.000 §9): after a one-line app change, 9.5 KB
instead of 238.6. Versioned builds, stage files whose names match their content,
and the files a JavaScript build names by content (`routes.json` `files`, LLP
1153.004) are cached as immutable; other files revalidate.

## Inspect and format Contract

```sh
cargo run -q -p contract -- build apps/messages/app.contract --json
cargo run -q -p contract -- build apps/messages/app.contract -o /tmp/messages.plan --map
cargo run -q -p contract -- symbols apps/messages/app.contract
cargo run -q -p contract -- symbols apps/messages/app.contract --name select
cargo run -q -p contract -- fmt --stdout apps/messages/app.contract
cargo run -q -p contract -- fmt --check apps/messages/app.contract
```

`contract migrate <file.contract>` rewrites retired spellings in place. It moves
`-exact-press-background-color` and `-exact-press-color` into the owner's
`:active` block, merging with an existing block and retaining expressions and
comments. It refuses conflicting active rows, directly nested inline text and
explicit `appearance="auto"` controls other than buttons before writing.
Classes and component uses are checked at compile time. Compile the result with
`contract build`; use `contract fmt` to format it. Arbitrary strings and
transition targets are unchanged.

`symbols` prints JSON with `definitions` and `references`. Each reference's
`to` is an index into `definitions`; locations include the original file,
1-based line and UTF-8 byte column, and an exclusive `end_col`. Component interfaces,
local bindings, parameters, typed shape fields, font families and literal IDs are navigable.
Shared shape/function/style/font files can be queried directly. The query uses the
compiler's import and type rules and writes no files. Repeated literal IDs have
an edge to each matching declaration; dynamic IDs have no static target.
An action's definition carries `writes`: the states and mutations its body
assigns or sends, inferred (an action declares none), in slot order: router
state, states, then mutations, each as declared.

`symbols --editor` adds what an editor integration reads
(`editors/content-mapper`, LLP 1035.005.003); nothing else asks for it, and without the flag none
of it is computed. A definition the type checker types carries `type`, its type in TypeScript as
`contract types` writes it in `app.d.ts`'s summary: a state, derive, prop,
inject, resource, mutation (`T | null`), local or parameter by its value, a shape
by its fields, a field by its value, an action or `fn` as a function of its
parameters. An enum is the union of its strings, `"a" | "b"`. A shape inside a type is written by its name,
and `shapes` lists each shape used so, its `name` and its fields as `type`,
`{ id: string }`. A shape TypeScript cannot name, one with a hyphen (`A-B`) or
named as a reserved word (`class`), is written in a type by an alias no shape
has (`A_B`, `class_`), which its row in `shapes` gives as `as`. When loading stopped and each file was read on its own, a
shape also has the `file` it was read in: two files may declare different
shapes of one name. A bare `action` prop or inject has no `type`: its arity is
what it is called with.
A route, component, source, style, task or ID has no `type`. When the type
check refused, only what is declared keeps one: shapes, fields, and `fn`s and
their parameters.
`elements` lists each view element as written, for an editor's hover and
completion: its `tag` (the compiler's name for it; an `h2` is a `text`), its
place and `component`, and `attrs`, each attribute written on it with the place
of its name and, for a string literal value, `string` with the value and its
place, quotes included. A button's one argument, which becomes its `text` child, is no element
of its own. A `--name` query has no `elements`.

Add an element filter to `symbols --editor` to return just the matching
`elements` across the source graph. `--component` matches the declared name,
and `--tag` accepts the authored or compiler tag. Repeat `--has <attr>` and
`--where 'attr=expression'` to require written attributes and exact source
values. Use `--has` for a bare attribute such as `multiple`. Repeat
`--match <text>` to look up several
case-sensitive substrings in one pass. A match can occur in the tag, a written
attribute name or value, or a positional text expression such as an option
label or a button's direct text child. Use `--or` to start an independent filter
group. An element that satisfies any group appears once, in source order
within its file.

```sh
contract symbols app.contract --editor --component Editor --tag button --match 'Save draft' \
  --or --component Preview --tag button --match Cancel
```

Filtered elements include `binding` on attributes with other written values,
`text` for text expressions, and `context` for enclosing `if` and `for` headers,
outermost first. An `if` reports which branch contains the element. Each entry
includes its authored `source` and range in the element's file. An `end_line`
appears when the range crosses a line. Element filters can be used with
`--stdin` and are separate from `--name` queries.

When the program is refused, `symbols` still prints what it found, adds an
`errors` array of the refusals in `build --json`'s form, writes them to stderr
and exits 1. A declaration or member the syntax refused is absent, and so is
every reference in its scope spelled as its name; a reference to a name
declared twice in one namespace (a component's members, or the program's
shapes and functions) has no edge. When the type check refused, a
reference that needs an inferred type (a field read through a value, an
action value called) has no edge, nor does one to a shape or function the
check refused. When loading stops (a `use` or `members` line that names no
file, a cycle, a missing name), every file it can still reach and parse is
read on its own, with no edge from one file to another. A file whose
declarations cannot be told apart (a line that does not lex, or a first
component whose view is refused) has no symbols; when that file is the root,
`definitions` and `references` are empty. A refused program's actions carry
no `writes`.

`--name <exact-name>` returns all matching definitions across scopes and their
references, with `to` indices into that response's smaller `definitions` array.
Names are exact and case-sensitive, not patterns; no match returns empty arrays.
The complete source graph is still checked. `symbols --help` shows the syntax.

`build --json` writes one diagnostics array to stdout: every independent
diagnostic the compiler can collect, at most 20, each with a stable `id`, a
`severity`, `message`, original `file`, `line`, `col`, `end_col`, and `related`
array. An `"error"` refuses the build (exit 1); a `"warning"` builds (exit 0) and
is printed on a success too, so `[]` means a success with nothing to say. Action-interface
mismatches link the invocation, declaration and caller binding, each with its
own source file, including forwarded props and injected actions. Locations use the same byte columns as symbols; zero
means no source range, and a null file means no file is associated. No prose is
mixed into JSON, including argument and output-write failures, and nothing is
written to stderr: advice the human form prints there (a resource fed by a live
clock, a game's stale surface arguments) is among the warnings. `bun exact.mjs
contract build app.contract --json` appends `app.ts`'s type-check errors to the
same array, one per TypeScript diagnostic (`id` `typescript-TS<n>`). Exit codes are
0 for success, 1 for compilation/I/O failure, and 2 for invalid arguments.
`-o <file.plan>` writes the same plan bytes in either output mode.

`--stdin` gives `build` and `symbols` the named file's text on stdin, an
editor's unsaved buffer, in place of what is saved there:

```sh
contract build app.contract --json --stdin < buffer
contract symbols app.contract --stdin < buffer
```

The text is read as if it were saved at that path: its `use` and `members`
lines resolve beside it, every other file is read from disk, and the output
on both streams and the exit code are those the same text saved there gives,
diagnostics' `file` and ranges and the text form's quoted lines included.
Only the named file's text is replaced. Unsaved text writes no file but
`-o`'s: `build` leaves an existing `<file>.d.ts` as it is (so it never says
it regenerated it), and `bun exact.mjs contract build app.contract --stdin`
neither refreshes the types `app.ts` imports nor type-checks `app.ts`.
`--stdin` takes no `--entry`.

`build --stdin-as <used.contract>` reads stdin as the text of a file the named
one uses, for the unsaved buffer of a file that is built with its app:

```sh
contract build app.contract --json --errors-only --stdin-as lib/row.contract < buffer
```

It is `--stdin` in every other way, and the two are not given together.

`build --map -o <file.plan>` also writes `<file.plan>.map.json`, keyed by
SHA-256 of the plan bytes. Each plan node has its original file and range,
component call-site chain, and one binding per style row: its base value's
origin (`own`, `class:<Name>`, or `tag`) and, where an author wrote it, `at`
and `attr`; and `states`, one `{state, origin, at}` per state part that
writes the row: `:active`'s (on the press paint row it lowers to), `:hover`'s
and `:focus-visible`'s (LLP 1121 D8, LLP 1122); a conditional class's
(`class=(c ? A : B)`) says no `at`, as its base row says none; a row a
`:disabled` or `:checked` part folds over its base says the base's attribute
as its base and each folded part in `states`. A row only a state part
writes, a press row among them, has no base fields. Each source is listed
with its digest, and a Contract source with its `home`, as its components'
identities name it (a param's full name is `<home>#<component>.<param>`). Slots, derives, resources and actions retain their declarations,
including state and actions lifted from child components. Maps are separate
files; ordinary compilation collects neither instantiation provenance nor lowering sites. The compiler API's
`compile_path_mapped` and `compile_path_source_mapped` return the map alongside
the plan; `SourceMap::bake_error` resolves measured layout refusals, and
`SourceMap::json` takes the final encoded bytes after baking. A consumer must
verify the map's digest against the plan actually accepted by its session.
The resident Contract, TypeScript and portable Rust producers emit maps after
baking. Temporary source captures retain the original app filenames. The dev
server keeps the matching map at the generation's `app.plan.map.json` URL,
declared under `dev.sourceMap`; it is never an asset or module payload. Static
builds and production publication omit it. `agent.mjs ... "layout <target>"`
reads the map beside `--plan` or from the development `--url`/`EXACT_DEV_PLAN`
envelope. It shows the declaration, component callers and winning authored style
origins only when the same node reply carries the matching plan digest. The
runner computes that digest lazily once per accepted plan; ordinary inspection
does no hashing. Missing, invalid or stale maps leave geometry available with a
source-unavailable explanation. The driver retains four recent map digests for
sessions that keep an older plan after a refused reload. A fresh driver may lack
that older map and refuses the join. “Compatible source map” means the compiled
plan matches: formatting-only edits can change source locations without changing
the plan, so this is not an original-source revision guarantee.

`fmt --stdout` previews source-preserving formatting; `--check` prints a diff
and exits nonzero when formatting differs. Plain `fmt <file>` writes the result
explicitly. `fmt --stdin <file>` formats stdin's text to stdout, an editor's
unsaved buffer: the file names the text and is not read. Formatting never runs
automatically on save.

## Contract evaluation limits

Each evaluation may take 65,536 list steps, build a string of 64 MiB of
UTF-8, or build a value of 16,777,216 nodes, 64 MiB of string bytes and
64 levels. A bound trap keeps its `Trap(Kind { pc: N })` token and adds the
evaluation and bound in words, in every build. For example:

```text
Trap(IterationLimit { pc: 343 }) in derive "visible": past the 65,536 list steps one evaluation may take
```

The evaluation names a derive, action, resource argument or state initializer.
Resource and task gates, family keys and sequence evaluations name their owner too.
A tree evaluation names its containing view and plan site, then the event
argument, region subject, each key, state initializer, surface argument or
list option. The JS target names the site alone. Argument numbers start at 1.
A prop or style binding already names its site and row; its trap adds only
the bound. A child's inlined derive or function names the evaluation that
reads it. The pc is local to that evaluation's code body, not a source location.

An action or derive trap refuses the commit. A prop or style binding trap
unsets its row and journals it once until the binding succeeds again. Other
tree evaluation traps poison the runner. Bake and boot errors carry the same
evaluation and bound text (LLP 1090 D6, LLP 1017.003 D3).

## Run a terminal app

`cargo build --profile host-dev -p harness-terminal -p llp-terminal` builds
`target/host-dev/harness` (inline; `--fullscreen` for the alternate screen)
and `target/host-dev/llp [corpus-or-document]` (full screen). Both embed their
terminal entry as a compiled plan. Rebuild after editing the entry or an
import; launching the binary needs no Contract source or compiler work.
`exact-terminal <entry.contract>` remains the source-driven development
host. All three take the same command line (LLP 1101.003; `--help` lists it):

- The words are the launch location: a `/…` word is the location itself,
  any other word one path segment, and `--name value` the query parameter
  `name=value` (`todo /active`, `harness --prompt "why?"`).
- With `--print`, `--json`, or a pipe on stdin or stdout, the entry runs
  once: it lays out at `--columns` (else the terminal's width, `$COLUMNS`,
  80), runs until no request is pending, no `after` timer is armed and no
  node is `aria-busy`, writes its rows (`--json`: its settled `state`, with
  `navigation` and `exit`) and exits. `exactViewport().update` is `"none"`.
  The exit status is 1 when the active `head`'s `status` is 404, 410 or
  503 or the runner refused something, 2 for a usage error, 124 after
  `--timeout` (120 s by default; `0` for none), 130 or 143 on SIGINT or
  SIGTERM, 141 when stdout closes; at exit every `beforeunload` hears it.
  `--color=auto` writes SGR to a terminal and plain text elsewhere;
  `always` and `never` force it.
- After `--drive`, the headless operations, at `--size COLSxROWS`.
- Otherwise the app runs in this terminal, at the location.

Only a terminal entry's plan runs here, and a GUI host refuses one.
Interactive terminal I/O runs on Unix only; Windows supports headless operations.

An interactive launch draws a fresh 32-bit seed for `exactTime()`. Its `locale`
follows `LC_ALL`, then `LC_MESSAGES`, then `LANG`; `formatLocale` follows
`LC_ALL`, then `LC_TIME`, then `LANG`. Empty variables are skipped, and POSIX
locale names become BCP 47 tags (`en_US.UTF-8` becomes `en-US`). With no format
locale set, `formatLocale` follows `locale`. Headless operations keep the
agent's deterministic launch facts, including `EXACT_AGENT_SEED` (1 when unset).

The harness's inline transcript is transferred to scrollback in bounded
batches, retiring each only after the terminal prints it. A large burst
still produces every entry immediately; fullscreen retains the full
transcript for scrolling.

Vertical `list virtualized=true` uses the shared collection window on the
terminal. Scroll and resize report the viewport and measured rows; Tab and
arrow navigation reach logical rows beyond the mounted window. A focused
row stays alive when scrolled away. Closed dialogs release their virtual
row views while retaining their list index and position. The LLP reader's
document tree uses this path, initializing its index on first open.

## Run an app from a terminal on macOS

```sh
bun scripts/exact.mjs list                       # the apps here, and their commands
bun scripts/exact.mjs run markdown README.md     # build and launch, log on this terminal
bun scripts/exact.mjs install markdown           # ~/Applications/Markdown.app + `mdview`
mdview README.md                                  # from anywhere, reusing a running copy
```

`bun link` (or a symlink into a directory on `PATH`) makes it plain `exact`.
Both verbs launch the executable inside `<Name>.app`, so the process has the
app's bundle identity: its name in the menu bar, its Dock tile, and the
document types Finder's Open With reads. `install` also writes a shim named by
the manifest's `app.command` into the first of `~/.local/bin`, `/usr/local/bin`,
`~/bin` that is already on `PATH` (`EXACT_BIN_DIR` overrides), and prints the
line to add when none is. It replaces only this app's own: a bundle with
another bundle identifier, a command it did not write, or a link at either
path is refused, and nothing is changed. `exact uninstall` leaves them too.

A standalone macOS app treats `kill -TERM <pid>` as an orderly quit: its
`beforeunload` handlers can cancel, pending storage gets the same five-second
hold, and each session's native module is destroyed before exit. Repeated SIGTERM
requests share that hold's original deadline. SIGKILL cannot run this cleanup.

`app.json`'s `preload` maps names to requests of shape `{ url: string,
method?: string, headers?: { [name]: string }, body?: string,
timeoutMs?: integer, when?: "start" | "foreground" }` (LLP 1162.000 D2).
`method` defaults to `GET`, `when` to `"start"`. URL, header values and body may
reference a kept secret as `{secret:NAME}`, except in a URL’s scheme or authority;
write `{{` for a literal brace. `{env:NAME}` references a setting declared in the
manifest's `env`, including in a URL's origin. It must have a build value (a
module's `??` default counts). The bake writes a fixed setting's value into the
request. A setting marked `launch: true` uses the same process-wide nonempty
launch value as `import.meta.env.NAME`, falling back to the build's value
(LLP 1153.002 D2). Android keeps the launch part and reads its build fallback;
the web folds it at bake.
A backend selection (`EXACT_BACKEND_ORIGIN`, `--backend` or a saved selection)
rewrites the module's `origin` literal, not its setting. The build refuses a
preload that names a setting at the origin the selection replaces. Filled URLs remain
subject to the binary's grants; invalid filled headers send nothing. The journal
names the filled origin, without the path, query, headers or body.
The bake checks at most eight entries against fetch and `secret.keep` grants.
Hosts that support preloads send them at process start; the app's own reads take
a preload's reply, and a read made for a tool is sent. A Rust data source may
implement `launch() -> Option<exact_runner::request::Launch>` and register its
`launch` function with `Launch::new(env!("CARGO_PKG_NAME"), launch)`. The function
takes `&mut dyn LaunchContext` and returns nothing. Hand each named `LaunchRequest`
to `context.preload(request)`: it returns `Result<(), String>` with a refusal reason,
else starts it immediately (`Foreground` waits for Opened). `when` defaults to
`Start`; the request's `timeout_ms` is its exchange deadline.
The context offers `secret`, durable `set_secret` and `forget_secret`, and blocking
`fetch` under the binary's grants, without the cache. `secret` returns the value
from when the function began, or the value the function last wrote; it does not
observe intervening app writes. A secret write succeeds only if nobody else has
written that secret since the function began or since its own last successful
write. A changed version returns a refusal reason. If `set_secret` or
`forget_secret` fails, the function cannot write that secret again during this
launch, and each request it hands over afterwards is refused: what is stored is
then unknown.
The context exposes no app storage.
On hosts that start preloads, it runs before the app's module, after the manifest's
start requests reach the client. Its requests share the eight-entry bound.
A handoff is refused if the app has since written any secret the function read
or wrote, even if the function read its snapshot again. Each accepted request
depends on all those secrets: a later write by either writer cancels it and drops
its reply.
While the function runs, a read waits only if it names a preload with no definition
yet. It proceeds when that name is handed over, the function returns, or five
seconds pass, then uses the ordinary holding rule. Unnamed reads and reads under
an update selection wait only for existing holds. The manifest's definitions may
hold unnamed reads for one second; a missing name waits through the handoff to the
function, or until the manifest's definitions are handed if there is no function.
The function's deadline also aborts its blocking fetch. Requests handed over
before it stand; later handoffs are refused.
A panic cancels the function's requests where the build unwinds (a development
build); a release build aborts on a panic there as anywhere in the crate.

A read may name its preload: `fetch(url, { exactPreload: "feed" })`, or
`Request::get(url).preload("feed")?` in Rust. Names must match `^[a-z][a-z0-9-]*$`.
The held reply must have the same method and URL; headers and body may differ.
With an update selected, including a plan-only update, only full request equality
can take a preload. Only a name with no definition yet waits while the launch can
still define it; when that wait ends, the read falls back to equality. A name
already defined for another method or URL falls back to equality at once.
Names are unique across the manifest and launch function: a second handover with
the same name is refused and leaves the first definition in place.
A refresh drops both the preload it names at an equal method and URL and any
fully equal hold; under an update selection only equality applies.
On the web the option is accepted and ignored. A TypeScript bundle naming it needs
runtime 13 (LLP 1162.000 D1, D2).

For a read the next screen will make, use `fetch(url, { exactPrefetch: true })`
or `Request::get(url).prefetch()` in Rust. On Apple, Linux, Windows and Android hosts,
the option starts a preload only when the app's own read would go to the network
now as an unconditional request and could take its hold; otherwise it is the plain read.
For example, a read waiting for earlier work or launch definitions, one taking a
hold or using an HTTP-cache entry, an `only-if-cached` miss, and a request with a
body over 64 KiB start none.
A forced ask, `reload` or `no-cache` cannot take a hold. A full worker lane alone
does not stop a preload. A skipped preload is never started later.
The caller's fetch and the app's own later equal reads wait for that preload and
take its reply; if it fails, is refused or is dropped, each read is sent itself. The preload uses
the asking source's grants at every redirect and is speculative, so Low Data Mode
may refuse it while the caller's read still runs. Forgetting or aborting the caller
leaves the preload running.

A reply is held for ten seconds after completion, within the cache's byte limits.
A refresh of the same request or a send to its URL drops it. Eight prefetch preloads
fit beside the launch's eight: a ninth evicts the oldest complete one; with eight
in flight it starts no preload. Prefetches have no name; `exactPreload` keeps its
usual meaning on the caller's read. These native executors hold replies
also when they adopted no launch preloads. The render host, the web and an executor
without an HTTP cache make an ordinary fetch. A tool's, wake's or device's plain read
starts no preload and never waits for or takes a hold. A send, a mutation, a stream,
a file-bodied or an independent request, or native work, refuses the option.
In TypeScript the value must be a boolean.
A TypeScript bundle naming it needs runtime 13; the Rust module seam needs ABI 9
or later (LLP 1162.000 D5).

A one-window Mac app ends when its window closes. An app with work the person
expects to go on without the window (a server, a sync), as Mail and Messages
have, sets `"keepRunning": true` in `app.json`'s `host.macos.window`. ⌘W,
File ▸ Close Window and the red button then hide the window without asking
`beforeunload`, and the app keeps running with its session and native module
alive. A Dock click or `open -a` shows the window again as it was, and a
document routed to the app shows it too. While the window is hidden,
`exactPage().visibilityState` is `hidden`, as for a minimized window, and it
is `visible` again when the window shows. The close and the reopen are also the
web's back/forward cache: `exactPage().persisted` is `true` from the close (a
`pagehide` with `persisted`) until the reopen, and `exactPage().restores`
counts the reopens (each a `pageshow` with `persisted`). A minimize, ⌘H, a
covering window or a locked screen changes neither. So an app that opens
something new on a reopen, as a Mac app makes a new window there, keys a task
on the count: `task fresh when page.restores > 0 key=page.restores`. The app
puts its own window away the same way with `hideWindow()`, as a menu-bar
utility does once its work is done; bare `focus()` or the status item's press
shows it again, as a restore. Where the app would end without its window (no
`keepRunning` and no status item placed), `hideWindow()` is refused in the
journal and the window stays. ⌘Q quits as before, and the app's own
`close()` still closes the window and ends the app. A `navigate-new` app
(`launch_handler`) already outlives its last window, and the key changes
nothing there. The agent's `state.documents.windows[].visible` says whether
the window is showing.

A Windows app ends when its window closes too, after `beforeunload`, unless
`"keepRunning": true` is in `host.windows.window`. Its close button, Alt+F4 and the taskbar's
Close window then hide the window, and the session keeps running, its timers, streams and
global shortcuts with it; `visibilityState`, `persisted` and `restores` read
as on the Mac. A Windows app is one process, with `keepRunning` or without:
launching it again (Start, its taskbar pin, a shortcut) shows the window it
has, and a launch with a location (a jump-list task) shows it and opens the
location there. A bare `focus()` shows it too, `hideWindow()` hides it as
the close button does (refused in the journal without `keepRunning`), and
the app's own `close()` ends the app. A login item
registered `hidden` (`@exact/login-item`) opens no window at that launch:
the app runs, and shows its window when launched again or when it calls
`focus()`. A `-exact-menu="status"` button is the app's notification-area
icon (LLP 1133.025 D4). It does not change what closing the window does:
without `keepRunning` the app still ends, since Windows' guidelines advise
against Close putting a program in the notification area
([notification area](https://learn.microsoft.com/windows/win32/uxguide/winenv-notification)).
The icon's menu ends with Exit, which asks `beforeunload` and ends the app.

A native module whose work must finish before the app exits (a child process
to stop, a socket to flush) overrides `willQuit(done:)`. On every orderly
quit (⌘Q, the app menu, an Apple Event, SIGTERM, the last window closing),
once no `beforeunload` has kept the app, the host calls it once, on the main
thread (a module first made during the hold, when it is made); start the work off the main thread and call `done` from any thread
when it has finished. The quit waits for every module's `done` and for
pending storage, five seconds at most for all of it together, and the window
stays responsive meanwhile; then `destroy()` runs, whether or not the work
finished. A repeated quit neither asks again nor extends the deadline. Letting
`done` go uncalled counts as calling it. iOS and tvOS have no quit and never
call it. `destroy()` runs synchronously on the main thread with no deadline,
so do not wait in it: force-end there whatever the deadline cut short, or a
child process outlives the app.

```swift
override func willQuit(done: @escaping () -> Void) {
    helper.terminationHandler = { _ in done() }
    guard helper.isRunning else { return done() } // a second `done` is ignored
    helper.terminate()                            // SIGTERM; it exits in its own time
}
override func destroy() {
    // After the deadline, finished or not: end a helper that ignored SIGTERM.
    if helper.isRunning { kill(helper.processIdentifier, SIGKILL) }
}
```

An app says what it opens with `file_handlers` in `app.json` — the W3C Web App
Manifest's own key — and the macOS bake derives `CFBundleDocumentTypes` from
it. A Linux binary carries the same declarations into the desktop portal's
Open and Save filters; a directory pick remains a directory pick rather than a
file-type filter. Each MIME type it accepts must be one the Apple hosts map to
a system type (`DOCUMENT_UTIS` in `scripts/app.mjs`; `application/octet-stream`
is `public.data`), which the app views (`Viewer`, rank `Alternate`), or the
app's own format: a vendor or unregistered type (`application/vnd.studio.board+json`,
`application/x-studio-board+json`) with its extensions (`[".board"]`), which the
bake exports as `<app id>.<subtype>` (`UTExportedTypeDeclarations`, conforming to
JSON for `+json`, XML for `+xml`, else data) with the app as its `Editor` and
`Owner` (studio diary R13). Every build refuses any other type when it reads the
manifest, the web's included. An extension another app already owns on a Mac
(Freeform has `.board`) can still resolve to that app's type there; the open
panels and drops take a declared extension whichever type the Mac gives it.

For URL schemes, `protocol_handlers` in `app.json` uses the Web App Manifest's
`{ "protocol": "web+notes", "url": "/open?u=%s" }` entries. A native Apple
bundle registers each declared scheme, and an Android APK each custom one (a
browsable `VIEW` intent filter on its `singleTask` activity, so a link from
another app reaches the running app). `%s` is the complete incoming URL
encoded as a URI component, so the scheme, query, and fragment survive;
`searchParam(top(nav), "u")` recovers it in the `/open` route. A cold link is
the launch location; warm links arrive one at a time at the navigation root's
`navigate` handler. The app must declare that root and an accepting route.
The web manifest carries the same entries; browser registration is subject to
the browser's supported schemes and installation behavior. Linux's CLI URL
argument retains the ordinary location conversion because it does not
register OS protocol handlers.

For packaged-only Apple registration, add `"distribution_only": true` to an
entry, for example `{"protocol":"notes","url":"/open?u=%s","distribution_only":true}`.
Archives and `--distribution` builds include it; local builds, including
`--bundle` and `--release` alone, omit it from both the registered schemes and
native handler templates. On Android, a production build (`--bundle`, or
`EXACT_UPDATE_TRUST=production`) includes it and a development build omits it.
The default is `false`. This option does not filter
the web manifest or explicit `host.<platform>.urlSchemes`.

`http` and `https` are the web's own schemes. An app declares both or
neither: each once, with the same `url` and the same `distribution_only`.
`readManifest` refuses any other form, so every build does, and
`host.ios.urlSchemes` and `host.macos.urlSchemes` refuse both. The pair makes
the app a web browser:

| host | what the pair does |
| --- | --- |
| macOS | a second URL type, `Web site URL`, role `Viewer`, and a `Web page` document type (`public.html`, `public.xhtml`; `Viewer`, rank `Alternate`), without which macOS lists no browser. The person chooses the app in System Settings › Desktop & Dock › Default web browser; the app never asks to become the default. A link arrives at the template. An HTML file that Finder opens in the app arrives there too, as its `file:` URL (`/open?u=file%3A%2F%2F%2F…`), unless the app's own `file_handlers` accept it. The web-page type is not offered by the app's pickers and does not widen what `drop` accepts. |
| iOS | With `host.ios.webBrowser: true`, registers the pair and signs Apple's managed `com.apple.developer.web-browser` entitlement. Apple must approve it for the app's ID; the selected provisioning profile must grant it. Without the opt-in, registers neither and says so. App Clips, widget extensions and the sample host never inherit the browser role. |
| tvOS | Registers neither, regardless of the iOS opt-in. |
| web | dropped from `manifest.json`: HTML's protocol handler safelist has neither. |
| Linux, Windows | not registered, as no protocol handler is there. |
| Android | not registered: an APK registers custom schemes only. |

With `"distribution_only": true` on both entries, only an archive or a
`--distribution` build is a browser. A local Mac or opted-in iOS build then registers neither
scheme, writes no `Web page` type, and does not join this Mac's browser list.

**Verified https links** (LLP 1038 D8). Whether the app's web addresses open
the installed app is one declaration in Web App Manifest vocabulary:

```json
"app": { "origin": "https://notes.example" },
"start_url": "/notes/", "scope": "/notes/", "handle_links": "preferred",
"scope_extensions": [{ "type": "origin", "origin": "https://share.example" }]
```

`handle_links: "preferred"` claims `app.origin` narrowed to `scope` (absent:
`start_url`'s directory, `/` for most apps) and each `scope_extensions` origin
whole. `"auto"` (what an absent member means) and `"not-preferred"` claim
nothing natively. These navigation claims exclude Exact's own paths under
`/.exact/` and HTTPS auth callbacks. Android registers a separate exact-path
App Link for a native `auth.callback`, so a browser without Auth Tab can return
to the app. The web popup's `/.exact/auth/callback` stays in the browser.
A link that opens the app arrives as any incoming URL does: its path is the
launch location, or one `navigate` event while the app runs.

| host | what `"preferred"` does |
| --- | --- |
| iOS, iPadOS | signs `applinks:<host>` for each claimed origin beside `host.ios.associatedDomains`, and an App Clip's `appclips:` for each concrete one |
| macOS | the same entries, signed through the Mac's provisioning profile; a build whose profile cannot sign associated domains says links open in the browser |
| Android | one `autoVerify` https filter per claimed host on the app's activity, `pathPrefix` the scope; from Android 15 `uri-relative-filter-group`s leave the excluded paths to the browser (under a scope other than `/`, only with `host.android.minimumOS` `"15"`), and where they cannot, the build of an app that signs in names the excluded paths that open the app |
| web | `manifest.json` copies the three members as written |
| tvOS, watchOS, Windows, Linux | nothing; the build says so in one line |

The web build (either target) writes `app.origin`'s files into the build:
`/.well-known/apple-app-site-association`, whose `applinks` `components`
exclude `/.exact/*` and each callback path before `<scope>*`, and
`/.well-known/assetlinks.json` with a `delegate_permission/common.handle_all_urls`
statement beside any passkeys' `get_login_creds`. A JavaScript build without a
bake writes `webcredentials` for an HTTPS auth callback on `app.origin`, but no
passkey `get_login_creds`. A production JavaScript build that copies a bake keeps
the bake's credential entries. The files need `host.ios.team` and `host.android.certificates`;
the build line names a missing one. For every other origin a claim names, the
build prints the files it must serve, including each extension origin's
`/.well-known/web-app-origin-association`, keyed by the manifest's processed
`id`. The files name the base app, never a variant. Serve each from the path
itself over HTTPS as `application/json`, with no redirect.

`host.ios.associatedDomains` is the raw Apple list, for what has no web name:
`webcredentials:`, `appclips:`, a wildcard `applinks:*.example.com`, an
`applinks:` host outside the claims. An entry that covers a claimed host is
refused, as is `"not-preferred"` beside any raw `applinks:`. Matching is the
platforms': Android matches the decoded path and any port, Apple and the web
the encoded path. Verification itself needs the live domain; without one,
`adb shell pm set-app-links --package <id> 1 <host>` marks a host verified on
an emulator.

A file chosen in the app's own picker (`showOpenFilePicker`,
`showSaveFilePicker`) joins File ▸ Open Recent — a save once it is written —
and becomes the window's document, as a routed one does. A path from the command line, from Finder, from ⌘O, or from a link inside
a document all arrive at the same place: the app's `open-file` node
(LLP 1033 D3). In a Mac browser, an HTML file from the command line or Finder
that `file_handlers` do not accept is a link instead, at the `https` template. One handed over at launch arrives before first pixel, before
app storage is ready and before a TypeScript data module has loaded: a send its
`change` makes waits for both and then runs, pending meanwhile, rather than being
refused (studio diary R14; notes diary). When nothing takes the path, the
host says why — no `open-file` field, or the `change` the app refused, and the
refusal — on stderr and in the journal. `exact uninstall <app>` takes both halves away.

On Windows, pass local files to the executable to open them in the app's
window. A cold launch and a later launch both deliver a `change` to the one
mounted field with `testId="open-file"` and a `change` handler, once per file
in argument order. Relative filenames resolve in the launching process's
working directory. An app that reads filenames in its Rust startup code can
move that logic into the handler to receive cold and warm opens once each.

Widget entries are top-level `app.json` `widgets` rows. For example:

```json
{"kind":"next-trains","families":["systemSmall","systemMedium","accessoryRectangular"],
 "component":"NextTrains","props":"NextTrainsProps","refresh":"nextTrains",
 "preview":{"station":"Mountain View","departures":[]}}
```

Optional `title` and `description` strings supply the widget gallery's labels;
each defaults to the humanised `kind` when omitted.

On Android 12 and later a row with a `system*` family is a home-screen widget
(LLP 1126.003): `systemSmall` is two cells square, `systemMedium` four by two,
`systemLarge`, `systemExtraLarge` and `systemExtraLargePortrait` four square, and it resizes from there. The
same component is laid out at the launcher's size in the system's colours, and
the same `refresh` keeps its timeline. Accessory families have no Android
counterpart. What the launcher's views cannot draw yet (a gradient, a rounded or
styled border, `clip-path`, an `image`, letter spacing) fails
the widget's render, which then shows its last good entry. A tap follows the
entry's `link` (one at the root takes the whole widget), else opens the app, and
the app asks its widgets to refresh whenever it leaves the foreground.

On iOS and macOS, an optional `configuration` lets the person choose in Edit Widget
(LLP 1126; LLP 1133.023). The
entry becomes WidgetKit's
[`AppIntentConfiguration`](https://developer.apple.com/documentation/widgetkit/appintentconfiguration),
its [`WidgetConfigurationIntent`](https://developer.apple.com/documentation/appintents/widgetconfigurationintent)
generated with one parameter per row, and the build extracts the extension's
App Intents metadata, through which the sheet shows them:

```json
"configuration": {"title":"Next trains","description":"Choose how many departures.",
  "parameters":[{"name":"rows","type":"enum","title":"Rows","default":"automatic",
    "values":[{"value":"automatic","title":"Automatic"},{"value":"one","title":"1"},{"value":"three","title":"3"}]}]}
```

On Android the same enum parameters are the widget's settings, one list of choices
each: a placed widget starts with the defaults, and the launcher's long-press
menu opens the settings. A choice there changes that widget's render alone:
Android keeps one timeline for a kind, and its refresh receives each parameter's
default (LLP 1126.003 D5).

On watchOS, the complication picker offers
[`recommendations`](https://developer.apple.com/documentation/widgetkit/appintentrecommendation)
instead of a parameter form. Exact generates one per combination of declared
values, with the default combination first and parameter/value titles as labels.
Repeated value titles include their identifiers. If complete labels still collide,
each label also includes the choices' UTF-8 hex identifiers to distinguish them. A widget
with watch families may declare at most 64 combinations (the product of its
parameter value counts); the bake refuses more and asks for fewer values or
parameters. This Exact bound keeps the watch picker and generated bundle small.
Entity parameters, below, take no part: only enum values combine and count.

`title` and `description` (defaulting to the title) are the intent's; the sheet itself
is headed by the widget's own `title` and `description`, as on iOS 27. Each
parameter's `name` is a lowerCamelCase field name, its `title` the row's label and
its `type: "enum"` a fixed list, and its `values` the menu, in order: `value` is what the render reads, `title` what the
menu shows. `default` is one of the values, the first when omitted; it is the choice
before the person makes one, and the gallery's. A component reads the chosen values
as it reads the family's size, through the reserved inject `snapshot_configuration`:

```
shape Rows
  rows: enum<"automatic", "one", "three">

component NextTrains
  inject
    snapshot_viewport: Viewport
    snapshot_configuration: Rows
```

The shape has one field per parameter, typed `string` or an `enum` of exactly its
values; every injecting component names one shape. A configuration that no component
reads, or an inject without one, fails the bake with an edit.

A parameter of `type: "entity"` offers the app's own content in place of a fixed
list (LLP 1126; LLP 1133.023 D2):

```json
"intents": {"entities": {"book": {"title": "Book"}}},
"widgets": [{"kind": "library", "configuration": {"title": "Your Shelf",
  "parameters": [{"name": "book", "type": "entity", "title": "Book", "entity": "book"}]}}]
```

`entity` names a key of `intents.entities`, whose `title` names the type in the
sheet. The row takes no `values` and no `default`. Its choices are the entries of
that kind that the app last published with `setContentIndex`
(`@exact/search-index`): Edit Widget lists each entry's `title`, with its
`description` beneath, in the app's order, and searches both. No app code runs
when the sheet opens, so the list is as fresh as the app's last publication.
Publish again when the content changes, and publish an empty list when the
person signs out. The entries are in the system's search too, as every
content-index entry is.

In the shape the field is `option<string>`: `none` until the person chooses, then
the entry's `id`. The refresh receives the same field (`string | null` in
TypeScript) and decides what to show for `none` and for an id it does not know.
Exact keeps a saved id when the published list no longer has it, so a list
cleared at sign-out does not turn a chosen widget into an unchosen one.

Edit Widget on iOS and macOS offers the entries. watchOS and Android offer none:
the field is `none` there, and a widget whose parameters are all entities has no
choices on those hosts.

A configured widget's refresh receives the configuration too, as its second
argument: a record with one field per parameter, in the order the injected shape
declares them, each one of that parameter's values or, for an entity parameter,
the chosen id or `none`. In TypeScript it is
`nextTrains: ([date, { rows }]) => …`. On iOS, macOS and watchOS the refresh runs
for each configuration a person has placed, with that placement's values, so it
can fetch different data for each. The timeline it returns serves that
configuration's placements in every family, and each render still reads its own
values. On Android the argument is the defaults, as above, so a render there must
not rely on the refresh having read its choice.

The configuration, labels included, is generated Swift and so a binary
compatibility input; Live Activities have none. Keep kind, parameter and value
identifiers stable across updates; changing a title does not change a choice.
A removed or renamed parameter is ignored, and a missing or removed value uses
that parameter's current default. Other valid choices are retained. Reordering
entries, parameters or values does not reassign a saved choice. The refresh
receives a saved choice by the same rules. Not supported: choices computed when
Edit Widget opens, entities that are not content-index entries, an image for an
entry, and a parameter whose choices depend on another's.

`component` and the `props` shape are declared or imported by `app.contract`;
every component prop must match a field of that shape. `preview` supplies all
fields, with their declared types. `refresh` names a data-source function taking
an epoch-millisecond number and, for a widget with a `configuration`, that
configuration's record, and returning a timeline: records with `date` (epoch
milliseconds) and `props` (the declared shape), in that field order. Widgets that
name one `refresh` share its signature, so they declare the same `configuration`
and props; the bake refuses a difference. The plan's
`snapshot_refresh` header selects its typed source row; the render has no resource
for it. A data bake checks an immediate refresh answer against that signature at epoch
zero with the configuration's defaults; pending answers are checked by the source declarations. TypeScript checks
the generated `Sources` type, including async refreshes. Compile entries once
with `contract::snapshot::entries(app)`, then pass that map to `source_plan`,
`check_refreshes` and receipt generation. `snapshot::bake_app` takes separate
data instances for the app and refresh, so refresh state cannot affect the app
first frame. The TypeScript and replaceable Rust producers use the same check.

On iOS, a TypeScript refresh runs under Hermes from the module bytes in the
app's pinned App Group generation (or the extension's embedded cohort before
the app first publishes it). The receipt admits both the entry plan and bytecode.
It can return immediately or await `fetch`
and `storage.fs` reads from `app:/`. File writes, SQLite, document handles and
native modules are unavailable in stage 1; `secret.keep` values are read from
the shared Keychain only during refresh. The extension never persists store
writes. The app reloads widget timelines after a successful write readable by a refresh, once the store has been quiet for half a second, so a burst of writes asks once.
Refresh has a 900 ms deadline starting when it begins, including boot. Each JS
turn gets the time remaining to that deadline, rather than the app's 100 ms
per-call budget. This leaves 100 ms below LLP 1126 D8's one-second
line for completion and teardown. A store write during a refresh restarts it under
the new store while the deadline allows. Failure or timeout uses the last good timeline
cached in the App Group for this kind, generation and configuration, only while the app's store
has not changed since it was cached (a store write, such as an account switch,
expires it), or else an “Open app” placeholder,
and schedules a retry no sooner than 15 minutes later. A kind keeps the last good
timelines of the eight configurations refreshed most recently. Rendering and gallery
previews do not load Hermes or read the store.

Live Activity entries are top-level `live_activities` rows, each with a `kind`
(LLP 1126 D9). The packaging reads two flags: `push: true` for an activity the
app's own server advances, which signs `aps-environment` into the app (never
the extension, which reads no token), and `frequent: true` for one that needs
the frequent-update budget (`NSSupportsLiveActivitiesFrequentUpdates`). An
entry of either kind, a widget or an activity, gives the iOS build its WidgetKit
extension, the shared `ExactCore` framework, the App Group and the Keychain
group; an activity entry alone adds `NSSupportsLiveActivities` to the app's
`Info.plist` (a widget-only app has no such key). An app with activities alone
ships the extension with an empty widget bundle until its activity
configuration lands (LLP 1126.001 WP10). The entry's root, state shape and
preview are stage 2's bake (WP8).

A device build of an app with entries embeds two profiles, the app's and the
extension's (`<id>.widgets`), of one team, App ID prefix and distribution
method (development, Ad Hoc, enterprise or App Store), each granting the App
Group `group.<id>` and the Keychain group `<prefix>.<id>` (Keychain Sharing;
the team wildcard `<prefix>.*` grants it), the app's also `aps-environment`
(`development` on a development profile, `production` on the others) when an
activity takes pushes. A team wildcard profile grants no App Group, so each is
a profile for its exact App ID; `build.mjs --device` refuses before building
anything, naming for each App ID the profile that was close and the condition
it failed (expired, not this phone, another team or prefix, the other method),
and the capabilities to register and profiles to install. `EXACT_PROFILE` names
the app's, under the same conditions; the extension's is found among the
installed profiles by its id. An archive (`--archive`) matches the extension's
profile to the named one's method, and an Ad Hoc pair must cover the same
devices.

On macOS, `bun host/apple/build.mjs --run` packages the app's widgets in
`Contents/PlugIns`. The Mac keeps `systemSmall`, `systemMedium`, `systemLarge`,
`systemExtraLarge` and `systemExtraLargePortrait`; if no families remain it builds no extension. Live
Activities do not create a Mac extension. The extension uses the same runtime
framework and snapshot driver, in its own sandbox with outbound network access.
The app keeps its existing sandbox policy and loads the reload arm on demand.

Development Mac widgets need an Apple Development identity (`EXACT_IDENTITY`
can name its SHA-1 or full certificate name). Without one, the build prints a
notice and builds the ordinary app ad hoc, with no extension; agent drives,
smokes and metrics remain available. Selecting a Developer ID identity for a
development build takes the same ad hoc path. Distribution widgets use the
Developer ID Application identity selected for release.
Their container is `<TEAMID>.<app id>.widgets`, separate from iOS's group.
Shared secrets additionally need matching macOS profiles for the app and
`<app id>.widgets`, granting that container (or `<TEAMID>.*`) and the restricted
Keychain group `<prefix>.<app id>` (or `<prefix>.*`). `EXACT_PROFILE` can name
the app's profile; an iOS profile is ignored with a notice. Both are embedded
when available. Without them, entries marked `"requiresSecrets": true` show
the “Open app” placeholder;
set this on each refresh that reads account data, leaving public entries false.
Those entries neither read the app's login Keychain nor reuse cached account content.
The first provisioned app launch moves its login-Keychain items to the shared
data-protection Keychain, deleting each old item only after the copy succeeds.
The move is one-way: an unprovisioned build cannot read the moved items.
Unreadable items remain for the next launch; a migration error leaves I/O bound.
Caltrain and Shelf need no shared secrets. See LLP 1126.001 WP16 and §5 Q5.

The render's props slot is named `snapshot_props`; find it by name, since a
`path(...)` link also needs a router slot. Its initial value is the preview; reached bindings cannot use that reserved name.
The bake writes `plans/<kind>.plan` beside `app.plan`. The asset index's `plans`
map indexes these authenticated assets in the app's compatibility cohort.
Kinds, families and gallery labels are binary compatibility inputs; views and preview props
are plan contents. A running host refuses a snapshot plan at decode, and a
snapshot reader requires its exact kind. A widget also requires a refresh
signature; an activity has named placements and no refresh source.

Live Activities are top-level `app.json` `live_activities` rows:

```json
{"kind":"departure","component":"DepartureActivity","state":"DepartureState",
 "preview":{"station":"Mountain View","at":1787915700000},"push":true}
```

The root's props match every field of `state`. Optional `title` and `description`
follow the widget label rule; `push` and `frequent` are Booleans, defaulting to
false. Kinds are unique across both entry lists, ignoring case. Unknown keys fail
the bake with an edit. Capability flags and labels require a new binary; state
shapes, previews and views ride the bundle.

On Android an activity is an ongoing notification that asks to be promoted to a
[Live Update](https://developer.android.com/develop/ui/views/notifications/live-update):
at the top of the shade and on the lock screen, with a chip in the status bar.
Android allows no custom view there, so the placements are not drawn; a row's
`android` object names the state fields the system's template shows, and only a
row with one is shown (Android refuses the sends of the others):

```json
"android":{"channel":"patrol","title":"headline","text":"detail","chip":"short","when":"at","progress":"done"}
```

`title` (required) names a `string` field. `text` and `chip` name `string` or
`option<string>` fields; `chip` is the chip's text (`setShortCriticalText`),
which Android shows whole only under seven characters, and which takes the chip
from the time. `when` names a `number` (or `option<number>`) field of Unix
milliseconds, the notification's time: one at least two minutes ahead when it is
posted, the chip counts down to ("5min"); one already past, the notification
counts up from as a chronometer, in the chip too. `progress` names a `number` (or
`option<number>`) field from 0 to 1, as HTML's `progress` without `max`: Android
16's `ProgressStyle`, a plain progress bar before it. An option that is none
shows nothing. Each send's state must have the mapped fields with those types,
or it is refused; the rest of it stays in the app's private storage, not in the
notification, which other apps with notification access can read. `channel` is a channel
`host.android.notificationChannels` declares, not of importance `min`, or
`default` when absent. The bake refuses a field the shape lacks or of another
type, an undeclared or `min` channel, and a mapping in an app that does not grant
`device.notifications`. A mapping adds `POST_PROMOTED_NOTIFICATIONS` (a normal
permission) to the manifest.

Starting one needs the notification permission, which the app asks for first
(`requestNotificationPermission()`, [Notifications](#notifications)); without it, or on a
channel the person turned off, `startActivity` is refused. The system promotes
the notification where it supports Live Updates and the person allows them for
the app; otherwise it stays an ordinary ongoing notification. Each activity in
`exactActivities()` says which with `promoted: bool`, the system's
`FLAG_PROMOTED_ONGOING` (false on iOS); `openURL("app-settings:")` opens the app's
Settings page, where the person allows them. The activity's id is its notification's
tag, so a relaunched process lists it again. If the person dismisses it (Android
14 and later let them swipe away an ongoing notification the system has not
promoted), it leaves the list when the app is next in front, and a later send to
it is refused. A send is answered once the system shows its notification.
An `updateActivity` with an alert alerts again, with its text as the
accessibility ticker; a stale date is listed as `stale` once past.
`endActivity` with `"immediate"` removes the notification; `"default"` leaves an
ordinary, dismissable one for four hours, and Unix milliseconds until then.

Declare the placements in that one component's view with `view slot="lockScreen"`,
`view slot="compactLeading"`, `view slot="compactTrailing"`, `view slot="minimal"`
and `view slot="expanded"`. Under `expanded`, declare four `view` children with
`slot="leading"`, `slot="trailing"`, `slot="center"` and `slot="bottom"`. Each slot
is required once. Put its layout attributes and content on that slot's view;
`expanded` only groups its regions. See `apps/caltrain/activities.contract`.

The activity plan has one `snapshot_state` slot and an `activity_slots` table
mapping each placement name to a node site under a synthetic container. One
session serves the activity. For each slot, the driver sets the session's viewport
to the region's proposed content size, lays out, takes that slot's subtree and
measures its `fit_content_height`. `snapshot_viewport` thus reads the current
region's size; no app-visible slot selector is injected. The other slots' frames
are not a shared surface layout. Rendering remains read-only, with the same
clock, request, input and handler refusals as widgets. `time` and `progress`
remain platform live views.

A development build with `--widget-gallery` also answers activity addresses:
`bun scripts/agent.mjs ios activity departure --placement expanded --state '<json>' tree "screenshot activity.png" state`.
The same command works on `macos`. Placements are `lockScreen`, `compactLeading`,
`compactTrailing`, `minimal` and `expanded` (all four expanded regions together).
Omit `--state` to use the entry's preview; `--scheme light|dark` selects its scheme.
`clock` advances the app's agent clock and the next read renders at that time.
These are gallery renders; the async simulator test checks the system's archive.

The extension renders every activity through one `ActivityConfiguration` for
the envelope type (`host/apple/widget/Activity.swift`, LLP 1126.001 WP10), a
thin adapter over `ExactSnapshot`'s `ActivityResolver`: the attributes' `kind`
names the plan, `plans/<kind>.plan` in the generation the attributes name,
which is the one published under that name in the installed cohort or the
bundle's embedded plans when their identity (`ExactWidgetGeneration`) makes
that generation; any other generation is refused, the activity shows "Open
<app>" until the app publishes again, and nothing is rendered in its place.
The state is the envelope's when its shape digest is the registry's and the
plan decodes it (then kept as the activity's last good state), else the last
decodable state kept for the activity id (shown dimmed, the driver's outdated
presentation; the system's `isStale` is the system's own), else the preview,
redacted as a placeholder. A render is keyed by the activity, its generation,
the state's bytes and every fact the session reads (the text size, Bold Text,
contrast, transparency, the scheme, the display scale, the locale and time
zone); the eight placements of one push share it. A placement's view answers
the system's size proposals from the kernel (`ActivityRender`): the bake
positions each placement's root absolutely at its region's top left, so in
the region's viewport CSS 2.1 §10.3.7 gives a `width: auto` root the
shrink-to-fit width (an authored `width` or `min-width` is kept, past the
region too; a percentage resolves against the region; `margin: auto` is
nothing; `flex-basis` does not apply, the root being no flex item), and the
Lock Screen and expanded bottom roots, which the system sizes, take the
region through `right: 0`. That one layout is measured and drawn: the region
reports the root's margin box, a paragraph the region wrapped settles to its
widest ink line, a content the region cannot hold is cut at the region, and
each view draws the layout its size was measured from, whatever region's
proposal last laid the session out. The Lock Screen placement sits inside the
system's content margins or, where the system gives none, the standard
layout margin for Live Activities on the Lock Screen, 14 points (Human
Interface Guidelines, Live Activities: "The standard layout margin for Live
Activities on the Lock Screen is 14 points"); its root's `background-color`
is the `activityBackgroundTint`, its `background-image`'s gradient layers
paint full bleed behind the margins over that tint, and both unsaid leave the
system's material. A Live Activity's content is account content as a
widget's is (D8): private on a locked phone but for a node that says
`data-privacy="public"`, which Caltrain's placements do, a train's departure
being nobody's account. A link in the Lock Screen or an expanded region is a
`Link`; the first link of any placement is its URL. The development gallery
shows the eight placements with the Island's geometry of one device (the
iPhone 17 Pro's, `IslandGeometry`), its expanded regions allocated together
as ActivityKit arranges them.
The sends that start, update and end an activity are LLP 1126.001 WP9.

The bake writes `plans/activities.json` and `activities/<kind>.schema.json` plus
`activities/<kind>.d.ts` (exporting `State`, `Attributes` and `ContentState`). The
state uses JSON Schema 2020-12 with required fields and no additional properties.
The shape digest hashes its canonical schema, including each record's field
order. The universal envelope's attributes are `{kind, generation, shape}`;
its content state is `{shape, state}`, where `state` is a JSON value, not an
encoded JSON string. `generation` names the immutable publication digest. The
preview and sends share a conservative 4096-byte JSON budget, including both
shape digests. Numbers reserve the longer of serde_json's and Foundation's
shortest Double spellings, capped at 24 bytes; the bake also covers a preview's
original integer spelling before Double decoding. Both reserve two bytes for
`\b`, `\t`, `\n`, `\f`, `\r` and slashes, and six for the other U+0000–U+001F
characters and U+2028/U+2029. Other characters and JSON punctuation use their
compact UTF-8 size. A refusal reports the budgeted bytes and asks for shorter
state. Shared Rust/Swift fixtures pin encoder spellings, escape counts, admission
of a 60-point route and 150 small integers, and refusal at 4097 bytes.

An activity pins its named generation within the installed compatibility cohort,
even after `current` moves. Publications currently keep every generation. The
cache has one file per activity id. The caller supplies the pinned plan's decoder:
a state it cannot decode falls back to its last decodable state with the driver's
outdated flag. Without a decodable kept state the placeholder has no outdated
flag. A failed write returns an error to the caller. ActivityKit's `isStale` is a separate system
fact. Rendering across a compatibility-changing binary update remains subject
to LLP 1126.001 §5 Q6's pending ruling.

`bun host/apple/build.mjs --watchos --no-install` builds a watch simulator
companion inside the iOS bundle. `--watchos --run --sim <iPhone udid>
--watch-sim <watch udid>` installs it on an existing, booted pair and opens it.
A device build of the iPhone app (`--device`, `--archive`, `--unsigned`) embeds
the watch app in its `Watch/` folder, built for arm64 from watchOS 26: it runs
on Series 9 and later and Ultra 2 and later (LLP 1126.002 D7).
In development, install the watch app directly with
`xcrun devicectl device install app --device <watch-udid> <watch-app-path>`,
using the watch app bundle inside the iPhone app's `Watch/` folder.
Installing the iPhone app may not install its watch companion.
`--extensions` without `watch` leaves it out. Signed, the watch app and its
complications each embed a profile of the iPhone app's team, App ID prefix and
method, for `<id>.watchkitapp` and `<id>.watchkitapp.widgets`, each granting the
App Group `group.<id>.watchkitapp`; the build names any that is missing.
An app whose `app.json` declares a `watch` entry (`{"source", "component"}`:
its own Contract file and root component, LLP 1126.002 D4) runs that entry live
on the watch; the bake refuses what the watch does not draw. An app with none
opens its complications' placeholders. Add a complication through the watch
face's editor.
The watch extension keeps the accessory families (`accessoryCircular`,
`accessoryRectangular`, `accessoryInline`, `accessoryCorner`); iOS drops
`accessoryCorner`. An optional `watchFamilies` on an entry overrides its watchOS
families; `families` still names its iOS families. An app with `app.ts` builds its
live watch entry when that plan reaches only Rust, runner-owned or native
module sources. The watch bake refuses a TypeScript dependency by name until
the watch Hermes gate passes (LLP 1126.001 §5 Q3); a mixed bake declares its
Rust ownership through `build_mixed_with`'s source list. Such an app's complications are still omitted
with a message, including its Rust refreshes. Rust-only apps' complications
refresh against the watch's own App Group and store.

Select phone-owned values explicitly in `watch.sync`:

```json
"watch": {
  "source": "watch.contract", "component": "Watch",
  "sync": { "state": ["liked"], "secrets": ["totp.entries"] }
}
```

State names must be root `persist` states with matching types in both entries.
Watch actions cannot assign them. Secrets require ordinary `secret.keep` grants;
presence policies, including optional ones, are refused. Watch data sources and complication processes cannot
write selected secrets or persisted states. Files, databases and kept resource answers do not transfer.
Changing the selection, or a selected state's type, requires a new binary. The watch rechecks a retained context
when its selection grows or shrinks, even at an already accepted revision. Removing a selection deletes only
previously imported names after accepting a snapshot under the new selection.
Removing the entire selection retires its state replicas the next time the watch app or
a complication runs, and its secrets the next time the watch app runs, even if the app no
longer uses storage or WatchConnectivity. Only the app reaches their Keychain group by then.

Open the watch app once to create its device-local Keychain key and send its
public key to the phone. The phone uses CryptoKit HPKE to seal the secrets before
placing them in WatchConnectivity's retained application context. State stays
plaintext; secret plaintext never enters that context. The watch imports secrets
into its Keychain with `AfterFirstUnlockThisDeviceOnly` accessibility. Reinstalling
the watch app generates a new key; a replacement pairing needs the new watch to
run and announce its key. Old encrypted contexts cannot initialize that watch.

The phone sends a complete snapshot at first pixel, after each durable change
(a session's write, or a launch function's `set_secret` or `forget_secret`) and
when a watch announces its key. Apple schedules delivery, so submission is not
acknowledgement and intermediate values may be skipped. A send WatchConnectivity
refuses is tried again while the app runs, after 1 s and then at doubling waits
up to 32 s, and at each foreground. A snapshot whose application context would
exceed 60,000 bytes is not sent: the session log says so once, the watch keeps
its last copy, and the next change that fits is sent. A snapshot's revision is
spent only when WatchConnectivity takes it.

At launch the watch opens readers on its last applied store without waiting for
WatchConnectivity activation, so stored state and secrets are available offline
at once. With no stored copy, readers see the app's initial state. When activation
completes, the watch imports the retained context and refreshes the live session;
a context for an old key or schema leaves the last applied store unchanged.
An unfinished import keeps new readers closed. The watch app tries a failed
import again while it is active, at the same waits, at each foreground and at
each later delivery, without restarting. A phone edit, deletion or logout
reaches it only after delivery and successful import.
A watch already holding a TOTP seed can keep generating codes until that seed is
replaced at the service; local logout does not revoke that copy remotely.
Imports and replica retirement invalidate cached complication timelines and request
a reload. A complication's refresh waits up to 100 ms for an import that overlaps
its store read. If the import is still unfinished the refresh is refused without
asking its source, and the import's end reloads the timeline. An app that selects nothing links
no WatchConnectivity session or replication crypto path.

See Apple's [application context](https://developer.apple.com/documentation/watchconnectivity/wcsession/updateapplicationcontext(_:))
and [HPKE](https://developer.apple.com/documentation/cryptokit/hpke) documentation.
For `accessoryInline`, emit exactly one `text` or `time` leaf plus at most one
optional image. Only that leaf and image survive the inline archive; a
container's background and layout do not. The bake evaluates the preview at
225 × 26, 168 × 11.5 and 13.5 × 13.5 pt; at runtime the driver checks the actual
size and props.
A violation refuses the bake with an edit, or logs the entry name and renders
“Open <app>” at runtime, including in the gallery (LLP 1126 D4). See
Apple's [inline family](https://developer.apple.com/documentation/widgetkit/widgetfamily/accessoryinline).

The `snapshot` columns on the style and prop rows declare admission. Unsupported
rows, scrolling, sticky positioning, timers, requests, native modules and handlers
fail with an edit. The author declares no resource; the bake synthesizes the
`exactViewport` resource for the size inject. The one fact a render reads
is the family's size, through the reserved inject `snapshot_viewport`: write
`inject` with `snapshot_viewport: <shape>` on the entry's component (or one it
uses; every injecting component names one shape), and the entry's plan
declares `exactViewport()` for it and provides it. `width` and `height` are the
family's content size in points, inside the system margins, given at boot and
again when the family's size changes; the preference fields are a still page's.
Choose a layout by them as `docs/recipes/responsive-columns.md` does for a page
(a Lock Screen row is under 100 pt tall; a medium widget is over 250 wide), so
one entry serves every family it declares. The app's own plan is untouched: an
unused component's inject is not demanded by the app's view. A configured widget's
second fact is its configuration, the `snapshot_configuration` inject above. A `performanceNow()` call is refused everywhere, including
inside `time` and `progress`; pass instants from refresh for platform time
presentation. A variable's name imposes no restriction. A class is a style name
or `class=(condition ? A : B)`. An entry uses only installed or generic font
families, such as `system-ui`: the system draws a widget or Live Activity in its
own process, which has none of the app's fonts, so the bake refuses a bundled
`font` the entry reaches (`snapshot-font-bundled`).
Images use a bundle-relative path, an `app:/` path written by the app, or a
`symbol:` source. Computed paths need a fixed local prefix; refresh downloads
remote images. A live `time` (`relative`, `timer`, `countdown`) must own its
text paragraph, so the widget can reserve its width; calendar formats may be
inline. Links must be their own elements, not runs among other paragraph text.
Snapshot handlers are refused; use a link to open the app.

Snapshot accessibility admits labels, roles, hints, heading levels and hidden
content. Live announcements and control-state accessibility rows are refused.

Snapshot paint uses document order and the system colour scheme. Use a condition
or `display: none` to hide content; `visibility` and `z-index` are refused.
Borders have one width, colour and solid, dashed, dotted or `none` style on all sides.
Radii are px or percentages; percentages on non-square boxes produce ellipses.
Radius calculations are refused. Overflow is the same on both axes: visible,
hidden or clip. Raster images author both width and height, and `object-fit` is
fill, contain, cover or scale-down. An entry declaring
any small or accessory family has at most one link; entries declaring only
medium, large and extra large families may have several.

Text aligns start, center or end; use `line-clamp` for truncation. Give an inline
highlight its own box. Colours are sRGB, system roles or named assets; wide-gamut
and profiled colours are refused. Gradients are nonrepeating linear or radial,
interpolated in sRGB or srgb-linear, with strictly increasing stops. Stop order
must hold at every box size; use a single unit when mixed units could cross.
Restricted paint values are literals or choices of literals, so their admission
is checked at bake.

App icons come from `app.json`'s Web App Manifest `icons` array, and in a dark
scheme from `color_scheme_dark.icons`, which replaces it there (LLP 1163). Each
host picks by `purpose`, then shape and size, and refuses a file smaller than
its slot. iOS and macOS take the smallest square `maskable`, else `any`, PNG of
at least 1024 px; iOS, watchOS and tvOS pass over one with transparent pixels,
and refuse it when no opaque file fits. iOS's dark icon is picked the same way
from `color_scheme_dark.icons`, and its tinted icon is a square `monochrome`
PNG, whose alpha iOS draws white on black. The web links each scheme's favicons
(the smallest square raster of at least 32 px, then an SVG when there is one)
and an Apple touch icon (the smallest opaque square PNG of at least 180 px, else
the largest). Android takes a square `maskable` of at least 432 px as its
adaptive icon, a `monochrome` one as its themed layer, a square `any` of 192 px
when there is no `maskable`, and a 16:9 `any` of at least 640×360 px as the TV
banner. tvOS takes the brand assets in the app's catalogs, else a 5:3 `any` of
at least 1280×768 px (an 8:3 and a 29:9 one give the Top Shelf images); the
watch app a square one of 1024 px, or `host.ios.icon` when its `icon.json` lists
watchOS; Windows a square `any` of at least 256 px, linked into the executable.
To select an
[Icon Composer project](https://developer.apple.com/documentation/xcode/creating-your-app-icon-using-icon-composer),
set `host.ios.icon` or `host.macos.icon` to its app-relative `.icon` directory,
for example `"host": { "ios": { "icon": "art/Primary.icon" }, "macos": { "icon": "art/Primary.icon" } }`.
Each platform selects independently; tvOS keeps its asset catalogs and does
not inherit the iOS selection. There is no `.icon` discovery. The selected
project must be inside the app and cannot coexist with a primary
`AppIcon.appiconset`, or a set with the project's name, in that platform's
catalogs. Missing paths and paths that are not `.icon` directories fail by name.
The project and its layers compile in the same `actool` pass as the app's
catalogs; changing them requires a new native build. `icons` still supplies the
web and the native raster fallback when the platform has no `icon` selection.
A variant selects its own project with the same two keys under `variants.<name>`
([variants](#install-a-development-build-beside-the-release-one-variants)).

An app may ship icons the person chooses for the Home Screen instead
(LLP 1163.000). `alternate_icons` in `app.json` lists them in the order a
picker shows them, each a `name` (ASCII letters and digits, starting with a
letter, at most 32, unique regardless of case) and the icons it is made from,
written and picked as the app's own are: `icons` with at least one square `any`
or `maskable`, `color_scheme_dark.icons`, a `monochrome` glyph for the tinted
icon. iOS and iPadOS build each one: from its own `host.ios.icon` project, else
an `.appiconset` of its name in the app's catalogs, else its icons. It is the
only roster: a catalog `.appiconset` that no entry names, other than the
primary's, is refused, as is a name that is `AppIcon`, the primary project's
or another asset's in the catalogs. Alternates need a primary. The names are in
the iOS compatibility id, so adding, removing, renaming or reordering one needs
a new binary; other hosts build none. `@exact/app-icon` reads the choice and
asks for another:

```
use appIcon, setAppIcon, AppIconStatus from "@exact/app-icon"

  resource shown = appIcon() as shape AppIconStatus else empty()
  mutation chosen as shape AppIconStatus refreshes shown
  action pick(name: string)
    send chosen = setAppIcon(icon=name)
  action pickPrimary
    send chosen = setAppIcon()
```

`appIcon()` answers `available` (the platform switches icons, and the binary
ships an alternate or one still shows), `icon` (the alternate shown, `none` for
the primary; it may name one an update removed) and `icons` (the roster when
available). `setAppIcon(icon=)` is a request: call it only from the action a
person's choice runs, never from a timer, a resource's answer or launch, since
iOS shows its own alert for each change and the runner does not enforce this.
The icon already shown answers at once with no alert, and `setAppIcon()`, with
no icon, always restores the primary. It refuses `NotAllowedError` (`person-required` from a tool or a
wake, or the app not in the foreground), `NotSupportedError` where `available`
is false (every target but iOS and iPadOS), `NotFoundError` for a name the
binary does not ship, and `UnknownError` with `apple: {domain, code}` when iOS
refuses. Under the agent the roster is the word `app-icon.icons` (`prefer`),
and each change holds an `app-icon` ticket standing for iOS's alert
(`docs/recipes/alternate-app-icons.md`).

What shows before the first frame is the launch colour, which `app.json` names
per colour scheme (LLP 1163 D2):

| `app.json` | Light | Dark |
|---|---|---|
| `"background_color": "#f4efe6"` | `#f4efe6` | `#f4efe6` |
| `"color_scheme_light": { "background_color": "#f4efe6" }` | `#f4efe6` | Exact's |
| `"color_scheme_dark": { "background_color": "#14110d" }` | Exact's | `#14110d` |
| `"background_color": "#f4efe6", "color_scheme_dark": { "background_color": "#14110d" }` | `#f4efe6` | `#14110d` |
| none | Exact's | Exact's |

A scheme's own member wins over the top-level one, which holds in every scheme.
Name none unless the launch must differ from what Exact shows: on iOS the app's
own background where the build can read it, every route it can open on agrees
(each route's `background-color`, else its root's, written as a colour or a
theme's colour) and nothing may paint over it out to the screen's edges, else
`systemBackground` (its grouped background where every route scrolls a grouped
list or writes `-exact-grouped-background`); on Android the theme's surface; on
the web the page's own colour, as a native app starts there. Name the colour
where the launch must match a background of the app's own on Android or the
web, or on iOS where something the build cannot see covers it, such as boxes
sized in points that fill the screen or a scheme the app restores at launch.
A colour is `#RGB`, `#RRGGBB`, `#RRGGBBAA`, or Display P3 as
`color(display-p3 r g b)` (`/ a` for alpha), which iOS and the web's first paint
show in Display P3 and Android takes clipped to sRGB. A fixed
`userInterfaceStyle` shows its side in both on iOS and the web; Android follows
the device. The web page has the colour until the first frame, the iOS launch
screen centres `host.ios.launchImage` on it, and the Android launch window is
it, with the bars' icons dark over a light colour. `theme_color` takes the same
three places for the browser's toolbar. The `manifest.json` a browser reads has
only the W3C members, so a colour named for light alone is not in it.

From Android 12 the launch window (the starting window) shows the launcher icon
in its middle, as every app's does. `host.android.launchImage` replaces it:
`{ "src": "<png>", "width": <dp> }` is a picture `width` dp wide, which the
platform shows within a centred square 192 dp across and, on stock Android, cuts
to the circle in that square (a `width` above 192 is refused); before Android 12,
whose launch window has no icon, the picture is centred on the window. `false`
shows no icon. Changing a colour or a picture requires a new native build.

### Store category, Game Mode and memory

`categories` in `app.json` is the W3C manifest's member, which the web manifest
carries: what the app is, for a store or catalog to file it under. A value is
one of the W3C's standardized categories or one of Apple's, written without
`public.app-category.` (`developer-tools`, `graphics-design`, `reference`,
`video`, and the game genres from `action-games` to `word-games`); the build
refuses any other. The iOS and macOS bundles' `LSApplicationCategoryType` is
the first that Apple has a category for: `fitness` and `health` are
`healthcare-fitness`, `photo` is `photography`, `social` is
`social-networking`, and `books`, `food`, `government`, `kids`, `magazines`,
`navigation`, `personalization`, `politics`, `security` and `shopping` have
none. App Store Connect requires a category of a Mac upload. A game the game
shell builds (`game`) with no category of Apple's is `games`.

```json
{ "categories": ["dice-games", "games"], "host": { "ios": { "increasedMemoryLimit": true } } }
```

An app whose Apple category is `games` or a genre is a game, and Game Mode
(priority on the CPU and GPU, lower Bluetooth latency for controllers and
AirPods, less background work) is offered to it: the bundles carry
`LSSupportsGameMode` (iOS 18.6, macOS 26), and the iOS bundle also
`GCSupportsGameMode`, which iOS 18.0 to 18.5 read, when `host.ios.minimumOS`
is below 18.6. `host.ios.gameMode` and `host.macos.gameMode` set it per
platform: `false` keeps a game too light to need it out of Game Mode, and
`true` offers it to an app that is not a game. tvOS and an App Clip carry
neither key.

`host.ios.increasedMemoryLimit: true` signs the app with
`com.apple.developer.kernel.increased-memory-limit`: on the iPhone and iPad
models that allow it, the app may use more memory than the default limit. The
app must still work without it (`os_proc_available_memory` says what it has).
Increased Memory Limit is a capability of the App ID, so a device build takes
only a profile that grants it and otherwise is refused
(`memory-device-profile`); a simulator build signs it as is. Extensions, the
App Clip and tvOS never take it.

Apple products live under the resolved app's target directory, scoped by
canonical source directory, manifest id, destination, composition and trust
policy. The Swift host is compiled once per destination for every app, in
`<target>/apple-swift/<destination>-<minimum OS>`; each app only links there,
one at a time, and its executable is copied to its own products before the next
app links. Outside production trust, the iOS minimum in that name is 17.0
unless the app has `launch` modules: the SDK's Rust, C and Swift compile for it
whatever `host.ios.minimumOS` says, and the app's minimum goes only to its Info.plist, its executable's link
and its own Swift modules and packages, which Swift checks for availability as
it compiles. A development build compiles it file by file and incrementally, and
beside the app's Rust; the host's two Rust modules build in
`<target>/apple-modules`, and a checkout that has not built one takes it from
`~/.cache/exact/apple-modules` when another checkout of this machine compiled
it from the same bytes. A target directory that has compiled nothing starts
with the registry crates this machine has compiled, and an app's also with the
checkout's own crates (runner, kernel, hosts) another app naming that checkout
compiled, when the app builds in its own `target/` (`~/.cache/exact/apple-crates`; Cargo decides which it can use, and a
crate edited, pulled or switched since compiles again). An app's own crates are
never kept; a checkout unused for 14 days, or gone, is forgotten, and
`EXACT_APPLE_KEEP_CHECKOUT=0` keeps registry crates only. Delete either
directory to compile everything here. Every production Apple build (`--release`,
`--archive`, `exact release`, or `EXACT_UPDATE_TRUST=production`) uses the whole-module
Swift build and strips its executable, writing `<executable>.dSYM` beside the staged
build product. Development builds keep their symbols. iPhone and iPad device app executables
link SafariServices in development and production so dyld loads the libraries
UIKit would load at launch (LLP 1047.001.001). A device App Clip copies its parent’s
executable and carries the same link; a simulator links the same target with the Clip’s
own embedded entitlements. An IPA writes `<name>.dSYM`
beside its archive; `exact release` copies the build's matching dSYM beside its release.
Production iOS and tvOS bundles ship a small receipt and strip Swift frameworks, whether
archived or not. The whole receipt stays beside `<name>.app` or `<name>.ipa` as
`<name>.receipt.json`; framework dSYMs are in `<name>.dSYMs/<Framework>.framework.dSYM`.
Rust module and package frameworks keep their linker output because Xcode's strip
can make them unloadable. GPU frameworks keep the bytes the bake authenticated.
All keep matching dSYMs outside the bundle.
Production widget and share extension executables are stripped before signing; their
source dSYMs are kept in `<name>.dSYMs/<Extension>.appex.dSYM`, outside the app.
Watch builds keep the watch executable's, complication extension's and runtime
framework's dSYMs in `<name>.dSYMs/Watch/`. Mac builds keep module library and
framework dSYMs outside the app too, including the bare products' shared runtime
at `<namespace>/ExactCore.framework.dSYM`.

To keep Spotlight from indexing build intermediates on your Mac, add the checkout's
or app's `target/` directory to Spotlight's Search Privacy settings. Keep the release
dSYMs for the executable, frameworks and extensions in an indexed archive so Xcode can
find the matching symbols for a crash report. [Apple's guide to symbolication](https://developer.apple.com/documentation/xcode/adding-identifiable-symbol-names-to-a-crash-report).
A production build links its Rust
with fat LTO and, when its plan is fixed, leaves out the loaded modules the
plan cannot reach (LLP 1036.000 §5–§10). `--bundle` prints the stable Mac bundle at
`<target>/clients/<source-key>/<id>/macos/<Name>.app`; `scripts/exact.mjs`,
`agent --app` and metrics use that same resolver. `--host` leaves both standalone
and sample products; simulator and device bundles have separate destinations.
On the Mac the sample host is a bundle beside the app's, `<Name> Host.app`, which
`agent host` launches. Under the app's macOS profile it embeds that profile and takes
the app's id and entitlements (iCloud, push, a keychain group), so its two sessions
keep what the app's do; with none it is `<id>.host`. It registers no document type or
link scheme, which stay the app's.
Two apps can build together. A second Apple build of the same source/app fails
with its owner's PID and lock path before baking; remove a stale lock only after
verifying that owner is no longer running. Failed packaging retains the previous
complete product. `EXACT_MAC_BIN` remains an explicit diagnostic override, checked
against the selected app's embedded identity before the driver launches it.

An app can ship macOS helper executables and resource trees separately from
its baked assets. Keep the tree in a dedicated directory beside `app.json`:

```json
{"host":{"macos":{"resources":[{"from":"server","to":"Resources/server"}]}}}
```

`mac --bundle` copies it to `Contents/Resources/server`, preserving file modes,
names such as `node_modules/@scope`, and relative symlinks within the tree.
These files have no bake size cap and are excluded from TypeScript capture and
web assets. `from` cannot overlap source, asset or output roots: it is refused
under `assets/`, `deck/`, `gpu/`, `fonts/`, `strings/`, `web/`, `apple/`, `linux/`,
`src/`, `data/`, `modules/`, `node_modules/`, `target/` or `dist/` (a JSON file the
app ships goes in a directory of its own, such as `data-files/`); `to` must name
a private subtree of `Resources/`, `Helpers/` or `Frameworks/`. Mach-O helpers
and libraries are signed before the outer bundle; `exact release` signs them
with its release identity. Resource changes require a new binary, not an asset
update. Find `Resources/server` through `Bundle.main.resourceURL` from a native
module. This field applies only to macOS bundles. A file written before the
build (a build stamp, a channel name) goes in such a tree under `Resources/`:
it is copied before the bundle is sealed, so it is inside the seal. Only code
goes under `Helpers/`, where `codesign` refuses a file that is not.

`host.ios.resources` (iOS, iPadOS and tvOS) takes the same entries for files a data
module reads by path (large, binary, or opened by a native module; data the app ships
and only reads is a JSON file `app.ts` imports, [shipped data](recipes/shipped-data.md)): `to` is under `Resources/`, and the tree lands in the bundle's
`ExactResources/` folder, which holds nothing else and is what `bundle:/` names there,
so `fs.read bundle:/<to>` (`fs.read bundle:/Resources/server` for the entry
above) reads it on both hosts. A tree holds no links (`bundle:` follows none)
and no Mach-O code; changing it requires a new binary, never an update. The
app reads it, not its extensions: a widget's refresh keeps only `fs.read app:/`
grants, so its `bundle:` read is refused as ungranted (an `Error`, code
`denied`), and a widget extension's process has no `bundle:` root besides.

A helper that needs a hardened-runtime exception (a JavaScript runtime's JIT,
say) declares it as a grant beside the app's others, naming the exception and
the one executable it is for, by its path under `Contents` (LLP 1069.008 D9):

```ts
export const grants = `
runtime.allow-jit Helpers/node/bin/node
runtime.disable-library-validation Helpers/node/bin/node
`;
```

The exceptions are `allow-jit`, `allow-unsigned-executable-memory` and
`disable-library-validation`; `exact release` signs that executable with
`com.apple.security.cs.<exception>` and nothing else in the bundle with it.
Only helpers take an exception: a line naming `app`, the app's own executable,
is refused (deferred until it has a consumer). A target must be a Mach-O
executable inside a `host.macos.resources` destination, not reached through a
link; a nested `.app`'s executable gives its grant to that
bundle. `mac --bundle` refuses one that is not, and other hosts ignore the
lines. Adding a line changes the grant ceiling, so the deploy classifier calls
for a binary release.

Mach-O code in content-addressed `assets/`, `deck/` or `shaders/` is refused
when assembling a Mac bundle: place helpers in `host.macos.resources` instead.
The release signs GPU modules before baking their digests and preserves those
signatures through packaging. iOS archives package primary and named GPU modules
as signed frameworks; development builds keep their loose dylibs.
Combining `--device --archive out.ipa --embed` produces both the IPA and the
embed directory without a connected phone. The embed directory contains the
static library, C header, assets, shaders, compatibility and capability metadata,
receipt, and signed GPU frameworks (primary and named modules). Copy those
frameworks intact into the embedding app's `Frameworks/`; their binaries must
retain the baked digests.

### Optional Apple host products

An external Swift host links `ExactKit` plus each capability product its plans
use, then calls that product's public `install()` before creating sessions.
The generated `--embed` composition lists the capabilities to link.

| Product | Capability | Installation |
| --- | --- | --- |
| `ExactMarkdown` | Markdown text and editing, and its atoms' icons in text | `ExactMarkdown.install()` |
| `ExactCollections` | Windowed lists, swipe rows, reordering rows (grips and groups) and iOS list row cells | `ExactCollections.install()` |
| `ExactDrag` | Height and transform drags | `ExactDrag.install()` |
| `ExactGroupedLists` | Native grouped lists | `ExactGroupedLists.install()` |
| `ExactSurfaces` | GPU and Canvas surfaces | `ExactSurfaces.install()`; links `ExactImages`, which Canvas 2D images read through |
| `ExactImages` | Image files, animated images and package pictures; the files icons in text read | `ExactImages.install()` |
| `ExactPickers` | File inputs' picker, `saveFile` and the File System Access pickers | `ExactPickers.install()` |
| `ExactMediaPicker` | Native media picking (the composition links `ExactPickers` with it) | `ExactMediaPicker.install()` |
| `ExactAuthSession` | Authentication sessions | `ExactAuthSession.install()` |
| `ExactNumerals` | Numeric content transitions | `ExactNumerals.install()` |
| `ExactTips` | Platform tips | `ExactTips.install()` |
| `ExactPresence` | Native presence checks | `ExactPresence.install()` |
| `ExactEffects` | CSS `filter` on a box | `ExactEffects.install()` |
| `ExactSvg` | SVG elements, SVG images and icons (links `ExactEffects` too) | `ExactSvg.install()` |
| `ExactNativeViews` | The app's native module, native views and hatches; links `ExactFormaBinding`, the public `ExactForma` | `ExactNativeViews.install()`, which installs Forma too |
| `ExactControls` | Switches, checkboxes, radios, ranges, selects, dates, progress and segmented tablists | `ExactControls.install()` |
| `ExactFlights` | Shared-element flights (`sharedElement`) | `ExactFlights.install()` |
| `ExactSticky` | `position: sticky` | `ExactSticky.install()` |
| `ExactNotifications` | Notifications, their actions and push registration | `ExactNotifications.install()`; the app delegate calls `ExactApp.installNotificationClicks` before launch completes |
| `ExactHostEventMailbox` | Host events a package takes (an accepted iCloud share) | `ExactHostEventMailbox.install()`; the app delegate calls `ExactApp.installHostEvents` before any scene connects |
| `ExactActivities` | Live Activities (links `ExactWidgetPublication`, whose generation their sends name) | `ExactActivities.install()` |
| `ExactActivities` | Live Activities | `ExactActivities.install()` |
| `ExactToolDoor` | The tool door: the system's calls of the plan's tools | `ExactToolDoor.install()`; the app delegate calls `ExactApp.installToolDoor` before launch completes |
| `ExactIndexMirror` | The content index `@exact/search-index` writes: its mirror, Spotlight, entity queries | `ExactIndexMirror.install()`; the app delegate calls `ExactApp.installContentIndex` before launch completes |
| `ExactWakeCarriers` | Wakes, background transfers and monitored regions | `ExactWakeCarriers.install()`; the app delegate calls `ExactApp.installWakes` before launch completes, and `ExactApp.handleTransferEvents` |
| `ExactGradients` | CSS gradients: `background-image` and `mask-image` | `ExactGradients.install()` |
| `ExactTraces` | The traces an app records, and `ExactSession.startTrace()` and `stopTrace(maxBytes:)` | `ExactTraces.install()` |
| `ExactWakeLock` | `wakelock="screen"` | `ExactWakeLock.install()` |
| `ExactContentRegions` | Content regions an entry registers (the Mac's region controller) | `ExactContentRegions.install()` |
| `ExactVideo` | `video` and `audio` elements' players; an image's picture in picture | `ExactVideo.install()` |
| `ExactWebViews` | `iframe` web views | `ExactWebViews.install()` |
| `ExactSound` | Declared sounds, sequences and the voice-table commands | `ExactSound.install()` |
| `ExactMIDI` | MIDI ports and a sequence's MIDI output | `ExactMIDI.install()` |
| `ExactAnimations` | What Core Animation plays for a box: its CSS animations (`animation`, `@keyframes`), transitions, drags and spring motion, so any of `animations`, `transitions`, `motion`, `drag` or `timelines` links it; SVG's element animations play through it too | `ExactAnimations.install()` |
| `ExactNavigation` | Route stacks, tabs and their bars, route modals; on the Mac route sheets, the list beside its detail, Back and Forward, the root tabs as the sidebar, and a route’s trailing full-height `aside` as the window inspector | `ExactNavigation.install()` |
| `ExactTextSelection` | iOS's selection of text a `user-select` of `text` or `all` makes selectable (the Mac's is in `ExactKit`, and every Mac composition names it) | `ExactTextSelection.install()` |
| `ExactWidgetPublication` | Widgets and Live Activities: the app's publication of its entries to the App Group and its observation of the store they write | `ExactWidgetPublication.install()` |
| `ExactDialogs` | iOS and tvOS: a `dialog` shown by `showModal()` as UIKit's sheet, or as the author's box when it paints its surface; any `dialog` in the plan links it. On the Mac the core presents a `dialog` | `ExactDialogs.install()` |
| `ExactSharing` | `share(…)`'s sheet, a link's context-menu Share item, and a received share's claim | `ExactSharing.install()` |

Development builds log a missing Markdown or drag installation once per session,
with the product and installation call. Without it, Markdown remains literal
text and drag gestures are unavailable.

### Replacing an embedded app's plan

`ExactApp.apply` and `applyGeneration` answer `ExactApplyResult`: `.applied`,
`.held(reason)` or `.refused(error)`. AppKit keeps marked text when the replacement
keeps its native editor. A replacement that removes the field or changes its
native control waits for composition to end. Marked text in an embedder's
own editor also holds the replacement. Other macOS holds include a mouse
button down in the app, a trackpad
gesture's fingers, a tracking loop AppKit runs (a menu, a control's press, a
scroller) and a gesture recognizer that has begun in a session's window. On
iOS and tvOS it is a touch UIKit has not ended, a press of a remote's or a
game controller's button (tvOS's Select has no touch) and, in a session's
window, a first responder with marked text, a scroll view tracking or
dragging, a gesture recognizer that has begun and a view controller
transition. An embedder's own recognizers and scroll views in that window
hold it too. A row the keyboard lifted to reorder holds it until the drop or
the cancel.
Loading a candidate's data module and finishing the running module's
requests also hold it; the reason names which. The app keeps the held candidate as bytes, with its assets,
modules and commit closure, and prepares it once the hold clears, so it carries the state
of that moment. A newer candidate supersedes it. Do not poll or submit it
again.

`ExactAppLifecycle.generationStarted` reports that a candidate applied.
`replacementEnded` reports, once, with its token and reason, that a held
candidate was refused when prepared, superseded, or dropped because every
session it was held for was destroyed or, for a generation from `connect`'s
URL, because `disconnect` or another `connect` closed that connection.
`state.replacement.pending` in the
agent's `state` names the hold. `ExactSession.apply`, which replaces one
session's plan, refuses instead of holding. The rule covers development saves
and `immediate` production updates (LLP 1029.000.002 D7, D9). The dev menu's
Reload is the person's own act: it ends a composition as a click elsewhere
does, keeping the marked text, and commits at once.

### Mac development signing

`EXACT_IDENTITY=<certificate-sha1>` selects a Keychain signing identity;
`EXACT_IDENTITY=-` selects ad hoc signing for a local build. Otherwise,
`DEVELOPMENT_TEAM=<team-id>` selects that team's Apple Development identity.
A build chooses the newest valid certificate of the selected team authorized by
the app’s profile and every embedded extension’s profile, when present. An explicit
identity excluded by any of those profiles is refused. With no team selected, certificates of one team select that team automatically. Certificates
of several teams require an installed macOS profile that resolves the team before
choosing widgets, a share extension or grant entitlements. If profiles do not
resolve it, the build refuses and names `DEVELOPMENT_TEAM=<team-id>`,
`EXACT_IDENTITY=<sha1>` and `EXACT_IDENTITY=-`. This applies to every app,
including an ordinary `secret.keep` app. With no identity at all, a build without
profile requirements or extensions can still sign ad hoc. Developer ID profile
ambiguity names `EXACT_DEVELOPER_ID=<sha1>`, the release identity selector.

Local builds made by `mac --bundle` omit the hardened runtime, so an ad hoc
host can load its ad hoc native modules. An external host signed ad hoc with
`codesign --options runtime` can start and pass strict signature verification,
but macOS library validation refuses its native modules because neither has a
Team ID. For a hardened host, sign the host and every native module with valid
certificates of the same team. A successful launch alone does not check module
loading; exercise the plan's native view or module call too.

Certificate signing keeps the app's login Keychain access stable across
rebuilds (LLP 1018 D7). Before each local development sign (`--timestamp=none`),
the build prints the identity and file on stderr. Each has a 30-second limit.
A failure retains the tool's error and explains how to allow access to the
signing key. Mac development failures also name `EXACT_IDENTITY=-` as a way
to build ad hoc; device failures never suggest it. Timestamped release signing
has no such limit. Ad hoc signing cannot authorize provisioning-profile
entitlements, Mac widgets or a share extension. Rebuilding an ad hoc app can
prompt again for stored login-keychain items, blocking the first frame.

### Mac provisioning profiles

Some entitlements hold on a Mac only under a provisioning profile that
authorizes them (Apple TN3125), and the Mac build signs them from one (LLP
1133.010 D7): push's `com.apple.developer.aps-environment` (`device.push`), App
Attest's environment (`device.attestation`), the Declared Age Range
(`device.age-range`), Group Activities (`device.shareplay`), CloudKit
(`device.cloudkit`), the iCloud key-value store (`device.icloud-kv`), Sensitive
Content Analysis (`device.sensitive-content-analysis`), the keychain group a locked secret is kept in
(`secret.keep` with a presence policy), and a passkey's associated domains
(`auth.passkey`). An app whose grants need any of them is built from the
newest installed macOS profile for its id (or the team's wildcard) that allows
them all, within `DEVELOPMENT_TEAM` when set: a development profile that names
this Mac for a development build, a Developer ID one for `exact release`.
An already resolved identity restricts automatic profile selection to its team.
`EXACT_MAC_PROFILE=<path>` names one, and
the build says why a named one does not fit. A named profile overrides
`DEVELOPMENT_TEAM`, as on devices. A team selected by `DEVELOPMENT_TEAM`
requires a suitable profile. An explicit `EXACT_IDENTITY` overrides that setting;
without a suitable profile for its certificate's team, a development build keeps
the certificate, signs no profile entitlements and logs each grant's effect.
With a profile, the build signs with its team's identity (Apple Development, or `EXACT_DEVELOPER_ID` for a
release; one `EXACT_IDENTITY` or `EXACT_DEVELOPER_ID` names must be of that
kind and team, or the build is refused), writes the application identifier,
the team and each entitlement (an environment `development` under a
development profile, `production` otherwise; CloudKit uses `Development` and
`Production`), embeds the profile as
`Contents/embedded.provisionprofile`, and always assembles the `.app`, where
the profile holds.

With no profile that fits, a development build signs none of them and says,
for each grant, what the app then reports (`available: false`,
`NotSupportedError`). A release build is refused (`grant-needs-profile`) for
App Attest, the age range, the iCloud key-value store, Sensitive Content
Analysis, a passkey or a locked secret that is not `optional`; one granting
push, SharePlay or CloudKit ships without it, the package reporting it unavailable.
An app with Mac widgets signs both under one profile, of the widgets' team,
chosen among the profiles that grant the widgets' App Group and shared
Keychain group and the grants' entitlements; with none that grants both, the
grants sign nothing. An ad hoc build (`EXACT_IDENTITY=-`) signs no profile's
entitlements.

`device.cloudkit` derives one container, `iCloud.<bundle id>`, or selects the one
written as `device.cloudkit iCloud.<name>`; it takes no purpose. A second container
is refused. On iOS and iPadOS, and under a Mac profile, it signs
`com.apple.developer.icloud-services = ["CloudKit"]`,
`com.apple.developer.icloud-container-identifiers = [<container>]` and
`com.apple.developer.icloud-container-environment = "Development"` or `"Production"`
by the signature's kind. A distribution archive uses `Production` (ITMS-90046).
The profile's services must include CloudKit and its containers must allow the
selected identifier; a profile for another container is refused. No push or
`remote-notification` mode is derived, and no `ubiquity-*` entitlement. tvOS,
App Clips and extensions sign none. A bound package declaring
`"hostEvents": ["cloudkit-share"]` receives shares and gets `CKSharingSupported`
and `ExactHostEvents` only where the grant is signed; a Mac build without a
profile claims neither.

### Web CI preparation

For a website's `web-build` and `test web` on Linux, check out Exact at the path the
app's `exact.mjs` uses and install Bun at the version in Exact's `package.json`.
Install rustup if the app or SDK tools will build from source. Run these commands from the app directory.

```sh
bun <exact2>/scripts/exact.mjs setup --platform web
bun install --frozen-lockfile
bun exact.mjs test web
```

A Contract and TypeScript site can use matching prebuilt tools for its web build and browser tests.
Web setup checks the tools and their recorded Cargo metadata against this checkout, then installs
the JavaScript dependencies and browser. `--check` uses the local cache. Setup can also fetch
from a tools origin when the checkout pins its producer key. Native builds, games, Rust data or
GPU crates, custom web build scripts and Rust page renderers require Rust.

The tools origin is a public bucket that `bun scripts/tools.mjs publish --watch` keeps from
the producer's Mac, which holds the signing key. A checkout reads it at the public https URL
that `scripts/tools.mjs` pins with the producer's public key; `EXACT_TOOLS_ORIGIN` names a
mirror of it. Until both are pinned, nothing is downloaded, and a machine without Rust uses a
set `bun scripts/tools.mjs keep` built. The origin keeps each commit's lookup for 90 days; an
older checkout finds a set only among the newest, and only if its inputs are that set's.

For a source build, the scoped setup installs the pinned stable and nightly Rust, the nightly's `rust-src`,
`llvm-tools`, clippy and `wasm32-unknown-unknown` target, its standard-library dependencies,
wasm-bindgen, Binaryen, Exact's `node_modules`, Playwright's Chromium and headless shell,
and Exact's locked crates. The wasm tools serve Markdown, motion, the markup editor,
text flow, Rust data modules and GPU modules as well as `--wasm` builds. `CHROME` may
name an installed browser instead. Setup reports the same required rows as
`setup --check --platform web`. It skips native targets, Hermes, game preparation and
the native-build disk warning. Games need the full `setup`. On a minimal Linux image,
`bun <exact2>/node_modules/playwright-core/cli.js install-deps chromium` installs
Chromium's system libraries.

### Native CI preparation

The [EAS iOS](recipes/eas-ios.md) and [EAS Android](recipes/eas-android.md)
recipes use the ordinary native builders (LLP 1030.003 D6, D8). Exact does
not manage an EAS account, credentials, build counters or store submissions.
These examples describe a single-target app; extensions need their own
matching profiles, as described in the widget signing sections.

Keep Exact's `app.json`. Add a separate `app.config.js` for EAS, returning
only Expo's configuration rather than merging Exact's manifest:

```js
module.exports = {
  name: 'My app', slug: 'my-app', version: process.env.EXACT_VERSION || '1.0.0',
  ios: { bundleIdentifier: 'com.example.myapp' },
  android: { package: 'com.example.myapp' },
  extra: { eas: { projectId: 'YOUR-EAS-PROJECT-ID' } },
};
```

Match `ios.bundleIdentifier` to Exact's `app.id` and `android.package` to
`host.android.id` (default: `app.id` with hyphens replaced by underscores).
Link the EAS project and install
`expo-modules-autolinking` as a development dependency for EAS's config
resolver (`bun add --dev expo-modules-autolinking`; commit the lockfile).
EAS CLI reads it from the app's `node_modules` on the machine that runs
`eas build`, so run `bun install` there first: without it, `eas build -p ios`
fails in its entitlements step, before anything is sent to EAS.
Exact does not need Expo or React Native at run time. Pin EAS CLI in the app's
dev dependencies too (`bun add --dev --exact eas-cli`). This `eas.json` uses file secrets for signing; set
`EXACT_REF` to the immutable Exact commit used by the app:

```json
{
  "cli": { "appVersionSource": "remote" },
  "build": {
    "exact": {
      "environment": "production",
      "distribution": "store",
      "withoutCredentials": true,
      "autoIncrement": true,
      "bun": "1.4.2",
      "env": { "EXACT_REF": "FULL-EXACT-COMMIT", "EXACT_VERSION": "1.0.0" },
      "ios": { "config": "eas-ios.yml", "image": "macos-tahoe-26.6-xcode-27.2" },
      "android": { "config": "eas-android.yml", "image": "ubuntu-26.04-jdk-17-ndk-r27b-sdk-58" }
    }
  }
}
```

Image names above are the reporter's working choices; choose an available
image with Xcode 27.1+ for iOS, or JDK 17+ and Android command-line tools for
Android. Bun must satisfy Exact's `package.json` engines range. Initialize
EAS's remote build counters above any number already uploaded; do not assume
the first remote build increments an existing store number. Resolve the
build config before reading `eas.job.version.buildNumber` or `versionCode`.
See [EAS custom builds](https://docs.expo.dev/custom-builds/schema/) and
[build configuration](https://docs.expo.dev/build/eas-json/).

Save the following as `.ci/prepare-native.sh` in the app repository. The
recipes run it with `ios` or `android`. It installs only that platform's
Rust targets and Hermes bundles, with no simulator, and fetches the crates
Exact's lock names: Exact's tools build offline, and a worker's empty Cargo
registry stops the first of them. Bun, git, curl and unzip come from the
selected image. For a clean local worker, install Bun
1.4.2 first and omit the final two EAS `set-env` calls.

```sh
#!/bin/sh
set -eu
export EXACT="$PWD/../exact"
git clone https://github.com/expo/exact.git "$EXACT"
git -C "$EXACT" checkout --detach "$EXACT_REF"
bun install --frozen-lockfile --cwd "$EXACT"
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/exact-rustup.sh
  sh /tmp/exact-rustup.sh -y --profile minimal --default-toolchain none --no-modify-path
fi
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
pin=$(bun -e 'console.log(Bun.TOML.parse(await Bun.file(process.env.EXACT + "/rust-toolchain.toml").text()).toolchain.channel)')
case "$1" in
  ios) triple=aarch64-apple-ios ;;
  android) triple=aarch64-linux-android ;;
  *) exit 2 ;;
esac
rustup toolchain install "$pin" --profile minimal --target "$triple"
cargo +"$pin" run --locked --manifest-path "$EXACT/vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml" -- --target "$triple"
cargo +"$pin" fetch --locked --manifest-path "$EXACT/Cargo.toml"
set-env EXACT "$EXACT"
set-env PATH "$PATH"
```

The installer also installs the host Hermes bundle used to bake TypeScript.
`setup --check` reports a registry that lacks any of Exact's locked crates
on its `Cargo crates` row.
If the launcher or Cargo manifests still name a developer's checkout, run
`EXACT=../exact bun exact.mjs update` locally and commit its changes before
submitting the job. An app's `host.ios` and `host.android` entries must be
present; `exact new --with ios,android` creates both. Native shells are
generated by the build. A native-view dependency must supply a roster that
can be read without a simulator; see [the CI archive path](#build-an-ios-app-on-a-ci-machine).

Cache Cargo's registry and git downloads, `$CARGO_HOME/hermes-lean-sys`,
Gradle's `GRADLE_USER_HOME` (default `~/.gradle`), and `~/.cache/exact` when
useful. Restore app and Exact `target/` only under keys that distinguish
OS/architecture, toolchain, lockfiles and build mode, and keep both
checkouts at the same absolute paths between runs: Cargo's fingerprints
include the location of a path crate outside the app's workspace. A cache
hit does not promise reuse; production Apple builds do not use the
development module cache, and
`CARGO_LOG=cargo::core::compiler::fingerprint=info` explains rebuilds. A
clean build must work without these caches. Do not cache signing keys or
provisioning profiles.

Cargo rebuilds a path crate when one of its source files is newer than its
last build, and a fresh clone dates every file at the clone, so a restored
`target/` alone still rebuilds every Exact crate. Dating the clone's files
at its commit's time lets Cargo reuse the restored output. Add these lines
to `.ci/prepare-native.sh` directly after the `git checkout`:

```sh
when=$(TZ=UTC git -C "$EXACT" log -1 --date=format-local:%Y-%m-%dT%H:%M:%SZ --format=%cd)
git -C "$EXACT" ls-files -z | (cd "$EXACT" && xargs -0 touch -h -d "$when")
```

This is sound only while each file so dated has the bytes it had when the
cache was saved, because Cargo then compares nothing but times: a changed
file with an old time is not rebuilt, and the build ships its stale output.
So restore that `target/` only under a key that names the Exact commit
(`EXACT_REF`), with no fallback key that restores another commit's cache,
and date only the files the checkout wrote, before any step changes the
clone. Leave the app's own files their checkout times unless the key names
the app's commit too.

### Reclaim an idle app's build output

On macOS and Linux, run `bun <exact>/scripts/sweep.mjs clean <app-dir> --dry-run --json`
to list an outside app’s rebuildable output. Omit `--dry-run` to remove it.
Exact keeps saved traces, the receipts that the run history points to,
production bundles and their symbols, build locks, and unrecognized paths.
A build, dev server or driver using the app’s output keeps it busy. A cleanup
that starts first holds off new Exact builds until the files have moved.
The app must own its target directory.

The JSON object has `version: 1`, `status`, `dryRun`, `target` and three lists.
`remove` gives absolute paths, `kept` gives a path and a reason (`record`,
`lock` or `unlisted`), and `held` names busy locks or leases. The exit code is
0 for `ok`, 2 for `refused`, and 1 for `busy` or `failed`. On failure,
`error` gives the cause and `remove` lists the paths already moved.

### Import an iOS signing identity on a CI worker

For the iOS recipe, save this as `.ci/sign-ios.sh`. Source it in the build
step so the exported identity reaches Exact. In the EAS production
environment, create secret file variables `CI_IOS_P12` and `EXACT_PROFILE`,
and a secret string `CI_IOS_P12_PASSWORD`. The files are a distribution
certificate with its private key and a matching App Store provisioning
profile. EAS supplies file variables as paths; see its
[file-variable documentation](https://docs.expo.dev/eas/environment-variables/faq/).
Use only on an ephemeral worker: this selects a temporary keychain for the
step. Do not turn on shell tracing around credentials.

```sh
set -eu
CI_SIGN_DIR=$(mktemp -d)
CI_KEYCHAIN="$CI_SIGN_DIR/build.keychain-db"
CI_KEYCHAIN_PASSWORD=$(openssl rand -hex 24)
trap 'security delete-keychain "$CI_KEYCHAIN"; rm -rf "$CI_SIGN_DIR"' EXIT
security create-keychain -p "$CI_KEYCHAIN_PASSWORD" "$CI_KEYCHAIN"
security set-keychain-settings -lut 21600 "$CI_KEYCHAIN"
security unlock-keychain -p "$CI_KEYCHAIN_PASSWORD" "$CI_KEYCHAIN"
security import "$CI_IOS_P12" -k "$CI_KEYCHAIN" -P "$CI_IOS_P12_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$CI_KEYCHAIN_PASSWORD" "$CI_KEYCHAIN"
security list-keychains -d user -s "$CI_KEYCHAIN"
EXACT_IDENTITY=$(security find-identity -v -p codesigning "$CI_KEYCHAIN" | awk '/[0-9A-F]{40}/ {print $2}')
test "$(printf '%s\n' "$EXACT_IDENTITY" | wc -l | tr -d ' ')" = 1
test -n "$EXACT_IDENTITY"
export EXACT_IDENTITY
```

If EAS already manages the credentials, download them through `eas
credentials` for these secret files, or adapt the step to decode
`eas.job.secrets.buildCredentials`: iOS uses the chosen target's
`distributionCertificate.dataBase64` (base64), its `password`, and
`provisioningProfileBase64`; Android uses `keystore.dataBase64`,
`keystorePassword`, `keyAlias` and `keyPassword`. When using EAS-managed
job credentials, remove `withoutCredentials`. Select the intended iOS
target explicitly when there are several. Do not use
`eas/configure_ios_credentials`: it expects an Xcode project. Exact signs
the app directly with `EXACT_IDENTITY` and `EXACT_PROFILE`.

### Build Android artifacts without a device

From the app's directory, `bun exact.mjs android --release --no-install --json`
builds an APK without querying adb. `bun exact.mjs android --bundle --json`
builds a production App Bundle (`.aab`) without installing. Both report the
artifact's absolute path. An outside app's outputs are
`target/android/app/outputs/apk/release/app-release.apk` and
`target/android/app/outputs/bundle/release/app-release.aab`.

`EXACT_BUILD_NUMBER` sets Android's `versionCode` (1 by default, an integer
from 1 through 2100000000); `EXACT_VERSION` sets `versionName` (0.1.0 by
default). The app reads their baked strings as `buildNumber` and `version`
through `exactDelivery()`; over-the-air updates keep both values.
Set all four of `EXACT_KEYSTORE` (a file path),
`EXACT_KEYSTORE_PASSWORD`, `EXACT_KEY_ALIAS` and `EXACT_KEY_PASSWORD` to sign
with your upload key. Partial credentials fail before compiling. A bundle
requires them; an APK without them uses Android's debug key and cannot be
submitted to Play. Passwords are passed through the environment, never as
Gradle command arguments (LLP 1030.003 D6, D8).

`--release` selects optimization. For an APK you will distribute, also set
`EXACT_UPDATE_TRUST=production`; `--bundle` sets it itself. With
`deploy.store.android` absent or `"0"` the APK has no update store, so every
change ships as a new build and a production build needs no update signing
keys or receipt. At `"A"` it takes signed plans, assets and TypeScript
modules, never native code (LLP 1030.006), and a production build needs
`deploy.signing.keys` and `EXACT_UPDATE_RECEIPT` or `EXACT_UPDATE_GENESIS=1`,
as iOS does. The [over-the-air updates recipe](recipes/over-the-air-updates.md)
covers the key, `app.json`, the first build, publishing and an in-app banner.

### Build an iOS app on a CI machine

A Mac runner needs no phone. An app with native view tags has its build read
their roster by trying three routes in order: a Mac Catalyst executable, an
iOS simulator, then a macOS library. A route whose xcframework has no slice
for it, or whose probe does not compile, load or run, falls through to the
next. The simulator route uses the simulator `--sim` or `EXACT_SIM` names,
else a booted one, else boots one and shuts it down again, which needs an
installed iOS runtime. When every route fails, an archive or a `--release`
build stops; a development build warns and keeps the manifest's
roster. From the app's directory:

```sh
bun exact.mjs ios --device --release --unsigned --json     # an ad-hoc device .app, no phone, profile, identity, icon or install
bun exact.mjs ios --device --archive out.ipa --unsigned   # an ad-hoc signed .ipa, for a service that re-signs it
bun exact.mjs ios --no-install                            # a development simulator .app; the build prints its path
bun exact.mjs ios --no-install --json                     # the same, its path in one JSON object on stdout
```

With `--json`, `ios`, `mac`, `web-build`, `linux`, `android` and `windows`
print one JSON object on stdout, its last and only line, however they end, and
send the build's own output to stderr. The object names the artifact's
absolute `path` (the `.app`, the `.ipa` of `--archive`, `--embed`'s directory,
`target/web-dist` or `--out`'s directory, the executable, the APK or AAB), its
`target` (`ios-simulator`, `ios`, `tvos-simulator`, `tvos`, `macos`, `web`,
`linux`, `windows` or `android`) and its `trust` (`development` or
`production`: what the build baked; the JS target's `web-build` is
`production` only with `--out`, and a game's reports the trust it baked). The Apple builds add `signing`, the signing identity's SHA-1 or
`ad-hoc`. `ios` without `--no-install`, `--unsigned` or `--archive` adds `installed`, the simulator's or
phone's `name` and `udid` (with `--watchos`, the paired phone's), and
`android` adds `installed`, the device's serial, or null when no device was
ready; its object is printed once the install has succeeded:

```sh
app=$(bun exact.mjs ios --no-install --json | jq -r .path)
```

A failure prints `{"code": …, "error": …}` instead, as `share --json` does,
and exits non-zero: `build_failed` (the build failed, an install after it
failed, or it named no artifact), `before_build` (app.json's `before_build`
failed), `app_json` (app.json does not parse, or a command of its own takes a
verb's name), `platform_refused` (the platform's setup is missing, or an Apple
build asked for on Windows), `unknown_flag` (a flag the verb does not take,
refused before anything builds), or `json_flags` (`--json` takes none of
`--run`, `--prebuild` and `--test`).

`--unsigned` requires `--device` and refuses `--run`: a stock device cannot
launch an ad-hoc signed bundle. It builds a `.app`, or an `.ipa` with `--archive`.
For a signed device `.app`, `DEVELOPMENT_TEAM=<team-id>` restricts the installed
development profile and Apple Development identity to that team. `EXACT_PROFILE`
and `EXACT_IDENTITY` override this selection. If none of that team's profiles covers
the bundle id and phone, the build refuses: unset `DEVELOPMENT_TEAM` to take any
team's, or run a project in Xcode with that bundle id and team on that phone with
Automatically manage signing enabled, then retry.
`EXACT_IDENTITY` and `EXACT_PROFILE` in place of `--unsigned` sign the `.ipa`
on the runner. The runner needs Xcode 27.1 or later selected (`DEVELOPER_DIR`,
or `xcode-select -s`); the build checks its destination’s SDK before compiling.
It also needs rustup and Bun 1.4.2; then `bun <exact2>/scripts/exact.mjs setup` installs the
pinned Rust with its iOS targets and the Hermes bundles, and `setup --check`
names what is still missing. An archive needs a square icon of at least
1024 px in `icons` (or `host.ios.icon`), which the build checks before it
compiles. An archive, and a simulator build with `--release` (the build that
ships), bake production trust: with an update store (`deploy.store.ios` not
`"0"`) each needs `EXACT_UPDATE_RECEIPT` or `EXACT_UPDATE_GENESIS=1`. A cold
build compiles Rust and the Swift host. Cache reuse depends on the source,
toolchain, paths and build mode; see [native CI preparation](#native-ci-preparation).
Export compliance is the developer's answer, so a production iOS or tvOS build
(`--release`, `--archive`, `--distribution`) writes `ITSAppUsesNonExemptEncryption`
into Info.plist only when `EXACT_NON_EXEMPT_ENCRYPTION` is `true` or `false` in its
environment (`false` when the app's only encryption is HTTPS and the system's);
without it App Store Connect asks at each upload. A development build never writes it.
Every Apple app bundle, including development and macOS builds, takes its version
and build number from `EXACT_VERSION` and `EXACT_BUILD_NUMBER` (`0.1.0` and `1`
when unset; an explicitly empty value stays empty). App Store Connect takes a
build number once for a version, so each upload needs a higher one.
Read both at runtime as the string fields `version` and `buildNumber` of
`exactDelivery()`, also mirrored in `state.delivery`. Native hosts read the
binary's baked values: an over-the-air update leaves them unchanged, and neither
enters the compatibility id. An older bake without the fields answers `""`.
The web JS build uses these variables at build time and answers `""` for each
unset variable. No `app.json` version key is defined. `exactDelivery().production`
is `true` in a binary baked with production trust (for example, Apple's
`--release`, `--distribution`, or `exact release`) and `false` in a development
build.

## Over-the-air updates

The [recipe](recipes/over-the-air-updates.md) goes from a signing key to an
update banner. This section is what it leaves out (LLP 1030).

`keygen` writes the private key to `~/.config/exact/keys/<id>.pem`, or to the
directory `EXACT_SIGNING_KEY_DIR` names; keep it out of the repository. A
binary takes only updates signed by a key its `app.json` named when it was
built. The web has no update store: there `exactDelivery().staged` stays
false.

### The origin a binary checks

A production binary checks the origin its build read from
`deploy.channels.<channel>` in `app.json`, and no other. Serve at that URL,
over HTTPS, the directory that `deploy --origin <dir>` and `push --origin
<dir>` write. In both commands `--origin` takes precedence over the manifest.
An `https://` origin is read-only to them, so `deploy --yes` and `push` need
`--origin` naming the directory it serves.

The channel `"appdrop"` is AppDrop's origin for the signing key, with no
account and no server. Run `deploy` and `push` without `--origin`: with a
directory they publish there while installed apps check AppDrop. AppDrop
holds update streams only, so `deploy` publishes no web root to it.

`bun exact.mjs serve --origin <dir>` serves a directory that `deploy`
published a web root to, on this machine (port 8765, loopback unless
`--lan`). To watch an app take an update locally, name that URL as the
channel before the release build (`"prod": "http://127.0.0.1:8765"`): a
binary built with another URL never asks the local server, and logs `exact
update: refused:` with the URL it could not reach.

### Starting and continuing a stream

A stream is a channel and a compatibility id: the binaries that run the same
updates. A release build for a platform with an update store bakes production
trust and needs one of two variables, and refuses without either:

- `EXACT_UPDATE_GENESIS=1` starts a new stream. The binary embeds sequence
  zero.
- `EXACT_UPDATE_RECEIPT=<file>` builds a binary for a stream that has
  releases. The file is a release record that `deploy` or `push` wrote,
  `<origin dir>/.exact/<channel>/<compatibility id>/releases/<release>.json`,
  and it must name exactly the plan and assets the build embeds. The binary
  embeds that release's sequence.

`deploy.store.<platform>` is `"A"` for a binary with an update store and
`"0"` for one without, on `ios`, `macos`, `linux` and `android`. The first
`ios`, `mac` or `linux` build of an app writes `"0"` for its platform, and an
absent `android` is `"0"`, so set `"A"` for each platform that takes updates
before its release build.

When `push` discovers a missing head with signed release history, it stops
before publishing to any stream. Run `deploy` to repair the head.
Keep the original production build receipt so `deploy` can restore missing
binary input evidence that matches the signed digest and byte length. On
Apple and Linux a fresh bake can also supply matching evidence. Deploy can
restore that evidence at the current sequence when the bundle is current.

### What `deploy` publishes

`deploy` builds each platform's update, which takes minutes, and its dry run
prints what each stream takes as an update and what needs a new binary. For
iOS it compiles the device target, which a Mac set up only for the simulator
lacks: `bun <exact2>/scripts/exact.mjs setup --check` names what is missing,
and `--platform <p>`, repeated, publishes only the platforms named.

`deploy` publishes the commit at `HEAD` of the app's repository and refuses
when the files it reads differ from that commit. `--dirty` publishes the
working tree instead, uncommitted edits and files git ignores included; the
table is marked `(DIRTY)` and lists them. An app outside the Exact checkout
is refused without `--dirty` even when committed, because ignored files count
as source: the `.exact/` its builds write, and generated files in the Exact
checkout. So commit, pass `--dirty`, and read the list. An app's first build
for a platform edits `app.json`: commit after it. `push` always publishes the
working tree.

### When a staged update applies

- `next-launch`: the next cold launch boots it; `deliveryActivate()` applies
  it sooner.
- `app-decides`: only `deliveryActivate()` applies it. No launch boots it,
  and `staged` stays true across launches.
- `immediate`: it applies as soon as a check stages it while the app is in
  the foreground, otherwise when the app next comes to the foreground.

Applying an update while the app runs (`deliveryActivate()`, `immediate`)
waits until no request is in flight when the update carries a data module,
as every update of an app with `app.ts` does. A resource still loading, a
mutation still sending and a stream that has not delivered its first value
are in flight; a stream that has is not. The running app keeps the ask and
applies the update when the requests finish or are cancelled, without a
second `deliveryActivate()`. The wait has no deadline and cancels nothing, so
a request that never ends, such as a socket that never connects, holds the
update for as long as the app runs. The next cold launch boots it under
`next-launch` and `immediate`; under `app-decides` it is still staged and
waits for `deliveryActivate()` again. An update without a data module applies
with requests in flight. Every host also waits while text is being composed,
a pointer is down or a gesture is under way, including an interactive Back,
and applies the update when that ends, without a second `deliveryActivate()`.

A check runs when the app comes to the foreground, at most once a minute,
and every `deploy.check.interval` seconds (900 by default) while it stays
there; never in the background. `deliveryCheck()` checks at once.

## Install a development build beside the release one (variants)

A variant is the app under another identity, so a development or staging build installs beside the store's
build instead of replacing it and its data (LLP 1030.008). Declare it in `app.json`:

```json
"env": ["API_BASE_URL"],
"variants": {
  "dev": {
    "idSuffix": ".dev",
    "name": "Shelf Dev",
    "icons": [{ "src": "assets/icon-dev.png", "sizes": "1024x1024", "type": "image/png", "purpose": "any" }],
    "host": { "ios": { "icon": "art/ShelfDev.icon" } },
    "env": { "API_BASE_URL": "https://staging.example" }
  }
}
```

and select it with `--variant dev` (or `EXACT_VARIANT=dev`; the flag wins): `bun exact.mjs ios --variant dev
--run`, `bun exact.mjs android --variant dev --run`, `bun exact.mjs mac --variant dev --run`, and the same on
`web`, `test`, `agent` and `verify`. No build selects a variant unless asked.

- `idSuffix` (required) is appended to `app.id`, and to `host.android.id` when declared. Every id derived from
  them follows: bundle and extension ids, App Groups, the Android package, data directories, the update store
  and the compatibility id. An App Clip id under `app.id` moves with it.
- `name` (required) is the launcher label, the home-screen and menu-bar name, and the executable's name. Two
  variants may not share a suffix, and no variant's name may install under the app's or a sibling's.
- `icons` replace every icon source of the app, `color_scheme_dark.icons` and Icon Composer's `host.ios.icon`
  and `host.macos.icon` included. Without them the variant keeps the app's icons.
- `host.ios.icon` and `host.macos.icon` name the variant's own Icon Composer project. It replaces the app's on
  that platform and wins there over `icons`, as the app's does. A platform the variant names no project for
  keeps the app's, unless the variant has `icons`. A variant has no other member of `host`.
- `env` gives values to settings the manifest's `env` declares. The order is the environment, the variant's
  `env`, `.env.local`, then `.env`.

A data source that names its own id (a Rust `fn app_id`, an app.ts `appId`) keeps naming the base app; the
build admits it as the variant's. A device build needs a signing profile for each of the variant's ids, as for
the app's; a Mac development build with extensions needs a team for them (`EXACT_IDENTITY=-` builds without
them). `deploy` and `push` take `--variant` and publish the variant's own update streams from the app's
deploy policy, never the web root or the association files, which list the app alone. `exact run`, `install`,
`uninstall`, `release` and `appstore`, `share`, and the Windows build take no variant. URL schemes, the
sign-in callback and the app's own document types are shared by both installs.

A brand that ships as its own store app is another app, not a variant: one directory and manifest per brand,
sharing its screens and data code through workspace packages ([one app under several brands](recipes/white-label.md)).

## Controls (Control Center, the Lock Screen, the Action button)

A control is a button the system draws outside the app; a press runs one of
the app's tools or opens one of its locations (LLP 1126.004). iOS and iPadOS
18 and later draw them: the build puts each offered control in the app's
WidgetKit extension, where the person adds it to Control Center, the Lock
Screen or the Action button. Android, the Mac and the watch do not yet.

Declare each in `app.json`:

```json
"controls": [
  { "kind": "refresh", "title": "Refresh", "symbol": "arrow.clockwise", "tool": "refresh" },
  { "kind": "inbox", "title": "Inbox", "symbol": "tray", "url": "/inbox" }
]
```

- `kind` matches `[A-Za-z][A-Za-z0-9_-]*` and is unique among the app's
  controls regardless of case. It is the control's identity: a placed
  control is found again by it.
- `title` and `symbol` (an SF Symbol name) are required; `description` is
  optional.
- Exactly one of `tool` and `url`. A `tool` names a `tool` line
  (`control-tool`) and needs no `intents.tools` face. It may not `destroys`
  (`control-destroys`) or take a required parameter (`control-arguments`);
  a press passes `none` for each optional one. A `url` is one app path
  (`/inbox`, not `//inbox`), and needs the navigation root and a declared
  route that matches it; a `notfound` route does not count (`control-url`).
- `authentication`, on a tool control only: `required` (the default) runs
  it only for the authenticated person, `local` only on an unlocked device,
  `always` on a locked one too (LLP 1133.023 D10).

The compiler walks the tool's action, the actions it calls, both arms of
every branch, the `then` of every mutation it sends, the resources it
refreshes, and the actions of the tasks and sequences whose gate or key reads
a slot it assigns, and records what it can reach: Live Activity starts,
updates and ends, microphone, camera and screen capture, speech recognition
on the microphone, a recorder, and a capture restart. A tool that can reach any
capture may not have an `intents.tools` face (`intents-capture-face`).

Each native binary's bake decides every control's placement, in this order,
and puts it in `compat.inputs.controls` and the compatibility id, so any
change to a row, its tool's signature or its effects is a new binary. The
web has no binary and its id does not move.

1. A host other than iOS and iPadOS (the Mac, Android, the watch, tvOS,
   visionOS, Linux, Windows): omitted, `control-host`.
2. A build that packages no control extension: omitted, `control-metadata`.
   The Apple build on Linux is one, since only Xcode extracts a control's App
   Intents metadata; so is `--extensions` without `controls`.
3. A `url`: offered from iOS 18.
4. A tool that is known to start camera, screen or speech capture: omitted,
   `control-capture`.
5. A tool that may record audio (the microphone, or a recorder): its tool
   must start a Live Activity that `live_activities` declares
   (`control-recording-activity`) and the app must grant
   `device.microphone-background` (`grant-needs`), or the bake fails; then
   omitted, `control-recording-unqualified`, until a locked iPhone has
   recorded through it.
6. A tool whose reach starts, updates or ends a Live Activity: offered from
   iOS 18, as a `LiveActivityIntent`.
7. Any other tool: omitted, `control-process`.

An offered control is a `ControlWidgetButton` in the widget extension, which
an app with controls and no widgets or Live Activities also gets, linking no
Exact runtime. Its WidgetKit kind is `<app id>.control.<kind>`. Its intent is
compiled into both the app and the extension and is not discoverable, so
Shortcuts and Siri do not list it. A URL control's intent opens the app and
navigates there; on a cold launch the location is the launch location. A tool
control's intent runs in the app's process, which iOS launches in the
background if it is not running, and presses the control there; the
extension's copy refuses without running anything.

A press enters only through a host's native entry, `Runner::press_control`,
with the compiled control's kind and digest; on Apple, the tool door's
`exact_tool` export takes it as op `press`, and the app's generated intent
calls `ExactTools.press`. It is for tool controls (a host opens a URL control
itself). The runner runs the tool as the person's action
(origin `action`), refused `unavailable` when the installed control's
digest is not the plan's, and `busy` while the same control's press waits or
runs. Its authority ends when its caller cancels it, when it ends, or 10
seconds after it starts running; work it started runs on without it. The
agent's `call` cannot press: it enters as `tool` whatever `--door` names.

`state.controls` lists each control with its `tool` or `url`, its `digest`
and this binary's `placement`, as its bake recorded it: `{"offered": [{"from":
"18.0", "intent": "tool"}]}` or `{"omitted": "control-host", "why": …}`. The
web, which compiles no control, says `absent`:

```
bun scripts/agent.mjs ios --app <app> "state controls"
```

A qualification build (LLP 1126.004 §6d) offers on iOS what items 5 and 7
omit only for want of a device run, so the device run can be made:
`EXACT_QUALIFY_CONTROLS=1 bun host/apple/build.mjs <app> --ios --device --run`.
Such a placement carries the omission it lifts, `{"offered": [{"from":
"26.0", "intent": "tool"}], "qualifying": "control-process"}`, so its
compatibility id is never an ordinary build's; a bake with production trust
(`--release`, `--archive`, a deploy) refuses the variable. A plain tool's
intent then asks to run in the app (`supportedModes = .background` from 26,
`allowedExecutionTargets = .main` from 27); a recording tool's
(`"intent": "recording"`) is an `AudioRecordingIntent` and a
`LiveActivityIntent`, and while its press runs, the tool and the `then`s of
its sends may call `getUserMedia` for audio alone and `startMediaRecorder`
with the app not in front. A press never asks for the microphone: with its
permission not yet granted, `getUserMedia` fails `NotAllowedError`.
Recorder's `record` control is the device run's subject.

## Apple bundle metadata

Exact writes Info.plist keys and entitlements from grants, named manifest fields,
packages and build inputs. Entitlements remain derived (LLP 1069.008 D4).
The `host.ios` and `host.macos` dictionaries `infoPlist` and `entitlements` refuse
all supplied keys together; their values never enter a bundle. Diagnostics give
specific grant advice only where tests apply those lines, including companion
grants, through the bake and platform writer. Other derived keys cannot be set
there. For a key Exact never writes, record a `bun exact.mjs feedback note` from
the app directory naming the key and what the native module needs.

### Language and direction on macOS

The Mac window and a document's unsaid layout direction follow AppKit's process
direction. An Arabic strings table alone leaves them left to right on an English
Mac. Set `host.macos.layoutDirection: "rtl"` in `app.json` to mirror the window,
split view, toolbar and sidebar, even with an English table. Omission or `"auto"`
keeps AppKit's direction. A launch argument naming `AppleTextDirection` or
`NSForceRightToLeftWritingDirection` takes precedence over the setting.

The setting applies before AppKit starts and requires a new binary. The host
checks both the application's direction and each window's
[`windowTitlebarLayoutDirection`](https://developer.apple.com/documentation/appkit/nswindow/windowtitlebarlayoutdirection).
A mismatch prints a development diagnostic; the agent's `state.layoutDirection`
reports the request, launch override, applied directions and whether both are
right to left. `language.dir` reports the document's applied layout direction.
iOS and the web continue to take their direction from the strings table.

## Apple privacy manifests

Apple builds write `PrivacyInfo.xcprivacy` before signing each app, App Clip,
watch app, extension and Exact framework. Declarations follow the APIs linked
into that bundle, so an unused category is omitted. Exact declares:

| Category | Exact's use | Reasons |
| --- | --- | --- |
| File timestamps | Metadata for app-container files and documents the person selected | `C617.1`, `3B52.1` |
| System boot time | Elapsed time between app events and timers | `35F9.1` |
| User defaults | App-owned settings and registrations | `CA92.1` |
| Active keyboards | Selecting the enabled emoji keyboard for an emoji input | `54BD.1` |

The motion and altimeter packages also declare `8FFB.1` for converting sensor
events to timestamps. Other packages declare their own uses beside their Apple
implementation. The build includes only the packages the app links. Hermes is
statically linked; its surviving calls are covered in the containing bundle.
Apple builds bundle SQLite. On iOS, tvOS and watchOS, its filesystem-type
probe is replaced with an empty result and filesystem-specific locking is
disabled: neither has an approved disk-space reason. This omits SQLite's
FAT/exFAT inode workaround and proxy locking, which app-container storage does
not need. macOS retains both. Recheck the probe's uses when updating SQLite.
Exact declares no disk-space use; an unaccounted-for disk-space import fails
packaging on platforms subject to required-reason reporting.

Put an Apple-format `PrivacyInfo.xcprivacy` beside your `app.json` to add your
app's declarations. A package puts the same file beside its Apple implementation's
`exact.json`; a prebuilt framework includes its manifest in its own resources.
API reasons are merged without duplicates for categories the owning binary uses. Tracking is combined
with Boolean OR; domains and collected-data purposes are unioned, and linked-data
and tracking-data flags keep any `true`. An app cannot erase a dependency's
disclosures. Each supplied reason must be one of Apple's approved codes for its
category; an unknown code or a code from another category fails the build.
The files are binary inputs: the build captures their bytes once, including
absence, and uses that snapshot through packaging. Changing one requires rebuilding and reinstalling, not an
over-the-air update.
Only your app's API reasons accompany Exact frameworks (including native modules
and launch services) and the watch companion. They must cover any additional APIs
your native code uses. Your tracking and collected-data disclosures stay on the
app, App Clip and host sample bundles.

Exact itself collects no data and adds no tracking, tracking domains or
`NSPrivacyCollectedDataTypes`. Your app must declare its own data collection and
tracking, and any additional reasons for its native code. A tracking-consent
grant does not infer these declarations. Review the finished manifests against
[Apple's approved reasons](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype)
and [privacy-manifest requirements](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files)
before submitting. Required-reason reporting applies to iOS/iPadOS, tvOS,
watchOS and visionOS; collected-data reporting applies to all platforms. Mac
builds also write manifests, under `Contents/Resources` for apps and
`Versions/A/Resources` for frameworks, following
[Apple's bundle layout](https://developer.apple.com/documentation/bundleresources/placing-content-in-a-bundle).

## Open the same development URL on Apple hosts

Start `bun host/web/dev.mjs` and open a printed URL in your browser. Its build
outputs belong to that app's loop and target. `test web`, `verify web` and
automatic builds for `agent web` each use a temporary directory of their own,
removed when the command exits; they can run beside the loop.

Build and launch the app's native client with that same address:

```sh
bun host/apple/build.mjs --run --url http://127.0.0.1:8765/
bun host/apple/build.mjs --ios --run --url http://127.0.0.1:8765/
bun host/apple/build.mjs --device --run --url http://192.168.1.20:8765/
```

For a phone, replace the example with the server's reachable LAN or HTTPS
URL. Device builds require a connected, provisioned phone. `--url` overrides
`EXACT_DEV_PLAN` for this launch; it does not change the app's production origin.
An external app uses these commands with `EXACT_APP_DIR` set as usual.
Plans and assets reload through the native URL loader. Admitted TypeScript
module clients reload logic on web/macOS/iOS/Linux/Android (`bun exact.mjs
android --run --url <url>`); declared Rust modules reload on
web/macOS/iOS/Linux using the executor selected below. Once built, an agent can drive
the same URL with `bun scripts/agent.mjs macos --url http://127.0.0.1:8765/ tree state logs`
(also `web`, `ios`, and `linux`; Linux polls the same URL after first pixel).
The Go/custom-client sequence
is in LLP 1030.000 §7.

An Apple client pins the native program the server names for its baked cohort
and target: the binary of that target's build receipt while its inputs are unchanged.
A web-only rebuild keeps the connection and carried state. Changed native binary inputs,
or no receipt for the client's cohort, keep the last good generation on screen until
the host is rebuilt and relaunched.

A native client finds a generation through its envelope, `exact.json`: a
pointer card of at most 64 KiB that names the plan, and names the asset
cards' file, `assets.json`, by URL, SHA-256, byte length and card count. The
asset index holds one card per asset (name, URL, SHA-256, length) and the
entry plans' `plans` map, at most 4 MiB, about 20,000 cards; its card URLs
resolve against the index's own URL. The web build prints both sizes against
their bounds with the asset count (`exact.json 0.6 KiB of 64.0 KiB, assets.json
101.2 KiB of 4096.0 KiB for 537 assets`). When publishing a stream, an
oversized envelope reports its size, limit and largest field. An oversized
asset index reports its size, limit and asset count.
Every client checks the index against the envelope's card before it reads a
card, and a signed head covers the index through its digest. A signed stream
head also names the frozen cohort receipt by URL, SHA-256 and byte length.

Agent replies carry `generation`, `epoch`, `incarnation` and `clock` (LLP 1029.000.002 D10).
`generation` starts at 1 and advances once for each committed replacement or
reboot, including a return to an earlier plan. A refused candidate leaves it unchanged.
The JS target carries it through its development document reload. `epoch` counts commits
that change the kernel and continues across replacements. `incarnation` counts resets
of one kernel; a replacement creates another kernel, so its incarnation can repeat.
Use `generation` to wait for a replacement. The terminal's JSON inspection replies
(`layout`, `plan`, `runner-tree` and `node`) carry these tags too.

`state.replacement` is null before a replacement while none waits. Afterwards it reports `at`, the
replacement's clock in milliseconds, and `dropped`, the root slots that could not carry.
`pending` names what a waiting replacement waits for (such as `"composition"`, `"pointer"`,
`"gesture"`, `"module"` or `"requests"`), or is null.
Each entry has `what: "state"`, `name`, and `why`: `"no place"` when the slot is gone,
or `"type changed"` when its value does not fit the new declaration. The replacement's
first journal line gives the same report.

A component outside a conditional, loop, or lazy tab panel keeps its state when
a use of a different component is inserted above it. Uses of the same component
under one parent are matched in source order. A parent element with a unique
literal `id` keeps its place when reordered. Moving the component under a new
parent starts fresh state.

When a native plan reloads, a virtualized list at the same source place opens
at the row key and offset the reader last saw. If that key is gone, the list
uses the next surviving key from the old window, then the previous one.
It returns to the start if none survive. A reader at the start stays there,
and a list following its end stays at the end.

The JS development reload restores reading anchors and focus, including a field's
selection range and direction when its text matches. A field may regain focus after
its resource answers; reader focus cancels pending focus and selection. Scrolling
caused by reader input cancels only the axes that move, including an
ancestor when scrolling chains into it. Records match by tree path and type, so
structural edits can prevent a match.
Pending records cross another reload with their remaining time. Restoration ends
when every record has applied or been cancelled, when the page leaves, or after
ten seconds. Autofocus stays suppressed
until then, and the browser's history scroll-restoration setting returns to its
prior value.
Browser Undo history is lost when the document reloads.

Agent sessions use `exactTime()` launch facts `seed: 1` (LLP 1069.007), `locale: "en-US"`,
`timeZone: "UTC"` and `epochAtZero` 2026-01-01T00:00:00Z (LLP 1027.000.000 D3, with the
zone's `utcOffset` at that instant, answered again when the virtual date crosses a DST change) on every host. Override them at session setup with
`bun scripts/agent.mjs web --seed 42 --locale fr-CA --time-zone America/Toronto --epoch 2026-09-21T14:13:20Z tree`
(`--format-locale en-KR` gives `formatLocale` a Region of its own; unsaid, it is the locale)
or `open({host, seed: 42, locale: "fr-CA", timeZone: "America/Toronto", epoch: "2026-09-21T14:13:20Z"})`;
a test file writes them as launch lines (`epoch "2026-09-21T14:13:20Z"`, `time-zone "America/Toronto"`,
[authored tests](contract-grammar.md#authored-tests)), which override the flags.

On macOS, `scale` enlarges the pixels of an `iframe` after WebKit lays out its guest.
Line breaks stay fixed, but enlarged text can look soft. Size the containing box to
reserve space for the transformed frame.

```text
box width=(320 * z) height=(200 * z) overflow="hidden"
  iframe "https://example.com/" width=320 height=200 scale=z transform-origin="0% 0%" border="none"
```

For sharp text, `iframe "https://example.com/" width=320 height=200 zoom=z` enlarges
the frame's own lengths and tells WebKit to render the page at that zoom. Line breaks
can change even when the guest still reports a 320 × 200 CSS pixel viewport. On macOS,
the time of the first displayed frame at the new size or zoom remains unknown to
Exact. A WebKit snapshot completes when its image is ready, even in a hidden window.
[Apple’s snapshot API](https://developer.apple.com/documentation/webkit/wksnapshotconfiguration/afterscreenupdates)

On the web (same-origin frames), macOS and iOS, `tap live at 40 80` presses
40 points right and 80 down from the top left of the iframe named `live`.
The driver converts that point to the guest viewport at its current scale.
Use `tap live {"x":20,"y":40}` for guest viewport CSS pixels, or
`tap live {"selector":"#save"}` to press an element. These taps report
`delivery: "synthetic"`. Guest listeners see `event.isTrusted === false`.

A drive can fail fetches by URL prefix (LLP 1103): `--fail-fetch <prefix>` (repeatable) arms one before the
first data load, `fail fetch <prefix> [times <n>]` and `pass fetch <prefix>` arm and clear one mid-drive (a form of `prefer`:
`{"op":"prefer","faults":{"fail":…,"times":…}}` or `{…{"pass":…}}`), and `state.faults` lists each prefix's `times`, `left`, `hits` and
`armed`. Native carriers pass the launch table as `EXACT_AGENT_FAIL_FETCH`, one
`<prefix>[\t<times>]` line a fault. Web carriers put only the marker `?failFetch=1`
in the URL and install the table before page code, only in the top-level document
on the original app origin. Child frames and pages on other origins receive no
table. Carriers also send it as a base64 `X-Exact-Faults` header on the app origin's
top-level document requests, so development
render servers can apply it before fetching data. A marked render with a missing or
invalid table returns 400 without fetching data. Fixture renders preserve unrelated
answers and route statuses, and are not shared between requests. Production builds
ignore these inputs. `--fail-fetch` with `--test` arms every test.
A Mac agent drive never takes the focus: the app is an accessory, never activated (a `type` makes its window key
without activating it), so AppKit draws its controls as in an inactive app. On the agent's clock its window, and a picture-in-picture
panel it opens, sit one level below every other app's windows, so they never show over them; `--timing platform` orders them in front. `EXACT_AGENT_ACTIVATE=1`, which only
`scripts/measure-controls.mjs` sets (never a default), activates that one run, and the app that was in front is put
back when it exits, however it exits (LLP 1121 stage 9 F2).
A data module's `crypto.getRandomValues()` and `crypto.randomUUID()` draw from the seed too, so every drive with the same seed
repeats the same identifiers: against a live backend that deduplicates on one, give each drive a seed of its own.
A Rust data crate asks whether its draws are seeded with `exact_data::crypto::seeded(&store)`; code that makes a key refuses while they are. The hosts keep a
production bake out of agent mode; a development build, a release build under another trust, a terminal entry and an embedded session honour `EXACT_AGENT=1`.
Seeds are whole numbers from 0 through 2^32 − 1, the range every host draws (LLP 1116.005 D5); an epoch is an ISO date, Unix milliseconds, or `now`: the machine's clock,
read once by the driver at launch, for a drive against a live backend on real time (`snapback4 dev`). Native carriers pass
`EXACT_AGENT_SEED`, `EXACT_AGENT_LOCALE`, `EXACT_AGENT_TIME_ZONE` and `EXACT_AGENT_EPOCH`
(milliseconds), and `EXACT_AGENT_FORMAT_LOCALE` when given; direct agent launches can set these too. Web agent pages accept
`?agent=1&seed=42&locale=fr-CA&timeZone=America/Toronto&epoch=1790000000000` (and `&formatLocale=`).
`agent` alone, or `agent=` with its empty value, asks for agent mode as `agent=1` does; any other value, `agent=0`
among them, asks for none, and a production build ignores the parameter (LLP 1069.007 D2).
The driver supplies its own defaults unless an option (or `open({env: ...})`)
overrides them. `clock +N` moves the date (`epochAtZero + performanceNow()`); `state.time`
reports all five facts. Before the first host report, the runner
also supplies usable `en-US`/`UTC` and seed 0. Ordinary launches draw their seed
from secure platform entropy. A development reload retains that launch's seed.
Linux takes its locale from the first nonempty `LC_ALL`, `LC_MESSAGES`, or `LANG`, and
`formatLocale` from `LC_TIME` by POSIX's precedence (`LC_ALL`, then `LC_TIME`, then `LANG`; with
none set, `formatLocale` is the locale),
normalizes POSIX names to BCP 47 (`C`/`POSIX` use `en-US`), and reads the zone from
`TZ` when it names a zoneinfo entry, otherwise the system's IANA zone (UTC fallback).
Windows takes `locale` from the first preferred UI language and `formatLocale`
from the user's regional format. A 12/24-hour choice that differs from that
region's default adds `-u-hc-h12` or `-u-hc-h23` to `formatLocale`. It reads the
system's IANA zone unless `TZ` names one. Windows settings and time-change
notifications refresh these facts without changing the launch seed. Agent
launches keep the drive's supplied facts.

A drive changes visibility with the `prefer` name/value pair `visibility-state hidden`
(`"prefer visibility-state hidden"`), and returns with `"prefer visibility-state visible"`.
The hidden step commits page visibility and the runner's background fact. `prefer frozen true`
separately commits `exactPage().frozen` and its defined transitions, such as stopping a
transport sequence. Neither preference suspends the driven process. A later clock step still
fires due timers, hatch frames and `after` callbacks, and replies and native-topic announcements
keep the host's ordinary pumping. Use these facts to test authored reactions; qualify process
suspension, coalesced delivery and waking on a real host outside the drive (LLP 1012 §1).

`bun scripts/agent.mjs trace --pid <pid> --out snapshot.json` asks a running development
Mac or simulator app to save its trace without taking ownership of the process.
The file includes `tree`, `state`, `layout` and `observation`. Tree and state read the
runner directly, without guest DOM inspection or the carrier's drain; layout reads
mounted host geometry. Each read is labelled passive. The frame ring ends before
these observations, and their wall-clock bounds, clock mode, scheduling and substitutions
are recorded. This path never seeks or settles the app.

`bun scripts/agent.mjs macos --app <app> --attach [pid] tree state logs layout "screenshot now.png"`
reads the development Mac app a person already has open, with its own window, store and wall
clock (LLP 1012.000). Nothing is built or launched. The person chooses **Develop ▸ Enable Agent
Attach** in the app, then **Allow** in the sheet the driver's first connection raises; the window
shows **Agent Attached** while the driver is connected. **Detach Agent** ends the connection and
closes the listener until it is enabled again; quitting the app or closing the window ends it too.
Each new connection asks again. One driver attaches at a time, to one window: `--session <label>` names
the window when several are open, and a PID chooses among several running copies.
The connection only reads: `tree`, `state` (masked, as the tree shows a password field), `logs`,
`layout`, `perf` and `screenshot` (the window server's picture of the window, which is not brought
forward). The app refuses `clock` with `live-clock` and input with `input-not-granted`, and the
driver refuses launch flags such as `--storage`, `--live` and `--url`. Only a Mac app that
`host/apple/build.mjs` builds to run carries the listener; a release, archive or embedded build has
none of it.

### Inspect work and native presentation

`perf <target> during "<operation>"`
reports the work that operation caused in the named subtree. Development measurements
include virtual-row construction, rebinding and retirement; Apple also counts kernel
layout API calls and native host work. Read the reported scope and measurement start:
process-wide native counts can include other sessions, and a reset makes a delta unavailable.
Missing coverage is not a zero.

On Apple, `"perf frames live 500"` observes display-link callbacks for 500 ms without
advancing or changing the current clock. Its reply identifies scheduling, substitutions
and dropped records. An ordinary launch on the wall clock is available on macOS and iOS with
`bun scripts/agent.mjs macos --live --storage latency "tap change-station" "type station-search Palo" state`
(use `ios --live` on a simulator or `ios --device --live` on a phone). The driver launches and closes this process and keeps input
available. `--live` uses normal scheduling and the wall clock: `performanceNow()` and
timers advance in real time between operations, and `perf frames` records real frames.
On iOS, `--touch platform` sends the gesture through XCTest so UIKit delivers the scroll callbacks.
For example, `bun exact.mjs agent ios --device --live --touch platform "tap s drag 0 -300 over 100" "perf frames health scroll"`
drags the scroll view named `s` on a phone. Unlock the paired phone, enable Developer Mode and UI Automation,
and allow local network access if prompted. The Mac needs an Apple Development identity and a provisioning profile
covering the phone and `com.exact.touches.xctrunner`. On the wall clock use `perf frames live <ms>` to observe an interval.
`--storage` chooses the drive's
scratch store. Set
`EXACT_STORE=real` in the launch environment to use the app's real secret and preference
stores and secret presence. App files still follow the drive's scratch-store setting
(`--storage`) and are unavailable when no scratch store is configured. `state.storage` and the observation's `store` describe those
app files. `state.observation` reports clock mode, scheduling and substitutions.
`--timing platform` alone keeps agent execution substitutions. A substituted run does not establish production-equivalent performance,
and callback timing does not prove compositor presentation. `layout` distinguishes target
and presented geometry; an unavailable presentation remains explicit.

On Android, `bun scripts/agent.mjs android --live` drives the reader APK on platform time
with layer playback enabled. `clock` and `screenshot … over … every …` are refused. See
[Driving the reader](../host/android/README.md#driving-the-reader-llp-1107-d1) for the recording command.

## Generate TypeScript data-source types

The compiler can derive the logic interface from a Contract's source signatures:

```sh
cargo run -q -p contract -- types path/to/app.contract -o path/to/app.contract.d.ts
```

In `app.ts`, use `import type { Sources, Answer } from './app.contract.d.ts'`.
Annotate the provider map as `Sources`; each function takes `(args, store, storage)` and
returns its declared result or a Promise of it. A `source` declared in `app.contract`
is the runner's to answer, so `Sources` leaves it out ([Declared
sources](contract-for-agents.md#declared-sources)). An `Answer` dispatcher can call
`sources[source](args, store, storage)` without casts. `bun install --frozen-lockfile` installs the pinned `tsc`.
An answer is held to its shape exactly, on every executor: each declared field
present (an `undefined` one is missing, as `JSON.stringify` leaves it out), none
undeclared at any depth, each value of its declared kind. TypeScript's excess
property check misses a spread (`{ ...row, amount }` keeps `row`'s other fields),
so the refusal is at run time, the same on the web as on a device:
``` `ledger` answered outside its shape: field `days`: field `transactions`: field `cents` is not in the shape ```,
which the agent's `state` shows under `failed` while the resource keeps its last value.

Use a distinct filename: adjacent `app.ts` shadows an `app.d.ts` import.
Generated declarations are build artifacts, not files to commit. A development
build writes them beside `app.ts` for an editor: the web build and the native
development bake both do.

For a mixed Rust/TypeScript app, annotate the map as `TypeScriptSources` and
the dispatcher as `TypeScriptAnswer`, imported from the same declaration:

```ts
import type { TypeScriptSources, TypeScriptAnswer } from './app.contract.d.ts';
const sources: TypeScriptSources = { greeting: () => 'Hello' };
export const answer: TypeScriptAnswer = (source, args, store, storage, native) =>
  sources[source](args, store, storage, native);
```

The app's `build_mixed_with` bake derives these types from its `rust_sources`
argument; no ownership list goes in `app.json` or TypeScript. Every remaining
provider is required. Run `bun exact.mjs contract build app.contract` to obtain
the narrowed declaration beside `app.ts`: its TypeScript check runs the app's
Cargo bake, which writes the declaration before checking `app.ts`, even if
the check fails. Native bakes use it too, and a development producer keeps
the roster from the previous bake's `rustSources` receipt. Rebuild after
changing Rust ownership. `Sources`, `Source`, `Args` and `Result` still
describe every app-owned source, including Rust's.
Standalone `contract types` has no composer and treats every app-owned source
as TypeScript's; for a TypeScript-only app the two provider types are equivalent.

Development builds also expose `resourceStatus`: opaque request and displayed-answer
generations, the last successful generation, pending status, and `displayed`
(`fallback`, `current`, `retained`). `argumentsEqual` compares full arguments using
the runtime's equality; `null` means the answer's full arguments are unknown or
the value is a fallback. `resourcesMeasuredFrom` names the observation's start on
the runner clock. Identifiers are local to this runtime and observation, and
unobserved seed requests have null generations. They never govern retention or
authorization. The driver joins each resource declaration to its file and location
using that reply’s plan digest; a missing or incompatible map is reported as unavailable.
`placeholder` identifies a declared stand-in; `keptSeed` is the
runner's boot admission mark, consumed at activation. The JS target reports
`keptSeedSupported: false`: its rendered-document checkpoint is separate from a
persisted boot answer. `failures` counts refused answers since observation began, with
`first` and `last` on that clock. A success clears `failed`, preserving the count.
On the web JS target, list members use `name[id]` in `resourceStatus`, `failed`
and `failures`. The opaque member id stays the same through reordering and is
retired on removal; it is local to the page. Singleton keys stay their resource
names. Each status's `resource` still identifies the declaration. The driver's
source location names the declaration for every member. In `perf.resources`,
each list member carries the same key as `member`, while `name` remains the
declaration name. `expect perf query <resource>` sums its members' asks.
A child's resources (LLP 1035.005.004) are not in `resources`: `owned` names
them as their component declares them, and `instances` lists each live
instance's under its instance path (`/2:0/5=3`, a region's arm or a row's key
per step), by the declared name. In `perf.resources` a child's resource counts
every instance's asks under the declared name, which `expect perf query`
takes.

### What a data module can use

Every build type-checks `app.ts` and what it imports with one configuration,
`js/bake/src/typescript.mjs`: the web build (alongside bundling, ~40 ms for a
small app, about nothing on the build's wall time) and the native bake, with
the same capture, entry and diagnostics, so an `app.ts` that builds on one
host builds on all of them and a type error stops every build the same way.
`strict`; target and library ES2023, plus ES2024's `Object.groupBy`,
`Map.groupBy`, `Promise.withResolvers` and well-formed strings; `WebWorker`'s
web APIs, never the DOM's UI types (`Document`, `HTMLElement`, `Window`).
Imports may name `.ts` files (`import { day } from './dates.ts'`) or leave the
extension off; `tsconfig.json` contributes only `paths` and `baseUrl`.

Native hosts freeze the built-in objects and their prototypes. A module cannot
replace `Array.prototype.map` or `Math.max`. Assignments on an inheriting object
still create its own property: an Error subclass can set `this.name`, and an
object can set its own `toString`. Configurable, writable data properties on
built-in prototypes become accessors to preserve those assignments while
keeping the built-in fixed.
The globals beyond the language are the browser's on the web and these on Hermes:

| Available on every executor | Notes on Hermes |
| --- | --- |
| `fetch`, `Headers`, `Response` | Grant-checked; `signal` aborts, and the host cancels the request (below). A request `body` accepts a string, `URLSearchParams`, or bytes from an `ArrayBuffer` or view (such as `storage.fs.readFile`'s bytes). Unless `headers` already names a `Content-Type` (case-insensitively, even an empty value), a string adds `text/plain;charset=UTF-8` and `URLSearchParams` adds `application/x-www-form-urlencoded;charset=UTF-8`, including empty strings and forms. The prelude adds none for bytes or an absent body; on Apple, URLSession adds `application/x-www-form-urlencoded` to an untyped POST with a body, including bytes. As Fetch does, a body on a GET or HEAD, or a view on a `SharedArrayBuffer` or resizable buffer, rejects, and a detached buffer sends no bytes. Natively, in the wasm target's module realms and with `exactStream` on the web JS target, a `ReadableStream` request body rejects with a `TypeError` before sending; collect it with `await new Response(stream).arrayBuffer()` first. Natively a request holds at most 4 MiB, its URL, headers and body together: a larger one is refused before anything is sent (`FetchError`, kind `Refused`, `native request exceeds 4 MiB …`), so send a larger body from a file with `exactBodyFrom`. `exactBodyFrom: "app:/tmp/…"` sends an app file instead: the host opens it when it sends the request, under `fs.read` as `readFile` is, and streams it from disk as the request goes out, with the size it had when opened as its `Content-Length`; the bytes never enter JavaScript, so a 2 MB upload costs the answer's step nothing, and a 300 MB video costs half a megabyte of memory (LLP 1133.008.000 D4). Exact sets no bound on its size; the server answers 413 for one it will not take. A file cut shorter while it is sent fails the request, naming the path; a file changed in place while it is sent sends a mix, so leave it alone until the reply. Two hosts still read the file whole and refuse one over 64 MiB, naming why: the web, for a file the app wrote with `writeFile` or `appendFile` (a picked file, a recorded video or an `exactBodyTo` reply is the browser's own Blob, and streams), and the asynchronous executor (`EXACT_HTTP_ASYNC`), with kind `Unsupported`. With `body`, or on a GET or HEAD, it is a `TypeError`, and a missing file or a denied path rejects the fetch (`FetchError`, kind `Refused`, naming why) before anything is sent, as does a host with no app files (a drive with no scratch store) or none built yet (Windows), with kind `Unsupported`; Exact adds no Content-Type (Apple's POST default above still applies; LLP 1108 D6 R2). An older binary's prelude would send it as an empty body, so a native app with a TypeScript data module carries `typescriptRuntime` in its compatibility id: the first build after this change gives every such app a new id, and each needs a new binary once. `exactBodyTo: "app:/cache/…"` is the reverse, for a download: the host writes a 2xx reply's body into that app file as it arrives, under `fs.write` as `writeFile` is, and the bytes never enter JavaScript, so a large file costs neither the answer's step nor the engine's 64 MiB heap. The body goes to a staging file beside the target, which replaces the target only once the body is complete; a failure, an abort, the deadline or a body over the bound leaves the target as it was. The bound is `exactIndependentHttp.maxResponseBytes`, which may then be up to 1 GiB, or 1 GiB when none is given. The `Response` resolves once the file is in place, with `status`, `ok` and `headers` as received, an empty body, and `exactBodyTo: {path, bytes}`, the bytes written (absent or `null` on any other response). A reply that is not 2xx is not written: the file is untouched and its body is read as usual, at most 1 MiB, so the server's error is readable. A path that is not a plain `app:/` one, a HEAD, or `exactStream` beside it is a `TypeError`; a denied path or a missing folder rejects the fetch (`FetchError`, kind `Refused`) before anything is sent, and a host with no app files does so with kind `Unsupported`. It may come with a `body` or `exactBodyFrom`. On the web the body becomes a Blob the browser holds, stored in the page's store in one transaction (LLP 1108 D6 R2, 2026-10-26). An older prelude would ignore it and read the body into memory, so it moves `typescriptRuntime` to 4. `redirect` is Fetch's: `follow` (the default), `error` (a redirect rejects the fetch), or `manual`, which answers with the 3xx itself; natively its `Location` header is readable (an OAuth step whose redirect names an app scheme), while a browser gives its opaque redirect (status 0, no headers); any other word is a `TypeError`. An older prelude would ignore it and follow, so it moves `typescriptRuntime` to 2: such an app needs a new binary once more, and a cohort on an older binary gets no bundle that relies on it. `cache` is Fetch's `"default"` (unsaid), `"no-store"`, `"reload"`, `"no-cache"`, `"force-cache"` or `"only-if-cached"`, controlling the native cache for a resource's `GET` and the browser's cache on the web; any other value is a `TypeError`, and a module that names it needs `typescriptRuntime` 11. A `Response` has `status`, `ok`, `statusText`, `headers`, `bodyUsed`, `text()`, `json()`, `arrayBuffer()`, `clone()`, as Fetch's does: a body reads once, a clone of a read one is a `TypeError`, a fetched reply's headers and a redirect's take no change, and a response's own refuse `Set-Cookie`; no `body` getter, `Request`, `Blob` or `FormData`. `new Response(body, init)`, `Response.json` and `Response.redirect` make a reply as Fetch does (a demo's server in the app), from a string, `URLSearchParams`, bytes or a `ReadableStream`. A stream brings no Content-Type; `text()`, `json()` and `arrayBuffer()` read its `Uint8Array` chunks to the end, rejecting any other chunk with a `TypeError` and propagating stream errors. A locked or already-read stream is refused; `bodyUsed` tracks reads and cancellation, and `clone()` tees an unread, unlocked stream so each response reads independently. A module that names `.clone`, `statusText` or `bodyUsed` needs `typescriptRuntime` 7, as an older prelude makes one with no status and no body, reads a body again, or throws. `Response`, `ReadableStream`, `TransformStream`, `CompressionStream` and `DecompressionStream` require runtime 8 so stream bodies cannot reach an older prelude that stringifies them. Natively and in the wasm target's module realms, `Response.redirect` takes only an absolute URL (a relative one is a `TypeError`; the web JS executor resolves it as the browser does), and natively a fetched reply's `statusText` is `""`; in the wasm target's browser realms, which are not hardened, a `Response`'s headers take no guard, and an app that replaces an intrinsic there (a prototype's method, `Math`, a `WeakMap`'s `get`) changes what `Response` does, as it changes what any code in that realm does, while what an app gives a `Response` (an object's own properties, its iterator, its conversion hooks) is read as Fetch reads it on every host |
| `structuredClone` | No transfer list |
| `ReadableStream`, `WritableStream`, `TransformStream`, their readers, writers and controllers, `CountQueuingStrategy`, `ByteLengthQueuingStrategy` | WHATWG Streams, with `pipeTo`, `pipeThrough`, `tee`, `for await` and `ReadableStream.from`. No byte streams: `type: "bytes"` and a BYOB reader are a `TypeError` |
| `DecompressionStream`, `CompressionStream` | WHATWG Compression's `"deflate"`, `"deflate-raw"` and `"gzip"`, worked by native code in the turn that writes each chunk: 8 MB of JSON inflates from 1.4 MB in about 25 ms of that turn's budget, and writing it in slices with `await scheduler.yield()` between spreads it over turns of their own. Its errors are the standard's `TypeError`s: corrupt data, data after the end, input that ends before its end at `close()`, a chunk that is not an `ArrayBuffer` or view. One stream may span many writes: Discord's `zlib-stream` (one zlib stream across a socket's messages, each ending in a sync flush) inflates message by message, each write's output queued once it settles; keep a `read()` pending, as a write waits for one |
| `TextEncoder`, `TextDecoder` | `TextEncoder` emits UTF-8. Hermes 0.4's built-in WHATWG decoder keeps the browser-style encoding labels, including UTF-8 and UTF-16LE/BE, plus `fatal`, streaming and `ignoreBOM` behavior |
| `URL`, `URLSearchParams`, `atob`, `btoa` | |
| `crypto.getRandomValues`, `crypto.randomUUID`, `crypto.subtle` | Inside an answer. Under the driver every host draws a repeatable stream from the launch seed (`agent --seed <n>`, 1 unless given), so every launch at one seed draws the same bytes, and `logs` says so at the first draw (the internal wasm web target draws it unsaid); a server that remembers a client by an id drawn here meets the same client again. A native drive of the real device (`EXACT_DEVICE=real`) draws the system's. `subtle` digests (SHA-256/384/512) and ECDSA P-256 keys (LLP 1069.005), and refuses the rest by name |
| `AbortController`, `AbortSignal` | `AbortSignal.timeout(ms)` is an answer's timer (below) |
| `setTimeout`, `clearTimeout` | Inside an answer, on the app's clock (below) |
| `queueMicrotask`, `Promise` | |
| `scheduler.yield()` | Ends the answer's turn; it goes on in a new one with a budget of its own (below). Outside an answer's turn it resolves at once; for an answer let go it rejects with an `AbortError`. `scheduler.postTask` refuses |
| `Intl.NumberFormat`, `Intl.DateTimeFormat`, `Intl.Collator`, `localeCompare`, `toLocaleString` | On Android, the pinned v6 bundle carries receipt-bound English ICU data: Hermes's ICU backend supplies `Collator` and Ibex supplies `NumberFormat` and `DateTimeFormat`. Date formatting needs an explicit timestamp. No `Intl.PluralRules`, `RelativeTimeFormat`, `ListFormat`, `Segmenter` or `DisplayNames` (Apple's engine; Linux's is built `--intl`). `Intl.Locale` is the prelude's on every Hermes host (the engine has none): a tag parsed and canonicalized as Chrome does, its options and getters, and `getWeekInfo()` with Chrome's `{firstDay, weekend}` from CLDR's week data, by the tag's region, its `-u-rg-`, or its language's likely region (two-letter languages and a few others; another reads Monday and a Saturday-Sunday weekend), and `-u-fw-`. It has no `maximize`, `minimize` or other `get…()` list, does not canonicalize aliases (`iw` stays `iw`, `en-840` keeps `840`, and its week is then the default, Monday, where Chrome's is `en-US`'s), a formatter given a `Locale` object rather than its string uses the default locale, and `structuredClone` copies a `Locale` as `{}` where Chrome refuses it. Apple's `ja-JP` long date puts a space before the weekday (`10月6日 火曜日`, Chrome `10月6日火曜日`). Apple's engine has no `notation: "compact"`: the prelude formats its short display as Chrome does for a decimal in en, en-GB, en-IN, de, fr, fr-CA, es, es-MX, it, pt, pt-PT, nl, sv, da, nb, fi, pl, ru, uk, cs, tr, ja, zh, zh-TW, ko, hi, he, id, th and vi (and their regions); `compactDisplay: "long"`, another locale or a compact currency is printed in full and said once in the logs. Apple's engine rounds a tie to even and ignores `roundingMode` (`$0.82` for 0.825 dollars, Chrome `$0.83`); the prelude rounds the number first, as Chrome does: from its shortest digits, by the formatter's `roundingMode` (half away from zero unless it names another), which `resolvedOptions()` then reports; a compact it prints in full rounds so too, to its style's digits (`$0.83` for a compact 0.825 dollars). A `unit` keeps the engine's rounding, since that engine converts it to the locale's unit (`0.622mi` for 1 kilometer). Apple's engine has no `formatToParts` |
| `console` | To the runner's logs after each answer and reply, including refused calls; available through the agent's `logs` on native hosts. Each line the app's code logs also goes to the system's log: on Apple the unified log, subsystem the app's bundle identifier, category `console` (`console.debug` at debug level, `console.error` at error, the rest at the default level); on Android logcat, tagged with the app's package name (`adb logcat -s com.example.app`), at the method's priority; and on Linux and Windows standard error, as `console: …`. Every build writes them, where anyone with the device can read them. Outside a drive the web writes it to the browser's console too |

A `net.fetch` line names one origin (`https://api.example.com`), every host
under one domain (`https://*.example.com`), or a whole scheme, written as CSP
writes a scheme source: `net.fetch https:` or `net.fetch http:` admits any host
at any port of that scheme, for a server the person names at run time (a URL
they type, a local server on a port chosen at launch). `net.websocket` takes an
origin, a subdomain wildcard (`wss://*.example.com` or `ws://*.example.com`),
or `wss:`/`ws:` the same way; its scheme is `wss` or `ws`, and a line with
another (`net.websocket https://example.com`) fails the build. In either family's subdomain wildcard, `*` must
be the whole leftmost label, the suffix must have at least two labels and
must not be an IP address, and userinfo, paths, queries and fragments are
refused (a trailing `/` is allowed). Only hosts strictly under the suffix
match, at the grant's scheme and port (explicit or the scheme's default);
the bare suffix and sibling domains do not. A scheme no line names is refused (`wss:`
does not admit `ws:`). The platform's own refusals still apply: App Transport
Security refuses `http:` beyond the local network unless the platform's
`appTransportSecurity` in `app.json` (`host.macos`, `host.ios`) sets
`allowsArbitraryLoads`, which turns it off for every connection the app makes
and which App Store review asks the app to justify (LLP 1030); a browser
refuses `ws:` from an `https:` page except to loopback. A render-server page's
`connect-src` carries the scheme source as written. The render server itself
doesn't hold a scheme-wide `net.fetch` line: a source that needs one keeps its
placeholder in the rendered page, and the client fetches it after adoption.

For `fetch` with the default `redirect: "follow"`, native hosts check each
redirect target against the source's `net.fetch` grants before contacting it.
The web lets the browser follow and checks only the final response URL. On
render-server pages, CSP `connect-src` allows the page's own origin and the
app's granted origins, while request admission uses the source's grants. A hop
outside the page origin and app grants is blocked before contact (`Network` on
web, `Refused` on native). An app-granted but source-ungranted hop can still
receive a 307 POST body and return to a source-granted final URL; web succeeds
while native refuses before that hop. Static and dev pages have no CSP: an
ungranted intermediate origin can receive the request (including a POST body
on a 307); if the chain ends at a granted origin, the
web succeeds while native refuses at the intermediate hop. Without CSP, an
ungranted final URL is refused after contact. CORS or another browser failure
can also hide the response and yield `Network`. Exact API grants do not
sandbox page code: it can call browser `fetch` directly, subject to the
rendered page's CSP where one is served.

On native hosts, a resource's plain HTTP requests overlap, up to six at a
time, and their replies are delivered in the order asked (LLP 1162 D1, D2).
A TypeScript resource's answer hands over every request it has made, so
`Promise.all` overlaps too. Reads wait for earlier sends, storage steps and
native continuations; those wait for the reads before them. In the sequence,
requests with `exactBodyFrom` or `exactBodyTo` still run one at a time.

Resource `GET`/`HEAD` reads retain the bound of 128 queued, running and
unconsumed requests, with at most 64 MiB of waiting request buffers. A
resource's other methods, including a GraphQL `POST`, overlap too, but count
against the 16-request effect bound and wait when it is full. They are never
aborted when their answer is let go. Other ordered work retains that
16-request admission bound and rejects explicitly when full; the runtime
never restarts the whole source on that refusal, since earlier turns may have
written already. For example, 20 resources fetching once beside one resource
fetching 20 times can all wait and answer. A failed resource keeps its last
value; `refresh` asks again. See LLP 1041 §8.4 for the byte reservations and
cancellation policy.

A mutation's answer hands over one request at a time, in order. Sends of the
same `parallel` mutation overlap, up to three at a time on the six shared
workers. They wait for everything before them except the other sends of
their mutation; later work waits for them. Replies retain the order asked.
The six workers, a source's scoped requests and the launch's preloads share
one connection pool natively (LLP 1162 D5), so a read reuses a connection
another left open to its origin, and on Apple their reads of an HTTP/2 or
HTTP/3 origin share one connection. The ordered lane keeps a pool of its
own, and a stream opens its own connection.

Native hosts keep a resource's `GET` reply when it says how long it is fresh
or carries a validator (LLP 1162 D4): in memory, 16 MiB per runtime, gone with
the process. An entry answers only a request equal in method, URL and every
header; a stale reply is revalidated when it has a validator. `refresh` and a
declared `refreshes` go to the network and replace the entry, and a successful
write to a URL drops that URL's entries. A browser uses its own cache.

The render server keeps one cache for all its renders, as an RFC 9111 shared
cache (LLP 1153.001 D7): a `GET` reply that gives an explicit lifetime
(`s-maxage`, `max-age` or `Expires`) and says none of `no-store`, `private`,
`no-cache` or `Vary: *`, up to 256 replies and 64 MiB, 8 MiB each. A request
with `Authorization` is given or stores only a reply marked `public`,
`s-maxage` or `must-revalidate`, and a redirected reply is not kept. Within
`stale-while-revalidate`, a stale reply is given while one refetch replaces it.

A read whose request and reply may both be handled out of order can leave
the sequence altogether: `fetch(url, { exactIndependentHttp:
{ maxResponseBytes: 4194304 } })` in TypeScript, or
`Request::get(url).independent_http(4 << 20)` from a Rust source. A native app
then runs up to six such reads at once, within their own budget of 128
requests and 68 MiB, room for a 64 MiB read beside the largest request. A
running read counts its ceiling, and one that does not fit waits for the reads
before it, so keep the ceiling near the largest reply you expect. An open
stream (`exactStream`) counts twice its ceiling on the same budget for as long
as it is open, and a read whose ceiling does not fit beside the open streams is
refused when it is made or while it waits if a later stream takes its room.
A download written to a file (`exactBodyTo`) counts
only the 1 MiB it can hold, whatever its ceiling. A browser has no lanes and schedules every fetch itself, so
there only the ceiling applies. On every host a reply larger than
`maxResponseBytes` fails as `refused`. The ceiling is 1 byte to 64 MiB (1 GiB
with `exactBodyTo`), and anything else is a `TypeError`. A `GET` is
not independent by default: ordinary resource reads already overlap, while
an independent reply settles as it arrives. Leave writes, and any read whose
parse depends on an earlier reply, in the queue. Storage steps and native
continuations cannot opt out of it.

A Hermes data module has a 64 MiB heap limit by default. Terminal heap exhaustion
fails its outstanding requests and tool invocations with
`held more than the 64 MiB a data module's heap may hold`. The executor
destroys that runtime, including its timers, streams and queued storage
work. The next new request or canvas call loads a fresh runtime; failed
requests are not retried automatically. Other modules keep running. A native
storage write already submitted may finish, so an OOM does not establish
whether it committed. Its completion is discarded, and queued JavaScript
writes never start.

On Hermes each turn of an answer has 100 ms of its own time by default: its call, or its
resumption after a `fetch`, storage step or yield it awaited, until it awaits
the next. Its own time is the calling thread's CPU clock where the platform keeps
one, so a turn that waits on a blocked host call spends little of it; a turn may
still take at most ten budgets (1 s) of wall clock. A turn over either is refused
as an overrun (`Overrun`: `` `work` took 904.5 ms of its own (CPU) time (2283.7 ms
of wall), over the 100 ms budget for one turn; … ``), and the message names the
repair: `await scheduler.yield()` every few thousand items of a long loop
([the guide](contract-for-agents.md#data-requests-and-side-effects) shows one).
For synchronous work that cannot yield, `app.ts` can export a constant map
such as `export const turnBudgets = { signOrder: 400 }`. A budget is 100 to
1000 ms inclusive and names a source owned by TypeScript in this app. It
applies to the source's initial call, continuations and stream messages. Its
wall-clock limit is ten times its budget. A snapshot keeps its own deadline.
A resource so refused keeps its last value and reads as pending, and is asked
again at 250 ms, then at 1 s; a third overrun in a row fails it as any refusal
does. A mutation fails on its first overrun. `failure(x).code` is `error`
and `failure(x).reason` is `overrun` for either failure. The app can show a
retry button for a failed mutation. A development build also reports the
overrun in the host console (the app’s logcat tag on Android). The Apple
development window shows it until the next input or plan replacement.
The runner never retries a stream message
that overruns.
The host asks a yielded answer again at once, in its place among the source's
requests, with no work and no clock, so an agent's `clock settle` runs it to its
end without moving the clock. The browser keeps no such budget and
yields to its own `scheduler.yield` where it has one (Chrome does), else to
a task posted on a `MessageChannel`. Bun has no `scheduler`: a Bun test of a
module that yields defines one (`globalThis.scheduler ??= { yield: async () => {} }`).

An answer may wait on a timer of its own (LLP 1027.000 D1, amended
2026-10-28): `await new Promise(r => setTimeout(r, 180))`, a retry's back-off,
`fetch(url, {signal: AbortSignal.timeout(5000)})`. A timer is due on the
app's clock, the agent's under the driver: `clock +180` or `clock settle`
fires it, and a jump lands each timer's turn before it goes on, so three
chained sleeps of 100, 200 and 400 ms all run within `clock +700`. It belongs
to the answer that armed it: arming one outside an answer (module
initialization, a stream's mapper, an overlay, background work, or after the
answer has given its value) throws, and an answer that gives its value, fails
or is let go clears the rest, whose callbacks never run. At build each timer
fires at once, so a source that waits still bakes.

A timer raced against a fetch its answer awaits (`Promise.race`, or a
`setTimeout` that calls `abort()`) runs on time, a module's call included:
natively the fetch then runs on as module work, its late reply delivered
with no answer awaiting it, and an aborted one is cancelled. In a
worker-placed source, an ordered set's member and the web's wasm target, an
answer waits on one thing at a time: the fetch goes first and the timer runs
only after it lands (LLP 1027.000 D1, amended 2026-11-04); the journal says
so once per answer, except for a set's member on the main thread.
For a deadline on every host, give the fetch `exactTimeout`: the host keeps
it on real time, not the app's clock. `AbortSignal.timeout(ms)` is one too:
on every host a fetch carrying it takes its time left as a real-time
deadline, unless an earlier `exactTimeout` is, and if the request is still in
flight when that passes, the signal aborts with `TimeoutError`, whether its
answer is over or not. Natively and on the wasm target, whose module realms
run the same prelude, a fetch whose signal has more than an hour left takes an
hour as its deadline, which ends it as a `Timeout`, the signal unaborted; the
web JS target waits for the signal. On the web JS target the signal is also
a timer on the page's clock while its answer runs, so under the agent
`clock +N` fires it there too. An answer waiting both for
another answer's work (a shared fetch) and for a timer of its own is woken by
whichever comes first on native hosts, for a source on the main thread, and on
the web JS target; in the wasm target and in a worker-placed source it wakes
only when its timer is due.

An aborted fetch rejects at once with the signal's reason, and the host
cancels its request, as a browser does, on every target and whatever its
method: one already sent has its connection closed, one not yet sent is
withdrawn, and its reply, if one comes, is never read. The answer that awaited
it goes on with the rejection then, natively too, without waiting for the
reply. The one exception is native: a write that runs to its end after its
answer was let go (module work, LLP 1146 D2) rejects in the module turn that
follows its cancel, not in the turn that called `abort()`, so what its
continuation writes to the store stands even when that turn's commit is
refused, and an answer that aborts it and then awaits it resumes in a later
turn. On the JS target the fetch is the browser's and rejects at once, and
its continuation runs after the aborting turn's commit, outside it, where a
store write is kept whatever that commit does, so the end is the same. So an app's own rule of one request in flight may count an aborted
request done when its fetch rejects: the request is closing and its reply will
never be read. The one caveat is the cancel's own latency: the host's own
capacity (its connections, per-origin places and buffers, and on some hosts
the ordered lane) comes back when the transport has actually ended, a moment
later, so the next request may wait briefly behind the closing one. On a
worker-placed source, whose answers run one at a time behind an ordered fetch,
another answer can abort it only once its turn comes: give a fetch another
answer may abort `exactIndependentHttp`. A request already sent may still have
reached the server and counted against its rate limits, and a reply it never
reads can't say so (a 429 included): treat an aborted request as an unknown
outcome. A stream (`exactStream`) runs until the runner lets its ticket go, as
before. Natively and on the wasm target, a fetch made by a module callback
that answers no resource or mutation (a payment sheet's or a tool's handler) is
not cancelled yet: it rejects at once, and its request runs to its end.
This is the TypeScript runtime 5: an older prelude would leave an aborted request
running, and would reject an aborted kept write in the turn that aborted it, so
it moves `typescriptRuntime` to 5, and a native app whose TypeScript data module
makes an abort signal needs a new binary once more.

A poll loop (`for (;;) { await sleep(5000); … }`) belongs in a Contract
`task`, not in an answer: an answer that always has a timer armed never
settles, so every `clock settle` stops at its bound and says `settled: false`.

Not in a data module, by design (LLP 1027.000): `setInterval`,
`requestAnimationFrame`, `performance.now()`, `Date.now()`, `new Date()`
without a value and `Math.random()`: time and seeds are source arguments.
Every executor refuses them by name, with the same message, on first use:
Hermes, the web's module realm, and the web build, whose bundler gives the
app's own modules guarded `Date`, `Math`, `Intl`, timers and `performance` in
place of the page's (LLP 1027.000 D3), so an app that reads the clock fails in
the web loop as it would on a device. The type check cannot see the difference, but every
build refuses a direct use in a module `app.ts` reaches, by file and line
(`logic.ts:2:28: Date.now() is unavailable in data sources; …`), so a test that
runs the module under Bun, which has no such guard, cannot hide it.
Literal bracket access such as `Date['now']()` and `globalThis['setInterval']()`
gets the same diagnostic as dot access, including in the web build's summary.
Aliases and dynamic property keys still reach the runtime guard. In a running
session, a refused TypeScript answer fails its resource and keeps its last value
or placeholder; the typed input or action still commits. `failure(x).code` is
`error` for the module's own error, unless its nonempty `reason` supplies a domain code. A failed send ends unsent, retains its previous
result and runs no `then`, including when queued. The journal names the failure.
Module initialization, bake and replacement validation still refuse failures.
Development JS builds name a derive
whose value fails its type check and report failed resource/source dependencies
that it read.
ES2024's resizable `ArrayBuffer`, shared memory and the RegExp `v` flag are not
in Hermes, so they are not in the library.

The dispatcher receives storage as its fourth argument:
`answer(source, args, store, storage)`. `store` remains the grant-checked secrets
interface. Native `storage.fs` provides byte-oriented files under `app:/data`,
`app:/cache`, and `app:/tmp`; `storage.sqlite` provides databases, prepared
statements, and batch transactions. Declare grants such as `fs.read app:/data`,
`fs.write app:/data`, and `sqlite.open app:/data/notes.db` in `app.ts`’s exported
`grants` string. The TypeScript module always receives `storage`, including
if no storage grants are declared. Once storage is available, ungranted
operations reject with `Unavailable` and code `denied`, without loading the
browser storage adapters. A drive without a scratch store still gets `agent`
for app-storage writes and SQLite, and `ENOENT` for a file read, as an empty store
answers; `doc:/` handles do not need that store.
Generated declarations export Ibex2's `Storage` and related types; Rust sources
can use the same implementations through `ibex2::host`.

Native hosts, including Apple, bundle the pinned SQLite (3.54.1); the web's
storage worker runs the same release's official WebAssembly build. Apple's
minimum supported OS versions have older SQLite libraries than rusqlite
requires (3.45.3), so linking the OS library can prevent the app from launching.
Rust and TypeScript storage share the bundled provider. Swift code in the same
process must not open a database the Rust provider owns: Apple's launch-service
dylibs still link system SQLite (`host/apple/launch-parts.mjs`), and closing a
file through another copy can release the provider's POSIX locks
([SQLite's locking rules](https://sqlite.org/howtocorrupt.html#posix_advisory_locks_canceled_by_a_separate_thread_doing_close_)).
A native production build links SQLite only when the app grants `sqlite.open`
or names `"sqlite"` in the manifest's `link` list. Naming it without the grant
links unused code: adding `sqlite.open` changes the compatibility identity and
requires a new binary, even when SQLite was linked. Development builds and
builds with unknown grants keep it. The bake refuses SQL grants that exceed
the linked composition with `Unlinked("sqlite")`.

The host sets a database's options (foreign keys on, temporary data in memory),
and app SQL may not change them or reach another file. `ATTACH`, virtual tables,
`load_extension`, `BEGIN`, `COMMIT`, `ROLLBACK` and `SAVEPOINT` (use `transaction`)
and every `PRAGMA` but `user_version`, `application_id`, `table_info`, `table_xinfo`,
`index_list`, `index_info`, `foreign_key_list`, `foreign_key_check`,
`integrity_check` and `quick_check` are refused with `Unavailable`; the message names
what was refused, the rule, and in a `transaction` the statement's place and SQL.
Remove such lines from generated migrations, except `PRAGMA foreign_keys` in the mode
below.

A migration that rebuilds a table other rows reference (create a new table, copy the
rows, drop the old one, rename the new one) cannot run with foreign keys enforced:
dropping the old table deletes its rows first, which fails for a child's reference or
runs the child's `ON DELETE CASCADE` or `SET NULL`. SQLite's procedure turns
enforcement off around the transaction and checks every reference before it commits
([Making Other Kinds Of Table Schema Changes](https://sqlite.org/lang_altertable.html#otheralter)),
and `transaction(statements, { foreignKeys: 'check' })` does that for you: the host
turns enforcement off before the transaction begins, runs the statements, runs
`PRAGMA foreign_key_check`, and commits only if no row names a missing row; then it
turns enforcement back on, whether the batch committed or not. While the batch runs,
dropping a table runs no `ON DELETE` action, so every child row and reference is kept.
The batch's own `PRAGMA foreign_keys` lines, which migrators write around a rebuild,
are accepted and do nothing. A migration as Drizzle generates it runs unchanged:

```ts
await db.transaction([
  'PRAGMA foreign_keys=OFF',
  'CREATE TABLE `__new_t` (`id` integer PRIMARY KEY, `name` text, `extra` text)',
  'INSERT INTO `__new_t`("id", "name") SELECT "id", "name" FROM `t`',
  'DROP TABLE `t`',
  'ALTER TABLE `__new_t` RENAME TO `t`',
  'PRAGMA foreign_keys=ON',
].map(sql => ({ sql })), { foreignKeys: 'check' });
```

A row that names a missing row fails the batch, which rolls back whole. The message
counts violations, one per broken reference (a row with two counts twice), and names
each by the row's table and rowid and the table it references, for example
`foreign key check failed, the batch is rolled back: 1 foreign-key violation: plain
row 11 (references t)`, or the first three and how many more. It covers
temporary tables as well as the database's own. The check reads rows, not the schema. A parent made again without a primary key or unique index on the
columns its children name fails the check with SQLite's `foreign key mismatch`. A
parent dropped for good passes when no row names it, and leaves its children's table
refusing every write (`no such table`) until the parent exists again, so drop or
rebuild those children in the same batch. Other batches on the database wait for this
one, so none runs with enforcement off. Options of `undefined` or `null` are none;
any other option, or a `foreignKeys` other than `'check'`, is refused before SQLite
runs, as any invalid argument to storage is, with `Unavailable` and code `'failed'`,
so a misspelt option never runs a migration enforced. `foreignKeys` is read once,
from the object or its prototype (a class's getter counts). A native binary's prelude
older than this option would drop it and run the rebuild enforced, and the options
may be data (a migration read from JSON), so a TypeScript data module whose `grants`
include a `sqlite.open` line needs `typescriptRuntime` 10, whether or not it uses the
option: `exact push` refuses such an update for a cohort on an older binary, which
needs a new binary first. A web page always runs its own current code. A Rust source's `sqlite.transaction` request
has no such mode and refuses a `foreignKeys` field.

Hosts configure app-specific directories after first pixel. Files and databases
survive module reload; temporary storage is a directory under the app cache,
without an automatic cleanup guarantee. Agent mode does not open disk storage.
Bake rejects storage calls with `Unavailable` (`code: 'bake'`), and `native.call`
and `native.later` the same way: the build links no native module. A resource whose
answer fails for it, caught or not, compiles no value: it shows its placeholder
(its `else` row, or the type's zero) and is asked when the app runs, on every build.
So a source that asks the app's module needs no bake code of its own
(`native!.watch('state'); return native!.call({op: 'state'});`). Catch it only to answer
something better than the placeholder. Browser storage uses app-scoped IndexedDB files
and SQLite WASM in a dedicated worker, loaded on the first database operation.
Use HTTPS or localhost for Web Locks. Data persists across reloads within the
same browser origin, subject to browser storage retention and quota policies.
An open database exclusively locks its file; conflicting opens or filesystem
mutations return `Unavailable` with a busy message. Agent mode skips storage.

In `app.ts`, a storage refusal is an `Error` with `kind: 'Unavailable'`, a `code` and a
`message`. The code is the same on every host; the message says more and may
differ. Storage itself is unavailable: `'bake'` (the build compiles no answer
that reads storage or calls the native module; show a placeholder), `'agent'` (a scripted drive that names
no scratch store, `--storage <name>`), `'unsupported'` (a host with no app
storage). The operation was refused: `'denied'` (the grants do not cover it),
the filesystem's POSIX name (`'ENOENT'`, `'EEXIST'`, `'ENOTDIR'`, `'EISDIR'`,
`'ENOTEMPTY'`, `'EBUSY'`, `'EACCES'`), `'full'` (256 operations already wait behind the
one in flight: the module's storage queue is full, LLP 1097), else `'failed'`. Branch on the code, never the
message: `catch (e) { if (e.code === 'ENOENT') return empty; throw e; }`.

On macOS, Linux and Windows, `desktop:/`, `documents:/`, `downloads:/`, `music:/`,
`pictures:/`, `videos:/` and `screenshots:/` name the person's well-known
folders (LLP 1173 D4). Declare `fs.write pictures:/` for the whole folder, or
`fs.write pictures:/Exports` for a subtree. `fs.read` and `sqlite.open` lines
on these namespaces are refused at build.

These paths take only `writeFile`, `atomicWriteFile`, `mkdir`, `realpath`, and
`copyFile` from an `app:/` file with `fs.read` on its source. Files are created
exclusively: an existing name returns `'EEXIST'`, including an atomic write or
copy. For `mkdir`, every component below the root must be new; an existing
component returns `'EEXIST'` without creating anything below it. Retry with
another name. `realpath` uses the write grant and returns the
native path of an existing item. Reads, listing, watching, appending, replacing,
renaming and removal are refused. No link below the resolved root is followed.
On macOS and Linux, a link at or above the root is followed only where the
app's write grants cannot replace it. Windows opens the OS-selected root,
including UNC paths, and refuses name-surrogate reparse tags below it while
admitting cloud placeholders.

```ts
export const grants = 'fs.read app:/tmp\nfs.write screenshots:/';
await storage.fs.copyFile('app:/tmp/capture.png', 'screenshots:/Shot.png');
```

Each operation resolves the folder again. A role whose settings the app can
write returns `'EACCES'`, including after relaunch: those settings may already
contain the app's changes. This covers all seven roles on Linux and only
`screenshots:` on macOS. Settings with multiple hard links also refuse, since
another name may be app-writable. Storage and process admission share this
resolver and retain every session's restrictions across sessions and reloads.
This prevents a write to the settings from widening the folder grant. macOS uses the account's folders
(`videos:` is Movies); `screenshots:` reads Screenshot's `location-screenshot`
preference, then `location`, then defaults to Desktop. A destination that is
not an existing folder returns `'ENOENT'`. Linux reads `$XDG_CONFIG_HOME/user-dirs.dirs`
(default `~/.config`), expanding only `$HOME`; unset, relative and disabled
entries return `'ENOENT'`. Linux's `screenshots:` is the Pictures folder's
untranslated `Screenshots`, and returns `'ENOENT'` when absent. Under the agent,
all seven resolve inside `--home`, or its temporary home, never the person's folders.
Links below the drive's home are refused. Windows asks `SHGetKnownFolderPath`
on each use, following redirected folders, and creates Screenshots if Pictures
exists. Controlled folder access may refuse a write with `'EACCES'`.

Windows folder writes are implemented but remain unqualified on Windows.
Run `cargo run -p exact-windows --example known-folders` there with a
OneDrive-backed Pictures and a UNC-redirected folder. The probe reports each
folder's path, attributes, reparse tag, exclusive creation and no-replace
rename, and removes its temporary subdirectories. Append `-- pictures OnlineOnly`
with the name of an existing online-only child to exercise placeholder recall.
Claim the Windows cell only
after that probe and an app drive copying an `app:/` capture into `screenshots:/`,
retrying the same name (`'EEXIST'`) and confirming the first file's bytes.

The web, Apple mobile platforms and Android refuse these paths with
`NotSupportedError` (`code: 'failed'`):
`well-known folders are unavailable on this host`. Platform access denial is
`'EACCES'`. A sandboxed Mac build admits Downloads, Pictures, Music and Movies
through their entitlements and refuses Desktop, Documents and Screenshots.

A drive's app storage is a scratch store it names (`--storage <name>`), on a
native host `default` when it names none, or none (a web drive naming none).
A native one is kept between drives until a drive passes `--reset-storage`, which
empties all of it (`app:/` files, SQLite, kept state, secrets) at the app's boot
and refuses a store reached through a symbolic link; one drive holds a native store
at a time. On the web it is Chrome's profile for the name and its page's origin,
empty each drive unless `--keep-storage` (a Firefox or WebKit drive's is its own). A drive that compares with a
reference starts empty on every host unless `--keep-storage`. An authored test gets a fresh
one of its own, removed after it. If Chrome (including its headless shell) survives an
interrupted drive, the next drive refuses before resetting or opening its profile and
names the profile and PID; close that browser or use another store name. A drive also refuses when the profile's
owner cannot be verified. A data module's `secret.keep` rides that same
store: files under the named tree on Apple and Linux, `localStorage` in the named
web profile ([web sessions](recipes/web-sessions.md)). Apple native modules receive
the same named data, cache and temporary roots across launches. A fresh drive clears
them; the picker's temporary documents alone are cleared on each launch. A web drive
with no `--storage` refuses app storage (`storage is unavailable
in agent mode unless the drive names a scratch store (--storage <name>)`; a file
read rejects `ENOENT` instead, as an empty store's) and keeps
no secrets. The driver's `state.storage` says
which (`{available: false, code: 'agent', message}` or `{available: true,
store}`), and the journal says `storage refused (agent): …` the first time
a read or write is refused (a refused read finds what an empty store would: nothing,
code `ENOENT`, so a source that catches it falls back silently), which the driver repeats once as a
note, after the first op that met it. A Rust module's storage request in such a drive is answered
with the same message, never refused outright (trivia F7).

An answer waits for the storage it awaits, and only that (LLP 1097). Storage
it starts and does not await finishes after it, as a page's does, on every
host: the answer is given when its value is ready, and the write lands behind
it as the module's background work. A save that has started lands on every host
(kanban F22, drums R10). Hermes drains the microtask checkpoint before
replying, including when the answer is a synchronous value, so a save chained
behind a promise that is already resolved is issued as that answer's work and
is not left with nobody to run it. So an editor answers from memory and saves
in the answer, unawaited:

```ts
edit(store, args) {
  song = apply(song, args);
  storage.fs.atomicWriteFile(PATH, new TextEncoder().encode(JSON.stringify(song))).catch(note);  // started now, not awaited
  return song;
}
```

Every storage operation of a module runs in one queue, in the order it was
issued, one at a time, so a read issued after a write sees it. Call storage in
the answer and let the queue order it, rather than chaining it on a promise: a
`.then` issues its write only when the promise before it lands, so a read
issued meanwhile overtakes it. Two answers interleave at their awaits, as two
async calls do on the web: writes that must stay together go in one
`transaction` (SQLite) or one operation. `clock settle` and a test's `reload`
wait for the module's storage; `clock +N` names what is left (`background`,
beside `inflight`), and `state.background` counts what landed and failed.
If a native storage wait times out, its answer fails: its unsent operations
are rejected, and so is any it issues afterwards except a lock's `unlock()`,
which runs once the operation in flight completes. That operation, which can
still write, keeps its place in the queue until it completes, delivered as
background work; other answers' operations, let go or not, stay queued behind
it. A worker placement, which has no background delivery, keeps waiting for
that completion. Image work whose right to write was revoked releases its place
immediately. Retiring a worker stops its wait and drops its unsent work;
storage already submitted to the host may still complete.
Every failed storage operation, every unhandled rejection and every `console`
line of the module reaches `logs` (`storage failed: …`, `data: unhandled
rejection: …`, `console: …`). Storage is refused, with its line, during module
evaluation (no answer has begun) and at bake; `fetch` and `native.call` are
refused in background work, which no answer waits for (a `fetch` belongs in an
answer). A storage or `fetch` call made when no answer is in flight is refused
and logged, never silently dropped. A worker-placed source still answers once
its storage has landed. A quit waits for the storage to land (macOS, five
seconds; a Linux exit, five; iOS holds a background task while it lands; a
native dev restart, one second); a web page being unloaded may lose what is in
flight, as any page does. A native answer that saves is a reply on real time:
under the driver it lands at the next `clock` step, not with the input (`tap`
then `expect` reads the state before it; on the web build an answer given at
once is there already), and on a device it lands a few milliseconds after the
input.

A data module hears its errors as a page does: `addEventListener("error", …)`,
`addEventListener("unhandledrejection", …)`, `onerror`, `onunhandledrejection`
and `reportError` exist on every host. A listener runs when a page's would. A
failed answer is not an `error` event; call `reportError(e)` in the answer's
`catch` to send it there. Natively, a listener may `fetch` a plain HTTP URL
while it runs, and no answer waits for that request. Fetch in the listener
itself, not after an `await`, and catch the request's failure. Every unhandled
rejection still reaches `logs` (`data: unhandled rejection: …`), and one that
no listener cancels also reaches the system's log. What the answer does not
catch fails it (`failure(x).code`, above) and reaches `logs`. The
[recipe](recipes/report-errors.md) sends them to a server.

A newer send may replace a mutation's reply. An operation already issued still
runs, in the order it was issued; the replaced reply is dropped, and that
answer's Store writes are not the live answer's. A send dropped before it has
issued storage does not run. A replaced answer waiting on a `fetch` is not
stranded: on the web build (the JS target) its fetch completes and the code
after the `await` runs; natively and in the web's wasm module realm the fetch
rejects with a `FetchError` of kind `Aborted` (the request may already have
been sent), so a `catch` or `finally` runs. A stream's fetch never settles. Its reply is
dropped either way; a mutation that needs every reply is declared `queue`. Unloading finishes storage the module already
started, within a second, and drops what has not begun. Reads remain
replaceable. An answer the runner lets go between storage steps (one asked in a
commit that is refused, a read whose arguments changed or that a `refresh`
replaced) still runs the steps it began, and the chain behind them, to
their end; only its answer is dropped, so
serializing storage through one promise chain composes with `refreshes` and
fast-changing arguments (ledger F12, minesweeper F10).

A `fetch` waits as long as the platform lets it (URLSession's 60 seconds
without data on Apple), holding an ordered source's lane meanwhile. Give it a
deadline with `exactTimeout`, in milliseconds (1 to 3600000), for the whole
exchange, headers and body: `fetch(url, { exactTimeout: 10000 })`. When it
passes the request is cancelled and the fetch rejects with a `FetchError`
whose `kind` is `Timeout` (`the request timed out after 10000 ms`). The same
holds on Apple, Linux, the web's wasm host and the web build; a stream
(`exactStream`) takes none, except an upload (below). Any other stream stays
open as long as the server holds it, however quiet it is, as in a browser: no
limit of the platform's ends it once it is open. Natively a WebSocket has
about 10 s to connect and 15 s for its handshake, and so has an HTTP stream
for its connection and its response's head off Apple.

So a stream can stay open and carry nothing. Give it `exactIdleTimeout`, in
milliseconds (1 to 3600000), and the host ends it when nothing has arrived
for that long: `fetch('wss://…', { exactStream, exactIdleTimeout: 60000 })`
(LLP 1016.000 D11). The count starts when the stream is asked. On a
WebSocket it starts again at the opening and at each message; protocol
pings and pongs do not count. On an event stream or a byte stream it starts
again at any bytes of the body, an event stream's comment lines included
(the `: keep-alive` a server sends every 15 s to hold a quiet stream open),
but not at the response's head; a byte stream waiting for its mapper to take
what it holds is not counted as quiet. Nothing the app sends counts. The
mapper is then called once more, with `{ type: 'error', kind: 'Timeout',
message: 'nothing arrived for 60000 ms' }`, after what it had already
received (each message, in order), and the stream is over; the app opens it again by refreshing the
resource. The time is real time on every host, under the driver too, so
`clock +60000` ends no stream: test it with a short timeout against a peer
that stays silent, and wait with `clock until state …`. A long poll and an
upload do not take it, nor does a URL of another scheme: it is a
`TypeError` there, and the web's wasm host and a Rust module across the
logic ABI refuse a request that has it, by name.

A stream's request is never sent at build, so an option the host refuses on one
(`exactTimeout` on a stream, `exactIdleTimeout` out of range) does not
fail a native build: the resource shows its placeholder and fails at launch
with that error, as on the web. The native build warns once per resource and
refusal, naming the rejected option. Valid streams and ordinary storage or native
module deferrals produce no warning. An error the source throws itself still
fails the build.

A long poll, a request the server holds until something changes, says so with
`exactHeld: true`: `fetch(url + '?wait=25', { exactHeld: true })` (LLP
1016.000 D9). It is pending as any request (`pending(…)` reads it), but no
agent clock waits on it: not a jump, not `clock settle`, not `clock data`. Each
of their replies names it (`held: 1, heldTargets: ["changes"]`) and `state.pending`
marks it `held: true`; its reply lands when the server answers. The answer's
work after that reply (another fetch, its parsing) is waited on as usual. A held
ask must be safe to supersede: a send to its resource meanwhile replaces it, and
its late reply is dropped, as a long poll that is simply asked again. A drive
that expects a push (after an action the server answers on the long poll) waits
for it with `clock held [<target>…] [<n>s]`: real time passes, the clock frozen,
until each held request has had one reply (20 s by default), then `clock data`. Mark only
the asks that wait (a first ask answered at once need not be); a stream never is
(`exactStream` stops counting when its first message commits), and with
`exactStream` it is refused; an upload (`exactBodyFrom`) is never held either, its file being read
before it is sent, nor is a download to a file (`exactBodyTo`). A Rust source's request takes `Request::held()`,
across the Rust module seam too (which refuses a deadline). A server render waits for a held
request as for any other, until its reply or the render's deadline. On the web build an
answer counts as held only while every fetch it has out is held: an ordinary
fetch beside it is waited on, and so is a held one made in a plain `.then`
callback, where the page cannot tell whose answer it is (made in the answer's
turn or after an `await`, it can). The count covers the data-source API (fetch,
`crypto.subtle`, storage, the store's keys, `native` and generated module calls,
`openAuthSession`); work a source reaches past it, through the page's own
globals (no boundary there, LLP 1027 D5), is not counted, so an answer awaiting
it beside a held fetch may count as held. Hermes reaches no such globals.

An answer that keeps coming (LLP 1016.000) is a `fetch` with `exactStream`,
returned as the answer: `return fetch(url, { exactStream: (event) => value })`.
The promise never settles; each message, and the end, is mapped and commits
as the resource's answer. A mapper may return a promise (an `async` mapper):
the message commits when it settles. The resource stays pending until its first
mapped value commits. Until a mapper settles, the messages after it, and
the end, wait, so they are mapped in the order they arrived and a later
message's mapper call never overtakes an earlier one's. Natively its later
turns are an answer's: each `await scheduler.yield()` ends one, and the next
has a budget of its own. A rejection is a throw: it ends the stream with that
error and the resource keeps its last value. A stream let go (new arguments,
`refresh`) lets go the call still pending, whose `scheduler.yield()` rejects
with an `AbortError`. A mapper may await promises and `scheduler.yield()`;
`fetch`, timers and storage are refused in it. At most 4096 messages, or 64 MiB
of them, wait behind it, however many arrived together; one more ends the
stream (`the stream's mapper fell behind: …`). The web's wasm host maps each message
now and refuses a promise. An `http:`/`https:` URL is read as server-sent
events under `net.fetch`; a `ws:`/`wss:` URL is a WebSocket under
`net.websocket` alone. Except for TypeScript sources on the web JS target and
native hosts, described below, and Rust sources on the web JS target and
native hosts ("Rust data sources", "A two-way socket"), sockets are receive-only. An event is
`{type, data, lastEventId, coalesced}`. Every message reaches the mapper, in
the order it arrived, as the browser's `EventSource` dispatches every event: a
mapper that folds deltas (an LLM's tokens) sees them all. Messages that
arrive together, in one network read or while the runtime is busy, are
mapped one after another and commit once, as the last one's value, so a view
updates once for them. At most 4096 of them, or 1 MiB of their data beyond one
message's ceiling, wait for the runtime; one more ends the stream with
`Refused` (`event stream receive queue exceeds 4096 events or 1 MiB`; a
receive-only socket's says `4096 messages`) after the mapper has seen those
waiting, so nothing is dropped unseen. `coalesced` is 0 for every event: only
an upload's progress folds (below). The end is
`{type: 'error', kind, message, status}`: `kind` is `Network` (the far side
closed: `the socket closed (1000)`), `Refused` (outside the grants), `Aborted`,
or `Response` with the status and body of a reply that was not an event stream.

For authentication, use the runnable [single-use ticket recipe](recipes/socket-ticket.md):
an authorized HTTPS request obtains a ticket, and a second source opens its WSS
stream. Each reconnect obtains another ticket.

A socket can offer protocol names in preference order with
`exactProtocols: ["events.v2", "events.v1"]` beside `exactStream` (LLP 1016.000).
Rust calls `Request::get(url).protocols(["events.v2", "events.v1"])`, handles
its `Result<Request, String>`, then passes the request to `Answer::stream`. Each name must be a
nonempty [RFC 7230 token](https://www.rfc-editor.org/rfc/rfc7230#section-3.2.6),
and duplicate names are refused by name before opening. Names are case-sensitive.
The mapper's `event.protocol` (Rust `Message::protocol`) reports the selection
on `open` and each message; receive-only hosts report it on each message.
An omitted or empty list preserves an ordinary socket with an empty selection.
If protocols were offered, a missing selection fails the connection, as does an
unoffered selection, following the
[WebSocket opening handshake](https://websockets.spec.whatwg.org/#opening-handshake).
Do not put access tokens in protocol names: their alphabets differ, and the server
echoes the selected protocol.

A TypeScript source's `http:`/`https:` stream whose 2xx body is not
`text/event-stream` is a **byte stream** (LLP 1016.000, the amendment of
2026-10-23): each `message` has an ArrayBuffer `data`, the bytes read since the
mapper's last turn, at arbitrary boundaries (a framing is the mapper's to
parse), never dropped or coalesced. At most 1 MiB, or the stream's
`exactIndependentHttp.maxResponseBytes` if smaller, waits in the receive queue. A read that would pass this bound waits until
the runner takes the queued bytes; the host stops reading in the meantime.
A slow drain loses no bytes and does not end the stream. The separate limit
on messages waiting behind an unsettled mapper still applies. The body's end is `Network`, `the byte
stream ended`. A 204, a 205 and a `HEAD` have no body and stay the one answer.
The request carries `Accept: text/event-stream`, as every stream's does, unless
the source names an `accept` header; name one where the server negotiates by
it. The web JS target, macOS, iOS, Linux and Android read it; the web's wasm host, Rust
sources and worker-placed sources still take such a body as the one answer.
A caller's abort signal maps an `error` with kind `Aborted` while the resource
is still held. New arguments, `refresh` or the resource leaving the view close
the stream without further mapper calls;
`state.pending` lists it until its first mapped value commits, `state.streams`
while it is open, with `grant`, the grant line that admitted it. The same holds on Hermes, the web's wasm host and the web build (the JS
target). The stream's `fetch` may follow an `await` in the answer (a token
fetched first, or the answer's own timer), on every host: it is made in one of
the answer's own turns, its source, its code after an `await` in an `async`
function, or its timer, while the answer has not given its value or been let go.
On the web build one made elsewhere (a plain `.then` callback, module
initialization) is refused, saying so.

An **upload** that tells its progress (LLP 1016.000 D10) is a stream with
`exactUploadProgress: true` beside its body, for a percentage the view draws
while the person goes on working. Make it a resource's answer, keyed by what
it uploads:

```ts
if (source === "upload") return fetch(uploadUrl, {
  method: "POST", headers: { "content-type": "image/png" }, exactBodyFrom: path,
  exactUploadProgress: true, exactTimeout: 300000,
  exactStream: (e) => e.type === "progress" ? { status: "uploading", progress: e.loaded / e.total }
    : e.type === "load" && e.status < 300 ? { status: "ready", progress: 1 }
    : { status: "failed", progress: 0 },
});
```

As the body (`body` or `exactBodyFrom`) goes out, the mapper gets
XMLHttpRequest's upload progress, `{type: 'progress', loaded, total,
lengthComputable: true}`: `loaded` is the bytes the platform has written
(URLSession's `didSendBodyData` on Apple, the portable transport's writes on
Linux, Windows and Android, `xhr.upload` on the web), `total` the body's
length, and the last has `loaded === total` once the whole body has gone out. A reply that comes first (a 413 before the body has gone) is the reply, and progress stops at the bytes written, never filled to `total` (LLP 1133.008.000 D4). A newer one replaces one the
runtime has not taken, counted in its `coalesced`: progress is a snapshot. The reply is read whole within the ceiling
(`exactIndependentHttp.maxResponseBytes`, 1 MiB unless given) and ends the
stream as `{type: 'load', status, data}`, whatever its status, as
XMLHttpRequest's `load`. A failure ends it as any stream's `error` does, of kind
`Timeout` once its `exactTimeout` passes (an upload may take one; a reply that
lands as it passes stands). The platform's own limits follow that deadline a
second later; without one they are the upload's, as they are a fetch's
(URLSession's 60 s without data; the portable transport's 30 s for the whole
exchange, 15 s for the reply's head and 15 s for its body), so give a
deadline to an inline upload that may take longer. An `exactBodyFrom` upload
without a deadline has no limit on the whole exchange: it fails after 60 s with
no byte written or 60 s with no byte read from the disk, and then the reply has
the same 15 s and 15 s. An empty body tells no progress.
If the mapper awaits, progress still leaves the upload pending, and the
reply's eventual mapped value is the answer kept for those arguments.
No `Accept: text/event-stream` is added. It is in flight until that reply:
`pending(…)` reads true, `state.pending` lists it, and every clock waits for
it, as for a fetch, while each progress event commits as it comes. No answer
awaits it, so it holds nothing else back: uploads run beside a source's other
work, and natively one waiting for room in the memory its bodies may take
holds none of the 16 places a native app's streams have.

An upload is **asked once for its arguments** while the app runs: nothing
asks it again at them, in flight, finished or failed (not a store write it
read, a topic it watches, nor `refresh`), and arguments that come back after
others answer as their upload ended, without asking the source. Its `with`
context is not among them: a change there sends nothing. Of a resource's
uploads the newest 32 that ended keep their answers; an older one's
arguments, asked again, fail with `failure(x).reason` `already-sent` and send
nothing. A plan reload and an update of the data module keep this, but for
the web JS target's development reload, a new page, which asks again an
upload the page still shows. An upload one of them, or new arguments, cut
off after it began to go out comes back failed, its `failure(x).reason`
`interrupted`, and is not sent again; one that had not begun is asked
again. A launch starts
afresh, so a relaunched app that still shows the resource at those arguments
sends the upload again: record what it answered in the store (the server's
id for an attachment, as upstream T3 Code keeps its attachment ids) and stop
showing the resource once you have it. New arguments that make the same
request keep the upload already running (LLP 1054.000.000 D3), so a retry
changes the request itself as well as the arguments: put an attempt number
in the URL's query or a header. An upload the host refused before it went
out (a limit, the grants, the local network's rule) was never sent, and
`refresh` asks it again.

It must be an `http:` or `https:` request other than GET or HEAD, without
`exactLongPoll`, and with `redirect` left to follow (XMLHttpRequest can only
follow), or it is a `TypeError`. On the web a cross-origin upload asks a CORS
preflight, as any XMLHttpRequest with an upload listener does. Uploads are
sent without consulting the browser's cache. `only-if-cached` fails with a
network error and sends nothing; a forced ask overrides it, as for other
fetches. The other cache modes need no change to the upload, and Exact adds
no cache request headers (LLP 1162 D4). An older
prelude would ignore the option and read the reply as a byte stream, so a
module that uses it needs `typescriptRuntime` 5, which it shares with the
abort changes above (no binary had shipped 5): the bake records each
module's lowest runtime, and `push` refuses such a module to a cohort whose
binary is older (it needs a new binary) while a module that uses nothing
newer still pushes. A Rust source's stream takes `Request::upload_progress()`
and reads each message with `Message::sent()`; the Rust module seam refuses
it. An upload that must finish after the person leaves the app is a
background transfer instead ([recipe](recipes/background-transfers.md), LLP
1133.017), whose progress a live source reports at most once a second.

A **long poll** (LLP 1016.000 D8) is a stream with `exactLongPoll: {cursor,
from}` beside `exactStream`, for an endpoint that holds each request until it
has news: `fetch('https://sync.example/changes?since=0&wait=25', {
exactStream: (e) => e.type === 'error' ? null : JSON.parse(e.data),
exactLongPoll: { cursor: 'since', from: 'seq' } })`. The host asks with GET
(`Accept: application/json` unless named), one request at a time. Each 2xx
JSON reply is one `message`: `data` its text, `lastEventId` its cursor, the
reply's field `from` (`seq`, or `meta.seq` one level down; a string, or a
whole number within 2^53; a string cursor is at most 4096 bytes with no control
characters, and one past either rule ends the stream, `Refused`); the next
request carries that cursor in the query parameter `cursor`. A heartbeat asks
again with no event: a 204, a 304, an
empty body, or a reply with the cursor it was asked with (`sameCursor:
"event"` delivers that one instead, for an endpoint that sends news under an
unchanged cursor); a heartbeat, or a reply that asks again, asks again no
sooner than a second after it was asked, so a server that answers at once is
not hammered. From its first reply of any kind no clock waits on it:
`state.pending` names it `held: true`, `clock held <name>` waits for its next
reply, and each reply raises `state.heldReplies` (LLP 1016.000 D9). The network, a timeout, a 408, 429, 502, 503 or 504 asks
again after `Retry-After` (at most 60 s) or a backoff of 1, 2, 4, 8, 16, then
30 s, each within a fifth, reset by any 2xx; no event is delivered, and
`state.streams` shows the stream's `retrying: {attempt, inMs, error}`. Anything
else ends the stream as any stream's end does: another status (`Response`, with
its body), a 2xx with no cursor at `from` (`Response`), an event stream or a
reply past the ceiling (`Refused`). It must be a GET with no body over
`http:`/`https:`, with `exactStream` (otherwise a `TypeError`). Letting the
resource go aborts the request held open, or the wait. TypeScript sources on
every host take it, and Rust sources with `Request::long_poll(cursor, from)` on
a stream's request. An older prelude would ignore it and open an event stream,
so it moves `typescriptRuntime` to 3: a native app with a TypeScript data module
needs a new binary once more. A Rust source's long poll is a new kind on the
module seam. The Rust module ABI is 10: held marks (LLP 1016.000 D9), prefetches (LLP 1162.000 D5), preload names (LLP 1162.000 D1), long polls,
two-way sockets (LLP 1016.003), cache modes (LLP 1162 D4) and admitted secret names
(LLP 1048.000 D9) require a matching binary.

On the **web JS target and on macOS, iOS, Linux and Windows, TypeScript sources**
receive `(event, socket)` for a WebSocket (LLP 1016.000 D7). Keep the handle by your resource's identity in
module state. Its events are `open`, ordered `message` (string or ArrayBuffer
`data`), and `close` (`code`, `reason`, `wasClean`); a refusal is still `error`.
Every protocol frame reaches the mapper; `coalesced` is zero. The mapper must
return a shaped snapshot synchronously. A mutation sends on a kept handle, and
the mapper may answer its own socket's frames (below):

```ts
import type { StreamSocket } from './app.contract.d.ts';
const feeds = new Map<string, StreamSocket>();
export function answer(source: string, [id, url, text]: [string, string, string]) {
  if (source === "feed") return fetch(url, {
    exactStream(event, socket) {
      if (socket) feeds.set(id, socket);
      return { status: event.type, text: typeof event.data === "string" ? event.data : "" };
    },
  });
  if (source === "sendFrame") {
    const socket = feeds.get(id);
    if (!socket) throw new Error("feed is not open");
    socket.send(text);
    return { accepted: true };
  }
}
```

Call `sendFrame` through a Contract `mutation`, after the feed reports `open`.
`send` accepts a string, ArrayBuffer or ArrayBufferView; bytes are copied at
the call. `close(code = 1000, reason = "")` accepts 1000 or 3000–4999 and a
reason of at most 123 UTF-8 bytes. In a mutation, both methods are allowed
during its source's synchronous call, and after it only when the source
answered with a promise (below). A source that stages socket operations
answers now with a value or with a promise (then it is an answer in flight,
below), not a stream answer, and makes no `fetch` in the same source. Whichever of a socket operation and a
`fetch` comes second throws `a mutation cannot both send on a socket and
fetch`, and the mutation is refused before either takes effect: no frame is
sent and no request starts. A thrown source, wrong-shaped answer or refused
commit sends none of them. The handle exposes read-only `readyState` (0–3)
and `bufferedAmount`: the transport's buffered bytes as this turn of the
module began, plus what this mutation has staged on the socket, so a turn
that sends nothing still sees a buffer that has drained. It cannot extend
the resource's lifetime.

The mapper may also `send` and `close` on the socket it was handed while it
answers `open` or a `message`: a subscribe on `open`, an `Ack` or `Pong` for
each frame, as a browser client answers in its handlers (LLP 1016.000 D7,
amended 2026-10-16):

```ts
exactStream(event, socket) {
  if (event.type === "open") socket.send('{"_tag":"Subscribe"}');
  if (event.type === "message") socket.send(JSON.stringify({ _tag: "Ack", id: JSON.parse(event.data).id }));
  return { status: event.type, text: typeof event.data === "string" ? event.data : "" };
},
```

What a mapper call stages is sent once the commit that settles that frame's
snapshot stands, in arrival order and ahead of later mutations' frames. A
throw, a wrong-shaped snapshot or a refused commit sends none of it, and
`logs` say `mapper <resource> failed: …; socket frames were refused`. Each
call has a mutation's bounds below; overload throws `QuotaExceededError` and
fails that frame's snapshot even if the mapper catches it. On `close` and
`error` the handle is already closed and refuses; so do another socket's
handle and a `fetch` in the call. A continuation the mapper schedules is not
the call's: it runs in a later turn (below).

A resource's answer may also `send` and `close` on a socket a stream
resource of the same module owns, so it sends its own request and awaits
the reply, with no timer, and so may a mutation whose source answers with
a promise (LLP 1016.000 D7, amended 2026-10-18):

```ts
const waiting = new Map<string, (reply: string) => void>();
// In `feed`'s mapper, for a reply: waiting.get(reply.id)?.(reply.text)
if (source === "request") {
  const socket = feeds.get(id);
  if (!socket) throw new Error("feed is not open");
  socket.send(JSON.stringify({ _tag: "Request", id: n }));
  return new Promise<string>(resolve => waiting.set(n, resolve));
}
```

What each turn of an answer sends goes out in the order it sent it once
the commit that ran the turn stands, as its fetches do; a refused commit
takes it back and says so, and nothing the answer does afterwards (a
value, a failure) takes it back. Earlier turns' batches precede later
ones, and frames are never coalesced.

On native hosts and the web JS target, the bundle's async helper restores
the answer an `async` function began in after every `await` (LLP 1133.002
D14), whichever answer settled the promise. That answer owns the send's
permission and bounds. Once it has given its value, failed or been let go,
its async steps cannot send, even if another answer's turn resumes them.

Natively, microtasks drain in a source's call and each delivery of its own
`fetch` or storage step. A synchronous mapper's call drains none: a waiter
it settles resumes when an answer is next asked, including the waiter's
own re-ask. The async helper restores the waiting answer's context in
either case. Native socket batches, however, name the answer whose turn
staged them: if answer B's turn resumes answer A's async step, its send
uses A's permission and bounds but the batch names B in `logs`. A plain `.then`
callback has no captured answer: it uses whichever answer's turn drains
it, including a waiter's own re-ask. With no answer current, its send
throws. Such a callback left behind by a let-go answer can therefore send
as another answer whose turn drains it; an async step still belongs to
the let-go answer and refuses.

On the web JS target, an `async` function's steps after an `await` and the
answer's own timers are its later turns, and each sends when it ends,
behind what its source sent. A plain `.then` callback runs under no
answer there, and its send throws. An answer that has given its value,
failed or been let go has no later turn: a send from code it left running
throws `InvalidStateError`. What a turn sent while its answer's commit
had not yet stood waits for it, and is refused, in `logs`, if the answer
is let go first.

An answer the runner lets go (new arguments, `refresh`, a newer send,
leaving the view) sends nothing more as its own; what it sent stays sent.
An answer asked anew runs its source again, so key a request that must not
repeat. An answer in flight may also `fetch`, though a mutation's
synchronous source still may not do both: a mutation whose source sent
fetches in a later turn (after an `await`). A refused commit's batches are
taken back without telling the code, so a protocol that awaits a reply
times out. Its bounds are a mutation's, over all its turns (`socket answer
exceeds 256 operations`, `… 1 MiB`); overload throws without failing the
answer. A handle that is not open, or whose socket a mutation's stream
owns, throws `InvalidStateError`, and so does an operation after the
answer's own `close` of that socket. A batch the host refuses is said in
`logs` as `answer <resource or mutation> failed: …; socket frames were refused`. Its
`bufferedAmount` counts the transport's bytes as the turn began, what
this turn has sent, and what earlier turns sent that has not left yet.

A mutation or mapper call stages at most 256 operations and 1 MiB, including
bytes already buffered by the browser. Overflow throws `QuotaExceededError`; a stale or
not-yet-open handle throws `InvalidStateError` (also after close, a deliberate
difference from the browser's silent discard). A change before publication
can still refuse transport admission, reported in `logs`: acceptance is not
server acknowledgement. The receive queue is bounded by 256 frames and
1 MiB plus the per-message response ceiling; overflow maps accepted queued
frames in order before ending with a `Refused` error.
`state.streams` adds `readyState`, `bufferedAmount` and `sent` for these sockets.

Natively `readyState` follows the events the mapper has seen, and
publication also bounds the transport's buffer at 256 frames; LLP 1016.000
D7 lists the differences. **Remaining host gap:** the web wasm target and
worker-placed TypeScript do not provide the send path (on wasm the handle never
opens); a Rust source's `Request::socket` is refused by name in both. No ambient `WebSocket` constructor is admitted. SSE
remains unchanged.

This first browser implementation targets modest app stores: filesystem
operations read the app's file records, and each SQLite mutation atomically
saves the whole database file. Database files share the filesystem namespace,
so closed databases can be copied or exported through `storage.fs`. SQLite integer results
are `bigint`: convert them to a Contract-compatible value before returning.

### Sign in through the system browser (`openAuthSession`)

On the web, a closed popup is not reported as cancelled: a provider's
Cross-Origin-Opener-Policy can make an open popup appear closed too. The wait
ends at the callback or the ten-minute bound (499), or when the app releases
or supersedes its request. Offer Cancel while pending: in Contract, assign
`none` to the mutation holding sign-in (`signed = none`) to release it and
allow an immediate retry. Returning focus to the app does not end the wait.

`openAuthSession` answers 501 on the headless Linux host and Windows, and on Android
when no system browser resolves or `ephemeral: true` is requested (this build has no
ephemeral session support). Android supports custom-scheme and claimed HTTPS callbacks
from `auth.callback` grants. HTTPS uses Auth Tab when the selected browser supports
it, otherwise a verified App Link on the callback's exact path. The bake writes
the filter; the callback host must serve `assetlinks.json` with the app's package
and signing fingerprints. See the [OAuth recipe](recipes/oauth.md) for the domain
associations and publishing. HTTP loopback and reserved `/.exact/auth/callback`
web popup callbacks answer 501. An Auth Tab wait ends on its result or the ten-minute
bound; returning to the activity alone keeps it. For a custom-scheme or App Link wait,
returning after the activity stopped without a callback cancels with 499, whether by
Back or another link that brings the activity forward. A pause without a stop,
such as a permission dialog or a multi-window focus change, keeps the wait. Callbacks
are routed by endpoint and state: a stale, wrong-state or missing-state callback is
consumed without completing the live sign-in or cancelling it on that resume.
A matching state with invalid callback data, such as repeated state or userinfo,
settles with 502. Callbacks without a live session are discarded. Reload retires the old wait. A browser wait survives activity
recreation and expires after ten minutes, including device sleep, with 499. A launch
delivered while no activity is attached cancels with 499. A sign-in
interrupted by process death must be started again.

### Notifications

`@exact/notifications` (LLP 1133.004) posts local notifications, hears their
click, sets the app's badge and reads the permission, by the Notification and
Badging APIs' names:

```text
use showNotification, closeNotification, setAppBadge, clearAppBadge, notificationStatus, requestNotificationPermission, notificationclick, NotificationStatus, NotificationEvent from "@exact/notifications"

component App
  action opened(tag: string, e: NotificationEvent)
    …
  action done
    showNotification(title="Build finished", tag="thread-3", silent=true, data="/thread/3")
  view
    column notificationclick=opened
      …
```

The app depends on the package (`bun exact.mjs add notifications`, or
`"@exact/notifications": "file:<exact2>/packages/notifications"`) and grants
`device.notifications purpose.notifications` (a strings key, LLP 1069.008;
iOS shows its own fixed prompt text): without the grant a post and the
request are refused (`notifications.showNotification: refused, grant-missing
(device.notifications)`).

A visible push from the app's own server is `@exact/push`, under its own
grant beside this one: see [Push](#push).

- `showNotification(title=, body=, tag=, silent=, data=, showTrigger=,
  navigate=)` and `closeNotification(tag=)` are statements in an action; data
  code calls them through the package's binding (`await showNotification({
  title, tag, silent: true })`). A newer notification with the same `tag`
  replaces the older. `silent=true` posts without the platform's sound.
  `showTrigger` is the time to show it in epoch milliseconds (the date now is
  `exactTime().epochAtZero + performanceNow()`). A post while the permission
  is `prompt` asks first; while `denied` it is refused. A post from a wake or
  a tool while `prompt` is refused `person-required`: ask from a person's
  press first (`requestNotificationPermission()`).
- A plain click with `navigate=` opens that route location: one `navigate`
  event to the navigation root while the app runs, or the launch location when
  it starts the app. `navigate=` is reduced by the router's `location_of`
  (`app://thread/42` becomes `/thread/42`).
- A click without `navigate=` is `notificationclick` on the root component's
  outermost node: the tag and the notification's `{tag, data, title, body,
  action, reply}`, delivered once, in order, after first pixel; one that
  starts the app (macOS, iOS) is held until then. A click always brings the
  app forward. `action` and `reply` are none for a click.
- `actions=[NotificationAction(action=, title=, foreground=, destructive=,
  authenticationRequired=, placeholder=)]` (macOS, iOS) gives a post the
  buttons the system shows when the notification is pressed and held, on the
  Lock Screen too. The list is written out, a literal (`contract-module-literal`):
  the build reads each list the app writes, and the host registers each as a
  category at launch, named by its actions' names joined by commas
  (`approve,note`). An action name means one thing in the app: one written
  twice differently is refused (`contract-module-declared`). Names must be
  nonempty, contain neither `,` nor `=`, and occur once per list. A list has
  at most four actions; Apple's banners show only the first two
  ([Apple's category guidance](https://developer.apple.com/library/archive/documentation/NetworkingInternet/Conceptual/RemoteNotificationsPG/SupportingNotificationsinYourApp.html)).
  Exact registers the union of live sessions' declared categories, preserving
  an embedder's own categories. Replacing or ending one session does not remove
  another's categories. Conflicting definitions of one category are neither
  registered nor authorized until the conflict ends. An embedder's existing
  category wins a name collision; Exact refuses its own conflicting category.
  Removing Exact's last declaration releases its ownership of the name.
  The host registers categories before data code starts. Responses wait for the
  destination's current declarations, registration and data activation after a
  cold launch or plan replacement. The system's action identifier includes its definition's
  revision: an old button cannot satisfy a changed unlock requirement, even
  when the category name stays the same. `e.action` remains the authored name.
  Data code's
  post may name only a list the app's Contract writes (`TypeError`
  otherwise). `placeholder=some("Note")` makes the action a text field;
  `destructive` and `authenticationRequired` are the platform's flags (the
  device must be unlocked first). Choosing one is `notificationclick` with
  `action` its name and `reply` what the person typed; it never opens the
  post's `navigate=`.
- An action with `foreground=true` brings the app forward, as a click does.
  With `foreground=false`, a running app is not brought forward. On iOS a
  terminated app starts with no view; macOS has no notification-specific
  cold-launch path and may show its ordinary startup window. Before the first
  unlock after a device restart, iOS refuses the response before booting or
  activating data. Locking again after that first unlock does not refuse it.
  The handler runs as a background action (LLP 1133.004 stage 2), and the app
  is kept running until the work the handler started has ended (its requests
  and native statements answered, its storage written) or the system's time
  for it, about 30 seconds, runs out. It is the person's
  act: it may make the request the action exists for, with the app's kept
  credentials. It shows nothing: a permission prompt is refused
  `person-required`, a name only the foreground may call is refused, and so
  are the host commands a wake may not run (`copyText`, `share`,
  `playSound`, …). A session that needs a sign-in it does not have cannot
  sign in there; post a notification that asks the person to open the app.
  An expired action is discarded even if the session has not booted yet;
  opening the app later does not run it. An action with no handler settles
  immediately as unheard. Replacing its runner releases an action's held
  completion as superseded; a response still awaiting declarations waits for
  the replacement.
- On Android (LLP 1133.004.000 D5) an action is a button that brings the
  app forward, with `foreground=false` too: an action boots no session with
  no view yet, so no handler runs with the app closed. Android shows
  three actions; a fourth is dropped there and journaled. A chosen action is
  raised only while the app still declares it as the post wrote it: a button
  posted before an update changed or removed it brings the app forward and
  raises nothing. `authenticationRequired` asks for the unlock from Android
  12; Android has no destructive style. After a reply, Android may keep the
  notification, showing the reply, until the person dismisses it.
- A remote notification's click opens its payload's `navigate` and nothing
  else ([Push](#push)).
- `setAppBadge(contents=)` sets the app icon's badge, best effort: a number, no
  number for a flag (`•` on the Mac's Dock tile, `1` on iOS), 0 to clear;
  `clearAppBadge()` clears it. The badge is the app's: an embedded host's
  sessions share one. It needs no grant. A negative count is refused
  `TypeError` on every host, under the agent too.
- `notificationStatus()` reads `{available, state, badge, apple, android}`
  (`state` `granted`, `prompt`, `denied` or `unknown`), read again when the app
  comes back to the front; `requestNotificationPermission()` asks while
  `prompt` and answers the status.
- On Android (LLP 1133.004.000) a post goes on a notification channel, which
  holds its importance and sound; the person may change both in Settings, and
  Android keeps them once the channel is made. The app declares its channels
  in `app.json`, and the host makes them after the first frame and when the app
  is updated, so Settings lists them from the first launch:

  ```json
  "host": { "android": { "notificationChannels": [
    { "id": "urgencies", "name": "channel.urgencies", "description": "channel.urgencies.about",
      "importance": "high", "sound": "assets/urgency.wav" }
  ] } }
  ```

  `name` and `description` are strings keys; `importance` is `min`, `low`,
  `default` or `high` (`high` shows a post heads-up); `sound` is an `.ogg`,
  `.mp3` or `.wav` file in the app. The declaration needs the
  `device.notifications` grant (`android-channels-need-notifications`). A post
  without `android.channelId=` goes on `default`, the platform's default
  importance and sound, named Notifications; declaring `default` names and
  tunes it. `android.channelId="urgencies"` names a declared one (another
  literal is refused at build, `android-channel-undeclared`; a computed one
  `TypeError`). A changed importance or sound for an installed channel does
  nothing, as on the platform: declare a new id instead. `silent=true` always
  wins: the post keeps its channel and is silenced alone. A post on a channel the person turned off refuses
  `NotAllowedError`, and `notificationStatus().android.channels` lists each
  channel's importance as the person left it (`none` when off).
  `android.category=` and `android.number=` are the platform's category and
  the launcher's count. The status bar's icon is the app's `"monochrome"` icon
  (without one, the platform's generic glyph, and the build says so).
  Android 13 and later ask `POST_NOTIFICATIONS`, which the grant adds to the
  manifest. There is no badge call (`setAppBadge` refuses
  `NotSupportedError`; launchers draw their own dot). The app's Kotlin may set
  more on each post through `NotificationsModule.builderHatch`.

| host | post | `showTrigger` | click | badge |
| --- | --- | --- | --- | --- |
| web | `new Notification` in the page | while the page is open: the web has no trigger that outlives the page | while the page is open | `navigator.setAppBadge` where the browser has it, else `NotSupportedError` |
| macOS, iOS | `UNUserNotificationCenter`, shown with the app in front too | the system's, delivered with the app closed | warm, or the launch | the Dock tile's label; `setBadgeCount` |
| Android | `NotificationManager`, shown with the app in front too | an inexact alarm (`setAndAllowWhileIdle`), delivered with the app closed and after a restart, possibly minutes late under Doze; a force stop cancels it until the app's next launch | warm, or the launch | none: `NotSupportedError` |
| Linux, terminal, tvOS, Windows | no implementation yet: each call is refused `NotSupportedError`, under the agent too, and the status reads `available: false` | | | |

On macOS, system notifications need an app bundle (`bun host/apple/build.mjs
--bundle`); the bare development executable has no notification centre, so
there `available` is false and every call refuses `NotSupportedError`.

Under the agent nothing reaches the system on any host. A post while
`prompt` waits on a `permission` ticket (`tap @t allow`, or `--permission
notifications=granted` at launch); `state.notifications` lists what the app
posted (`{title, body, tag, silent, data, showTrigger, navigate, delivered,
ref}`, a tag replacing its older one, `closeNotification` removing it, an
entry for later `delivered: false` until `clock` passes it); `state.badge` is
the badge (a number, `"flag"` or `null`). `tap @ref` on a delivered entry is
its click; `tap @ref <action>` chooses one of its `actions`, `tap @ref
'<action>=<text>'` with a typed reply, a background action running hidden
as `state.wake` shows it (`origin` `action`); `--notification '<entry>'` relaunches the app with one, as the
system hands a closed app its click. Scheduling is one time per call: a daily
reminder posts the next one when the app runs.

### Push

`@exact/push` (LLP 1133.004 §9; LLP 1133.004.000 for Android) is a visible
push from the app's own server, on iOS, iPadOS, macOS and Android, by the Push
API's names. Exact sends nothing and relays nothing: the app sends its
subscription to its own server, and the server sends to APNs, or on Android to
Firebase Cloud Messaging (FCM).

```text
use subscribePush, pushSubscription, PushSubscription from "@exact/push"

component App
  resource subscription = pushSubscription() as shape option<PushSubscription>
  mutation subscribed as shape PushSubscription
  action turnOn
    send subscribed = subscribePush()
```

The app depends on the package (`bun exact.mjs add push`) and grants both lines:

```text
device.notifications purpose.notifications
device.push
```

`device.push` names no purpose and needs `device.notifications` beside it. It
signs `aps-environment` into an iOS or iPadOS build: `development` on a
simulator and under a development profile, `production` under an Ad Hoc,
enterprise or App Store one, the value a Live Activity that takes pushes signs
too. A device build is refused unless its profile has Push Notifications with
that value. A Mac build signs `com.apple.developer.aps-environment` only under
a macOS provisioning profile that allows it (see "Mac provisioning profiles"),
with the same values; without one it signs nothing, says so, and is not
refused. A tvOS build writes nothing for it. An app with Live Activities may
register beside them under this grant.

- `subscribePush()` is the one call that asks: while notifications'
  permission is `prompt` it shows the system's prompt; while `denied` it is
  refused `NotAllowedError`; while `granted`, which includes provisional and
  ephemeral authorization, it asks nothing. Then the app registers, and the call answers the
  system's answer: the subscription, or `AbortError` with the system's
  message.
- `pushSubscription()` reads `{apple: {deviceToken, appId, installationId}}`, the
  token's bytes in lowercase hex, the installed bundle id and a UUID for the
  installation, or on Android
  `{android: {installationId, token, appId}}`, the Firebase Installations ID,
  FCM registration token and installed package id. Apple's `installationId`
  stays the same when APNs replaces the token, so a server can replace the
  installation's old token. The package keeps it in the Keychain on this device
  only, so a backup restored to another device gets a new one. It is empty while
  the Keychain cannot give one (before the device's first unlock, or a signature
  without a data-protection Keychain); on iOS readers are told again after the
  first unlock.
  Otherwise it reads null. It never asks: its
  first read in a launch registers once when permission is already granted, as
  Apple asks of every launch (on Android, once the app has subscribed), and a
  new token or ID is a new value, which the app sends to its server again. Read
  it in every launch so the server keeps the current one. While permission is
  `prompt` or `denied` it reads null and registers nothing; a change of
  permission in Settings is read again when the app comes back to the front.
- `pushPermissionState()` reads `{available, state}`, notifications'
  permission, which the two modules share.
- On a Mac build signed without the entitlement, on an Android build without
  Firebase's configuration (below) or a device without Google Play services,
  and on tvOS, the web, Linux, Windows and the terminal, each call is refused
  `NotSupportedError`, `pushSubscription()` reads null and the state
  `available: false`.

The server sends APNs an alert (`apns-push-type: alert`, `apns-topic` the
subscription's `appId`), APNs's own `aps` dictionary and one key of Exact's:

```json
{ "aps": { "alert": { "title": "Delayed", "body": "The 9:41 now leaves at 9:52" }, "sound": "default" },
  "navigate": "/trips/941" }
```

The system shows it, with the app in front too. Its tap opens the top-level
`navigate` string, warm or at launch, as a local notification's `navigate=`
opens. A push sent through the Expo push service (its payload has Expo's
`experienceId` or `scopeKey`) carries the message's `data` in a `body` object,
so there a `navigate` in `data` is opened; a top-level one comes first. Another
sender's `body` is not read. Either way `location_of` reduces the location to one of the
app's own, whatever its scheme or origin, never through the app's
`protocol_handlers`. Nothing else of the
payload is read and no event is raised; without `navigate` the app only comes
forward. A server writes that string, so a route must not act on being opened.
On Apple, Exact runs no Contract or TypeScript when a push arrives: `content-available`
does nothing and the `remote-notification` background mode is refused. An iOS
or iPadOS app may instead put one app-authored Swift
`UNNotificationServiceExtension` subclass named `NotificationService` under
`modules/apple/notification-service/`. The build packages it as
`PlugIns/ExactNotificationService.appex`, with bundle id `<app id>.notifications`,
when the app grants both notification lines above. It carries no Exact runtime.
The source tree ignores dot-prefixed entries and `target/`; every selected file is
part of binary compatibility and is checked again immediately before compilation.
The folder remains ordinary app source to generic freshness, watches, captures and
scanners; its recorded inventory governs only packaging and changed-member naming.
Apple invokes it only for a visible alert with `"mutable-content": 1`; if it
does not return changed content before the time limit, the original alert is
shown. See Apple's
[`UNNotificationServiceExtension`](https://developer.apple.com/documentation/usernotifications/unnotificationserviceextension)
and [content-modification guide](https://developer.apple.com/documentation/usernotifications/modifying-content-in-newly-delivered-notifications).
The [recipe](recipes/notification-service-extension.md) gives the folder,
privacy manifest and device-profile rules.

`aps.category` names one of the categories the app declares with
`@exact/notifications`' `actions=` (above), by its actions' names joined by
commas (`"category": "approve,note"`); a name the app did not declare shows
no actions. A chosen action is `notificationclick` with the tag the
notification's identifier (`apns-collapse-id` when the server set one), the
action's name and the typed reply; `data`, `title` and `body` are empty, and
nothing else of the payload reaches the app, so the app looks up what the
action is about by that identifier. An explicitly selected session receives the
push action if it declares it; otherwise the first matching live session does.
A background action runs as a local post's does.

The system answers a registration on the app delegate. When the package
loads, the app delegate is given the two methods that carry the answer where
it does not answer them already, its own or inherited, so an app that does
not import the package has neither. An app that embeds Exact's sessions (LLP
1031) and whose app delegate answers them keeps its own and forwards the
answers with `ExactApp.registeredForRemoteNotifications(deviceToken:)` and
`failedToRegisterForRemoteNotifications(error:)`. One that keeps the
notification delegate itself shows notifications in front itself and forwards
the taps it wants routed with `openNotification(_:session:)`, naming the
session; a tap for a session that is gone opens nothing. Each may be called on
any thread.

**On Android** the app links Firebase's messaging client
(`com.google.firebase:firebase-messaging`) only by importing the package, so
its first Android build writes the Gradle locks (`bun exact.mjs build android
--write-locks`). Firebase's configuration is its `google-services.json`, with a
client whose `package_name` is the app's id: beside `app.json`, or at the path
`"packages": {"@exact/push": {"android": {"googleServices": "<path>"}}}`
names (for a file CI writes). The build reads it without the Google Services
Gradle plugin. A release build without it is refused
(`push-google-services-missing`); a development build says so and reads
`available: false`. Firebase starts at the package's first call, not with the
process. `subscribePush()` obtains the FCM registration token with `getToken()`
and the installation ID with Firebase Installations. Messaging uses token mode
(`firebase_messaging_installation_id_enabled=false`), since Firebase's FID
messaging mode disables the token Expo needs. `onNewToken` keeps and announces
rotations. Registration is checked once per launch after the app subscribes;
`subscribePush()` waits up to a minute, then refuses `AbortError`.

The server sends an FCM v1 notification message to `android.token`, with
`navigate` in `data`:

```json
{ "message": { "token": "…",
  "notification": { "title": "Delayed", "body": "The 9:41 now leaves at 9:52" },
  "android": { "priority": "high", "notification": { "channel_id": "delays", "tag": "trip-941" } },
  "data": { "navigate": "/trips/941" } } }
```

It shows in the background (Firebase shows it) and in front (the package
shows it), on its `channel_id` when the app declares that channel
(`host.android.notificationChannels`), else on `default`, with the app's small
icon. Leave out `icon` and `color`: Firebase honours them for a notification it
shows. A `tag` replaces a shown notification with that tag, a local one
included, and `closeNotification(tag)` closes it. Its tap opens `navigate`,
warm or at launch, as on Apple. The top-level `navigate` takes precedence.
When it is absent and `data` carries Expo's `experienceId` or `scopeKey`,
Exact reads `navigate` from the JSON object encoded in `body` by the Expo push
service. A blank or non-string location, or a malformed body, opens nothing.
Another sender's `body` and the other `data` keys are not read.

A data-only message raises `push` on the root with `PushEvent = {data: string}`:
`JSON.parse(e.data)` reads the FCM `data` map. It uses the live session, or
boots a headless session with no activity. Boot, queueing and handler work
share ten seconds from receipt; at expiry its pending work is cancelled.
The handler has origin `wake`, even in front: it may fetch and post a local
notification under an existing grant, but cannot ask permission or open UI.
A notification message follows the presentation path above and raises no
`push` event. Delivery remains subject to FCM, Doze and the phone's battery
policy; a server must keep alerts available for the app to reconcile.

`getExpoPushTokenAsync(subscription, manifest, options)` from
`@exact/push/expo` exchanges the subscription's token through Expo's service:
`android.token` as an FCM token, or `apple.deviceToken` as an APNs one. Pass the
app's imported `app.json`: `extra.eas.projectId` names its EAS project. The
subscription's `appId` supplies the installed package or bundle id, with the
selected variant's suffix and, on Android, its normalization.

An APNs token also needs `options.development`: `true` when the build
registers with APNs's sandbox (a simulator, or a development profile), `false`
for an Ad Hoc, TestFlight or App Store build. Expo uses the sandbox for a token
exchanged with `true`
([`ExpoPushTokenOptions`](https://github.com/expo/expo/blob/main/packages/expo-notifications/src/Tokens.types.ts)),
and APNs answers `BadDeviceToken` to a token from the other environment. The
app states it because Exact does not read it: iOS has no public call that
reads the signed `aps-environment` (LLP 1133.004 §9 P3). Without it the call
throws and sends nothing. An FCM token ignores it.

Expo's `deviceId` is a UUID made from the first 16 bytes of the SHA-256 of the
subscription's `installationId` (case-sensitive), with the v4 version and
variant bits set. It stays the same while the installation ID does, including
across FCM and APNs token rotations, so Expo replaces the installation's token.
A subscription without an installation ID throws and sends nothing.

The call returns `{type: "expo", data: "ExponentPushToken[…]"}`. An HTTP
refusal throws with Expo's status and the first 500 characters of its reply;
if the reply cannot be read, the error still carries the status.
Grant `net.fetch https://exp.host`, exchange again when the live subscription
changes, and send the Expo token to your server. The [push recipe](recipes/push.md)
includes the handler, channel and backend send.

### Expo push on iOS and iPadOS

The app needs no Firebase file. Give the EAS project an Apple Push
Notifications service key with `eas credentials`, as
[Expo's setup guide](https://docs.expo.dev/push-notifications/push-notifications-setup/#get-credentials-for-development-builds)
describes. The Contract is the Android example's below, without the `push`
handler. In `app.ts` the exchange names the APNs environment the build
registers with:

```ts
const APNS_SANDBOX = import.meta.env.APNS_SANDBOX ?? '';

export const answer: Answer = async (source, args) => {
  if (source === 'exchangePush') {
    const [subscription] = args;
    if (!subscription?.apple) return { type: 'expo', data: '' };
    const token = await getExpoPushTokenAsync(subscription, manifest, { development: APNS_SANDBOX === '1' });
    // Save token.data on the backend, as the Android example does.
    return token;
  }
  throw new Error(`Unknown source ${source}`);
};
```

Exact does not read the environment, so the build supplies it: declare
`"env": ["APNS_SANDBOX"]` in `app.json` and set `APNS_SANDBOX=1` in the
commands that build for a simulator or sign with a development profile. The
value follows the signing, not the variant or the build mode: App Store
Connect signs a TestFlight or App Store build for production. A build that
leaves it unset exchanges as production. A wrong value still exchanges, and
APNs then refuses the token as `BadDeviceToken`.

On Apple no app code runs when a push arrives. The backend sends `title` and
`body` at the top level of the message, which the system shows, and `navigate`
in `data`, which the tap opens.

### Android Expo push example

The backend sends a data-only message, with title and body inside `data`, as
the [push recipe](recipes/push.md) shows. Expo wraps that object as JSON in
`envelope.body`; the handler reads it and posts the notification itself.
Top-level `title` or `body` makes Expo send a notification message instead:
Firebase shows it in the background and Exact shows it in front, without
raising `push` ([FCM message types](https://firebase.google.com/docs/cloud-messaging/customize-messages/set-message-type)).

The Contract subscribes from a press and hears the Push API's `push` event
on its root. A live subscription refreshes the token exchange after rotation.
Merge these declarations and the root's `push` handler into the app:

```text
use subscribePush, pushSubscription, PushSubscription, push, PushEvent from "@exact/push"

shape ExpoToken
  type: string
  data: string

component App
  resource subscription = pushSubscription() as shape option<PushSubscription>
  resource expoToken = exchangePush(subscription) as shape ExpoToken
  mutation subscribed as shape PushSubscription
  mutation delivered as shape bool
  action subscribe
    send subscribed = subscribePush()
  action received(e: PushEvent)
    send delivered = receivePush(e.data)
  view
    main push=received
      button press=subscribe
        text "Enable alerts"
```

In `app.ts`, keep the app's literal `appId` and add the grants and answer
branches below. Replace the backend origin with the app's own. The exchange
uses the same request fields and JSON header as
[expo-notifications](https://github.com/expo/expo/blob/main/packages/expo-notifications/src/getExpoPushTokenAsync.ts).
It returns `{type: "expo", data}`; store `data` on the backend for this guard.
An offline exchange throws; refresh the resource to retry.

```ts
import type { Answer } from './app.contract.d.ts';
import manifest from './app.json';
import { getExpoPushTokenAsync } from '@exact/push/expo';
import { showNotification } from '@exact/notifications';

export const appId = 'com.example.guard';
export const grants = `device.notifications purpose.notifications
device.push
net.fetch https://exp.host
net.fetch https://guard.example`;

export const answer: Answer = async (source, args) => {
  if (source === 'exchangePush') {
    const [subscription] = args;
    if (!subscription?.android) return { type: 'expo', data: '' };
    const token = await getExpoPushTokenAsync(subscription, manifest);
    const saved = await fetch('https://guard.example/push-token', {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ token: token.data }),
    });
    if (!saved.ok) throw new Error('Could not save the push token');
    return token;
  }
  if (source === 'receivePush') {
    const envelope = JSON.parse(args[0]);
    // Expo encodes the sender's data-only payload in body.
    const alert = JSON.parse(envelope.body || '{}');
    await showNotification({ title: alert.title || 'Urgency',
      body: alert.body, tag: alert.id, navigate: alert.navigate,
      android: { channelId: 'urgencies' } });
    return true;
  }
  throw new Error(`Unknown source ${source}`);
};
```

The handler may also refresh the urgencies resource. Await notification and
network work inside its mutation so the wake owns it. A socket's
`exactStream` mapper cannot post; have its mapped counter key a resource
whose answer calls `showNotification` instead.

Under the agent nothing reaches APNs or FCM, and `pushSubscription()` reads null
while the permission is not granted, as on the phone. A subscribe while
`prompt` waits on the
`permission` ticket (`tap @t allow`), then on the subscription's value (`type
@t '{"apple":{"deviceToken":"ab12","appId":"com.example.app","installationId":"6f1d3a52-8c0e-4b7a-9e21-5d4c3b2a1f00"}}'`); `prefer extension
push.pushSubscription=<json>` sets the value later, as a rotation.

Android's data-message routing, deadline, registration and token rotation have
JVM coverage; Expo's exchange request, for an FCM token and for an APNs one,
is checked with mocked fetch. Real
registration and FCM/Expo delivery need the app's Firebase project and have
not been verified here. The [device checklist](../packages/push/test/checklist.md)
names the foreground, background and killed-process checks.

Run on an iOS 27 simulator: the real prompt; a registration the simulator
answered with a token, read with the installed bundle id; a relaunch that only reads `pushSubscription()`
answering the same token; the entitlement in the built app; pushes sent with
`xcrun simctl push` shown and their taps opening `navigate`, with the app in
front, from the Home Screen and at launch. On 2026-11-14, Xcode's iOS 27.0
simulator accepted a `mutable-content: 1` push for the extension fixture but
did not launch its service extension or write its App Group marker. The direct
reference separately constructed requests and called the handler's receipt and
expiry methods with a controlled download callback. Not yet run: delivery through APNs, an
APNs token exchanged with Expo's service and a push delivered through it, the
entitlement and extension signed against device profiles, and a production
build. `packages/push/test/checklist.md` is those checks; until they pass,
nothing here is claimed for a device.

### A newer binary from the store (`@exact/store-update`)

A binary change the bake cannot carry (a new host, a native module, a raised
OS floor) reaches an install only through its store. `@exact/store-update`
(LLP 1133.038; [recipe](recipes/store-update.md)) says what the store has and
opens it. It is not Exact's own signed updates: those are `exactDelivery()`
and the update store (LLP 1030), which this package never touches.

`storeUpdateStatus()` is live, announced again on a return to the foreground
and as a download moves:

- `available`: the store this package asks made this install: Google Play on
  Android (the installer is `com.android.vending`), the App Store on iOS and
  macOS (a production receipt; a development, TestFlight or Developer ID
  build has none). It says who made the install, not whether an update
  exists.
- `updateAvailability`: `offered` (Play offers this install a newer binary
  now), `listed` (the App Store lists a newer version of this platform's
  binary for the person's storefront; whether it will offer it to this
  device is unknown), `current`, `in-progress` (Play's flow is running), or
  `unknown` (not a store install, the store not asked or not answering, or a
  listing that does not compare).
- `apple`: the App Store's listing: `lookup` (`found`, `not-found`,
  `failed`), `version`, `minimumOsVersion`, `releaseNotes`,
  `currentVersionReleaseDate`.
- `android`: Play's own fields: `availableVersionCode`, `updatePriority` (0
  to 5, set when the release is published), `clientVersionStalenessDays`,
  `flexibleAllowed`, `immediateAllowed`, `installStatus` (`pending`,
  `downloading`, `downloaded`, `installing`, `installed`, `failed`,
  `canceled`, ...), `bytesDownloaded`, `totalBytesToDownload` (0 with no
  download) and `installErrorCode`.

`openStorePage()` leaves the app for the store's page: the App Store app
(`itms-apps://`), the Mac App Store (`macappstore://`), or Play's store app
(`market://`). It answers when the store has opened. Apple's page comes from
the lookup's `trackId`, so an app the lookup does not find (unlisted, a
Custom App) is refused `NotFoundError`.

On Android only, imported from `@exact/store-update/android`:
`startStoreUpdate(appUpdateType=, allowAssetPackDeletion=)` shows Play's flow
over the app: `"flexible"` downloads in the background while the app runs,
`"immediate"` covers the app until the update installs. It answers `outcome`:
`accepted`, `canceled` or `failed`. Nothing offered fails `NotFoundError`; a
type Play does not allow now fails `NotAllowedError`. An immediate flow the
person left starts again when the app resumes, while Play still reports it in
progress. `completeStoreUpdate()` installs a downloaded flexible update and
restarts the app; with none downloaded it fails `InvalidStateError`.

The App Store lookup is the package's own request to
`https://itunes.apple.com` (the bundle id and the storefront's country, no
identifier of the person), made only in a store install and kept a minute.
Both tier 1 names answer only under a `net.fetch` line, and the line to write
is that one:

```ts
export const grants = 'net.fetch https://itunes.apple.com';
```

```
use storeUpdateStatus, openStorePage, StoreUpdateStatus from "@exact/store-update"
use startStoreUpdate, completeStoreUpdate, StoreUpdateResult from "@exact/store-update/android"
  resource store = storeUpdateStatus() as shape StoreUpdateStatus else empty()
  mutation started as shape StoreUpdateResult
  action update
    send started = startStoreUpdate(appUpdateType="immediate")
```

| Host | Answer |
| --- | --- |
| Android | Play Core `app-update:2.1.0`. An install Play did not make (`adb`) answers `available: false` and Play is not asked. An error from Play answers `unknown` with `installErrorCode`. |
| iOS, macOS | The iTunes Search API lookup for the bundle id in the storefront's country (StoreKit's `Storefront`). Only this platform's record compares: a universal purchase's one record is the iOS binary's, so the Mac answers `unknown` there. A listing above this OS's `minimumOsVersion` is `current`. Play's sends fail `NotSupportedError`. |
| web, Linux, Windows, tvOS | Declared default: `available: false`, `updateAvailability: "unknown"`; every send fails `NotSupportedError`. |

Under the agent no store is asked: the status is a store install with nothing
newer, and `prefer extension store-update.storeUpdateStatus={…}` lays the
status a drive needs. `startStoreUpdate` holds a `store-update` ticket the
drive taps `accepted`, `canceled` or `failed`; the implementation then
refuses as Play would against the laid status. `openStorePage` and
`completeStoreUpdate` journal what they would have done.

Play's flows are tested against Play's `FakeAppUpdateManager`. They have not
run against a Play listing: Play offers an update only to an install from
Play, of a release with a higher `versionCode` (`EXACT_BUILD_NUMBER`) on a
track the device can see; internal app sharing is the quickest way to try
one. The App Store page has not been opened from a store install.

### Local network

Services on the local network are found by DNS-SD through `@exact/local-network`
(LLP 1133.036; [recipe](recipes/local-network.md)) and reached with `fetch` or a
WebSocket. Three kinds of grant line:

- `device.local-network purpose.<key>`: the strings key's text is
  `NSLocalNetworkUsageDescription` on macOS, iOS and tvOS, the prompt Apple shows
  before the app first browses or reaches the local network (macOS 15 and later,
  iOS 14 and later; Apple TN3179).
- `net.dns-sd <type>`, one per service type the app browses or resolves
  (`_wled._tcp`; RFC 6335 §5.1), which needs the line above. It derives
  `NSBonjourServices` on Apple and `CHANGE_WIFI_MULTICAST_STATE` on Android. A
  browse or resolve of a type no line lists fails `SecurityError`.
- `net.fetch local-network`, `net.websocket local-network`: any host the URL writes
  as an RFC 1918, link-local or unique-local address, or a `.local` name, never a
  name that resolves to one. It derives `NSAllowsLocalNetworking`, so cleartext
  `http` reaches such a host. Beside `appTransportSecurity.allowsArbitraryLoads`,
  which already allows every host, the build leaves it out, since ATS ignores
  `NSAllowsArbitraryLoads` when `NSAllowsLocalNetworking` is present.
  `net.fetch local-network development` admits loopback
  too, in a development build only; a development bake writes the word itself, so
  a fixture at `127.0.0.1` answers under the agent, and a production bake refuses it.

`localNetworkStatus()` is a fact: `granted`, `prompt`, `denied` or `unknown`, where
`unknown` means "a reach may ask" (Apple reports no state, so each launch starts
there on iOS and macOS 15). A tool invocation, a wake or a package's event never
starts a browse, and its `fetch` or WebSocket to the local network is refused
before it is sent unless the fact is `granted` (`NotAllowedError`, message
`person-required` while undecided): on Apple at every hop a redirect leads to, on
the web at the first, with the redirect not followed. The app's own code and a
person's input are never refused: their reach is the ask. The web, Linux and
Windows have no browser (`available` false); a typed address still works there.

### Child processes (`@exact/child-process`)

A desktop app's data code starts programs with Node's names (LLP 1133.007):
`spawn(program, args, {exactStream, …})` is a resource's answer, a child that
lives while the resource holds it, and `execFile`, `execPath`,
`loginShellEnvironment()` and `appEnvironment()` are steps an answer awaits.
Each program is named by a grant line: `process.spawn path:ssh`,
`process.spawn /usr/bin/tar`, `process.spawn home:/.t3/runtime/versions` or a
`bundle:` helper; the call names it as its line does
(`spawn("path:ssh", …)`). A `path:` name runs the first program of its name
on the login shell's `PATH`, and none where an `fs.write` line lets the app
write (LLP 1133.007 D7, §9). The mapper receives `spawn`, `stdout` and `stderr`
chunks, `exit`, `close`, `drain`, `ready` and `ready-timeout` in order, with
that incarnation's handle (`write`, `end`, `kill`); a mutation writes through
a kept handle, staged with its commit. A `write` that answers `false` hears
`drain` once the child's stdin queue is empty, as Node's does. `exactRestart`
and `exactReady` keep a service alive and say when it serves. Only a person's
input or the app's own code starts, writes to, ends or signals a child (the
package declares those names `"origin": "app"`, as a module declares its
names). From a tool's call, a wake or the device's event, a start fails
`person-required`; a re-ask of a child's resource keeps the child, and the
arguments asked stay refused, by any origin, until they change (a refused
start's too); what such a commit leaves for `data_ready` keeps its origin
there; a handle's `write`, `end` or
`kill` answers as usual, and the operation is dropped from its batch and
journalled, the rest of the batch going. A tool's call ends `person-required`
at each of these. There is no headless origin (LLP 1133.002 D12): a call in a
session no one sees is a tool's or a wake's, and the rest of its code is the
app's. A plain `fetch` to `exact-process:` is refused. macOS and Linux
implement it; iOS, tvOS, Android, Windows and the web reject every name with
`NotSupportedError`, and `processStatus()` says which (`{available, state}`).

A child starts in a process group of its own with the default signal
dispositions and only its three pipes; releasing its resource, a `kill`, the
app's quit and its exit end the whole group, so a grandchild that holds the
pipes after its parent exits ends too. After a crash, the next launch ends a
group whose leader is still running; with the leader gone it ends the group
only when the app led its own session, so a Mac app launched from the Finder
(session 1) or from a terminal leaves that group's survivors running. `storage.fs`
reads and writes `home:/…` (the user's home, under a grant narrower than
`home:/`) and reads `bundle:/…` (the app bundle, read-only: on macOS its
`Contents`; on iOS, iPadOS and tvOS a folder holding only the trees
`host.ios.resources` declares, so `fs.read bundle:/<to>` reads the
same tree on both, and an iOS or tvOS build refuses a `bundle:/Resources/…`
line that reaches no tree either platform declares or the Mac build writes; a
line on the Mac's `Contents` (`bundle:/Info.plist`) builds, and its read on iOS
fails with the filesystem's not-found (an `Error`, code `ENOENT`) beside
declared trees. Where no root is installed (nothing declared, Linux,
Android), a `bundle:` read rejects with `NotSupportedError`, and so does a
`home:` read on iOS, iPadOS and tvOS, which have no home directory; `home:` is
macOS's and Linux's), following no
link below either root, nor below a native `fs.read` or `fs.write`
directory (one is refused, as under `app:/`); a native line is reached
through a system link such as macOS's `/var` but not through one the user
owns or could replace (`~/Dropbox`): such a line must name the link's target, and a native
line names a directory, not a file; and it gains
`lstat`, `readlink`, `access`, `chmod`, `symlink`, `digest(path, "SHA-256")`
and `flock(path, {timeout})`, whose lock is held until the lock's `unlock()`,
else until the answer that took it ends (a TypeScript source's only, for
now); `stat`
answers `mode` and `isSymbolicLink`. These are the TypeScript runtime 6: an
older prelude has none of them, so a call throws a plain `TypeError` and the
fields read `undefined`, and a module that names one needs `typescriptRuntime`
6 (one that reads only `stat().mode` is not detected). An `env.read
login:<NAME>` name is a shell variable's (`[A-Za-z_][A-Za-z0-9_]*`).

Under the agent nothing runs unless the drive names it: `--process
<target>=<executable>` runs that file in the target's place,
`--process <target>=script:<file.json>` answers from a script with no process
(the format is `host/executor/src/process_script.rs`'s), and
`--process <target>=real` runs the granted program itself. A script's events,
restart delays and readiness move with `clock`; a real child's run on the
wall, which `clock +<ms> real` waits for. `--login-env NAME=value`
(repeated) stands in for the login shell and `--home <dir>` for the home
directory. A tool's call (`call <tool> <json>`) runs as a tool's commit,
to see the refusal.
`packages/child-process/test/` holds a fixture app, its script and a helper
program that starts a grandchild.

### Bluetooth in a native module

An app's own native module can call CoreBluetooth on Apple or the Bluetooth
APIs on Android. Declare `device.bluetooth purpose.<key>` in the data module's
grants, with the purpose text in `strings/en.json` (and translations). The bake
refuses a missing or blank purpose, as it does for other device grants.

- iOS, tvOS and macOS get
  [`NSBluetoothAlwaysUsageDescription`](https://developer.apple.com/documentation/bundleresources/information-property-list/nsbluetoothalwaysusagedescription).
  A sandboxed Mac build also signs `com.apple.security.device.bluetooth`.
- Android gets `BLUETOOTH_CONNECT`, the `android.software.companion_device_setup`
  feature, and optional `android.hardware.bluetooth_le`. When `host.android.minimumOS`
  is below Android 12 (API 31), it also gets `BLUETOOTH` capped at API 30.
  The module uses CompanionDeviceManager for discovery and
  [requests connection permission](https://developer.android.com/develop/connectivity/bluetooth/bt-permissions)
  itself. This grant declares no scan or location permission.
- The served web's `Permissions-Policy` allows `bluetooth` for the app's origin.
  Linux and Windows get no host metadata.

For an iOS central that must receive peripheral updates while the app is in the
background, add `device.bluetooth-background` beside the Bluetooth line. It
takes no purpose key and adds `bluetooth-central` to `UIBackgroundModes` on iOS
only. Exact supplies no Bluetooth API, state restoration or Android background
service; the native module owns the calls and their lifetime (LLP 1067).

### Apple Health

Exact has no Health API: the app's own Swift module (LLP 1067) calls
HealthKit. What Exact does is let the binary ask. The app's grants name
`device.health-read purpose.<key>`, `device.health-write purpose.<key>`,
or both, each with its own strings key (LLP 1069.008.000):

- iOS gets `NSHealthShareUsageDescription` and
  `NSHealthUpdateUsageDescription` (one direction granted writes both, the
  other borrowing its text, which iOS never shows), and the
  `com.apple.developer.healthkit` entitlement in the signature, on a
  simulator too.
- macOS gets the two keys and no entitlement (it is restricted there, and a
  development build carrying it does not launch), so a module should treat
  a Mac's request as unavailable. tvOS, the web, Linux and Windows get
  nothing.
- A phone build needs a development profile for the app's own id with
  HealthKit turned on. A team wildcard never allows it, and the build
  refuses (`grant-device-profile`) rather than sign one that fails at its
  first request.
- An app with a Health grant keeps no answers across launches: its first
  frame never shows last launch's data from the store, and any kept answer
  on disk is forgotten at boot.

### Recording other apps' sound (macOS)

Exact has no API for it: the app's own Swift module (LLP 1067) makes a Core
Audio process tap ([Capturing system audio with Core Audio
taps](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps)).
The grant `device.system-audio-recording purpose.<key>` writes
`NSAudioCaptureUsageDescription` into a Mac build's Info.plist with the
strings key's text, which macOS shows when the module first asks. It adds
no entitlement, and no other platform gets anything.

### Sounds

A declared WAV (`sound "assets/…wav"`) is played by `playSound(src, at=,
gain=, group=, playbackRate=, pan=)`, `playSounds(hits)`, changed while it plays
by `setSounds(group=, gain=, playbackRate=, pan=, ramp=)` and ended by
`stopSounds(group=)` (LLP 1096; the rate and pan, LLP 1125 D10; `setSounds`,
LLP 1096 D14). A change starts at the commit's time and moves each value given
linearly over `ramp` ms (20 by default) from where it is; the runner follows each
voice's place in its file, so a rate change moves its end, which a group cuts
and the voice bound counts as any other end. The runner keeps the voice table, so what was scheduled, when, and how
each voice ended is the same on every host and under the driver's clock
(`state.sounds`, `expect sound`).

| host | output |
| --- | --- |
| web (both targets) | Web Audio: a one-shot `AudioBufferSourceNode` per voice at `start(when)` with its `playbackRate`, through a `StereoPannerNode` when its `pan` is not 0, aimed at the speaker through the context's output timestamp; a change is `setValueAtTime` then `linearRampToValueAtTime` on the voice's `gain`, `playbackRate` (k-rate: it moves once per 128-frame render quantum) and `pan`, a panner added when a change first gives a pan; the first sound after the page's first tap or key (one before it is dropped and journaled `sound blocked`) |
| macOS, iOS, tvOS | one `AVAudioEngine` with a C mixer behind an `AVAudioSourceNode`, sample-accurate (a playback rate reads between frames by linear interpolation; a pan follows the `StereoPannerNode`'s equal-power algorithm; a change's ramp is read at every frame), in an arm loaded on demand (`libexact_sound.dylib`; `ExactSound.framework` in an `.ipa`) |
| Android | AAudio with the shared C mixer in `libexact_sound.so`, loaded at the first play; scheduled by the stream's output timestamp, with gain, playback rate, pan, live changes and cancellation |
| Linux, Windows | no Contract sound output: the runner keeps the voice record |

Seekable drives keep Contract sounds silent (`output: "agent"`). Android
reader drives with `--live` play sounds through AAudio and read voice state
on platform time.
Their `state.sounds.output` reports `kind: "aaudio"`, the stream's state and,
while running, its estimated output latency in seconds. The runner initializes
the field to `"agent"`. Linux and Windows retain that value in ordinary
launches. When macOS, iOS and tvOS enable audio output, they report
an object whose `kind` is `"avaudioengine"` and whose `state` is `"closed"`,
`"running"`, `"stopped"` or `"failed"`. Both web targets report an object with
`kind: "web-audio"` and the audio context's `state` once the output loads; before
that, the JS target reports `{kind: "web-audio", state: "loading"}` and the wasm
target retains `"agent"`.

`app.json`'s root `audio_session` chooses the session every sound, video and canvas shares.
On iOS and tvOS, `"ambient"` (the default) obeys the ring/silent switch and
mixes with other apps' audio. `"playback"` ignores the switch and interrupts
other audio. `"playback-mix"` is for an instrument, toy or metronome played
over another app's audio: `.playback` with only
[`.mixWithOthers`](https://developer.apple.com/documentation/avfaudio/avaudiosession/categoryoptions-swift.struct/mixwithothers),
so it ignores the switch without stopping or ducking the other audio. It is
never Now Playing; an iOS media-session claimant requires `"playback"` itself.
Under either playback value the session is held
only while something has sound: the sound arm and a canvas's audio once they
start, a `video` or `audio` while it has a source and is not muted, and a
media-session claimant (`metadata=`, which Now Playing needs). With no capture
or speech claim and no call owning the session, muting or removing the last of
those returns a `"playback"` session to ambient, which mixes with other apps'
audio, as Bluesky's player does on re-mute; a `"playback-mix"` session already
mixes and keeps its category. Either is given up, so a paused podcast resumes,
once no player is left.
A call keeps its category, mode and options when a sound or player ends, and
leaves activation to the system until its last claim ends and the system
deactivates it. Either playback value can continue in the background with
`"audio"` in `host.ios.backgroundModes`.
Before a GPU module makes a surface, the host sets a session that nothing
holds to ambient, without activating it: a module's output that opens before
its `exact:audio` request is handled mixes and obeys the switch until the
request takes the app's category (`gpu/README.md`). Under `"playback-mix"` it
sets the app's own configuration instead, so the category never changes while
that output plays (a change then interrupts other apps' audio). The driver's frozen clock
skips preparation and opens no surface output. With `EXACT_AGENT_TIMING=platform`,
preparation runs, but the driver drops `exact:audio` requests, so they do not
raise the prepared ambient session.
On the web, page initialization sets the Audio Session API's type where supported:
`"ambient"` and `"playback"` pass through, and `"playback-mix"` maps to `"ambient"`: the
[W3C draft](https://w3c.github.io/audio-session/)
has no mixing playback type. This preserves other apps' audio but obeys the
ring/silent switch in WebKit. The policy applies to `video`, `audio` and GPU audio
without a `sound` declaration or an `AudioContext`. Without `audio_session`, the
browser keeps `auto`. `data-audio-session` keeps the manifest's value.
On Android, Contract effects use the host's single audio-focus owner.
`"ambient"` uses sonification attributes, mixes without focus and skips effects
while the ringer is silent or vibrating. `"playback"` uses media/music attributes
and requests full focus; `"playback-mix"` uses the same attributes without focus
or ducking. Device volume and Do Not Disturb still apply. All three stop output
and discard pending effects when the activity leaves the foreground or the
output becomes noisy (for example, headphones are unplugged). A transient focus
loss also stops effects; new effects may play after focus returns. A permanent
loss waits until the activity returns to the foreground.

On Android, `@exact/speech-synthesis` requests audio focus for its speech queue.
Set `speak`'s `audioSessionType` to `transient` to let music duck, or to
`transient-solo` to pause the other player temporarily so it can resume when
speech ends. Use `playback` for sustained listening. It takes permanent focus,
so the listener must restart the other player. [Android audio focus](https://developer.android.com/media/optimize/audio-focus).

The Android arm decodes declared WAVs at first play. It uses
[AAudio's speaker timestamp](https://developer.android.com/ndk/reference/group/audio#aaudiostream_gettimestamp)
to place frames; startup waits up to 100 ms for that timestamp, then uses the
stream's buffer size as a latency estimate if none is available. Late effects
start from their attack. The idle stream closes after its last frames reach the
speaker and releases its focus claim. A failed open or disconnected stream is
reported; a play at least one second after the failure tries again. MIDI output
and timed sequence haptics on Android remain unavailable.

On Android, `"audio_session": "playback"` lets an audible `audio` element
keep playing when the user goes Home or locks the device. Android's foreground
media service shows the playback notification and supplies a system pause
control. An audio element with empty bounds can play, including inside
a closed dialog or a canvas that composites its children. `exactPage().frozen`
stays false while the playback service is in the foreground. Pausing in the app
or through the system ends background playback for that element, as does
removing it or reaching the end of its source. Opted-in audio lets the display
sleep by default. Android pauses a `video` when the activity stops and keeps
its position.

The Mac has no session. A development run with `EXACT_SOUND_CHECK=1` taps the
engine's main mixer and journals (and prints) how far each onset after a
silence reached the speaker from its time.

**When the Apple output opens.** At the app's first voice, never at boot, so
declaring a `sound` costs a launch nothing. As a hand-built app starts its
engine on first use, that voice's commit activates the audio session (iOS and
tvOS), opens the output, reads every declared sound and starts the engine on
the main thread: the voice starts as late as that took (from its first frame,
never cut), about 100 to 200 ms on a Mac. An app that never plays and has no
GPU canvas never touches the audio session.

**When it cannot open** (no output device, a session that does not activate,
an engine that does not start or raises), the app keeps running silent: the
line `sound: the output did not open: <why>` is journaled (and printed),
`state.sounds.output` is `{state: "failed", error, skipped}`, the voices
are skipped, and the first voice a second or more later tries again. A
failed output gives its hold on the audio session back, so it does not keep
other apps' audio paused. An interruption that ends with
`shouldResume` restarts the engine (a failure fails the same way); one
without it (the person started other audio) is left to the next voice. An
engine the system stopped (a route or format change, a media services reset)
is opened again by the next voice; one that stops again within a second of
opening is a failure. Outside the driver `state.sounds.output.state` is
`closed` before the first open, then `running` or `stopped`. One failure is
beyond any app's catch: when CoreAudio's own RPC to the audio server times
out (`_ReportRPCTimeout`, seen on iOS simulators whose audio is wedged), it
aborts the process inside `AVAudioEngine`; opening at the first voice keeps
that from launch and from an app that plays nothing.

**A sequence** (`sequence NAME when cond` holding `play(tracks, bpm=…)`, LLP
1125 D2–D5) is expanded by the runner, never by a host: its steps become
voices in the same table a second ahead of the runner's clock, refilled when
half a second is left on a wake of its own that runs no action and that
`clock settle` does not wait for, so a main-thread stall of up to 500 ms
loses no step. The bound is the same for a sequence's voices and a tap's: at
most 32 voices sound at any voice's start (another is dropped and journaled),
and at most 256 wait to start (another tap is dropped; a sequence stops
expanding early and wakes when the first starts, dropping nothing).
Retracting a future group voice restores the preceding voice's pending
cut after the commit's commands and sequence expansion. If that extension
exceeds the bound, excess future voices are cancelled and journaled;
sounding voices play on. At each start, the last admitted voices are
cancelled first.
The Apple mixer has 288 slots and a ring of
1024 ops for it (a Play that finds no free slot takes one whose voice ends inside the buffer, mixing that voice's last frames first), and frees a voice
retracted before it starts at once; the web lets its node go. A step's haptic
plays on the main thread at its time (Apple's feedback generators;
`navigator.vibrate` on the web, where it exists): a blocked main thread makes
it late, and `state.sounds.output.hapticsLate` counts those more than 10 ms
late. Linux and Windows keep the record. On iOS and tvOS a sequence stops
when the app goes to the background (`exactTransport().reason` is
`background`), unless `audio_session` is `"playback"` or `"playback-mix"` and
`host.ios.backgroundModes` has `"audio"`; a Mac app and a web page play on.
`state.transport` is the record: what `exactTransport()` answers, the sequence
that last played, how far its hits are issued (`issuedTo`), its last 16 tempo
segments, and its last 64 haptics with their states.

**Clock in** (`play(…, clock="<input id>")`, LLP 1125 D9) follows a MIDI
input's clock. The input's ticks, Start, Continue, Stop and Song Position
reach the runner with their arm timestamps in its ordinary input deliveries:
ticks alone at most 50 ms after the oldest, anything else at once with the
ticks ahead of it. The runner takes the tempo from the last 48 tick stamps;
an estimate above 2,500 BPM leaves the previous estimate, or the `bpm=` prior,
standing. Before each fill, a track that would loop more than 1,000 times a
second at the followed tempo is dropped and journaled. The runner counts
the position in ticks and issues hits only to 100 ms past each
delivery (the batch plus the 50 ms hold), so a hit is timed from the newest
estimate; a new tempo or phase that moves a hit not yet within the hold by
more than 2 ms takes the hits back (`by clock`) and issues them again. The
record is a function of the deliveries, so a replay of them is the same take.
Only the followed input's ticks are passed on (Apple's hub and midi-glue.js
drop the rest, and Active Sensing always); nothing ends a lost clock but the
hits running out. `state.transport.follow` shows the input, the ticks heard,
the window, the estimate, the ticks since Start and where Stop held it.

**MIDI** (`exactMIDI()`, `requestMIDIAccess()`, `sendMIDI`, a step's note and
`clockOut=`, `midimessage`; LLP 1125 D6–D8) needs the grant `device.midi purpose.midi`
on every host, `purpose.midi` a key of `strings/en.json` (the purpose the person is shown). The runner keeps one queue for every message, a sequence's notes and
clock ticks included, and a host hands each to its output 50 ms before it is due
and holds it until then: CoreMIDI could take it early and flush, but
`MIDIFlushOutput` empties the whole destination and Chromium has no
`MIDIOutput.clear()`. So an edit or a stop takes back what is held, and a note
already handed gets its note-off (a stop moves it to the stop). At most 1,024
messages wait; note-offs, Stop and ticks are never refused. Per host:

- **Apple** (macOS 14, iOS 17): a loaded arm, `libexact_midi.dylib` (the
  `ExactMIDI` framework on iOS), built only for a plan that uses MIDI. CoreMIDI
  runs on the arm's own thread and run loop, made at the first
  `requestMIDIAccess()` (no prompt: the state is `prompt` until then), so its
  notifications arrive and a cold MIDIServer never blocks the main thread. Port
  ids are `kMIDIPropertyUniqueID`, so a replugged device keeps its id; sends
  hold on the arm's thread (a main-thread stall does not delay them) and go out
  with `MIDISendEventList` stamped in host time; inputs are stamped from the
  packet's own time. It enumerates again when the app comes to the foreground
  and on a 3 s watchdog while a port is in use. tvOS: `unavailable`.
- **Web**: Web MIDI through midi-glue.js, loaded after the first frame for a
  plan that uses MIDI; `granted` without asking when the permission already
  holds; held sends on the page's timer, so a main-thread stall over 50 ms makes
  those messages late (a declared limit, counted in `state.midi.output.late`).
  WebKit, and so every iOS browser, is `unavailable`. SysEx is never asked for.
- **Linux, Windows**: the record; a request finds `unavailable`.

Under the driver nothing is sent: `prefer midi.*` names the ports a request
finds, `tap midi` delivers a message (`tap midi "<input>" clock <bpm> <ms>`
its clock, a 50 ms slice at a time), and `state.midi` is the record — the ports,
the inputs the listeners open, how many messages wait, the last 64 queued with
their states (`waiting`, `handed`, `retracted` and why), the last 64 heard with
their beat, and clock out's tick count.

### Media session

An `audio` or `video` with `metadata=MediaMetadata(…)` claims the platform's
media session, and the six actions it binds (`seekbackward`, `seekforward`,
`seekto`, `previoustrack`, `nexttrack`, `stop`) are the controls offered (LLP
1098). Play and pause are always offered and act on the element; the position
and playback state are the player's. The owner is the claimant that most
recently started playing, across every session of a process on Apple.

| host | published through |
| --- | --- |
| web (both targets) | `navigator.mediaSession`: the metadata (the artwork resolved as the page resolves an asset), the handlers, the declared `playbackState` and `setPositionState`; the browser decides where it shows (Chrome's media hub, the system's Now Playing) and routes the media keys; under the driver too, in the driver's own browser |
| macOS, iOS, tvOS | `MPNowPlayingInfoCenter` and `MPRemoteCommandCenter`, from the video arm (AVKit's own publication is off while a claimant owns the session); under the driver nothing is assigned and `state.mediaSession.published` is `"agent"` |
| Android | the system playback session follows audible `audio` with `audio_session: "playback"`; Contract metadata and custom actions stay in `state.mediaSession` |
| Linux, Windows | none: the record only (`published: "none"`, no `play` or `pause`) |

On iOS the lock screen and Control Center show only a non-mixable playback
session, and audio stops at the lock without the `audio` background mode, so
an app whose plan claims the media session states both in `app.json`, or
`build.mjs --ios` refuses it:

```json
{
  "audio_session": "playback",
  "host": { "ios": { "backgroundModes": ["audio"] } }
}
```

`state.mediaSession` reads `owner` (a view id), `testId`, `claimants`,
`metadata` (as authored), `actions`, `seekOffsets`, `playbackState`, `position`,
`published` and `readback` (what the platform holds: on the web the page's
declaration, `playbackStateDeclared`); `artworkError` says why an artwork was
not published. `tap <element> mediasession <action> [seconds]` calls the handler
the platform would call (`delivery: "substituted"`).

### Keep the screen awake

`wakelock="screen"` on any element keeps the display from dimming or locking
for idleness while that element is mounted and displayed: neither it nor an
ancestor is `display: none` (or laid out so by its host, as an unopened tab
is), inside no `dialog` or popover its host has not opened, and under no tab
the navigation leaves unselected and no screen a stack keeps under its top,
so a screen kept mounted in a hidden tab panel holds nothing; the page a
sheet covers, and every sheet between it and the top, is still shown, and
holds. An exit
animation's ghost holds nothing (LLP 1125
D11). Natively an element the host shows as its own control (a header in the
navigation bar, a confirmation as a native alert, a menu as a native menu)
holds nothing, because a whole host cover marks both what the host hides and
what it shows natively and the kernel cannot tell them apart; the web holds
there. `"none"` is the default. Bind it to what needs the screen:
`wakelock=(playing ? "screen" : "none")`. A literal other than the two words
is refused; a computed one is journaled (`view N: invalid wakelock value …;
unset`) and holds nothing. The runner decides which elements hold it
(`exact_runner::wakelock`), so the rule is one on every host.

| host | held through |
| --- | --- |
| web (both targets) | `navigator.wakeLock.request("screen")`, asked again on `visibilitychange` when the page is visible again (a hidden page loses it) and released when no element holds it; `wakelock: unavailable: …` is journaled once where the API is missing or a request is refused |
| iOS, tvOS | `UIApplication.isIdleTimerDisabled`, true while any session's element holds it; under the driver the timer stays disabled for the whole drive |
| macOS | an `IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleDisplaySleep)` assertion, held while any session's element holds it |
| Android | `View.setKeepScreenOn` on the reader's view (request kind 8) |
| Linux, Windows | none: `wakelock: unavailable: this host keeps no display awake`, journaled once |

`state.wakelock` is `{held, by}`: whether any element holds it and their view
ids, on every host's driver.

### Lock the screen orientation

`orientationlock` on any element keeps the window to the orientations it
names while that element is displayed, by the wake lock's rule above, and
outside every `dialog` and popover (LLP 1133.029).
The words are the Screen Orientation API's lock types, `portrait`,
`landscape`, `portrait-primary`, `portrait-secondary`, `landscape-primary`,
`landscape-secondary`, `natural` and `any`, and `current`, the orientation
the window has when the lock begins; `"none"`, the default, holds nothing.
Bind it to what needs it: a game steered by tilting the phone keeps the
orientation the player holds while tilt is on,
`orientationlock=(tilt ? "current" : "none")`.

When several displayed elements hold a lock, three rules decide which
count. The topmost sheet that holds wins over the routes beneath it, so a
sheet can turn a locked screen and a sheet that holds nothing leaves the
lock beneath it. A holder with a holder among its descendants is replaced
by it, as an inherited CSS property is: a portrait app's root says
`orientationlock="portrait"` and one screen under it says `"landscape"`;
`"any"` frees a subtree. The holders left intersect, and holders that share
no orientation hold none: the journal says `orientationlock: conflict: view
12 "portrait", view 30 "landscape"; none held`.

A type narrows the manifest's `orientation` and never widens it: iOS refuses
a screen whose orientations fall outside the app's Info.plist. So a literal
type that shares no orientation with the manifest's (`"landscape"` in a
`"portrait"` app) is refused at bake (`orientationlock-outside-manifest`):
widen the manifest and lock the screens that need portrait. A literal or
computed lock inside a `dialog` or popover is refused too
(`orientationlock-in-top-layer`): whether a box is open is the host's
state, and a dialog over a locked screen leaves its lock. A computed word
outside the table is journaled (`view N: invalid orientationlock value …;
unset`) and holds nothing.

| host | held through |
| --- | --- |
| iOS | the root and every presented controller's `supportedInterfaceOrientations`, turned by `requestGeometryUpdate`; `current` is `prefersInterfaceOrientationLocked` from iOS 26 and the scene's orientation before |
| iPadOS | `current` alone, in full screen (iPadOS 26 and later): a multitasking app supports every orientation, so iPadOS asks no controller for a type |
| Android | the activity's `requestedOrientation` (request kind 13): the manifest's mapping for a type, `locked` for `current`; on a screen 600 dp wide or wider Android 16 ignores a type, and `current` holds |
| web (both targets) | `screen.orientation.lock()`, where the browser locks: in fullscreen, on Android's Chrome and Firefox; asked again on entering fullscreen and on becoming visible. `orientationlock: unavailable: <DOMException name>` is journaled once where it refuses (desktop Chrome, Safari) |
| Windows | the process's `SetDisplayAutoRotationPreferences`, which a convertible or a tablet follows |
| macOS, tvOS, Linux, the terminal | none: `orientationlock: this host has no screen orientation`, journaled once |

`state.orientationlock` is `{held, by, words, lock, conflict, applied}`: the
counted holders' views and words, the lock the host is told, the holders of
a conflict, whichever side found it, and `applied`, what the host asked the
platform for and whether it holds (`"unavailable"` where there is none). A
request is not an effect: a drive judges a lock by the orientation the
window keeps (`layout`'s `screen.orientation` on iOS), and reads `applied`
for why.

### The displays and the pointer (`@exact/window-management`)

`getScreenDetails()` answers the displays by the Window Management API's
names (`screens`, `currentScreen`, each `ScreenDetailed`), a live source that
answers again when the displays or the window's display change;
`getPointerPosition()`, a send, answers `screenX`, `screenY`, `clientX`,
`clientY` and the display holding the pointer with its `clientRect` in the
viewport (LLP 1178). The line is `device.window-management`; only the web
asks, from a press. [Recipe](recipes/displays.md).

| host | read through |
| --- | --- |
| macOS | `NSScreen.screens` flipped at the primary's top, `visibleFrame` for the work area, `CGDisplayIsBuiltin`, `localizedName`; the pointer from `NSEvent.mouseLocation` |
| web | the browser's `window.getScreenDetails()` (Chromium on the desktop), `screenschange`, `currentscreenchange`; no pointer (`NotSupportedError`); elsewhere `available: false` |
| Linux | the one display the host fills: the KMS connector (`label` its name, `isInternal` for eDP, LVDS and DSI) or the headless buffer, at the host's scale; the pointer the host draws |
| Windows | `EnumDisplayMonitors` and `GetMonitorInfoW` (`rcMonitor`, `rcWork` for the work area), each scale `GetDpiForMonitor`'s times the Text size setting, laid out into CSS pixels as Chromium lays monitors out, so the numbers are a page's in Chrome on the same machine; `label` (the EDID name, else empty) and `isInternal` from `QueryDisplayConfig` (an LVDS or DSI panel is internal here and not in Chrome); the pointer from `GetCursorPos`; `clientRect`, `clientX` and `clientY` from physical pixels over the window's own scale, which the Text size setting does not change. A run with no window (an agent's session) reports the buffer it paints as its one display, as the Linux host does |
| iPadOS, Android | not built: `NotSupportedError` |

### Still desktop capture (`@exact/screen-capture`)

`captureScreens({directory, type?, quality?})` writes a still of every
display, in display-list order, without a picker or window activation
(LLP 1178). It answers `{captures: [{path, width, height, type, screen}]}`:
physical pixel sizes and the corresponding `ScreenDetailed`, with
`clientRect` in the session window's CSS pixels (null without a window).
`directory` is an `app:/` root such as `app:/tmp` or a directory below one.
Every call writes a fresh file per display. Use the returned paths to draw or
remove those files. The default format is PNG. Encoding options follow
`captureCanvas`. The grants are `device.screen-recording <usage-key>` and
`fs.write <directory>`; drawing a file also needs `fs.read`.

`screenRecordingStatus()` is a live `{available, state}` fact.
`requestScreenRecording()` requests permission from a foreground app;
capture itself refuses without permission and never prompts. A tool or
wake cannot request or capture. Files publish in call order after all
encodes finish; cancellation before publication leaves previous files.
A failure before publication preserves every previous file; a failure
between renames reports `EncodingError` and may leave some files new.
Limits are 8,192² pixels per display and 2²⁷ per call, checked before
allocation (`QuotaExceededError`). Unsupported encodings fall back to PNG;
JPEG is supported on both desktop hosts.

On macOS, status is `prompt` until this install has asked, `denied` after an
unsuccessful request, and `granted` when the OS permits capture. It is read
again when the app becomes active. A denial is changed in System Settings.
See the [capture recipe](recipes/screen-capture.md) for setup and fixtures.

| host | still capture |
| --- | --- |
| macOS 14+ | `SCScreenshotManager`, sRGB at backing pixels, without the pointer; Screen Recording permission, checked at each capture |
| Windows | per-monitor `BitBlt` with `SRCCOPY \| CAPTUREBLT`, physical pixels, no pointer or permission dialog; GDI's SDR colors, tagged sRGB |
| web, Linux | deterministic fixture images under the agent; real capture answers `NotSupportedError` |
| mobile, TV, watch, terminal | unavailable |

### Pick a file of any type

An `input type="file"` with no `accept` (or `accept="*/*"`) offers any file;
`multiple` allows more than one. Tap a visible input or call `showPicker(id)`
from an action to open it. `change` receives `Picked[]` with each file's name, type, size and
`app:/tmp/picked/` path. The copy at `path` is named `<id>.<ext>`, `<id>` from the
launch and a counter, never after the file: show or upload it under `name`. A
specific `accept` list is limited to media types and types in the app's
`file_handlers`. The live Linux picker is unavailable;
its agent substitute accepts any file. Android uses the system photo picker for
`image/*`, `video/*` or both wildcards on Android 13
or Android 11 with R extension 2, and `ACTION_OPEN_DOCUMENT` otherwise. `multiple`
works with either. Files are copied before `change`; the app needs no persistent
access to the provider. Cancellation,
a launch failure or a copy failure fires `cancel`, with failures journaled. A missing
Photo Picker falls back to the document picker once. An unknown extension in `accept`
uses an unrestricted document picker, then the selection is checked against `accept`.
HEIC/HEIF is kept when accepted (including `image/*`); otherwise an image accept that
requires D6's JPEG conversion receives JPEG at quality 0.9, with orientation applied
and source metadata removed. A specific list containing `image/jpeg`, `.jpg` or
`.jpeg` uses the document picker and includes `image/heic` and `image/heif` alongside
the requested types. A PNG-only filter offers no HEIC conversion. The delivered result is checked against `accept`. Provider
metadata, stream opening and copying run on a worker under the result's temporary
read grant. Access lasts while the receiving activity's task stack remains active;
finishing the task can end it before copying completes. No persistable permission
is taken or released. Under the agent the picker stays held. Image pixel readback and a canvas's
synchronous readback (`getImageData`, `toDataURL`, `toBlob`) remain deferred;
a canvas's pixels go to a file (below).

### A 2D canvas's pixels as an image file (`captureCanvas`)

`captureCanvas(surface, path, type, quality)` writes what the host has drawn and
kept for the one mounted 2D canvas of `surface` (every draw applied, its images
and text as drawn, an accumulating drawing whole) as an image at the `app:/`
file `path`, under `fs.write`, and resolves `{path, width, height, scale, type}`:
the size in device pixels, the device pixels per CSS px (1 for a canvas with
`bitmap-width` or `bitmap-height`), and the media type written. `type` and
`quality` are `canvas.toBlob`'s: `type` defaults to `image/png`, and `quality`, a
number from 0 to 1, sets a lossy type's quality (absent or out of range, 0.92
for a JPEG, as browsers have it). Every host writes `image/jpeg`, with its system encoder where it has one;
a type the host does not encode is written as a PNG, as `toBlob` falls back, so
check the answer's `type`. Beyond PNG and JPEG, Android writes `image/webp`
(`Bitmap.compress`), macOS and iOS what ImageIO encodes (`image/heic` and
`image/avif` among them, not WebP), and the web what the browser's `toBlob`
does (`image/webp` in Chrome and Firefox). A capture takes the first completed drawing at or after the
commit that requested it. If another drawing completes before the host snapshots,
it takes that later drawing. Later commits, advances and animation frames never
retire a pending capture. Unmounting the canvas, replacing its surface or ending
the session retires it with `EncodingError` and leaves the file untouched.
The image is sRGB; a PNG has straight alpha, and a JPEG, which has none, is the
drawing over opaque black, as the HTML standard serializes a canvas to a type
without alpha. A completed draw with no operations keeps the bitmap,
transparent when nothing has painted it. No pixel
crosses the source: show the file with
an `image`, send it with `fetch(url, {exactBodyFrom})`, or hand it to a module
(LLP 1056 D9):

```ts
export const grants = 'fs.write app:/tmp';
// in a mutation's answer: the annotation as drawn, for the clipboard
const shot = await captureCanvas('annotate', `app:/tmp/shot-${seed}.png`);
// a photo's capture, smaller as a JPEG
const photo = await captureCanvas('annotate', `app:/tmp/photo-${seed}.jpg`, 'image/jpeg', 0.85);
```

Call it from a live answer, directly or after `await`. On the JS web target,
a plain `.then()` callback has no answer owner and capture rejects with
`InvalidStateError`. Cancelling the owning answer prevents publication.

A Rust source answers `Answer::Later(Request::capture_canvas(surface, path))`, or
`Request::capture_canvas_as(surface, path, "image/jpeg", Some(0.85))` for another
type, and reads the reply with `exact_runner::canvas_capture::captured_image(&outcome)`.
`path`'s directory must exist (`app:/tmp/x.png`, not `app:/tmp/new/x.png` before
a `mkdir`); the file is replaced whole or left as it was. A refusal is a
`DOMException` (in Rust, `"<name>: <message>"`): `NotFoundError` when no canvas of
the surface is mounted, `InvalidStateError` when several are or its size was
refused, `IndexSizeError` when it has no bitmap (a box with no area),
`QuotaExceededError` over 8,192² pixels, `NotSupportedError` for a `display-p3` or
`float16` canvas or a drive with no store (`--storage <name>`), `NotAllowedError`
for a path that is not an `app:/` file under `fs.write`, and `EncodingError` when
the encoder or the write fails. Linux, Windows and Android encode the tiny-skia
bitmap (a JPEG with WIC on Windows, `Bitmap.compress` on Android and the
`jpeg-encoder` crate on Linux, which has no system encoder), macOS and iOS the
Core Graphics bitmap or the GPU's surface with ImageIO, and the web the canvas
element's own `toBlob`.

For a PDF picker, declare the type in `app.json`:

```json
"file_handlers": [{ "action": "/", "accept": { "application/pdf": [".pdf"] } }]
```

In `app.contract`, receive the selected file's display name and app path.
Cancelling keeps the previous choice:

```text
component PdfPicker
  state name = "No PDF selected"
  state path = ""
  state cancels = 0
  action picked(files: Picked[])
    name = match files.at(0) { case some(f) => f.name, case none => name }
    path = match files.at(0) { case some(f) => f.path, case none => path }
  action cancelled
    cancels = cancels + 1
  view
    column
      input type="file" id="pdf" testId="pdf" accept="application/pdf" change=picked cancel=cancelled aria-label="Choose a PDF"
      text name testId="filename"
```

Put a PDF at `fixtures/report.pdf` beside `app.test.contract`, then run
`bun exact.mjs test web`. The test's `pick` answers the picker that `tap`
opened; paths are relative to the test file:

```contract-test
test "choose a PDF and cancel the next picker" on web
  tap "pdf"
  pick "pdf" "fixtures/report.pdf"
  expect text "filename" == "report.pdf"
  tap "pdf"
  pick "pdf" cancel
  expect state cancels == 1
  expect text "filename" == "report.pdf"
```

The picker needs no grant. Reading `path` through `storage.fs.readFile`
needs `fs.read app:/tmp/picked`. In a drive, `tap pdf` followed by
`type @pick fixtures/report.pdf` selects a file; `tap @pick cancel` dismisses it.

### Shrink a picked image for upload (`storage.fs.compressImage`)

`storage.fs.compressImage(from, to, {maxDimension, maxBytes, quality, type})` decodes the image
at `from`, scales it so its longer side is at most `maxDimension` pixels, and
writes a JPEG of at most `maxBytes` bytes at `to`, searching JPEG quality the way
Bluesky's composer does (from quality 51 down and up; when 51 and 26 both miss,
the size shrinks by 0.8, at most four sizes). Orientation is applied, and none
of the source's metadata is written (no EXIF, GPS or orientation); transparency
becomes white and the colour is sRGB. With `type: 'image/png'` it writes a PNG
instead, which keeps transparency: PNG has no quality, so it is encoded at the
fitted size and, while over `maxBytes`, the size shrinks by 0.8, at most four
times. It resolves `{path, type, size, width, height}`, `type` the one written
(LLP 1069.002 Amendment A1, A1.11).

`quality`, from 0 to 1 as `canvas.toBlob` takes it, sets the JPEG quality
instead of leaving it to the search (A1.12). With `maxBytes` it is a ceiling:
each size is encoded at `quality` first, and when that is over `maxBytes` the
search runs below it, then shrinks as above. Without `maxBytes` it is one
encode at `quality` at the fitted size (at most 64 MiB). One of `maxBytes` and
`quality` is required. A PNG ignores `quality`.

```ts
export const grants = 'fs.read app:/tmp\nfs.write app:/tmp';
// files from `change` on an `input type="file"`; posts: 4000 px and 2 MB
await storage.fs.mkdir('app:/tmp/upload');
const out = await storage.fs.compressImage(files[0].path, `app:/tmp/upload/${seed}.jpg`,
  { maxDimension: 4000, maxBytes: 2_000_000 });
const body = await storage.fs.readFile(out.path);
// a thumbnail at a quality of its own, no byte limit
const thumb = await storage.fs.compressImage(files[0].path, `app:/tmp/upload/${seed}-thumb.jpg`,
  { maxDimension: 400, quality: 0.7 });
```

Both paths are `app:/` files (`fs.read` on `from`, `fs.write` on `to`); create
`to`'s parent directory first with `storage.fs.mkdir`. `to` is replaced atomically,
and left as it was on any failure. It always re-encodes, so
compare the pick's `size`, `width` and `height` first to skip it. It is one
storage operation in the module's queue; natively it runs off the JS thread. A bad option
(a `type` other than `'image/jpeg'`, the default, or `'image/png'`; a
`quality` that is not a number from 0 to 1; neither `maxBytes` nor `quality`)
rejects with a `TypeError`; otherwise a refusal's `code` is `denied`, a
filesystem code, `failed` (a path that is not `app:/`), `too-large` (over 64 MiB,
or a header over 64 Mi pixels, checked before decoding), `undecodable`, `unfit`
(nothing fits `maxBytes`), `timeout` (the search's 20 s, or a native wait that
ran out with nothing written) or `unsupported`. macOS and iOS encode with
ImageIO (an HDR photo is tone-mapped), the web with a canvas (in the page or the
module's worker; JPEG, PNG, GIF, WebP and BMP, never HEIF or AVIF, whose size it
cannot read before decoding), and Android with `ImageDecoder` and `Bitmap`.
Linux and Windows have no image encoder and answer `unsupported`, so
upload the original when it already fits. Under the agent it runs for real with
`--storage <name>`.

`rotate`, `flip` and `crop` edit the picture before the search, with
`transformImage`'s meanings (below), so an avatar cropped from a turned photo
is one call; a crop that misses the picture rejects with `code` `outside`
(LLP 1133.019 D15).

### A camera track's calibration

Calibration stays in a package. A package can read the camera behind an open
`@exact/media-capture` video track; it needs no new core API and opens no second
camera. Keep a consumer, such as `exact-camera`, showing the track while reading.

The complete example is [@fixture/camera-calibration](../packages/fixtures/camera-calibration/).
Its [interface](../packages/fixtures/camera-calibration/exact.json),
[package exports](../packages/fixtures/camera-calibration/package.json),
[Android implementation manifest](../packages/fixtures/camera-calibration/android/exact.json)
and [Apple implementation manifest](../packages/fixtures/camera-calibration/apple/exact.json)
are part of the fixture. Copy it into your package, or add it to an app by path:

```sh
bun add @fixture/camera-calibration@file:/absolute/path/to/exact/packages/fixtures/camera-calibration
```

Pass a video track ID, not its stream ID. For example, place this component beside
the viewfinder after the app's `getUserMedia` has returned a track:

```contract
use getCameraCalibration, CameraCalibration from "@fixture/camera-calibration"

component App
  view
    Calibration(track="camera-track")

component Calibration
  props
    track: string
  resource calibration = getCameraCalibration(track=track) as shape CameraCalibration else empty()
  action read
    refresh calibration
  view
    column
      button press=read
        text "Read calibration"
      text calibration.reason
```

The read is a snapshot. Refresh it after the viewfinder starts, and after changing
camera, format, focus or zoom. The fixture's agent substitute returns empty arrays
with `reason="agent"`; bind the implementation to inspect a real camera.

| Field | Meaning |
|---|---|
| `intrinsicMatrix` | Nine row-major values, `[[fx, skew, cx], [0, fy, cy], [0, 0, 1]]`; focal lengths and principal point in reference-frame pixels |
| `referenceFrame`, `referenceDimensions` | The matrix's coordinate system and its width and height |
| `distortion` | Android's five `LENS_DISTORTION` coefficients, in the platform's order; empty on the iOS video path |
| `focalLengths` | Android's available focal lengths in millimetres, not the current frame's focal length |
| `sensorSize`, `pixelArraySize` | Android's physical sensor width and height in millimetres, and full pixel-array width and height |
| `preCorrectionActiveArray` | Android's `[left, top, right, bottom]` rectangle in the full pixel array |
| `deviceId` | The native camera identifier, for associating the answer with the camera |
| `reason` | Empty when intrinsics are present; otherwise why they are absent. Other fields may still be present |

An empty array means unreported or unavailable, never zero distortion or a guessed
calibration. Record the reference dimensions and camera identity with the matrix.

#### Android

`VideoSource.camera` in [Tracks.kt](../host/android/canvas/src/main/java/dev/exact/canvas/Tracks.kt)
is the bound CameraX `Camera`. The package's manifest names the same
`androidx.camera:camera-camera2:1.6.2` as the capture package, so it can use
[Camera2CameraInfo](https://developer.android.com/reference/androidx/camera/camera2/interop/Camera2CameraInfo)
and `getCameraCharacteristic`. Read on the main thread through the track table.

`LENS_INTRINSIC_CALIBRATION` is `[fx, fy, cx, cy, skew]`. Its origin is the top-left
of `SENSOR_INFO_PRE_CORRECTION_ACTIVE_ARRAY_SIZE`, before distortion correction,
not the full sensor's origin and not the preview's. `LENS_DISTORTION` is
`[k1, k2, k3, p1, p2]` in Camera2's model. Many devices omit either or both;
keep null characteristics empty. See [CameraCharacteristics](https://developer.android.com/reference/android/hardware/camera2/CameraCharacteristics#LENS_INTRINSIC_CALIBRATION).

This is the complete Kotlin implementation, compiled by the reader's Kotlin unit
tests. It reads the camera characteristics without binding another
use case or taking another camera lease:

```kotlin
package fixture.calibration

import android.hardware.camera2.CameraCharacteristics as Keys
import androidx.camera.camera2.interop.Camera2CameraInfo
import androidx.camera.camera2.interop.ExperimentalCamera2Interop
import androidx.camera.core.Camera
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject

class CalibrationModule(context: ExtensionContext) : ExactExtension(context) {
    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        if (name != "getCameraCalibration") return reply(JSONObject().put("error", "NotSupportedError"))
        val track = context.tracks.lookup(args.optString("track"))
        val camera = track?.video?.invoke()?.camera as? Camera
        reply(if (camera == null) empty("track-not-running") else read(camera))
    }

    companion object {
        fun empty(reason: String) = JSONObject().apply {
            put("reason", reason)
            put("deviceId", "")
            put("referenceFrame", "")
            for (key in listOf("intrinsicMatrix", "distortion", "focalLengths", "sensorSize",
                "pixelArraySize", "preCorrectionActiveArray", "referenceDimensions")) put(key, JSONArray())
        }

        @OptIn(ExperimentalCamera2Interop::class)
        fun read(camera: Camera): JSONObject {
            val info = try { Camera2CameraInfo.from(camera.cameraInfo) }
                catch (_: IllegalArgumentException) { return empty("camera2-unavailable") }
            val k = info.getCameraCharacteristic(Keys.LENS_INTRINSIC_CALIBRATION)
            val distortion = info.getCameraCharacteristic(Keys.LENS_DISTORTION)
            val focal = info.getCameraCharacteristic(Keys.LENS_INFO_AVAILABLE_FOCAL_LENGTHS)
            val sensor = info.getCameraCharacteristic(Keys.SENSOR_INFO_PHYSICAL_SIZE)
            val pixels = info.getCameraCharacteristic(Keys.SENSOR_INFO_PIXEL_ARRAY_SIZE)
            val active = info.getCameraCharacteristic(Keys.SENSOR_INFO_PRE_CORRECTION_ACTIVE_ARRAY_SIZE)
            return empty(if (k?.size == 5) "" else "intrinsics-not-reported").apply {
                put("deviceId", info.cameraId)
                put("referenceFrame", "pre-correction-active-array")
                if (k?.size == 5) put("intrinsicMatrix", JSONArray(listOf(
                    k[0], k[4], k[2], 0f, k[1], k[3], 0f, 0f, 1f)))
                if (distortion != null) put("distortion", JSONArray(distortion.toList()))
                if (focal != null) put("focalLengths", JSONArray(focal.toList()))
                if (sensor != null) put("sensorSize", JSONArray(listOf(sensor.width, sensor.height)))
                if (pixels != null) put("pixelArraySize", JSONArray(listOf(pixels.width, pixels.height)))
                if (active != null) {
                    put("preCorrectionActiveArray", JSONArray(listOf(active.left, active.top, active.right, active.bottom)))
                    put("referenceDimensions", JSONArray(listOf(active.width(), active.height())))
                }
            }
        }
    }
}
```

These are static, default-pixel-mode sensor characteristics. They are insufficient
by themselves to map a processed video pixel back to a ray: account for crop,
scaling, rotation, mirroring, zoom and distortion correction. A logical camera may
switch physical lenses. A package recording frames should also retain the frame's
capture-result metadata, camera identity and image transform; the available focal
length list is not a per-frame measurement. Camera2's maximum-resolution pixel mode
has separate calibration keys.

#### iOS

Attach an `AVCaptureVideoDataOutput` through
[`ExactVideoSource.configure(stopped: true)`](../host/apple/modules/ExactPackage.swift).
With `stopped: true`, Exact stops a running session for this configuration and
resumes it afterwards;
Apple requires intrinsic delivery to be enabled before `startRunning()`. Check
[`isCameraIntrinsicMatrixDeliverySupported`](https://developer.apple.com/documentation/avfoundation/avcaptureconnection/iscameraintrinsicmatrixdeliverysupported)
before setting
[`isCameraIntrinsicMatrixDeliveryEnabled`](https://developer.apple.com/documentation/avfoundation/avcaptureconnection/iscameraintrinsicmatrixdeliveryenabled).
Support depends on the active format and connection, including stabilization.
The `.photo` session preset does not itself request calibration.

Read [`kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix`](https://developer.apple.com/documentation/coremedia/kcmsamplebufferattachmentkey_cameraintrinsicmatrix)
from each delivered sample you need to associate with a frame. Its `CFData` holds
a column-major `matrix_float3x3`, whose SIMD columns contain padding; it is not nine
packed floats. The focal lengths are in pixels, with origin at the frame's upper
left. The fixture copies the native matrix and serializes nine row-major numbers,
with the sample's dimensions. It leaves the connection's image orientation alone.

This complete Swift implementation shares one output for concurrent reads, returns
the first frame's matrix and removes the output. It answers `output-unavailable`,
`intrinsics-not-supported`, `intrinsics-not-reported` or `no-frame` when it cannot
supply a matrix; its two-second frame deadline starts after session configuration
and restart finish. A camera change restarts pending reads on a new output and
discards callbacks from the old one. It takes no lease.
The macOS build returns `intrinsics-not-supported` because Apple's connection
switches are unavailable there.

```swift
import AVFoundation
import ExactPackage
import Foundation
import simd

public final class CalibrationModule: ExactPackage {
    private let context: ExactPackageContext
    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        guard name == "getCameraCalibration" else { return reply(["error": "NotSupportedError"]) }
        guard let track = args["track"] as? String, let source = context.tracks.lookup(track)?.video?() else {
            return reply(VideoCalibration.empty("track-not-running"))
        }
        let calibration = source.shared("fixture.camera-calibration") { VideoCalibration(source) }
        calibration.read(reply)
    }
}

// One output per capture, shared by concurrent reads and removed after the first frame.
final class VideoCalibration: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    private weak var source: ExactVideoSource?
    private var deviceId = ""
    private let frames = DispatchQueue(label: "fixture.camera-calibration.frames")
    private var output: AVCaptureVideoDataOutput?
    private var waiting: [([String: Any]) -> Void] = []
    private var deadline: DispatchWorkItem?
    private let scheduleDeadline: (DispatchWorkItem) -> Void

    init(_ source: ExactVideoSource, scheduleDeadline: @escaping (DispatchWorkItem) -> Void = {
        DispatchQueue.main.asyncAfter(deadline: .now() + 2, execute: $0)
    }) {
        self.source = source
        self.scheduleDeadline = scheduleDeadline
    }

    static func empty(_ reason: String) -> [String: Any] {
        ["reason": reason, "deviceId": "", "referenceFrame": "", "intrinsicMatrix": [Double](),
         "distortion": [Double](), "focalLengths": [Double](), "sensorSize": [Double](),
         "pixelArraySize": [Double](), "preCorrectionActiveArray": [Double](), "referenceDimensions": [Double]()]
    }

    func read(_ reply: @escaping ([String: Any]) -> Void) {
        guard let source else { return reply(Self.empty("track-not-running")) }
        waiting.append(reply)
        if output != nil, deviceId == source.device.uniqueID { return }
        start(source)
    }

    private func start(_ source: ExactVideoSource) {
        detach()
        deviceId = source.device.uniqueID
        let output = AVCaptureVideoDataOutput()
        self.output = output
        output.alwaysDiscardsLateVideoFrames = true
        output.setSampleBufferDelegate(self, queue: frames)
        source.configure(stopped: true) { session in
            var reason: String?
            if session.canAddOutput(output) {
                session.addOutput(output)
                #if os(iOS)
                if let connection = output.connection(with: .video), connection.isCameraIntrinsicMatrixDeliverySupported {
                    connection.isCameraIntrinsicMatrixDeliveryEnabled = true
                } else { reason = "intrinsics-not-supported" }
                #else
                reason = "intrinsics-not-supported"
                #endif
            } else { reason = "output-unavailable" }
            if let reason { DispatchQueue.main.async { self.finish(output, Self.empty(reason)) } }
        }
        source.run { _ in
            DispatchQueue.main.async {
                guard self.output === output else { return }
                let timeout = DispatchWorkItem { self.finish(output, Self.empty("no-frame")) }
                self.deadline = timeout
                self.scheduleDeadline(timeout)
            }
        }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sample: CMSampleBuffer, from connection: AVCaptureConnection) {
        let answer = Self.decode(sample)
        DispatchQueue.main.async { self.finish(output, answer) }
    }

    private func detach() {
        guard let output else { return }
        output.setSampleBufferDelegate(nil, queue: nil)
        self.output = nil
        deadline?.cancel(); deadline = nil
        source?.configure { session in
            if session.outputs.contains(where: { $0 === output }) { session.removeOutput(output) }
        }
    }

    private func finish(_ output: AVCaptureOutput, _ answer: [String: Any]) {
        guard output === self.output else { return }
        if let source, source.device.uniqueID != deviceId {
            start(source)
            return
        }
        detach()
        var answer = answer
        answer["deviceId"] = deviceId
        let replies = waiting
        waiting.removeAll()
        for reply in replies { reply(answer) }
    }

    static func decode(_ sample: CMSampleBuffer) -> [String: Any] {
        guard let data = CMGetAttachment(sample, key: kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix,
                                         attachmentModeOut: nil) as? Data,
              data.count == MemoryLayout<matrix_float3x3>.size else { return empty("intrinsics-not-reported") }
        // SIMD columns have padding; copy the platform matrix before serializing its nine entries.
        var matrix = matrix_identity_float3x3
        _ = withUnsafeMutableBytes(of: &matrix) { data.copyBytes(to: $0) }
        var answer = empty("")
        answer["intrinsicMatrix"] = (0..<3).flatMap { row in (0..<3).map { column in Double(matrix[column][row]) } }
        answer["referenceFrame"] = "sample-buffer"
        if let description = CMSampleBufferGetFormatDescription(sample) {
            let dimensions = CMVideoFormatDescriptionGetDimensions(description)
            answer["referenceDimensions"] = [Int(dimensions.width), Int(dimensions.height)]
        }
        return answer
    }
}
```

A video attachment supplies the intrinsic matrix, not physical sensor dimensions
or lens distortion tables. [`AVCameraCalibrationData`](https://developer.apple.com/documentation/avfoundation/avcameracalibrationdata)
comes with supported depth or photo delivery: read `AVDepthData.cameraCalibrationData`
from a depth output or depth-enabled photo, or `AVCapturePhoto.cameraCalibrationData`
from a calibration-enabled photo capture. For the latter, check
[`isCameraCalibrationDataDeliverySupported`](https://developer.apple.com/documentation/avfoundation/avcapturephotooutput/iscameracalibrationdatadeliverysupported)
and request `AVCapturePhotoSettings.isCameraCalibrationDataDeliveryEnabled`.
Apple's photo support requires constituent photo delivery and disabled geometric
and content-aware distortion correction. A plain video output cannot return this
object, and the camera's ordinary `takePhoto` does not enable those settings.

When depth/photo calibration is present, store its
[`intrinsicMatrixReferenceDimensions`](https://developer.apple.com/documentation/avfoundation/avcameracalibrationdata/intrinsicmatrixreferencedimensions),
`pixelSize`, `lensDistortionCenter`, `lensDistortionLookupTable` and
`inverseLensDistortionLookupTable` with the matrix. Apple's distortion tables are
not Android's five coefficients. For a clip, read the attachment on the same
samples the recording package encodes, preserving timestamps and any image
transform; this one-shot read is a camera-model snapshot, not a per-frame log.

#### Verification

The camera's [Apple suite](../packages/camera/test/apple.mjs) compiles this Swift
file with the track hooks for macOS and iOS. Its calibration tests check that
configuration stops and resumes a running session, leaves an idle session idle,
and decodes a sample-buffer matrix with its reference dimensions; missing or
truncated attachments return a reason. Pending reads follow a camera switch,
reject old frames, and start the frame deadline after restart completes.
A controlled scheduler checks when the deadline is armed and its `no-frame` reply.
The Android reader's
`CalibrationTest` compiles the Kotlin implementation and checks all six
characteristics through an open track, including partial and absent data.
A [phone check](../packages/camera/test/checklist.md) is still needed to establish
what an individual camera delivers.

### Save an edit of a photo (`storage.fs.transformImage`)

`storage.fs.transformImage(from, to, options)` decodes the image at `from`
once, makes the edit and encodes it once at `to`, with no search. The order is
fixed: the picture upright (its orientation applied), then `rotate` (degrees
clockwise, about its centre), then `flip` (`'horizontal'` or `'vertical'`, in
the turned frame, so `'horizontal'` mirrors what the person sees), then `crop`
(`{x, y, width, height}` in the turned frame's pixels, clipped to it and its
edges rounded), then the size (`resizeWidth`, `resizeHeight` and
`resizeQuality` as `createImageBitmap` takes them; one side keeps the aspect),
then the encoding (`type` and `quality`, 0 to 1, as `convertToBlob` takes
them; `quality` is 0.9 unsaid). A quarter turn resamples nothing; any other
angle turns the picture into its bounding box, clear where the type keeps
alpha and white in a JPEG. `type` unsaid is the source's where the host writes
it, else JPEG; a type the host does not write rejects with `unsupported`, never
another silently. A `to` with no extension gains the type's.

Unlike `compressImage`, which is for uploads, it keeps what the camera made where
the host can: the source's colour space (a Display P3 photo stays P3), an HDR
photo's gain map (JPEG and HEIC on macOS and iOS), and, by `metadata`, the
source's EXIF, TIFF, GPS, IPTC and XMP with the orientation reset (`'keep'`,
unsaid), the capture's standard tags and the IPTC rights with no GPS, maker
note or XMP (`'no-location'`), or nothing (`'none'`). It resolves `{path, type,
size, width, height, colorSpace, hdr, metadata}`, the last three what was
written (LLP 1133.019 D14, D15):

```ts
export const grants = 'fs.read app:/tmp\nfs.write app:/tmp';
// the editor's angle, and the crop in the turned photo's pixels
const saved = await storage.fs.transformImage(photo, `app:/tmp/edits/${n}`,
  { rotate: edit.angle, crop: edit.crop });
// saved.path ends in .jpg or .heic, the camera's own type
```

Its grants, queue, atomic write and refusals are `compressImage`'s, with
`outside` for a crop that misses and no `unfit`. macOS and iOS encode JPEG, PNG
and HEIC; the web encodes PNG, and JPEG and WebP where the browser does, and a
canvas keeps no metadata, colour space but sRGB or gain map, so there the
record says `colorSpace: 'srgb'`, `hdr: false` and `metadata: 'none'` whatever
was asked. The iOS simulator's ImageIO writes no gain map, so there an HDR
photo is saved as SDR and says `hdr: false`. Android encodes JPEG, PNG and
WebP. It copies source attributes readable by `ExifInterface`, including
embedded EXIF XMP, and, for JPEG sources and output, IPTC and standard external
XMP packets. `no-location` keeps only capture tags and IPTC rights. Unsupported
preservation reports `metadata: 'none'`. This includes
`keep` requests with external PNG/WebP metadata, extended XMP, qualified XMP
orientation or dimensions, unsupported Photoshop resources or multi-picture
JPEGs. Malformed metadata and conversions that cannot carry the retained
segments also report `none`. Linux and Windows answer `unsupported`.

### Read text in a still image (`@exact/text-detection`)

`textDetectorAvailability(languages=)` answers `availability`, optional `reason`,
and the matched `languages`. `detectText(image=, languages=)` reads a file admitted
by `fs.read` and answers `texts`, `width` and `height`. Each text is one line:
`rawValue`, `boundingBox` (`x`, `y`, `width`, `height`) and `cornerPoints`, clockwise
from the top left. EXIF orientation is applied first; coordinates and dimensions
are pixels of the upright image. Lines keep the recognizer's reading order.

| Host | Answer |
| --- | --- |
| iOS, iPadOS, macOS, tvOS | Vision; omitted languages use the request's defaults. Named languages match Vision's supported languages. `apple.confidence` is the top candidate's confidence. |
| Android | Bundled ML Kit Latin recognizer (`text-recognition:16.0.1`); available without a download. Omitted languages return the Latin list; named languages return the matched, deduplicated tags. Other languages or an explicit non-Latin script answer `unavailable`, `unsupported-options`, and `languages: []`; detection fails `NotSupportedError`. `android.script` is `Latn`; each line's `android.recognizedLanguage` is ML Kit's BCP 47 tag, or `und`. Built and JVM-tested; recognition on a device is not yet verified. |
| Web, Windows, Linux, terminal, and targets without an implementation | Declared default: `unavailable`, `reason: "not-supported"`, `languages: []`; `detectText` fails `NotSupportedError`. |

Availability is fixed for an Apple or Android build; neither uses `downloadable`
or `downloading`. Unsupported languages fail before the image is read. A missing
file fails `NotFoundError`; an undecodable file fails `InvalidStateError` with
message `decode`; a recognizer failure is `OperationError` with its message. No
text found is a successful empty list. Text on camera frames and non-Latin Android
recognizers are not implemented (LLP 1133.027). Vision and ML Kit may divide and
read text differently, particularly handwriting.

ML Kit's [language table](https://developers.google.com/ml-kit/vision/text-recognition/v2/languages)
distinguishes supported, experimental and mapped languages; all its Latin entries
are admitted. Its [line result](https://developers.google.com/android/reference/com/google/mlkit/vision/text/Text.Line)
defines the corner order and the unknown-language tag.

### Read barcodes (`@exact/barcode-detection`)

`getSupportedFormats()` answers `formats` and `tracks` (whether live scanning is
implemented). `detectBarcodes(image=, formats=)` answers `barcodes`, `width` and
`height` for an upright still. Each barcode has `rawValue`, `format`, `boundingBox`
and `cornerPoints` in the same pixel coordinates as text detection. Omitted
formats use the platform's defaults; an empty list or an unknown requested format
fails `TypeError`. A valid format unsupported by the host fails `NotSupportedError`.

| Host | Answer |
| --- | --- |
| iOS, iPadOS, macOS | Vision, stills and live scanning. |
| tvOS | Vision on stills; `tracks: false`. |
| Web | The browser's `BarcodeDetector`, where present; otherwise empty `formats`, `tracks: false`, and detection fails `NotSupportedError`. |
| Android | Bundled ML Kit (`barcode-scanning` 17.3.0), stills and live scanning; `tracks: true`. |
| Windows, Linux, terminal, and targets without an implementation | Declared default: `formats: []`, `tracks: false`; `detectBarcodes` and `detectedBarcodes` fail `NotSupportedError`. |

`detectedBarcodes(track=, formats=, interval=)` analyses a camera track already
shown by a viewfinder; it never starts or keeps a camera open. Its answer includes
the latest `barcodes` and up to 32 distinct codes in `seen`, with first/last seen
times and frame counts. Hiding the viewfinder idles scanning; stopping the track
ends it. The default interval is 100 ms, with a minimum of 50 ms.

UPC-A is returned as 12 digits when `upc_a` is requested or formats are omitted;
when only `ean_13` is requested, it is 13 digits with a leading zero. An image
with no codes succeeds with an empty list. Missing files, decode failures and
platform errors use the same error names as text detection (LLP 1133.027).

### Documents the person chose (`doc:`)

A file or folder the person picks (`showOpenFilePicker`, `showDirectoryPicker`,
`showSaveFilePicker`), opens from the system at the app's `open-file` node,
drops on a node with `drop` or pastes (below), arrives as a `doc:/<n>/<name>`
path (LLP 1069.010 D1). `storage.fs` reaches it,
and paths beneath a chosen folder, under the grants `fs.read doc:/` and
`fs.write doc:/` — the same grants, operations, refusals and codes whether the
data module is TypeScript or Rust, on every host:

```ts
export const grants = 'fs.read doc:/';
// listing([folder]) with folder = 'doc:/1/notes', from `change` on the picker's node
for (const name of await storage.fs.readdir(folder)) {
  const stat = await storage.fs.stat(`${folder}/${name}`);       // a folder's size is 0
  if (stat.isFile) bytes = await storage.fs.readFile(`${folder}/${name}`);
}
```

| Operation on a `doc:` path | Grant | What it does |
| --- | --- | --- |
| `readFile`, `stat`, `readdir` | `fs.read doc:/` | `doc:/<n>` itself lists only `<name>` |
| `writeFile`, `atomicWriteFile`, `appendFile` | `fs.write doc:/` | Creates a file beneath a chosen folder |
| `mkdir` | `fs.write doc:/` | Makes the folders above it too |
| `rm` | `fs.write doc:/` | One file or one empty folder beneath the chosen document; never the document itself, never a tree |
| `copyFile(from, to)`, `from` a `doc:` file, `to` an `app:/` path | `fs.read` covering `from`, `fs.write` covering `to` | The host copies the bytes; they never enter JavaScript, so a file past `readFile`'s 16 MiB result copies too. A regular file only, with `readFile`'s refusals |
| `realpath` | `fs.read doc:/` | The chosen item's native absolute path on macOS, Linux and Windows, for a program the app hands it to; `NotSupportedError` (`'failed'`) on the web, iOS and Android. A removed item is refused `'ENOENT'` |
| `rename`, and `copyFile` to a `doc:` path | — | Refused (`'failed'`): read the bytes and write them |

A path never minted, `.`/`..`, and a closed window's or page's handle are
refused. Native folder handles retain the selected directory even if its
pathname moves. Descendant symlinks and Windows reparse points are refused,
including in intermediate folders; explicitly choosing a file through a
symlink still selects its target. A native selection whose filename is not
valid Unicode is refused rather than addressing a lossy spelling. A document needs no app storage: a drive without `--storage` reaches
it, though a `copyFile` into an `app:/` path needs app storage as any app file does. On the web the paths are the `FileSystemHandle`s the page's picker
returned (Chromium; Safari and Firefox refuse the pickers), and a module placed
on a worker on the wasm web host cannot reach them (`'unsupported'`); on macOS
and iOS they are security-scoped URLs held for the session; in a Linux desktop
session the XDG desktop portal opens the document pickers, with Open and Save
bounded by `file_handlers`. An extension follows the File System Access
specification's [suffix rule](https://wicg.github.io/file-system-access/#validate-a-suffix),
with the leading dot optional in `app.json`: after adding it, the suffix has at
most 16 ASCII alphanumeric, `+` or `.` code points, and does not end in a dot.
The JavaScript reader (`scripts/app.mjs` `readManifest`) and the Rust reader
(`contract/cli/src/manifest.rs` `Manifest::read`) each lowercase that canonical
spelling once, so `"board"` becomes `".board"` for every host and `.tar.gz`
remains a compound suffix. Their Bun and Rust tests read the same cases from
`scripts/fixtures/document-suffixes.json`. An invalid suffix or an empty or
control-bearing handler name makes the build fail.

The web passes those suffixes to its picker and matches a drop by the whole
filename suffix. Apple registers each without its leading dot; its picker offers
the handler's content types and the complete extension tokens the system resolves,
and Exact's own drop and route check matches a compound suffix against the whole
filename, without case. Linux sends only glob rules when a handler declares any
extension (`.md` becomes `*.[mM][dD]`, `.tar.gz` becomes
`*.[tT][aA][rR].[gG][zZ]`); a handler with MIME types and no extensions sends
only those MIME rules. These are the rules the portal receives. The portal may
still allow another file, as the
[FileChooser specification](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)
permits. Their behaviour has been checked against the current
[GTK portal](https://github.com/flatpak/xdg-desktop-portal-gtk/blob/main/src/filechooser.c),
[KDE portal](https://github.com/KDE/xdg-desktop-portal-kde/blob/master/src/filechooser.cpp)
and [KIO](https://github.com/KDE/kio/blob/master/src/filewidgets/kfilewidget.cpp)
sources, not in a real GNOME or KDE session. A headless Linux host opens no panel
(a drive's held pickers still answer). Directory sends no filters and does not
parse the handler metadata. Open and Save refuse a non-canonical embedded suffix
rather than normalizing it; current builds cannot embed one, and older binaries
embed no handlers. The handles end with the session, unless
the app keeps what the person chose (below).

On Windows, the three document pickers, file inputs and `saveFile` use the
shell's common item dialogs, owned by the app's window. A headless host refuses
them; under the agent they remain held pickers. Document pickers offer every
file until the Windows build carries `file_handlers`. File inputs copy the
chosen files into `app:/tmp/picked/` and leave the originals in place.

**Where a picker opens** (LLP 1173 D2). The three pickers and `saveFile`
take the File System Access API's `startIn=`: a well-known folder by name,
or a `doc:` path the person chose earlier.

```contract
component Capture
  state folder = ""
  action saveCapture
    saveFile("saved", "app:/tmp/capture.png", "Screenshot.png", startIn="screenshots")
  action openBeside
    showOpenFilePicker("opened", startIn=folder)
  action chose(value)
    folder = value
  view
    column
      input id="opened" display="none" change=chose
      input id="saved" display="none"
      button press=saveCapture
        text "Save"
      button press=openBeside
        text "Open another beside it"
```

`folder` holds what the last pick's `change` carried.

| Host | A name | A `doc:` path |
| --- | --- | --- |
| macOS | The folder, each time the panel opens: the home's `Desktop`, `Documents`, `Downloads`, `Music`, `Pictures`, `Movies`; `screenshots` where the Screenshot app saves | The chosen folder, or a chosen file's folder |
| iOS | Ignored: iOS has no such folders | The document picker's `directoryURL` |
| Web (Chromium) | Passed as is; `screenshots` is `pictures` | The handle |
| Linux desktop | The portal's `current_folder`, from `user-dirs.dirs`; `screenshots` is the pictures folder's `Screenshots` | `current_folder` |
| Windows | The shell's known folder, including a redirected folder; `screenshots` is its Screenshots folder. Used as a default only when Windows has no last-used folder | The held document's folder, overriding the last-used folder |

A name that does not resolve here (Linux's Downloads disabled, the Screenshot
app saving to the Clipboard), a value that is neither a name nor a `doc:` path,
and a handle the session does not hold are journalled as
`<command>: startIn: refused: …`, and the panel opens where it would have.
Linux's `saveFile` has no panel, so `startIn` there reaches only the pickers.
Under the agent the held picker's `args` carry `startIn`.

**A change on disk asks the answer again** (LLP 1069.010.000 D1). An answer
that calls `storage.fs.watch(path)` is asked again when the file, a folder's
own entries, or with `{ recursive: true }` everything beneath it, is created,
written, removed or renamed by any process, this app included. The change
carries nothing: the answer reads again, as LLP 1016.002's `native.watch`.
Call it inside an answer, on a `doc:` path the app may read (`fs.read
doc:/`):

```ts
await storage.fs.watch(folder);                  // its own entries
return (await storage.fs.readdir(folder)).filter((n) => n.endsWith('.md'));
```

A Rust source records the same topic:
`store.observe_topic(&exact_data::documents::watch_topic(path, false))`.
macOS listens with FSEvents, Linux with inotify, the web with Chromium's
`FileSystemObserver`; iOS, the Linux host built for macOS and a browser
without the observer accept a watch that never fires. `state.watches` lists
the watched paths.

**A write can expect what it read** (D2a). `writeFile`, `atomicWriteFile`
and `appendFile` on a `doc:` path take `{ expect: stat }`, the `stat` the
app read: if the file's `modifiedMs` or `size` has changed since, the write
is refused with code `ECHANGED` and nothing is written (a file that is gone
refuses `ENOENT`). The comparison and the write are queued together but are
not one atomic step; what they close is the time an editor spends editing.
Read the new file and let the person choose.

**Documents kept across launches** (LLP 1069.010.000 stage 2; macOS, the
web, and Linux in a desktop session). An
app granted `doc.keep` keeps exactly what the person chose — a file or folder
from a picker, Finder, ⌘O, Open Recent, the command line or a drop; never a
typed path, a followed link, or a route's folder — and its `doc:/<n>/<name>`
reads again after a relaunch, in any of the app's windows, with no panel.
The kept set is File ▸ Open Recent's: Clear Menu, or an entry falling off
the end of the list, ends its reopening (a window that has it open keeps it
until the app quits).

```ts
export const grants = ['fs.read doc:/', 'doc.keep'].join('\n');
storage.fs.watch('doc:/');                       // asked again when the list changes
const recent = await storage.fs.recentDocuments();
// [{ path: 'doc:/12/notes', name: 'notes', kind: 'directory', mode: 'readwrite', opened: 1760572800000 }, …]
await storage.fs.forgetDocument(recent[0].path); // off the list and Open Recent; ENOENT if not kept
```

A Rust source sends `fs.recentDocuments` and `fs.forgetDocument` storage
requests (`apps/markdown/data`). A kept item is the object chosen: a move
or rename is followed and keeps its path (`<name>` is the name it was
chosen by); a file another program replaced at its name is not the
document, and the app's own `atomicWriteFile` carries the entry to the file
it wrote, from Rust or TypeScript. When the kept set cannot be saved, that
write fails after writing the file, and writing it again keeps it. A path whose entry is gone
refuses `ENOENT`: ask the person to choose again. Without `doc.keep`
both calls are refused. Under the agent on a Mac the kept set is the
drive's scratch store (`--storage`), never the person's.

On Linux the list is the desktop's recent files (`recently-used.xbel`),
each entry marked with the app's id, and the person clears it there. The
path is the handle: a kept document moved, or replaced at its path, is
forgotten rather than followed. Only a desktop session keeps anything; a
headless run, a drive and the DRM kiosk keep and list nothing. The Linux
host has no file chooser of its own yet, so today nothing there is chosen
to keep. iOS keeps nothing.

On the web a kept document is the `FileSystemHandle` the browser gave,
kept in the page's IndexedDB, at most 20 of them; clearing the site's data
revokes them. The browser owns the permission: after a reload it may
withhold it, and a read or write then refuses `EACCES` (an entry no
longer kept refuses `ENOENT`). Ask again from a press with the command
`requestPermission(id, path, mode)`, `mode` `"read"` or `"readwrite"`;
`change` at `id` hears `granted` or `denied`. A native host answers
`granted` for a document that resolves. A drop keeps a file of a type the
manifest declares; a paste, a dropped folder and the agent's files are the
session's only.

```contract
component Reopen
  state permission = "unknown"
  action ask(path: string)
    requestPermission("permission", path, "read")
  action answered(value)
    permission = value
  view
    column
      input id="permission" display="none" change=answered
      button press=ask("doc:/12/notes.md")
        text "Allow access"
```

### Files dropped or pasted

A `drop` hands its action a `DragEvent` whose `files` are `doc:` handles of
any type, a folder's included; a `paste`'s `ClipboardEvent` has `files` too,
image data with no file (a screenshot) as one `image.png`. Read them as any
`doc:` path, under `fs.read doc:/`; `storage.fs.stat(path).isDirectory` tells
a folder. A file drag over a `textarea` or `input` whose node or an ancestor
declares `drop` lands there, not as path text in the field. `dragenter` and
`dragleave` bracket the drag over a node, for a drop overlay
([grammar](contract-grammar.md#pointer)):

```text
action took(e: DragEvent)
  paths = e.files
action pasted(e: ClipboardEvent)
  if e.files.length > 0
    paths = e.files
    preventDefault()
column dragenter=entered dragleave=left drop=took
  textarea paste=pasted
```

| Host | `drop`, `dragenter`, `dragleave` | `ClipboardEvent.files` |
| --- | --- | --- |
| Web (JS and wasm targets) | yes: Chromium's handles, else read-only copies; a folder's entry as a read-only folder | `clipboardData.files`, read-only copies |
| macOS | yes: every file URL Finder drags, a field forwarding the drag to its `drop` node | file URLs, else PNG or TIFF data written as `image.png` |
| iOS | none (declared; the driver's drag forms answer `unsupported`) | file URLs, else the pasteboard's image as `image.png` |
| tvOS | none | empty: tvOS has no pasteboard |
| Windows | yes, files and folders in Explorer selection order, including over an editable field | empty (text only) |
| Linux, Android, terminal | none | empty (text only) |

The driver: `tap <target> drop <path…>` (files and folders), `tap <target>
dragenter <path…>` and `tap <target> dragleave`, `type <target> paste --file
<path…>` and `type <target> paste --image <png>`.

### Shares from other apps (`share_target`)

`share_target` in `app.json` lists the app in other apps' share sheets, in the
[Web Share Target](https://w3c.github.io/web-share-target/) member's words. A share
arrives as one location: `action`, a path one of the app's routes matches, with the
shared members in its query under the names `params` gives them.

```json
"share_target": {
  "action": "/share", "method": "POST", "enctype": "multipart/form-data",
  "params": { "title": "title", "text": "text", "url": "url",
    "files": [{ "name": "files", "accept": ["image/*", ".pdf"] }] },
  "apple": { "presentation": "confirmation" }
}
```

Read a text member with `searchParam(entry, "text")` and the files with
`sharedFiles(entry, "files")`
([grammar](contract-grammar.md#standard-functions-and-intrinsics)). `params.files`
needs `POST` and `multipart/form-data`. A shared link arrives under `url`, or at the
end of the text when `url` is not mapped. On Apple it is the first web link the sender
attached, without any userinfo: `https://name:password@x.test/trip` arrives as
`https://x.test/trip`. An Apple build derives a share extension
from the member and an Android build its intent filters; the web's member takes the
text members only. `apps/fieldnotes` receives text and files this way.

On iOS and macOS the extension saves the share in an inbox. The app claims each share
once and removes its inbox record when it delivers the action route. If the action needs
sign-in or a network connection, save the pending text or URL in persistent app state
when the route receives it. Restore it after a relaunch and clear it when the action
succeeds or the person discards it. The extension’s checkmark means the share reached
the inbox. The app’s submission may still be pending.

`share_target.apple` holds Apple's words:

- **`presentation`** is what the extension shows. `"compose"`, the default, is the
  compose sheet: the shared text to edit, then Cancel or Post. `"confirmation"` has no
  editor, for an app that saves in one tap: choosing the app in the share sheet takes
  the share at once, shows the app's name under a checkmark for a second, and closes.
  The share's text is the sender's, unedited; text a sender gives only as an attachment
  is taken up to 1 MB, and longer text fails the share. If the share cannot be saved, an
  alert says why and nothing reaches the app; if the person dismisses the sheet while
  files are still copying, nothing is kept. Either presentation delivers the share the same
  way, at `action`. A route that keeps what arrives without asking should remember
  that any link can open `action` with the same query.
- **`postTitle`** is the word on the compose sheet's post button, in place of Post,
  shown as written: it is not a key into the app's strings. The build refuses it with
  `"presentation": "confirmation"`, which has no button to name.
- **`NSExtensionActivationRule`** replaces the rule the build derives from `params`:
  Apple's dictionary of keys, or a predicate string, which is used as written.

What does not arrive is left out, and the rest of the share is saved: a link that does
not load, and a file that does not load, is over 64 MB or that no files member accepts
(each such file is counted in `dropped`). A share left with no link, text or file is
not saved, and nothing reaches the app: the confirmation shows its alert, and the
compose sheet closes with the request cancelled. A title alone is not a share.

**A page shared from Safari.** When `url` is mapped, Safari runs a script of Exact's,
the same in every app, in the page it shares. The share's `url` is then the page's
canonical URL (`<link rel="canonical">`), or the page's location when it names none,
and its `title` is the page's title, cut at 4,096 UTF-16 units. A canonical link that
is not an http or https URL, that is longer than 4,096 characters, or that would take
an https page to http, is ignored. The page's `url` carries no userinfo either. An app
cannot supply its own script, and one that maps no `url` ships none.

A link pressed in a page and shared from its menu arrives as that link. Where a link
arrives beside the page's details, the details stand only if the link is the page's
own location. The two are compared as URLs: the scheme and host in either case, a
default port and an empty path or `/` are the same; a last slash, a query or a
fragment that differs is not. So a link to another place in the page, or to another
page, arrives as that link with the sender's title, and a link that is no web link
leaves the share without a `url`.

For such an app, a link that does not load is left out, and the page's details do not
stand in for it. Page details too large to read (over 1 MB, which takes a page whose
own address is about that long) are not read either: the list is then a file like
another, kept by a files member that accepts it or counted in `dropped`. The share is
saved with what else it holds, its text and its files. Safari gives a page's title as
the share's text, so an app that maps `text` then receives the title with no `url`.
With nothing else, the share is not saved.

Safari hands the extension the page's details as a property list
(`com.apple.property-list`) in place of the page's link, and Exact reads it. The
derived rule asks for the page, and an app's own dictionary rule gains
`NSExtensionActivationSupportsWebPageWithMaxCount` unless it gives that count itself.
An app's own predicate is used as written. On the iOS 27 simulator, Safari offers a
page to a predicate that accepts only `public.url` and still hands over the details;
this has not been run on a phone or a Mac.

On iPhone, register the App IDs `<app id>` and `<app id>.share` on the same Apple
Developer team and enable the App Group `group.<app id>` for both. Make and install a
profile for each ID. For `bun exact.mjs ios --device`, both profiles must cover the
phone and its development signing certificate. For a distribution archive, use profiles
of the same distribution method. `EXACT_PROFILE` can select the containing app's profile.
Exact finds the extension's by its ID and reports a missing grant before compiling.
[Register an App Group](https://developer.apple.com/help/account/identifiers/register-an-app-group/).

An unsigned device build includes `PlugIns/ExactShare.appex`. To install it, the signer
must provision and sign both bundles for the same App Group. To build without the share
extension, list the kinds to keep with `--extensions`, or use `--extensions none`.
A Mac build signed ad hoc leaves out the share extension
([Mac development signing](#mac-development-signing)).

### Clipboard images

Data code writes images with [`@exact/clipboard`](contract-grammar.md#clipboard).
Windows accepts an `app:/` PNG file, up to 64 MiB and 64,000,000 pixels;
in-memory PNG bytes and raw pixels are not accepted. It offers sanitized
PNG, a bitmap for untagged or explicitly sRGB images without an ICC profile,
and optional Unicode text together.

Full-range sRGB cICP `(1, 13, 0, 1)` without mDCV or cLLI is normalized
to an sRGB chunk and gets a bitmap. cICP takes precedence over other colour
chunks; normalization removes those lower-priority tags. Other cICP values,
mDCV and cLLI are refused.

Images carrying gAMA or cHRM without sRGB, and images carrying iCCP, are
PNG-only when no cICP overrides them. iCCP takes precedence over sRGB, as
[PNG third edition specifies](https://www.w3.org/TR/png-3/#color-chunk-precendence).
No colour conversion is performed. On Windows 11, Paint 11.2609.401.0
pastes a gAMA image without its gamma interpretation, and Edge 157.0.4341.46's
ordinary paste editor receives no image from these PNG-only items. Preserving
the PNG's colour information therefore does not guarantee a correct paste.
Use untagged or sRGB PNG input when a bitmap fallback is needed.

### Bake and deliver a TypeScript module

Build an app-local `app.ts` module and bake its Contract through the resulting
Hermes bytecode with the compiler from the verified install-once bundle:

```sh
cargo run -q -p exact-js-bake -- path/to/app --out path/to/new-generation
```

The app exports `grants` and an `Answer`-typed `answer`, and may export `appId`,
which must then be `app.json`'s `app.id` (the build supplies that id when it does
not); `appId` and `grants` are constants the build reads from syntax without
running the module: a string, a template or `+` of constants, an array of them
`.join('\n')`, or a top-level `const` holding one. Numbers keep their type:
`1 + 2` is `3`, while `'a' + 1 + 2` is `'a12'`. Conditional expressions are refused.

Plain `grants` may also use constants imported from app-local modules, including
local re-exports, arrays and spreads of those arrays:

```ts
import { SECRET_GRANTS } from './src/session';
export const grants = ['net.fetch https://api.example.com', ...SECRET_GRANTS].join('\n');
```

A namespace import reads the same constant: `import * as session` and
`...session.SECRET_GRANTS`.

The imported bindings follow the same literal grammar. Calls other than the
array's `.join`, computed arrays, package bindings (including re-exports
from packages), and dependencies on launch-marked settings are refused with a
source location. An `app.ts` that is `export * from` a package exports its
own `grants`, which the star does not override. Imports resolve through
Rolldown, to the file the bundle runs.
Re-exports follow ECMAScript export resolution: a loop through star exports
keeps searching, ambiguous bindings are refused, and a star never exports
`default`. A cycle through constant initializers is refused. `app.ts` declares
the grants explicitly; importing a module adds no grants. The captured export
becomes the resolved string, so mutating an imported array at run time cannot
change it. The rewrite preserves initialization of imported and re-exported
modules. The bake receipt's `grants` is the
complete resolved list; `grantSources` lists each nonempty line with its `grant`,
`file`, `line` and `column`. Locations name the source expression that starts the
line; generated `env.launch` lines name their setting in `app.json`.

When no setting is launch-marked, `app.ts` may also re-export literal grants
from an app-local module, by name or through `export *`. The captured export
becomes the same fixed string. Launch-marked settings require a grants
declaration in `app.ts`, where the bake adds their `env.launch` lines.

A setting declared in app.json's `env` is read in a top-level const of `app.ts` or an
app-local module it imports transitively:
`const API = import.meta.env.API_BASE_URL ?? 'https://api.example'`.
Each read of a name must use the same default, including no default. A declared name
must be read in app.ts or its transitive app-local imports; other files do not count.
The graph uses the bundle's browser resolution, including package main fields and
conditions. Discovery and freshness read the bake's recorded module list; they do not resolve
imports again. Type-only imports and re-exports read no settings. Every source file loaded
by the TypeScript check remains a freshness input, including declarations, `import()` types
and triple-slash references. Resolution inputs also count: manifests, configuration, and
the names a resolution would have chosen in each place it searched (the specifier with each
extension, as a directory, a package name in each `node_modules`), present with their bytes or
absent, including inside packages. An unrelated file or directory, such as `out/`, is none.
An implementation source omitted from declaration capture remains a file input if it exists.
Web, Linux and Apple builds write the same discovery record beside
the app. The checker types declared settings even in modules reached only through types;
those modules supply no setting values or defaults. Grants may use a setting's const from
`app.ts` or an imported module, as any literal constant, unless the setting is launch-marked.

Server discovery gives a local setting read by `app.ts` priority. Otherwise it walks the
recorded imports depth-first in source order, visiting each module before its imports,
and takes the first local setting, then the first literal URL. Different local settings
in the first setting-owning module are refused, naming the values; keep the server's
setting in `app.ts` and other clients' settings in their modules.

A backend override (`--backend`, `EXACT_BACKEND_ORIGIN` or `.exact/backend-origin`)
needs one top-level `const origin`, set to a literal http(s) string or a setting
such as `import.meta.env.API_BASE_URL ?? 'https://api.example'`, and used in
`net.fetch ${origin}`. Settings are inlined before the override.
Plain fetch and WebSocket grant lines at the original origin move with it,
including imported constants; matching normalizes host case and default ports.
Their receipt entries keep the authored source locations.

An unannotated setting without an authored type assertion has type `string`, so
comparisons such as `MODE === 'live'` type-check whatever value the build supplies. Binding
annotations and surrounding assertions are preserved for inlined values. Native launch reads
retain `as` and angle-bracket assertions, parenthesized around the whole read;
`satisfies` and const assertions are removed because a launch can supply any
string. Binding annotations on native launch reads must accept `string`.
A refused selection names its selector and reports `backend-selection`, including in
`contract build --json`.

For grants that differ by backend, write an object literal such as
`export const grants = { 'http://127.0.0.1:4790': ['net.fetch http://127.0.0.1:4790', 'secret.keep fixture-token'], 'https://discord.com': ['net.fetch https://discord.com', 'net.websocket wss://gateway.discord.gg', 'secret.keep discord-token'] };`.
Keys must be unique string literals naming http(s) origins without credentials,
paths, queries or fragments; values are string literals or arrays of them, one
grant line per literal. Identifiers, templates, computed keys, spreads and methods
are refused with `app.ts:line:col`. The bake selects the key exactly matching the
top-level literal `const origin`, after `EXACT_BACKEND_ORIGIN` or
`.exact/backend-origin` overrides it, and refuses a missing entry, listing the
origin and keys. Before type-checking, it replaces only the captured initializer
with a string of the selected lines joined by `\n`, so web and native read the
same grants. Aliased exports select the initializer exported as `grants`;
private bindings of that name do not contribute grants or receipt locations.
Receipt locations name each selected literal. Authored source
stays unchanged. Keyed grants move to another origin in a development build
(`EXACT_UPDATE_TRUST` unset or `development`): an
unlisted loopback or private-network origin (RFC 1918 or link-local IPv4,
`fc00::/7` or `fe80::/10` IPv6) takes a loopback entry (`127.0.0.1`, `localhost`
or `[::1]`) of the same scheme. A matching port wins; otherwise exactly one
candidate is required. Its matching `net.fetch` and `net.websocket` lines move
to the selected host and port, preserving paths and unrelated grants. A
production build refuses it, as it refuses any missing entry.

A declared server under `fixture/` or `fixtures/`, with a top-level literal loopback
`const origin`, gets one stable development port in 4790–5789 per real app directory.
The bake and driver select it together. `--backend`/`EXACT_BACKEND_ORIGIN`, then
`.exact/backend-origin`, the server command's `--port`, and `PORT` override the default.
`bun scripts/agent-launch.mjs --fixture-port --app <app>` prints the selected port.
An agent drive refuses an occupied fixture port unless its launch record and listening
process belong to this app directory and origin. Choose a free port with `--backend`
or `PORT` when another checkout holds it. Ordinary app servers keep their authored ports.

**A setting a launch gives** (LLP 1153.002). Beside a name, app.json's `env` takes
`{ "name": "API_BASE_URL", "launch": true }`. In `app.ts` or an app-local module it imports
transitively, read it as `const API = import.meta.env.API_BASE_URL ?? '…'`. Its build's value (the
environment, `.env.local`, `.env`, the default) is what the web and Android read (the web's
bundles inline it), what a process that renders the web's pages reads, at build or while
serving, and what the bake's own evaluation reads, so the first frame it compiles. On Apple
platforms, Linux and Windows, a non-empty value in the launched process's environment replaces
it, read once in the process, so every instance in that launch (a worker's, a reload's, an
update's) reads the same, and every answer the bake compiled but an `else` placeholder's is
asked again once the module loads: `SIMCTL_CHILD_API_BASE_URL=… xcrun simctl launch <udid>
<bundle-id>` on a simulator, an XCUITest's `app.launchEnvironment["API_BASE_URL"]`, `xcrun
devicectl device process launch --environment-variables '{"API_BASE_URL": "…"}'` on a phone,
the process's environment on a desktop. A placeholder (`resource r = load() … else
fallback()`) is the bake's for every launch, so one made from a marked setting shows the
build's value until its resource answers, and after that if the answer fails. A native module
is not the way to read one: this read holds the value before the module's first request.
Grants are fixed at build, so a grant over a marked name is refused at bake: grant what a
launch may name (`net.fetch https:`, `net.fetch http:`, or each origin), and a `fetch` outside
them is refused naming the setting its origin came from (a socket's refusal does not name it).
The bake adds `env.launch <NAME>` to the module's grants for each marked name, and a
host answers a launch's value only for a name its grants carry, so nothing else of the
environment is read; app.ts may not write the line, and an update that marks a new name changes
the grants, which takes a new binary. A name that begins with `EXACT_` is the host's and cannot
be marked. The build's record (`.exact-build.json`'s and the receipt's `env`) says `"launch":
true` beside the value and its source. A module that reads one needs `typescriptRuntime` 14: an
older binary's prelude cannot give it the launch's value, so a signed update of it goes to no
such binary. A drive judges a build by the environment it starts the app with, where a marked
name set is the launch's, not a change to the build: the Linux, macOS and Windows drives start
the app with their own environment and what `open({env: ...})` adds, and the iOS simulator
drive with each `SIMCTL_CHILD_<NAME>` of its own environment and what `open({env: ...})`
passes; the Android and web drives count it. Whatever set a marked name in that environment, the
app reads it as the launch's value: a test, a launcher, a shell that exports it for something else
(another project, CI), or Bun, which loads the `.env` files of the driver's working directory into
it, `.env.<NODE_ENV>` among them (`.env.development` by default), which the build does not read.

The producer captures local imports, type-checks, bundles with Rolldown, compiles HBC, and
bakes with an empty store. It writes `app.plan`, `app.js`, `app.hbc`, generated
types, and an `app.module.json` pairing receipt into a **new** directory; it
never overwrites an existing generation. An installed package's code is captured
file by file as the bundle reaches it; the receipt's `packages` names each
`name@version` with a digest of what was taken (LLP 1027 D5, "Package code").
A linked (workspace or `link:`) package's `peerDependencies` resolve from the
app for both declarations and runtime code, including subpath exports. The app
must have a version that satisfies the declared range, even when the linked
package has its own install.
A peer imported by a reachable declaration or runtime file is refused if missing
or incompatible. Installed packages, including those an isolated install (pnpm,
Bun) links from its store, keep Node's ancestor lookup.
`EXACT_TSC` and `EXACT_ROLLDOWN` override producer tools. `hermesc` is always
the compiler paired with the selected `hermes-lean-sys` bundle.
A module's placement (LLP 1027.002) is the manifest's: `typescript.placement`
and `rust.placement` are `main` (the default) or `worker`, overridable per
platform under `platforms.<platform>.placement`; `EXACT_TYPESCRIPT_PLACEMENT`
and `EXACT_RUST_PLACEMENT` override a bake for a measurement. A change is a new
compatibility id, never an update. A Rust-only app is placed too: its generated
entry wraps the source in `exact_data::Placed`. A Rust worker is native only:
on the web it runs inline, and the build says `warning: <app>: rust.placement
worker runs inline on web: a Rust worker is native only (LLP 1027.002 §7)`.
With Rust updates (a `rust` mode other than `off`) the bake refuses `worker`:
"rust.placement worker with Rust updates (rust mode <mode>): an update module
runs inline, so its placement would be lost. Set rust.placement to main, or
turn Rust updates off". `build_mixed_with` bakes a mixed app through
its own composer, so a resource only Rust owns needs no TypeScript placeholder;
the development producer keeps such a resource's last Cargo-baked value and
the composer's ownership roster for the generated TypeScript provider types.

Native module clients can supply a `Module` factory to `exact_apple::host!`
(the sixth argument) and apply an `ExactGeneration` containing an `ExactModule`
through `ExactApp.applyGeneration`. All sessions prepare before any commit;
changed logic re-asks resources while preserving compatible slots and clock.
Initial module loading happens after first pixel. The binary's app identity
and grants must match; a client must include the candidate's executor. Pairing
hashes are not authentication: this API requires an admitted development origin,
and does not accept signed-delivery generation tokens.

For a module client's `build.rs`, depend on `exact-js-bake` and call
`exact_js_bake::build(Path::new(".."), "web")` (or `"macos"` / `"ios"`). This writes the
paired artifacts, `compat.json`, and `module.rs` constants (`APP`, `GRANTS`,
`REVISION`) into `OUT_DIR`, and on Apple `host.rs`, the host entry its `deploy.store`
names, which includes `baked.rs`: `exact_plan()` and `exact_compat()`. An Apple build
(`host/apple/build.mjs`) bakes before Cargo and links the plan, `compat.json` and the
module's bytecode into the executable, so an edit to the Contract or the TypeScript alone
compiles no Rust; there `module.rs` has no `REVISION`. With a store (`"A"`), a signed update carries the module pair under
`typescript/` (`app.module.json`, `app.hbc`); it needs the binary's TypeScript runtime
and exactly its grants. An app with both a TypeScript and a Rust module still sets its
`deploy.store` entries to `"0"`: signed delivery of both is not implemented.

The web crate links `exact-js-web`, not Hermes. Include the generated constants
and artifacts, then use the host macro's factory and paired-artifact arguments:

```rust
include!(concat!(env!("OUT_DIR"), "/module.rs"));
exact_web::host!(exact_js_web::Module,
    include_bytes!(concat!(env!("OUT_DIR"), "/app.plan")),
    include_str!(concat!(env!("OUT_DIR"), "/compat.json")),
    || exact_js_web::Module::new(APP, GRANTS, REVISION), [
        include_bytes!(concat!(env!("OUT_DIR"), "/app.module.json")),
        include_bytes!(concat!(env!("OUT_DIR"), "/app.js")),
        include_bytes!(concat!(env!("OUT_DIR"), "/app.hbc")),
    ]);
```

Run the ordinary build scripts and `bun host/web/dev.mjs --app <name>` with
`EXACT_APP_DIR` set for an external app. The dev server watches local TypeScript
imports and Contract, publishes complete immutable generations, and the same URL
delivers plan/logic/assets to the browser and an admitted macOS/iOS client without
rebuilding either binary. Browser code runs after first paint in a disposable
private realm; page and guest globals are untouched. This is trusted app code,
not a security sandbox. Corrupt, incompatible, or failing candidates preserve
the running app.

Browser providers support async answers and sequential/parallel `fetch` through
the existing grant-checked host transport. Executor-local continuation tickets
drain microtasks without re-entering wasm; stale incarnations cannot fulfill the
replacement app. 

Linux uses the same installer command at the top of this reference with its
Rust target triple. Exact selects Ibex's `intl` feature and `INTL` group on Linux
and Android:
`en`/`en-US` and complete currency data are present, and unsupported locales
fall back to `en-US`; it does not select `intl-all-locales`. The verified
bundle supplies both VM archives, all three ICU data tiers and the matching
compiler, while the feature links the English tier. Apple does not install
Ibex's `INTL` group because Hermes retains OS-backed Intl there. iOS, tvOS and
Windows use their pinned v6 bundles and must not fall back to the former
sibling-Ibex/private-cache recipes. tvOS builds set a 17.0 deployment target,
above the bundle's 15.0 minimum. The normal native
build captures bundle inputs under the `hermes-lean-sys/` bake receipt root.
`smoke.mjs --app-only` runs the selected app and its tests without unrelated
bare-plan host fixtures. The driver supports `ios --device [--phone <name|udid>]`:
the phone connects outward to a temporary Mac-side port with a per-launch token.
Use a trusted LAN, allow local networking, and keep the app visible;
`EXACT_AGENT_HOST` overrides the Mac IPv4 address. The carrier is not encrypted.
The first launch of each bundle id can raise Apple's Local Network prompt. Allow
it on the phone; the driver cannot answer it, and a prompt left by another app
can also keep this launch inactive. `No route to host` or `waiting for foreground`
can mean that prompt is waiting. Unlock the phone before launching. Startup
failures retain devicectl's lock diagnostics and say when the app printed nothing.
Android APK drives (`bun exact.mjs agent android`, or `agent.mjs android --reader`) check and wake the screen, dismiss an insecure keyguard, and refuse a secure lock screen: unlock the device and retry; a readiness timeout includes the observed screen state.

The dev page's **Open in native…** link offers an installed-client action and
local setup instructions at `/__dev/open`. Development Apple builds register an
app-specific opening scheme and pass its HTTP(S) locator to the existing loader;
production builds do not register that development handler. iOS handles cold and
warm URL delivery. For a local macOS bundle, use
`bun host/apple/build.mjs <app>-apple --bundle` and open the printed `.app` once.
The bundle includes its assets and native modules; it is not a notarized download.
Browser navigation, both Apple cold/warm handlers and malformed-link refusals are
tested. The page cannot detect installation, and does not trigger signing/builds.

Linux native apps can run TypeScript through `exact-js` and Hermes;
`apps/shelf/linux` is an entry using that executor.

**Limits.** `app.ts` imports pure-JavaScript packages only (Node's modules, the DOM, a computed `import()` and native addons are refused; a use that a `typeof` test guards is not, nor a read through the global object, such as `globalThis.process?.env`, whose value is only tested, compared, spread into an object, returned, or held in a `const` whose uses are guarded in turn). Code from a string throws on every host through `Function` and `eval`, and through their own `constructor`: each throws `SyntaxError: Parsing source code unsupported`, as Hermes does. The build warns (`code-from-string`) at the uses of the global `Function` or `eval` it can see by name, a package's or `app.ts`'s: a direct call or `new`, a member of `globalThis`, destructuring a global object's property, a parameter initializer, or a JSX component name (`<Function />`). A use reached another way builds without a warning and throws the same error when it runs. Either way, code that uses them works only if it catches that error and takes another path, as zod does. What reaches `Function.prototype.constructor` or a function's own `constructor` (`(function () {}).constructor`) still runs code from a string on the web, and not on Hermes: the warning is a tripwire, not a sandbox, and refuses nothing. Under the render server's CSP and in the JS server render that throws too. A package may not read `import.meta.env`: an app-local module reads the setting and passes it in; a read the bundle drops, in a function nothing calls, is not one. That check keeps a module portable between Hermes and the page; it is no security boundary: on the web the module runs in the page's realm, and a package is code the author chose, trusted as their own. A package's `/// <reference lib>` to an ES library newer than the data module's (`esnext.disposable`) loads nothing, so what that library declares (`Symbol.dispose`, which Hermes lacks) stays undeclared for the app.

Signed delivery carries one module pair, and which pair depends on the host. The
Apple loader accepts TypeScript or a Rust module enabled by the binary's Rust policy.
A Linux binary built with its update entry installs a Rust pair only. A Linux app
whose module is TypeScript, and every Windows app, sets that platform's
`deploy.store` to `"0"`: with any other level the binary exits at start. The
installed binary must supply the required runtime and admit the module's app
identity and grants. An app with both TypeScript and Rust modules sets every
`deploy.store` entry to `"0"`: delivery of both together is not implemented. See LLP 1027 for the design record.

## Access hatches

An access hatch hands the app's native code the platform object Exact built for
a node, at defined moments, so the app can do what only that object can: add a
gesture recognizer, set a navigation bar's look, draw into a view. It configures
what Exact made; it never replaces a route's authored tree. (LLP 1075.003,
1075.003.000, 1075.003.000.001; its §13 is the as-built record.)

A route's `header` or navigation root's `tablist` with `appearance="none"`
keeps its authored bar on iOS, including layout and type; a header button with
it keeps that button and its contents authored in place; the rest of the header
still projects. An authored Back replaces UIKit’s Back. Search and a header
tablist can opt out on their own element too. Tab/tabpanel navigation and retained stacks
remain intact. An opted-out root tablist also stays authored on the Mac and in wide
web windows instead of becoming a sidebar, and either stays the page's on Android
(the shared rule, `kernel/src/bars.rs`). On the Mac a header button with it keeps
the whole header painted: the window's bar shows a header whole or not at all.
See [tabs](recipes/tabs.md).

```sh
bun scripts/exact.mjs hatch avatar <app>     # in an app `exact new` made: bun exact.mjs hatch avatar
bun scripts/exact.mjs hatch --app <app>      # the app scope; --window for the window's
```

The verb writes a stub for each target the app builds (Swift in
`modules/apple/`, the page module in `modules/web/`, Rust in
`modules/linux/`), adds the word to
`app.json` `hatches` with those platforms, and tells you the node to mark:

```
column hatch="avatar" data-tone=(busy ? "busy" : "idle")
```

`"hatches": ["avatar"]` means every platform the app builds; `"hatches":
{"avatar": ["ios", "web"]}` names the platforms whose module handles the word.
Elsewhere the node is shown and no hatch is called for it. A hatch reads state
only through the node's `data-*` words (declared in `app.json` `data`), and is
called again when one changes.

An action can read those same declared words with
`querySelectorAll("[data-tone~=\"busy\"]")`: it returns the own ids of
mounted, displayed, non-inert matches in tree order; covered routes and
unselected tabs are hidden; a split list answers alongside its detail where
the host presents it (the web at desktop widths). Closed popovers
and dialogs are hidden, and open ones answer. For an Inspect overlay,
`elementsFromPoint(x, y)` returns the own ids of the boxes under a viewport
point, front to back. Both are action snapshots; `frame(id)` gives each
returned node's box for an outline. Point reads skip inert subtrees and use
the presenter's placement and order for open top-layer content.
The front modal's native backdrop names its dialog or presented route where it accepts input,
below any later popover, without exposing the page or a lower modal.

| Scope | Swift (`ExactModule`) | Page module (`modules/web/index.js`) |
|---|---|---|
| A node marked `hatch="word"` | `element(_:)`, `elementEnded(_:)`; a frame or video's `elementConfiguration(_:_:)` | `element(e)`, `elementEnded(e)` |
| A route, a navigation stack, tabs (iOS; the web's elements) | `route`, `routeEnded`, `navigation`, `tabs`, `tabContainer` | `route`, `routeEnded`, `navigation`, `tabs` |
| The window toolbar (macOS) | `toolbar(_:)` | none |
| The app: its facts, by the web's names, and the root node's `data-*` words as `data` (each in `app.json` `data`, which `hatch --app` adds; Swift reads `data-booted` as `app.data[.booted]`) | `app(_:)`, `appEnded(_:)` | `app(a)`, `appEnded(a)` |
| The window the session presents into | `window(_:)`, `windowEnded(_:)` | `window(w)`, `windowEnded(w)` |

A navigation controller (`navigation`) and Exact's tab container (`tabs`) are
UIKit's, and their `delegate` slot is Exact's, as More's is (LLP 1075.003
§3.5). Give a delegate to `navigation.delegate` or `tabs.delegate`: Exact
passes each call on to it after its own handling (a tab's `shouldSelect` and a
transition's animator ask it first), and sends it straight the calls Exact does
not implement, such as an interaction controller or the supported orientations.
Exact keeps it until another or nil is given, or the container goes. A
`controller.delegate` written directly takes the slot from Exact, which then
hears none of the container's calls, and a development build journals it. An
app that needs the tab container's slot itself owns the container
(`tabContainer`).

An `iframe` marked `hatch="word"` hands `element.platform` its `WKWebView` on
macOS and iOS, or the `PDFView` of a local PDF it shows, after the arm set its
data store, preferences, delegates and content worlds; `changed` hands the new
one when the arm replaces it (a new `partition`, a PDF to HTML). The arm owns
`navigationDelegate`, `uiDelegate`, `pageZoom`, the appearance, the user
scripts of its content controller and a PDF view's `delegate`, and a
development build names one the hatch changed (LLP 1020.002 §9); a script
message handler a hatch adds is not detected. Before each web view is made,
`elementConfiguration(_:_:)` hands the `WKWebViewConfiguration`. The arm has
set its data store and content worlds, and the frame has its own new
`WKPreferences` with the arm's two settings. Set an `applicationNameForUserAgent`, content rule lists, URL scheme
handlers for the app's own schemes, or the frame's preferences; the data store
and the content controller must stay the arm's. Change only the configuration
there: the node may not be in the tree yet. A hatched frame's first web view
waits for the module to connect, so this call comes before the web view is
made. The web view then arrives at `built`; if `built` has no `platform` (as
on macOS or in an iOS list row), the `changed` after this call hands it.

A hatched `video` on Apple calls `elementConfiguration(_:_:)` synchronously
before each item, including a changed source or `load(id)`. It hands an
`NSMutableDictionary`: `url` is the resolved source `URL`; set `asset` to an
`AVURLAsset` for custom loading. The arm applies the authored playback and
rendition preferences to that asset's item. `element.platform` then hands the
`AVPlayer` at `built`, or `changed` for a deferred player. The arm owns the item,
playback and authored preferences; retain any resource-loader delegate yourself.
See [video loading](contract-for-agents.md#sound-midi-and-media) for an example
and AirPlay/FairPlay limits. Android has no video player hook yet.

**Linux, Windows and Android paint their own pixels**, so there is no platform
object to hand over. A hatch there is Rust: one type that implements
`exact_painted::Hatches` in `modules/linux/*.rs` (or `modules/android/`,
`modules/windows/`), named once with `pub type ExactHatches = App;` and
included by the app's Linux crate (`contract::native::rust_hatch_entry` in its
`build.rs`, which the Rust-data apps here already call). Its handle carries the
node's box and words, the same acts, and two things in place of a view:
`element.overlay().draw(|c, w, h| …)`, a Canvas 2D recording that replaces the
last one whole, clipped to the node and painted over it; and
`context.observe(&element, |me, input, cx| …)`, the pointer and key input that
lands in the box, after Exact has handled it, to read only. It has the app and
window scopes, `context.frames` and `context.after`, and `diagnostics`. There
`changed` is also called when the node's size alone changes, and `ended` after
the commit that removed the node. An Android build is the same crate built for
Android; its words are the ones `app.json` gives `android`.

No hatch runs before first pixel. A node's end runs while its view is still
there, so take back there whatever the hatch added. On iOS a hatched node is a
view of its own and its list row is not reused unless the hatch sets
`element.reusable`; `logs` says what each word gives up. On the Mac a row that
leaves a virtualized list's window can be rebound to the item the list needs
next (LLP 1078): its hatched node hears `ended`, then `built` for the new item,
as a new node would.

**Into Contract only as a person could.** A handle acts on an authored node:
`click()`, `focus()`, `blur()`, and `input(text)`, which replaces a text
field's whole value without moving focus. Each is queued and runs after the
hatch returns. There is no dispatch and no state write.

**The frame clock.** `context.frames { frame in … }` (the web:
`exact.hatches.frames(frame => …)`) ticks once a frame after that frame's
tasks, and `context.after(ms) { … }` waits on the session clock. Under the
agent both run on the virtual display, so a drive repeats. They are for
behaviour, not for measuring the display. A view animated on the wall clock
(`UIView.animate`, a display link) is not on the agent's: `clock +N` leaves it
where it was, and only `clock +N real` lets it run, so a splash drawn from
`frames` or `after` is the one a drive can step past.

**The design's tokens** (LLP 1121 D12). A native view draws in the design's
colours by name: `ExactForma(view).color(.surfaceCard)` on Apple (the view a
module made, or one inside it; in the process, a view under any node, or
`ExactForma(session:)` for a screen of the host's own), a `UIColor` or
`NSColor` that resolves the scheme, contrast and elevation it is drawn in;
`ExactForma(view).color("surface.card")` on Android, a `Color` int resolved
for the view's scheme. Each reads its place's app tuple: a view under an
`-exact-mode` pin answers that mode. `observe { … }` runs after each batch that
moved what the place reads (a `setMode`, a pin above it, a live edit, the
scheme, the root font size), so re-apply a colour kept in a layer there. A
name the design lacks answers the fallback (`color(_:fallback:)`) and is
logged once. `bun exact.mjs forma gen swift .` writes the names
(`.exact/forma/forma.swift`, never committed), which a module's build
compiles in. `apps/forma-host` is the sample.

**Whose window.** `window.window` is nil unless the embedder set
`session.hatchesOwnWindow`, and `app.application` unless it set
`session.hatchesOwnProcess`. The standalone iOS app sets both; the standalone
Mac app sets the first for each document's session.

**Make the hatch visible to the agent.**

| In hatch code | Where it shows |
|---|---|
| `diagnostics.log("…")` | `logs`, as `hatch element avatar: …` |
| `diagnostics.count("swipes")`, `publish("last", value)` | `state.hatches` |
| `diagnostics.measure("swipe.worst", ms:)`, `begin("swipe") … end()` | `perf hatches`, with Exact's own timing of every call |
| `element.owns(view: added, "what it is")`, `owns(recognizer:…)` | `tree`, under the node, beside what the host observes of it |
| `element.parts = [ExactPart(id: "seal", view: v, role: "button", label: "Verified")]` | `tree`; `tap avatar/seal` clicks it as a real pointer event |

Diagnostics are bounded and kept only in a development build. `EXACT_HATCHES=off`
(`?hatches=off` on a web page) runs a development build with no hatch
connected; the app must still work. If a run dies inside a hatch, the next
launch's `logs` say which one.

## Rust data sources

A data crate answers the view's sources in Rust instead of `app.ts`: an app's
`data/` crate ([Caltrain](../apps/caltrain/data/src/lib.rs),
[Fieldnotes](../apps/fieldnotes/data/src/lib.rs)) or a game's `game.data`
(`game/README.md`). Its type implements `exact_runner::DataSource`:

| Method | What it does |
|---|---|
| `query(source, args)` | Answer now, with no I/O: the build bakes this into the plan for the first frame. `Err(DataError::Unavailable(…))` leaves the resource unbaked; every host asks again at launch. |
| `answer(store, source, args)` | At run time: `Answer::Now(value)`, or `Answer::Later(request)` for the host to run (storage, `fetch`). The default is `query`. A resource the build left unbaked is asked at boot, before the host has started storage. There `Err(DataError::Unavailable(…))`, `UnknownSource`, `BadArguments` or `Interface` refuses the boot: the app does not start (the agent says `the app booted with an error`; a release build shows no view). On a resource with something to show (its type's zero, or a declared placeholder), `Failed(code, …)` boots with the resource failed (`failure(x)`); `DeferredAtBake` and `Answer::Later` boot with it pending; `Overrun` boots with it pending and asks again twice, at 250 ms and 1 s, and a third fails the resource while the app runs on. A row that is another resource's `else source()` has nothing to show and must answer now: there each of these refuses the boot too. A source that needs storage answers `Answer::Later` and finishes in `parse`. |
| `parse(store, source, args, outcome)` | What the host brought back for that request: answer, or hand back one more request. |
| `app_id()` | The app's identity, which names its storage: `com.example.my-app-3f9a2c` (`exact new` writes the name and six random hex digits), or for a game `com.exact.<Game::ID>` unless its app.json names one. A source that names none takes app.json's `app.id` in the app's build, so its storage (`app:/data`, kept answers, `persist` states, an agent drive's `--storage`) is the app's; one that names its own keeps it, so write the same id as app.json or leave it out. |
| `grants()` | One grant per line, the lines `app.ts`'s `grants` takes (`net.fetch …`, `sqlite.open …`, `fs.read …`). |

An app outside this checkout names its data crate in app.json (LLP 1153.003 D2):

```json
"rust": { "data": { "crate": "field-log-data", "type": "field_log_data::FieldLog" } }
```

`source`, optional, is what every host and the web's module run: the type itself, its `::default()` (the
default) or its `::new(…)` with literal arguments (`"field_log_data::FieldLog::new(true)"`). A source
built from anything else (a local, a path read at build time) is exported by a crate of the app's own,
named in app.json's `rust.module.package` (Update Lab's `logic/`), which supplies the module instead.

The crate sits beside app.json and joins the workspace exact2 generates: its `Cargo.toml` sets
`package.workspace = "../.exact/shells"` and takes exact2's crates as workspace dependencies
(`exact-runner.workspace = true`; `contract.workspace = true` for a build script). The host crates are
generated under `.exact/shells/` and link it; without `app.ts` they link no JavaScript engine. With
`app.ts` beside it the two compose: Rust answers the sources the crate lists in `pub const RUST_SOURCES:
&[&str]`, TypeScript the rest. `exact new <path> --rust` writes such an app: `data/`, whose type answers
the starter's source (the view's shapes are generated into it by `contract rust`), and `rust.data`.

`cargo test`, `cargo clippy` and rust-analyzer work in `data/` once a verb has written `.exact/shells/`
(`bun exact.mjs lock` writes it): exact2 also writes `rust-toolchain.toml` and `.cargo/config.toml`
beside app.json, listed in `.gitignore`, so they use exact2's compiler and the app's `target/`. The
workspace resolves against exact2's `Cargo.lock` until the crate needs a version that lock lacks; a build
then stops and says to run `bun exact.mjs lock`, which captures `Cargo.lock` beside app.json. Commit it.

An app that `exact new --rust` made with `apple/` and `web/` crates and a root `Cargo.toml`, or one that
copied the generated workspace up only for a data or GPU crate, moves by hand:

1. Delete the root `Cargo.toml`, `Cargo.lock`, `.cargo/`, `rust-toolchain.toml` and the host crates
   (`apple/`, `web/`, and any `linux/`, `android/` or `windows/`).
2. In each remaining crate's `Cargo.toml`, set `package.workspace = "../.exact/shells"` (a GPU crate:
   `"../.exact/gpu"`), and write exact2's crates as workspace dependencies: `exact-runner.workspace =
   true`, `contract.workspace = true`; a GPU crate's `exact-gpu`, `wasm-bindgen`, `wasm-bindgen-futures`
   and `web-sys` likewise (`docs/recipes/gpu-module.md`).
3. Add `rust.data` to app.json, naming the source the deleted build scripts baked.
4. Run `bun exact.mjs lock`.

A source that wraps another implements `exact_runner::DataSourceWrapper`.
Every method is required, so adding a data-source method cannot silently use
a leaf's default in a wrapper. Forward each call or handle it explicitly.

A record is positional: `Value::record(vec![…])` takes its fields in the order
the shape declares them. `bun exact.mjs contract rust app.contract -o
/tmp/shapes.rs` writes each shape as a struct with its fields in that order.
It includes native-module call records only when an authored shape or a data
source's parameter or result type reaches them; `contract::module_rust`
generates the module bindings. A data crate's build script calls
`contract::build_rust(Path::new("../app.contract"))` to write `OUT_DIR/shapes.rs`.
Exact refreshes the generated Rust before each Cargo build, writing it only
when its contents change. The first build creates the file; Cargo establishes
its watch on the following build. Once warm, a view edit leaves the data crate
fresh when its shapes stay the same. Compiler flags, features and other Cargo
inputs still trigger builds. The dev watcher follows the generator's Contract
source graph even when the generated Rust stays unchanged. Existing build scripts
keep working. Replace their shape-generation calls with `contract::build_rust`
to get this reuse. Direct Cargo builds watch the Contract source graph. For custom generation,
`contract::compile_path_for_rust` returns the plan and call-record names to
pass to `contract::rust`; the names are compiler metadata, outside the binary
plan. Scoped names such as `Point__exact` become `PointExact`.
`Value::Number`, `Value::str`, `Value::Bool` and `Value::list` are the rest.

### GPU device requirements

A package declares `gpu.limits.required` and `gpu.limits.optional` as objects of
WebGPU limit names to integer values in its main `exact.json`. It declares
`gpu.features.required` and `gpu.features.optional` as lists of WebGPU feature names.
The Contract compiler validates names and bounds in the declaration. A package
artifact requests its own requirements before its device is created. Optional values
are clamped to the adapter, with alignment minima ordered in reverse. Buffer binding
sizes also raise `maxBufferSize`; workgroup dimensions also raise
`maxComputeInvocationsPerWorkgroup`. A required value that exceeds either adapter
limit is refused; an optional value is clamped to both. An unavailable required limit or
feature refuses the owning package's surfaces while the other surfaces keep drawing.
The web backend does not expose `subgroups`, `primitive-index` or `immediates`:
optional requests omit them and required requests refuse the package's surfaces;
native grants them where the adapter offers them.
An app without requirements retains the existing device request.

`Surface::device_ready(&mut self, features: wgpu::Features, limits: wgpu::Limits)`
receives the actual grant, including on recovery and target reattachment. A fixture
requests the same grant through `exact_gpu::fixture::device_with_requirements`.
An authored GPU module passes a static `&[exact_gpu::grants::Requirements]` as the
second argument to `exact_gpu::module!`. See [the GPU module recipe](recipes/gpu-module.md)
for a declaration and [package GPU surfaces](contract-for-agents.md#a-packages-gpu-surfaces)
for packaging. Packages declare `exactGpu: "0.4"` and depend on `exact-gpu = "0.4"`.

A surface opts into raw input with `Surface::wants_input`. On macOS the canvas
also dispatches Contract pointer, wheel and context menu events. [GPU module recipe](recipes/gpu-module.md)

### GPU storage

On Apple (macOS, iOS and tvOS), Android, Linux and Windows, a surface may implement:

```rust
fn storage(&mut self, data: &std::path::Path, cache: &std::path::Path,
    temporary: &std::path::Path) {
    self.cache = Some(cache.to_path_buf());
}
```

The host calls this hook once for each canvas, after creation and before its first
`bind`. The three absolute paths resolve that session's `app:/data`, `app:/cache`
and `app:/tmp`. They are borrowed for the call; copy any path the surface keeps.
The native ABI preserves Unix path bytes and Windows UTF-16 code units, including
non-Unicode names. On Android, Linux and Windows, a storage delivery failure keeps
the canvas and logs once at creation. The module does not create directories or retain
paths for a surface that ignores them. Create a directory when needed and handle
filesystem errors in the surface. Device recovery keeps the surface and its paths; a recreated canvas receives them
again. Keep paths on the surface, since a loaded artifact can serve several sessions.

A file written under `cache` can be shown with `image "app:/cache/<name>"`, shared
by that path, or read as an asset. Use distinct names for changed snapshots. Cache
and temporary files may be removed by the platform; durable files belong in `data`.
An agent drive's named store supplies its own roots. An unnamed drive uses temporary
process storage, not the installed app's files.

The web does not call `storage`. Its GPU artifact is WebAssembly with generated
wasm-bindgen JavaScript; Exact supplies it no filesystem or browser-storage bridge.
A GPU module that needs persistence must supply its own browser API integration.
The app's data module can separately use Exact's `storage.fs` and `storage.sqlite`
APIs, backed by IndexedDB in this repo. Those APIs are not injected into GPU modules
and supply no paths usable by `std::fs` or native SQLite.
`exact-gpu` 0.4 carries the hook and the native `gpu_storage` export; rebuild GPU
artifacts with the host.

### An authored workspace

An app whose host crate does what no generated one does (its own `build.rs` or render entry, or a
dependency of the host crate itself) writes its whole Cargo workspace (LLP 1153.003 D3). A `Cargo.toml`
beside app.json makes every crate in it the app's: exact2 generates no host crate, and checks the
workspace's `[patch.crates-io]` and toolchain against its own. Nothing converts one shape into the
other. To start one, copy the generated workspaces up and rebase what pointed into them:

1. Run a native verb once (`bun exact.mjs mac`): it writes `.exact/shells/`, and `.exact/gpu/` for a
   GPU crate.
2. Copy the host's up: `cp -R .exact/shells/. . && rm -rf module .stamp.json`. The web build makes its
   own Rust module for an authored workspace.
3. The copied files are two directories shallower than they were, so each of their paths drops two
   `../`. Run this on the root `Cargo.toml` and each copied host crate's `Cargo.toml` and `build.rs`, not
   on your own crates: `sed -i.bak 's#"\.\./\.\./#"#g' Cargo.toml web/Cargo.toml web/build.rs apple/Cargo.toml
   apple/build.rs` (and `linux/`, `android/`, `windows/` when copied), then delete the `.bak` files.
4. In the root `Cargo.toml`, `members` lists `"data"` and `"gpu"`, not `"module"`. For a GPU crate, add
   the lines under `[workspace.dependencies]` in `.exact/gpu/Cargo.toml` to the root's, each path
   dropping two `../` as in step 3.
5. In each of your own crates, `package.workspace` becomes `".."`.
6. Delete `.exact/` and `Cargo.gpu.lock`, and remove `/rust-toolchain.toml` and `/.cargo/config.toml`
   from `.gitignore`: the copied toolchain and configuration are the workspace's now, and committed.
7. Run `bun exact.mjs lock`, then build as before.

### A two-way socket

On native hosts and the web JS target a Rust source sends on a WebSocket with
TypeScript's rules (LLP 1016.003). Answer `Answer::Later(Request::socket(url))` for a `ws:` or
`wss:` URL under a `net.websocket` grant. Its `parse` is the mapper: it gets
an `Outcome::Message` whose `event` is `open`, then one for each frame in
order (text in `data`, a binary frame in `binary`), then `Outcome::Closed {
code, reason, was_clean }`. While it answers `open` or a frame,
`store.socket()` is that socket's handle; keep it to send from a mutation:

```rust
fn answer(&mut self, store: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
    match source {
        "book" => Ok(Answer::Later(Request::socket("wss://stream.example/ws"))),
        "subscribe" => {
            let socket = self.book.ok_or(DataError::Unavailable("the book is not open".into()))?;
            socket.send_text(store, r#"{"method":"SUBSCRIBE","params":["depth.SOL_USDC"]}"#)?;
            Ok(Answer::Now(Value::Bool(true)))
        }
        _ => Err(DataError::UnknownSource(source.into())),
    }
}
fn parse(&mut self, store: &mut Store, _: &str, _: &[Value], outcome: Outcome) -> Result<Answer, DataError> {
    match outcome {
        Outcome::Message(m) if m.event == "open" => {
            let socket = store.socket().expect("a socket's mapper has its handle");
            self.book = Some(socket);
            socket.send_text(store, r#"{"method":"SUBSCRIBE","params":["trade.SOL_USDC"]}"#)?;
            Ok(Answer::Now(Value::str("open")))
        }
        Outcome::Message(m) => Ok(Answer::Now(Value::str(&m.data))),
        Outcome::Closed { code, .. } => Ok(Answer::Now(Value::str(&format!("closed {code}")))),
        other => Err(DataError::Unavailable(other.summary())),
    }
}
```

`Socket` has `send_text`, `send_binary`, `close(store, code, reason)`,
`buffered_amount` and `is_open`. A mutation's `answer` may send on any open
socket of the app's resources, and must then answer `Answer::Now`; a mapper
sends only on its own socket. What either stages goes once its commit
stands. A refusal is an `Err` whose text names the DOM exception a
TypeScript handle would throw: `InvalidStateError` for a socket not open, a
handle from before the resource was asked again, or a send from a resource's
answer; `QuotaExceededError` past 256 operations or 1 MiB a call, which
fails a mapper's frame even when it ignores the error. The web's wasm host
and a worker-placed source cannot hold the handle, so they refuse
`Request::socket` by name rather than open a socket that only listens
(`Request::socket is unavailable on the web's wasm host`, `… in a
worker-placed source; place it on main`).

### Where data lives: app storage, not secrets

Keep app and game data (best times, settings, saves, notes) in **app
storage**: SQLite databases and files under `app:/data`, the storage `app.ts`
reaches as `storage.sqlite` and `storage.fs`. A Rust source asks for it with
`exact_data::storage::request(op, args)` as an `Answer::Later`, and reads the
reply in `parse` with `exact_data::storage::response(outcome)`, which is the
JSON result or the host's message. The operations:

| `op` | `args` | Reply |
|---|---|---|
| `"sqlite"` | `{"path": "app:/data/x.db", "commands": [{"kind": "execute" or "query", "sql", "params"}]}` | One result per command, in order: a query's `{"columns", "rows"}` (an integer is `{"integer": "7"}`, text a string), an execute's `{"changes", "lastInsertRowid"}` |
| `"sqlite.transaction"` | the same, `execute` commands only | the executes' results, all or none |
| `"fs.readFile"`, `"fs.atomicWriteFile"`, `"fs.writeFile"`, `"fs.appendFile"` | `{"path"}`, and `"text"` (or `"bytes"`) to write | read: `{"base64"}`; write: `null` |
| `"fs.mkdir"`, `"fs.rm"`, `"fs.stat"`, `"fs.readdir"`, `"fs.rename"`, `"fs.copyFile"` | `{"path"}`, and `"destination"` to move or copy | as `storage.fs` answers |
| `"fs.compressImage"` | `{"path", "destination", "maxDimension"}`, `"maxBytes"`, `"quality"` or both, and `"type"` (`"image/png"`; absent or `null` is JPEG), `"rotate"`, `"flip"`, `"crop"`; `null` is absent | `{"path", "type", "size", "width", "height"}`, as `storage.fs.compressImage` answers |
| `"fs.transformImage"` | `{"path", "destination"}` and `storage.fs.transformImage`'s options beside them (`"rotate"`, `"crop"`, `"metadata"`, …; `null` is absent) | `{"path", "type", "size", "width", "height", "colorSpace", "hdr", "metadata"}`, as it answers |

Grant what it touches: `sqlite.open app:/data/x.db`, `fs.read app:/data`,
`fs.write app:/data`. Storage is asynchronous and starts after first pixel. It
is absent at build, so `query` answers a placeholder, and in a scripted drive
that names no scratch store: there `response` is an error, and the source
answers as if nothing were kept. `--storage <name>` gives a drive a store kept
between native drives (`--reset-storage` empties it); on the web the next drive
keeps it with `--keep-storage`. An authored test gets an empty one, and its `reload` restarts
on it. A game's data crate takes these crates from the SDK
(`exact-data.workspace = true`, `serde_json.workspace = true`).

Best times, one row a level, decided in SQL. This is the platformer's source
(the diary's R6). One request reads the previous best and writes the run where
it beats it:

```rust
use exact_data::storage;
use exact_runner::{Answer, DataError, DataSource, Outcome, Store, Value};
use serde_json::json;

const DB: &str = "app:/data/scores.db";
const TABLE: &str = "CREATE TABLE IF NOT EXISTS best (level INTEGER PRIMARY KEY, ms INTEGER NOT NULL)";

#[derive(Default)]
pub struct Scores;

impl DataSource for Scores {
    fn app_id(&self) -> &str { "com.exact.hopper" }
    fn grants(&self) -> &str { "sqlite.open app:/data/scores.db" }
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        match source {
            "best" => Ok(Value::record(vec![Value::list(vec![])])), // the build's first frame: nothing kept
            _ => Err(DataError::Unavailable("asked at run time".into())),
        }
    }
    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        let commands = match (source, args) {
            ("best", _) => json!([{"kind": "execute", "sql": TABLE, "params": []},
                {"kind": "query", "sql": "SELECT level, ms FROM best ORDER BY level", "params": []}]),
            ("finish", [Value::Number(level), Value::Number(ms)]) => json!([{"kind": "execute", "sql": TABLE, "params": []},
                {"kind": "execute", "sql": "INSERT INTO best VALUES (?, ?) ON CONFLICT(level) DO UPDATE SET ms = excluded.ms WHERE excluded.ms < best.ms", "params": [level, ms]}]),
            _ => return Err(DataError::UnknownSource(source.into())),
        };
        Ok(Answer::Later(storage::request("sqlite", json!({"path": DB, "commands": commands}))))
    }
    fn parse(&mut self, _: &mut Store, source: &str, _: &[Value], outcome: Outcome) -> Result<Answer, DataError> {
        let results = storage::response(outcome);
        let int = |cell: &serde_json::Value| cell["integer"].as_str().and_then(|n| n.parse::<f64>().ok());
        Ok(Answer::Now(match source {
            // No storage here (a drive with no scratch store): nothing kept.
            "best" => Value::record(vec![Value::list(results.ok().and_then(|r| r[1]["rows"].as_array().cloned()).unwrap_or_default()
                .iter().filter_map(|row| Some(Value::record(vec![Value::Number(int(&row[0])?), Value::Number(int(&row[1])?)]))).collect())]),
            _ => Value::record(vec![Value::Bool(results.map_err(DataError::Unavailable)?[1]["changes"].as_str() == Some("1"))]), // a new best?
        }))
    }
}
```

`Store` (`store.get`, `store.set`, under `secret.keep <name>`) is for
**secrets**: a session token, a key. Apple keeps them in the Keychain, Linux
in a file per secret only the user can read (`0600`, under
`$XDG_DATA_HOME/exact/<app id>/secrets`; not encrypted), Android in AES-GCM
files with Android Keystore-wrapped keys, and the web in origin-readable
`localStorage`; Windows keeps them in memory for now.
**For web refresh and session tokens, use a server-set HttpOnly, Secure cookie:**
[web sign-in tokens and sessions](recipes/web-sessions.md) covers the shared
native/web flow, selected at run time by `exactHost()` inside an answer
(`'web'` or `'native'`, no grant; also on web workers). It refuses at module
initialization and outside an answer; the bake leaves its answers for the host.
Web `secret.keep` does not protect against injected scripts.
Opening Android's secret store deletes legacy
plaintext files without reading them; only encrypted entries are restored.
The host reads secrets into a snapshot before boot, so a read is synchronous,
and a scripted drive never keeps them. A best time is not
a secret: keep it in app storage. A lease on a secret (`SharedSecrets::acquire`,
from `configure_secrets`) needs the same grant: on Apple a name the grants do not
keep reads and writes as `denied: secret.keep`.

## The six checks

```sh
cargo build --all-targets --keep-going                                  # build
cargo test --tests --no-fail-fast                                       # test
cargo clippy --all-targets --keep-going -- -D warnings                  # lint, and
cargo fmt --all -- --check                                              # lint (run both)
bun scripts/caps.mjs                                                   # caps
bun scripts/boot.mjs                                                   # boot graph
cargo check --workspace --all-targets --keep-going                      # workspace
```

The first three cover the root `default-members`: the deterministic, in-process
crates, the web host's and the Apple host's Rust among them (the Apple host's
real-socket, wall-clock and toolchain-launching tests are `async lane:`; a test
may still re-run its own binary to isolate its environment). The sixth compiles
every crate and target of the workspace and runs none of their tests: without it
a crate outside `default-members` can stop compiling on `main` while the other
five pass, as `exact-web-js` and `exact-painted`'s tests did on 2026-10-11. It
needs the pinned Bun first on `PATH` (or named by `BUN`): app crates' build
scripts run Bun, and under an older one each fails with `Bun <version> is older
than …`. The async lane runs the first three with
`--workspace` (the other hosts, GPU, Hermes, platform shells, stress fixtures),
which covers the sixth, and tests the separate game workspace with
`cargo test --manifest-path game/Cargo.toml --workspace --locked --lib --bins --tests`.
Changes to Apple host files, its Cargo path dependencies, or apps trigger the
iOS simulator tests, touch smoke and the tvOS compile
(`EXACT_JS_ENGINE=stub bun host/apple/build.mjs --tvos --no-install`: the Apple TV
bundle built and signed, installed on nothing, so it needs the tvOS SDK and
Rust's `aarch64-apple-tvos-sim` target and no simulator). Docs-only changes do not. The hourly
`--tier 2` lane installs that tvOS build on an Apple TV simulator and runs target-specific `cargo check` for the Linux
host and every Linux app (`-p '*-linux'`: each `apps/<app>/linux` crate is a
workspace member named `<app>-linux`) and for the Windows host and Windows Desk, on x64 and ARM64. Install
`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-gnu` and `aarch64-pc-windows-gnullvm` with `rustup target add`
and provide `zig` on the Mac: it compiles C, and C++ for the Linux target
(Hermes's JSI bindings). The check runs offline, so install the pinned Linux
Hermes bundle once per Hermes pin (again whenever the pin changes): `cargo run
--manifest-path vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml --
--target x86_64-unknown-linux-gnu`. Until then the step fails, and the
`hermes-lean-sys` build error in its log names that command. These cross checks
cover compilation; native window presentation, KMS, and input devices remain
unqualified until run on their platforms.

Development and test builds optimize the third-party CPU rasterizer `tiny-skia`.
Debug assertions and overflow checks remain enabled; the normal development
profile still leaves app and engine code unoptimized. Tests keep their full frame
counts. Use release builds when comparing application frame costs.

Production Apple builds are balanced on iOS and tvOS, and optimize for speed on
macOS. Embeds default to size on every Apple platform; a game keeps speed.
`host.ios.optimize` (also used by tvOS) or `host.macos.optimize` in app.json
selects `"speed"`, `"size"` or `"balanced"`; `--optimize speed|size|balanced`
overrides it. Size uses Cargo's `size` profile and Swift `-Osize
-internalize-at-link`; speed uses `release` and `-O`; balanced uses the
`balanced` profile (size, with the engine's hot crates at opt-level 3) and
`-O`. Games refuse size and balanced because their generated Cargo workspace
has neither profile.
Development builds keep `host-dev` (LLP 1047.001 D9).

An unset `EXACT_UPDATE_TRUST` bakes development trust. A development binary
admits unsigned heads, so it checks only an origin named by
`EXACT_UPDATE_ORIGIN`, never the manifest's. `EXACT_UPDATE_TRUST=production`
bakes a release: an updating native artifact requires
`EXACT_UPDATE_RECEIPT` pointing to the authenticated publisher receipt for its
exact plan and complete asset roster. A new production stream instead requires
`EXACT_UPDATE_GENESIS=1`; it starts at sequence zero. Existing streams retain the
receipt's sequence and verification keys. Updater-free Level 0 artifacts require
neither input.

`kernel/tables/schema.json` is the one declaration authority for node types, props,
style rows, enums, and opcodes; `kernel/build.rs` generates the Rust from it at build
time. Edit the table, never the generated code.

## Installation page

Every web build includes `/.exact/install/`, with explicit `web/`, `ios/` and
`macos/` pages. Each shows stacked Web, iOS and macOS sections; unavailable
platforms are gray. Web works by default and shows its destination URL. Native
methods appear only when configured. The header shows the build id, available
source facts and serving context. Development builds record their build time;
release builds leave it unrecorded. Optional `brand.logo` and `brand.wordmark`
reuse app images or a text wordmark with an asset font; the footer uses the gray
Exact mark.
Beside the pages, `/.exact/install.json` carries the same content as data (the
app, its build, each platform's methods in order with labels resolved, and what
it can reach), for a native client such as Exact2 Go to draw its own install
sheet. Both web targets write them; a deploy's JS root carries its bake's.

Configure `install.<platform>.methods` in the app’s `app.json`; use
`recommended` to name a configured method. The schema is
[`scripts/app.schema.json`](../scripts/app.schema.json) (LLP 1030.003 D6a). A method links to an already available installation
flow; configuring it does not build, sign, upload or verify a native app.
`terminal` displays a command and offers Copy, never executes it.

On a Mac, the development server adds a dev-only iOS method to the page. It lists
available iOS Simulators and reachable paired devices, then **Build and install**
runs the existing `--ios --sim` or development-signed `--device` build, installs
the app, and opens it on the same development URL. Simulator builds work from a
loopback server. A physical phone must be unlocked, paired, in Developer Mode and
able to reach the server, so start the dev server with `--lan`; the build opens the
app on a LAN address the server printed. The action appears only on a page loaded
over loopback on the Mac itself, and its request is protected by a random token that
exists only for that server process. Static and hosted pages never expose it.

For additional web destinations, set `install.web.urls` to entries such as
`{"label":"Public", "url":"https://interview.example/"}` alongside the web
method. These are explicit HTTPS destinations; the build does not guess a public
hostname. Development install pages also list localhost and interface addresses
for the actual listener, labeling LAN and Tailscale/VPN addresses. Loopback-only
servers omit other interfaces. Published pages do not expose the host's private
network addresses. Address discovery uses the OS interface list, with no external
commands on the request path.

## Rust live replacement

Rust replacement is enabled by default in development and production: native
shared libraries on macOS/Linux, the browser's Wasm executor on web, and the
`wasmi` interpreter on iOS devices and simulators. Native hosts start with their
linked Rust; interpreted execution applies to replaced logic. Android/Windows
have policy defaults but their hosts remain future work. LLP 1029.000
records the boundary and verification status.

Control inclusion in `app.json`. `"rust": false` disables everywhere;
`"rust": {"prod": false}` keeps it in development only. A platform can override
both or either environment:

```json
{
  "rust": {
    "module": { "package": "caltrain-logic" },
    "platforms": {
      "ios": { "prod": false },
      "macos": { "prod": "wasm" }
    }
  },
  "dev": { "rebuild": { "rust": "manual", "typescript": "save" } }
}
```

Modes are `auto`, `native`, `tiered`, `wasm`, and `off`; `true` means `auto` and `false`
means `off`. Resolution is global → environment → platform → platform environment.
`off` removes the replacement path from the native bake; changing that requires
a new binary. TypeScript and delivery cadence remain separate choices. Save is
the default trigger for both languages while the development watcher runs;
manual is useful when an agent wants to finish a set of edits before rebuilding.
After the app's initial web bake, run `bun scripts/rust.mjs caltrain` to build
the independent Rust module variants; the running dev server consumes the
completed output. At the dev
server’s terminal, `r` + Enter rebuilds Rust and `t` + Enter rebuilds TypeScript.

For portable modules without essential private state,
`"rust": {"platforms": {"macos": {"dev": "tiered"}}}` runs new Wasm immediately
after publication and promotes to native when the library finishes loading.
Both variants must declare `exact_logic_abi::export!(Data, constructor, stateless)`:
essential state lives in host inputs, Contract or host-owned storage; initialization
and destruction have no external effects. Promotion preserves the app generation
and does not restart TypeScript or replay business calls. Native loading failures
leave Wasm running. This mode adds the interpreter and both artifact variants;
explicit `native` avoids that interpreter. The usual dev/prod overrides apply,
but `tiered` is unavailable on iOS/web. Update Lab opts in for macOS development;
`auto` defaults are unchanged. Both variants still compile before publication.

A binary runs its own compiled data crate until an update replaces it, and only a
module that differs from that code is interpreted. The build records the digest of
the source the module compiles from (`nativeSource` in `scripts/rust.mjs`: the
module's local crates, the workspace's `Cargo.toml` and `Cargo.lock`, and the
compiler) and every Rust receipt carries it as `source`. A candidate whose
`source` is the binary's own becomes the native data source again
(`Swappable::with_native`). On iOS an update that changes the Rust runs
interpreted until a binary built from that source is installed; an update that
changes only the Contract or TypeScript leaves the Rust native.

The bake accepts `cfg(target_*)` branches and native dependencies in the data
crate: the source digest reads every arm, so an edit to any of them moves it
(LLP 1030 D3a, amended 2026-11-12). An interpreted module is built for
`wasm32-unknown-unknown`, though, and runs only the crate's
`target_arch = "wasm32"` arms, never one for the device's `target_os`. A crate
whose module can be replaced should branch on nothing else
([pitfall](agent-pitfalls.md#actions)).

The digest also covers the path crates the data crate is built from, and
theirs in turn, including an optional one no default feature enables: the
build that links the crate can enable it. One case is not covered. A path
crate that `[patch]` puts in place of a registry or git dependency counts
only where Cargo's default-feature resolve reaches it. When only an optional
dependency that resolve leaves off reaches it, an edit to it does not move
the id; declare that dependency with `path` to have it counted.

The repository's app entries use `contract::rust_entry` to select a concrete
factory at bake time. A custom host adapter must do the same (and depend on
`exact-logic`), or compose `Swappable::native`, `Swappable::tiered`, `Swappable::wasm` or
`Swappable::browser` explicitly. Merely declaring a module does not retrofit
a custom, manually implemented `DataSource` host.

[Update Lab](../apps/update-lab/README.md) composes one Rust probe with one
TypeScript executor. Its complete development candidate carries both modules,
so a TypeScript or Contract edit retains the last accepted Rust version.

The replacement unit is the declared portable logic-wrapper `cdylib`, not an individual
Rust function or file. Keep business logic in small Cargo crates and native I/O
in host adapters; Cargo reuses unchanged helpers, but their dependent loadable
module still relinks. Independent replacement of several domains requires
separate loadable artifacts and explicit host composition; a multi-module
manifest registry is not implemented.

Production capability does not grant store permission to deliver arbitrary
code updates. Apple's [review guidelines](https://developer.apple.com/app-store/review/guidelines/)
restrict downloaded feature-changing code, including interpreted code; Google
Play's [policy](https://support.google.com/googleplay/android-developer/answer/16559646?hl=en)
restricts downloaded native `.so` code and describes a VM/interpreter exception.
Use the production opt-out where the app's distribution requires it. Native
macOS libraries also need platform-appropriate code signing: the production
producer requires `EXACT_RUST_SIGN_IDENTITY` for the host's signing team, or
select `wasm` for macOS production.

## Interactive stress examples

[Messages stress](../apps/messages-stress/README.md) compares a bounded synthetic
history page with deliberately eager construction while typing and streaming
updates. [Completion Storm](../apps/completion-storm/README.md) holds and releases
real local HTTP requests, including failures and replies to a departed screen.
[Markdown stress](../apps/markdown-stress/README.md) exercises the shipped parser
and reader components with large documents, huge individual blocks and reflow,
and the Markdown editor with a toolbar and a link sheet
([`markup`, `format`, `select`](contract-grammar.md#markdown-markup-format-select)).
These are opt-in developer workloads; none claims automatic virtualization or
120 Hz performance. LLP 1041 specifies the
graceful-overload direction and records what the first examples actually prove.

With a fixture running, `bun scripts/metrics.mjs --stress-url
http://127.0.0.1:4318 --seconds 10 --target-hz 120` samples typing-to-echo and
frame-callback gaps (`CHROME` selects the browser). Repeat `--tap <testId>` for
workload controls. This is a headless diagnostic, not a physical-display FPS test.

On macOS, the driver’s `resize 900x700` requests 900 points of full content
width (including a sidebar) and 700 points of page height. A page with
`viewport-fit="cover"` or an active `window-controls-overlay` includes the titlebar
in that height; other pages start below the window’s bars. Its reply reports the
page’s actual viewport separately.

After building a native app, `bun scripts/native-resize-metrics.mjs macos
--app messages-stress` interleaves window resizing, typing and scrolling, then
checks geometry, echo and recovery. Substitute `linux` on an actual Linux host
or select `--app markdown-stress` / `completion-storm` (with its fixture running).
It records raw command acknowledgments and executable identity; AppKit window
resizing and Linux headless presenter resizing do not measure physical refresh.

### Intersection visibility

`reachstart` and `reachend` belong only to `list virtualized=true` with one
direct keyed `for` (LLP 1010). A vertical list with `appearance="none"` can
also hold always-mounted lead and trail children. Only a non-empty data
collection fires edges. Without fixed content,
its geometric window reaches the first or last supplied row. With fixed content,
the edge is the lead's start or the trail's end. An empty data collection fires
neither event. A plain list lays out all its children and has no collection window;
the Contract compiler refuses either handler there, on every target.

In a list with no fixed content, a loading or start-of-thread notice can be
one more item of the `for`'s list, put first by a `derive`. The `for`'s root
draws it by its key, and `reachstart` watches that edge row. For a notice in
fixed content, see [Long lists](recipes/long-lists.md): the lead's start is
the edge.

```text
derive rows = [Message(id="older", body=(atStart ? "Start of the chat" : "Loading…"))].concat(thread)
```

`reach-margin` on such a list sets how far ahead of an edge they fire: the edge
row may be up to that far past the port. It is a literal, one or two nonnegative
px or % lengths (before and after; a percentage is of the list's port along its
axis), and defaults to one port, `"100%"`. `reach-margin="200%"` asks for the
next page when the last row is two ports away (LLP 1145 D4). That is the
distance at rest. While the list scrolls toward an end, the margin on that side
grows by as much as the list's window leads for the scroll: a quarter second of
travel, up to two ports. A fling up a transcript at 4,000 px/s in a 320 px port
asks for older rows once the first row is within 960 px (LLP 1027.004 D3).

For a plain feed or masonry, use `intersect=` on a marker at the end of the
content, after the columns, inside the same scroller:

```text
view intersect=loadMore intersect-margin="0px 0px 800px 0px"
  height=1
```

`loadMore(entry: IntersectionObserverEntry)` should load only when
`entry.isIntersecting` is true, a next page exists, and no request is pending.
The bottom margin starts loading up to 800px before the marker enters the
scrollport. This observes the marker, not the last row of each column.
See [paging columns](recipes/responsive-columns.md#paging-columns) for a complete example.

`intersect=action` on a node offers a final `IntersectionObserverEntry` record
(`isIntersecting: bool`, `intersectionRatio: number`) after authored arguments.
The nearest ancestor scroll container is the root, otherwise the viewport;
threshold is fixed at 0. It delivers after mounting and on enter/leave, not
on every ratio change. Entries in one root/margin group follow observation order,
after resize callbacks and authored scrolling. `intersect-margin="200px 0px"`
is a literal rootMargin (one to four case-insensitive px/% lengths, default 0px).
Numbers follow CSS syntax: `.5px` and `1e2px` are valid; `1.px` is refused at compile time.
Percentages use the corresponding root dimension.
On iOS, a `resizes-visual` route's viewport can be the root, including at offset zero.
`overflow: clip` clips descendants without becoming a root. The margin expands
only the root; clips between the target and root keep their original bounds.
Ancestors above an explicit root only map its coordinates.
This can keep T3 sidebar subscriptions active near the viewport and track timeline turns.
Removal cancels observation without a leave callback. Both web targets use the
browser's IntersectionObserver after first paint. AppKit, UIKit and Linux use
the shared native sampler after resize settlement and presented scrolling.
Native inline runs, moving presentation geometry, rotated roots and SVG clip references remain unqualified;
percentage margins follow Chrome (LLP 1001). See LLP 1005's intersection
visibility deviation; other native hosts remain unqualified.

### Scroll drags and swipes

`scrolldragstart` and `scrolldragend` on a scroller bracket the reader's drag
of it, as UIKit's `scrollViewWillBeginDragging` and
`scrollViewWillEndDragging(_:withVelocity:targetContentOffset:)` do. The end's
`ScrollDragEvent` carries `velocityX` and `velocityY` in UIKit's units, CSS
pixels per millisecond, positive in the direction `scrollLeft` and `scrollTop`
grow, and `targetLeft` and `targetTop`, where the momentum will stop, `some` on
iOS and Android and `none` where the host cannot say before it gets there. macOS
takes a trackpad's two fingers as the drag and the web a touch, each estimating
the velocity from the last 100 ms of the fingers' moves; Android takes a finger
on each scroller it moves, with the platform's velocity; Linux and
Windows send neither. A wheel, the keyboard and a programmatic scroll send
neither
([events](contract-grammar.md#events)).

`swipeleft` mirrors `swiperight`: the node follows the finger, and one action
runs when it lifts 64 points or more that way. Both directions are the
screen's, in a right-to-left layout too, and a node may have both
([Swipe to reply](contract-for-agents.md#swipe-to-reply)).

## Timer options

### A timer that can wait

Timers have precise deadlines by default. Choose an admission condition for work that can wait:

```
task refresh mount
  every(5000, tick, wait=scrollend, max=1000)
task autosave when dirty
  after(800, save, wait=idle, max=5000)
```

| Option | Live behavior |
| --- | --- |
| `wait=frame` | Run at the next frame callback at or after the deadline; request a frame if necessary. |
| `wait=update` | Join existing frame demand; run immediately when the host observes none. |
| `wait=scrollend` | Wait for the session's observed scroll activity to end. |
| `wait=idle` | Wait for no observed scrolling, frame demand, unapplied batches, or conflicting timer work; keep a 60 Hz frame period clear before the next known deadline. |

`frame` and `update` have a 100 ms waiting bound. `scrollend` is unbounded unless `max=` names a bound. `idle` defaults to the larger of its interval and 1,000 ms. Bounds start at the nominal deadline; reaching one makes the firing ordinary due work. They limit intentional waiting, not OS scheduling latency. A hidden session admits frame and scroll waits without waiting for display callbacks. Observations can arrive late; a web scroll that ends without an event uses a quiet-period fallback.

The web host treats a virtualized list as still scrolling until its final motion report is accepted and any scroll event owed by an accepted position change is delivered.

Use `wait=scrollend` for list refreshes and `wait=idle` for saves or housekeeping. For a clock display, prefer live `time` rather than an action that copies the clock into state. A waiting repeat keeps its original phase and merges missed beats. A false gate or changed key cancels its held firing.

The agent's clock is deterministic: `frame` runs at the next virtual 60 Hz frame; `update`, `scrollend` and `idle` run at their nominal deadlines, with idle tasks last at the same time. `state.taskTiming` reports each task's nominal deadline, requested wait kind, held state, active cap and wake tolerance. `perf` counts firings by admission kind, including caps.

### A wake the OS can coalesce

```
task housekeeping mount
  every(5000, clean, tolerance=500)
```

`tolerance=` allows a wake to be delayed by up to that many milliseconds. It accepts a literal whole number from 0 through 4,294,967,295; zero requests no intentional delay.

Apple gives each ordinary millisecond task its own native timer with the full requested `Timer.tolerance`; the OS chooses coalescing and may limit tolerance. Windows uses an independent thread-pool timer with its full requested tolerance and native repeat period. Each native callback runs only its own task; live order follows OS delivery. These ordinary timers do not request display-link callbacks.

Hosts without a native tolerance backend schedule a shared wake at the latest permitted time, bounded by every known timer and internal deadline; another wake may run a tolerant action once it is due. Actions never run early. Suspension and executor load can still make them late.

Positive tolerance and `wait=` are alternatives. Frame tasks cannot wait or have positive tolerance. Tolerance affects host wakes only: seeks retain nominal times, and a repeating timer retains its phase. It permits wake coalescing; it makes no claim about measured battery savings.

## Live time and progress

`time` is HTML's instant, in epoch milliseconds. `format` names the platform
presentation: `relative`, `timer`, `countdown`, `date-short`, `date-medium`
(the default), `date-long`, `date-full`, or `time-short` and `time-medium` (with seconds). Date and time styles use the viewer's locale and
time zone (Linux desktop currently uses English). Children are fallback text if the host cannot format the instant.

```contract
component Clock
  view
    column
      time datetime=1792209600000 format="relative"
        text "Departure"
      time datetime=1792209600000 format="countdown"
      progress start=1792209000000 end=1792209600000 aria-label="Trip"
```

Relative time changes at minute boundaries, timers and countdowns each second.
A timer counts elapsed or remaining time on either side of its instant; a
countdown stops at zero. The runner drives them without an app task, including
under the agent's `clock +60000`. A standalone live `time` reserves its box
at its font: two localized units for relative time, one extra hour digit
for a timer, and the first displayed shape for a countdown. Apple timers use
SwiftUI’s first field (`0:05`, `1:00:00`). A live `time` inside a `text` with other runs reflows as on the web;
make it its own text node when its width must stay fixed. Changing numeric
text uses tabular digits by default.

`progress start=… end=…` advances from zero to one over the interval and
clamps outside it. Supply both instants with `end` after `start`, without
`value` or `max`. The existing forms remain: `progress value=0.4 max=1` is
a determinate bar, and `progress` is the platform activity indicator. A
progress control takes no children; `aria-label` names it.

### Widget development gallery

A development Apple build with `widgets`, made with `--widget-gallery` or
`EXACT_WIDGET_GALLERY=1`, has **Widgets…** in its dev menu (Develop on macOS). It shows every declared family in full colour, accented and
vibrant modes, light and dark, at large and XXXL text sizes. It uses the entry's
headless plan and the extension's snapshot driver. In-process containers and
mode presentation are stand-ins; archive fidelity needs the real extension.

| Agent address | Reads |
|---|---|
| `bun scripts/agent.mjs ios widget next-trains --family systemMedium --mode accented --scheme dark --size xxxl tree "screenshot target/widget.png" state` | Snapshot tree, the entry alone, and its props |
| `bun scripts/agent.mjs macos widget next-trains tree state` | First declared family, full colour, light, large text |
| `bun scripts/agent.mjs ios widget next-trains --epoch 2026-08-28T09:05:00Z "clock +600000" tree state` | Refresh and render at the app’s advanced clock |

| `bun scripts/agent.mjs ios widget next-trains --configuration '{"rows":"one"}' "screenshot target/one.png" state` | A configured widget's refresh and render with that choice; `state` reports the `configuration` and the props its refresh returned |

A configured widget's cells render its defaults, then each other value of each
enum parameter (the rest at their defaults) in full colour, light and dark;
`--configuration <json>` names parameters and values over the defaults, and a name or
value the configuration does not declare is refused with the ones it does. An
entity parameter takes any entry's id, as `--configuration '{"book":"<id>"}'`:
the gallery reads no published list. Each cell's refresh receives the values it
renders.

An immediate refresh answer supplies its first timeline entry. Pending, empty
or unavailable data keeps the preview; `state` reports which. The gallery's
refresh reads none of the app's saved data and sends no requests, so a widget
whose refresh reads storage shows what it shows with none; every read of a
refreshed preview says so (`storage`). Check such a widget with `--props`, or
on a Home Screen, where the extension reads both. `--props <json>`
replaces the preview props for this read and takes precedence over refresh. Widget
reads accept only `tree`, `screenshot <png>`, `state` and the app’s `clock`; film,
window, compare and `--scale` options are refused. Ordinary builds name the
`--widget-gallery` rebuild. Production builds include no gallery code.
