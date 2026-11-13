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
the one-time command. `HERMES_LEAN_SYS_DIR` remains a development override.
The v4 set supports macOS, Linux, Windows, iOS devices and the universal iOS
Simulator, plus tvOS devices and the arm64 tvOS Simulator. Exact selects
Ibex's English `intl` data tier on Linux, not `intl-all-locales`; Windows
uses OS ICU and Apple keeps Hermes's OS-backed Intl. Android's published v5
bundle builds without Intl. A bundle whose receipt enables Intl and binds the
English ICU tier selects that tier and Ibex's Intl group. A Rust-only app needs no engine at run time.
`exact setup` installs the host bundle and, on macOS, the iOS and tvOS Simulator
bundles, and the iOS device bundle where Xcode's iphoneos SDK is installed.
`exact setup --check` runs Ibex's own offline resolver validation over that
same set. On a CI worker, `bun <exact2>/scripts/exact.mjs setup --check
--platform ios --device` checks the archive's prerequisites; `--platform
android` checks the APK/AAB prerequisites, including JDK 17+ and the NDK's
actual compiler paths. A scoped check fails when any of its rows is missing
and does not check unrelated web tools. `--platform` is check-only; install
commands are in the platform's CI recipe. The general check still reports
optional tools separately (LLP 1086 D6).

### Windows painter

An app with no `host.windows.painter` uses ink on a DirectX 11 composition
swap chain, except an app declaring `game`, which keeps the CPU painter so
its module presents directly. Explicit `"ink"` or `"cpu"` selects that painter;
CPU keeps the `softbuffer` window. `EXACT_PAINTER` overrides either choice.
`EXACT_INK_DEVICE=wgpu` selects ink's wgpu device (DirectX 12 with
`WGPU_BACKEND=dx12`, as the measurements set); `warp` selects software
DirectX 11. An Ink build requires the Windows SDK's `fxc`; without it,
the build refuses and names the SDK and `"cpu"` remedies.
`agent windows`, `test windows` and `verify windows` explicitly select CPU
for their headless drives unless `EXACT_PAINTER` is set. Those screenshots
do not qualify the window's composition path.
A direct `EXACT_SMOKE=1` launch uses the app's painter choice.
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
a pin at 0.4.43 or later drops the override. `snapback4/` is one client for an
app's Rust and TypeScript on every host ([its README](../snapback4/README.md)):
the protocol in Rust without I/O, the native device, the web's wasm, and the
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
`wasm-bindgen` 0.2.127 (`cargo install wasm-bindgen-cli --version 0.2.127`).

The canonical [Messages](../apps/messages/README.md) app is the Exact port of Expo's
chat demo, with model conversations through a local OpenRouter service. It belongs
to the root workspace and does not use Snapback. Run `bun apps/messages/service.ts`
and `bun host/web/dev.mjs --app messages --port 8767`.

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

## Serve RealWorld

RealWorld's public pages can be served by the native renderer:

```sh
EXACT_WEB_DIST=target/realworld-dist bun host/web/build.mjs realworld-web
cargo run --release -p realworld-web --bin realworld-render -- --serve target/realworld-dist --port 8080 --name Conduit
```

The home feed, articles and profiles arrive as anonymous HTML. Their public
responses are cached for 60 seconds by default (`--lifetime` changes this).
Links work without the runtime, which loads when the page is idle; a press
made before it is ready is replayed once it is. Login and editing routes
start the client normally. The renderer compresses what it sends (brotli, else
gzip, by `Accept-Encoding`), and SIGTERM drains it. Reading-page transfer and
the later runtime download are separate costs. With `--generations <dir>` it
keeps the builds of `app.wasm` it has served, and a browser holding an earlier
one gets the new build as a delta against it (LLP 1047.000 §9): after a
one-line app change, 9.5 KB instead of 238.6.

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
explicitly. Formatting never runs automatically on save.

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
write `{{` for a literal brace.
The bake checks at most eight entries against fetch and `secret.keep` grants.
Hosts that support preloads send them at process start; the app's own reads take
a preload's reply, and a read made for a tool is sent.

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
the app's own `close()` ends the app. A login item registered `hidden` (`@exact/login-item`) opens
no window at that launch: the app runs, and shows its window when launched
again or when it calls `focus()`. The notification-area icon, which would
be the usual way back and its Exit, is not built yet (LLP 1133.025 §14), so
an app that keeps running gives the person a way back and a way out of its
own: a global shortcut whose action calls `focus()`, and a button whose
action calls `close()`.

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
it. Each MIME type it accepts must be one the Apple hosts map to a system type
(`DOCUMENT_UTIS` in `scripts/app.mjs`; `application/octet-stream` is
`public.data`), which the app views (`Viewer`, rank `Alternate`), or the app's
own format: a vendor or unregistered type (`application/vnd.studio.board+json`,
`application/x-studio-board+json`) with its extensions (`[".board"]`), which the
bake exports as `<app id>.<subtype>` (`UTExportedTypeDeclarations`, conforming to
JSON for `+json`, XML for `+xml`, else data) with the app as its `Editor` and
`Owner` (studio diary R13). Every build refuses any other type when it reads the
manifest, the web's included. An extension another app already owns on a Mac
(Freeform has `.board`) can still resolve to that app's type there; the open
panels and drops take a declared extension whichever type the Mac gives it.

For URL schemes, `protocol_handlers` in `app.json` uses the Web App Manifest's
`{ "protocol": "web+notes", "url": "/open?u=%s" }` entries. A native Apple
bundle registers each declared scheme. `%s` is the complete incoming URL
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
native handler templates. The default is `false`. This option does not filter
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
| Linux, Windows, Android | not registered, as no protocol handler is there. |

With `"distribution_only": true` on both entries, only an archive or a
`--distribution` build is a browser. A local Mac or opted-in iOS build then registers neither
scheme, writes no `Web page` type, and does not join this Mac's browser list.

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

Widget entries are top-level `app.json` `widgets` rows. For example:

```json
{"kind":"next-trains","families":["systemSmall","systemMedium","accessoryRectangular"],
 "component":"NextTrains","props":"NextTrainsProps","refresh":"nextTrains",
 "preview":{"station":"Mountain View","departures":[]}}
```

Optional `title` and `description` strings supply the widget gallery's labels;
each defaults to the humanised `kind` when omitted.

On iOS and macOS, an optional `configuration` lets the person choose in Edit Widget
(LLP 1126; LLP 1133.023). The
entry becomes WidgetKit's
[`AppIntentConfiguration`](https://developer.apple.com/documentation/widgetkit/appintentconfiguration),
its [`WidgetConfigurationIntent`](https://developer.apple.com/documentation/appintents/widgetconfigurationintent)
generated with one enum parameter per row, and the build extracts the extension's
App Intents metadata, through which the sheet shows them:

```json
"configuration": {"title":"Next trains","description":"Choose how many departures.",
  "parameters":[{"name":"rows","type":"enum","title":"Rows","default":"automatic",
    "values":[{"value":"automatic","title":"Automatic"},{"value":"one","title":"1"},{"value":"three","title":"3"}]}]}
```

On watchOS, the complication picker offers
[`recommendations`](https://developer.apple.com/documentation/widgetkit/appintentrecommendation)
instead of a parameter form. Exact generates one per combination of declared
values, with the default combination first and parameter/value titles as labels.
Repeated value titles include their identifiers. If complete labels still collide,
each label also includes the choices' UTF-8 hex identifiers to distinguish them. A widget
with watch families may declare at most 64 combinations (the product of its
parameter value counts); the bake refuses more and asks for fewer values or
parameters. This Exact bound keeps the watch picker and generated bundle small.

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
reads, or an inject without one, fails the bake with an edit. The refresh does not see
the configuration: one timeline serves every configuration and family, and each render
reads its own. The configuration, labels included, is generated Swift and so a binary
compatibility input; Live Activities have none. Keep kind, parameter and value
identifiers stable across updates; changing a title does not change a choice.
A removed or renamed parameter is ignored, and a missing or removed value uses
that parameter's current default. Other valid choices are retained. Reordering
entries, parameters or values does not reassign a saved choice. Data-backed
options and fetching different data for each placement are not supported.

`component` and the `props` shape are declared or imported by `app.contract`;
every component prop must match a field of that shape. `preview` supplies all
fields, with their declared types. `refresh` names a data-source function taking
an epoch-millisecond number and returning a timeline: records with `date` (epoch
milliseconds) and `props` (the declared shape), in that field order. The plan's
`snapshot_refresh` header selects its typed source row; the render has no resource
for it. A data bake checks an immediate refresh answer against that signature at epoch
zero; pending answers are checked by the source declarations. TypeScript checks
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
writes. The app reloads widget timelines after a successful write readable by a refresh.
Refresh has a 900 ms deadline starting when it begins, including boot. Each JS
turn gets the time remaining to that deadline, rather than the app's 100 ms
per-call budget. This leaves 100 ms below LLP 1126 D8's one-second
line for completion and teardown. Failure or timeout uses the last good timeline
cached in the App Group for this kind and generation, only while the app's store
has not changed since it was cached (a store write, such as an account switch,
expires it), or else an “Open app” placeholder,
and schedules a retry no sooner than 15 minutes later. Rendering and gallery
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
`Contents/PlugIns`. The Mac keeps `systemSmall`, `systemMedium`, `systemLarge`
and `systemExtraLarge`; if no families remain it builds no extension. Live
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
refresh against the watch's own App Group and store. No account transfer is
performed.
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
or `class=(condition ? A : B)`. Only literal font families reached through the
entry are bundled; a dynamic font string selects installed or generic families.
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
catalogs. On iOS, other `.appiconset` sets in the catalogs compile as alternate
icons, also with a raster primary. Pass the set's basename, without the extension,
to `UIApplication.setAlternateIconName`; `nil` restores the primary. Missing paths
and paths that are not `.icon` directories fail by name. The project and its
layers compile in the same `actool` pass as the app's catalogs; changing them
requires a new native build. `icons` still supplies the web and the native
raster fallback when the platform has no `icon` selection.

What shows before the first frame is `app.json`'s Web App Manifest
`background_color`, and in a dark scheme `color_scheme_dark.background_color`
(`#RGB`, `#RRGGBB` or `#RRGGBBAA`); a fixed `userInterfaceStyle` takes its side
alone. The web page has it until the first frame, the iOS launch screen centres
`host.ios.launchImage` on it, and the Android launch window (the starting window
from Android 12) is it, with the bars' icons dark over a light colour. Unset,
each is the platform's surface: iOS's `systemBackground` (its grouped background
under a first route that scrolls a grouped list) and Android's `surface`.
Changing either colour requires a new native build.

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
with the registry crates this machine has compiled
(`~/.cache/exact/apple-crates`; Cargo decides which it can use). Delete either
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
web assets. `from` cannot overlap source, asset or output roots; `to` must name
a private subtree of `Resources/`, `Helpers/` or `Frameworks/`. Mach-O helpers
and libraries are signed before the outer bundle; `exact release` signs them
with its release identity. Resource changes require a new binary, not an asset
update. Find `Resources/server` through `Bundle.main.resourceURL` from a native
module. This field applies only to macOS bundles. A file written before the
build (a build stamp, a channel name) goes in such a tree under `Resources/`:
it is copied before the bundle is sealed, so it is inside the seal. Only code
goes under `Helpers/`, where `codesign` refuses a file that is not.

`host.ios.resources` (iOS, iPadOS and tvOS) takes the same entries for files a data
module reads: `to` is under `Resources/`, and the tree lands in the bundle's
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

### Mac provisioning profiles

Some entitlements hold on a Mac only under a provisioning profile that
authorizes them (Apple TN3125), and the Mac build signs them from one (LLP
1133.010 D7): push's `com.apple.developer.aps-environment` (`device.push`), App
Attest's environment (`device.attestation`), the Declared Age Range
(`device.age-range`), the keychain group a locked secret is kept in
(`secret.keep` with a presence policy), and a passkey's associated domains
(`auth.passkey`). An app whose grants need any of them is built from the
newest installed macOS profile for its id (or the team's wildcard) that allows
them all: a development profile that names this Mac for a development build, a
Developer ID one for `exact release`. `EXACT_MAC_PROFILE=<path>` names one, and
the build says why a named one does not fit. The build signs with that
profile's team's identity (Apple Development, or `EXACT_DEVELOPER_ID` for a
release; one `EXACT_IDENTITY` or `EXACT_DEVELOPER_ID` names must be of that
kind and team, or the build is refused), writes the application identifier,
the team and each entitlement (an environment `development` under a
development profile, `production` otherwise), embeds the profile as
`Contents/embedded.provisionprofile`, and always assembles the `.app`, where
the profile holds.

With no profile that fits, a development build signs none of them and says,
for each grant, what the app then reports (`available: false`,
`NotSupportedError`). A release build is refused (`grant-needs-profile`) for
App Attest, the age range, a passkey or a locked secret that is not
`optional`; one granting push ships without it, reporting push unavailable.
An app with Mac widgets signs both under one profile, of the widgets' team,
chosen among the profiles that grant the widgets' App Group and shared
Keychain group and the grants' entitlements; with none that grants both, the
grants sign nothing. An ad hoc build (`EXACT_IDENTITY=-`) signs no profile's
entitlements.

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

Match both identifiers to Exact's app id. Link the EAS project and install
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
`EXACT_UPDATE_TRUST=production`; `--bundle` sets it itself. Production trust
requires your update stream's signing keys and receipt (or
`EXACT_UPDATE_GENESIS=1` for a new stream). Android uses the Linux bake:
set `deploy.store.linux` to `"0"` for a binary without Exact updates;
`deploy.store.android` does not disable the bake's update checks.

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
unset variable. No `app.json` version key is defined.

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
Apple builds use the system SQLite, avoiding the bundled SQLite filesystem-type
probes, for which no approved disk-space reason applies. Exact declares no
disk-space use; an unaccounted-for disk-space import fails packaging on
platforms subject to required-reason reporting.

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

Start `bun host/web/dev.mjs` and open a printed URL in your browser. Build
and launch the app's native client with that same address:

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
module clients reload logic on web/macOS/iOS; declared Rust modules reload on
web/macOS/iOS/Linux using the executor selected below. Once built, an agent can drive
the same URL with `bun scripts/agent.mjs macos --url http://127.0.0.1:8765/ tree state logs`
(also `web`, `ios`, and `linux`; Linux polls the same URL after first pixel).
The Go/custom-client sequence
is in LLP 1030.000 §7.

A native client finds a generation through its envelope, `exact.json`: a
pointer card of at most 64 KiB that names the plan, and names the asset
cards' file, `assets.json`, by URL, SHA-256, byte length and card count. The
asset index holds one card per asset (name, URL, SHA-256, length) and the
entry plans' `plans` map, at most 4 MiB, about 20,000 cards; its card URLs
resolve against the index's own URL. The web build prints both sizes against
their bounds with the asset count (`exact.json 0.6 KiB of 64.0 KiB, assets.json
101.2 KiB of 4096.0 KiB for 537 assets`); a build, a dev generation or a
publish whose file would pass its bound is refused with the same numbers.
Every client checks the index against the envelope's card before it reads a
card, and a signed head covers the index through its digest.

Agent sessions use `exactTime()` launch facts `seed: 1` (LLP 1069.007), `locale: "en-US"`,
`timeZone: "UTC"` and `epochAtZero` 2026-01-01T00:00:00Z (LLP 1027.000.000 D3, with the
zone's `utcOffset` at that instant, answered again when the virtual date crosses a DST change) on every host. Override them at session setup with
`bun scripts/agent.mjs web --seed 42 --locale fr-CA --time-zone America/Toronto --epoch 2026-09-21T14:13:20Z tree`
(`--format-locale en-KR` gives `formatLocale` a Region of its own; unsaid, it is the locale)
or `open({host, seed: 42, locale: "fr-CA", timeZone: "America/Toronto", epoch: "2026-09-21T14:13:20Z"})`;
a test file writes them as launch lines (`epoch "2026-09-21T14:13:20Z"`, `time-zone "America/Toronto"`,
[authored tests](contract-grammar.md#authored-tests)), which override the flags.
A drive can fail fetches by URL prefix (LLP 1103): `--fail-fetch <prefix>` (repeatable) arms one before the
first data load, `fail fetch <prefix> [times <n>]` and `pass fetch <prefix>` arm and clear one mid-drive (a form of `prefer`:
`{"op":"prefer","faults":{"fail":…,"times":…}}` or `{…{"pass":…}}`), and `state.faults` lists each prefix's `times`, `left`, `hits` and
`armed`. Native carriers pass the launch table as `EXACT_AGENT_FAIL_FETCH`, web pages as `?failFetch=`, one
`<prefix>[\t<times>]` line a fault, read only in agent mode; a production build ignores both. `--fail-fetch` with `--test` arms every test.
A Mac agent drive never takes the focus: the app is an accessory, never activated (a `type` makes its window key
without activating it), so AppKit draws its controls as in an inactive app. On the agent's clock its window, and a picture-in-picture
panel it opens, sit one level below every other app's windows, so they never show over them; `--timing platform` orders them in front. `EXACT_AGENT_ACTIVATE=1`, which only
`scripts/measure-controls.mjs` sets (never a default), activates that one run, and the app that was in front is put
back when it exits, however it exits (LLP 1121 stage 9 F2).
A data module's `crypto.getRandomValues()` and `crypto.randomUUID()` draw from the seed too, so every drive with the same seed
repeats the same identifiers: against a live backend that deduplicates on one, give each drive a seed of its own.
Seeds are whole numbers from 0 through 2^32 − 1, the range every host draws (LLP 1116.005 D5); an epoch is an ISO date, Unix milliseconds, or `now`: the machine's clock,
read once by the driver at launch, for a drive against a live backend on real time (`snapback4 dev`). Native carriers pass
`EXACT_AGENT_SEED`, `EXACT_AGENT_LOCALE`, `EXACT_AGENT_TIME_ZONE` and `EXACT_AGENT_EPOCH`
(milliseconds), and `EXACT_AGENT_FORMAT_LOCALE` when given; direct agent launches can set these too. Web agent pages accept
`?agent=1&seed=42&locale=fr-CA&timeZone=America/Toronto&epoch=1790000000000` (and `&formatLocale=`).
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

### Inspect work and native presentation

`perf <target> during "<operation>"`
reports the work that operation caused in the named subtree. Development measurements
include virtual-row construction, rebinding and retirement; Apple also counts kernel
layout API calls and native host work. Read the reported scope and measurement start:
process-wide native counts can include other sessions, and a reset makes a delta unavailable.
Missing coverage is not a zero.

On Apple, `"perf frames live 500"` observes display-link callbacks for 500 ms without
advancing or changing the current clock. Its reply identifies scheduling, substitutions
and dropped records. A normal Mac event loop is available through
`const session = await open({host: 'macos', env: {EXACT_AGENT: 'live'}, storage: 'my-scratch-store'})` in a
driver script importing `open` from `scripts/agent.mjs`; call
`await session.perf(null, {live: 500})` and close the session afterward.
This carrier still substitutes storage and secret presence. `--timing platform` alone
keeps agent execution substitutions. Neither establishes production-equivalent performance,
and callback timing does not prove compositor presentation. `layout` distinguishes target
and presented geometry; an unavailable presentation remains explicit.

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
| `fetch`, `Headers`, `Response` | Grant-checked; `signal` aborts, and the host cancels the request (below). A request `body` accepts a string, `URLSearchParams`, or bytes from an `ArrayBuffer` or view (such as `storage.fs.readFile`'s bytes). Unless `headers` already names a `Content-Type` (case-insensitively, even an empty value), a string adds `text/plain;charset=UTF-8` and `URLSearchParams` adds `application/x-www-form-urlencoded;charset=UTF-8`, including empty strings and forms. The prelude adds none for bytes or an absent body; on Apple, URLSession adds `application/x-www-form-urlencoded` to an untyped POST with a body, including bytes. As Fetch does, a body on a GET or HEAD, or a view on a `SharedArrayBuffer` or resizable buffer, rejects, and a detached buffer sends no bytes. Natively, in the wasm target's module realms and with `exactStream` on the web JS target, a `ReadableStream` request body rejects with a `TypeError` before sending; collect it with `await new Response(stream).arrayBuffer()` first. `exactBodyFrom: "app:/tmp/…"` sends an app file instead: the host reads it when it sends the request, under `fs.read` as `readFile` is, at most 64 MiB, and the bytes never enter JavaScript, so a 2 MB upload costs the answer's step nothing; with `body`, or on a GET or HEAD, it is a `TypeError`, and a missing file, a denied path or one over 64 MiB rejects the fetch (`FetchError`, kind `Refused`, naming why) before anything is sent, as does a host with no app files (a drive with no scratch store) or none built yet (Windows), with kind `Unsupported`; Exact adds no Content-Type (Apple's POST default above still applies; LLP 1108 D6 R2). An older binary's prelude would send it as an empty body, so a native app with a TypeScript data module carries `typescriptRuntime` in its compatibility id: the first build after this change gives every such app a new id, and each needs a new binary once. `exactBodyTo: "app:/cache/…"` is the reverse, for a download: the host writes a 2xx reply's body into that app file as it arrives, under `fs.write` as `writeFile` is, and the bytes never enter JavaScript, so a large file costs neither the answer's step nor the engine's 64 MiB heap. The body goes to a staging file beside the target, which replaces the target only once the body is complete; a failure, an abort, the deadline or a body over the bound leaves the target as it was. The bound is `exactIndependentHttp.maxResponseBytes`, which may then be up to 1 GiB, or 1 GiB when none is given. The `Response` resolves once the file is in place, with `status`, `ok` and `headers` as received, an empty body, and `exactBodyTo: {path, bytes}`, the bytes written (absent or `null` on any other response). A reply that is not 2xx is not written: the file is untouched and its body is read as usual, at most 1 MiB, so the server's error is readable. A path that is not a plain `app:/` one, a HEAD, or `exactStream` beside it is a `TypeError`; a denied path or a missing folder rejects the fetch (`FetchError`, kind `Refused`) before anything is sent, and a host with no app files does so with kind `Unsupported`. It may come with a `body` or `exactBodyFrom`. On the web the body becomes a Blob the browser holds, stored in the page's store in one transaction (LLP 1108 D6 R2, 2026-10-26). An older prelude would ignore it and read the body into memory, so it moves `typescriptRuntime` to 4. `redirect` is Fetch's: `follow` (the default), `error` (a redirect rejects the fetch), or `manual`, which answers with the 3xx itself; natively its `Location` header is readable (an OAuth step whose redirect names an app scheme), while a browser gives its opaque redirect (status 0, no headers); any other word is a `TypeError`. An older prelude would ignore it and follow, so it moves `typescriptRuntime` to 2: such an app needs a new binary once more, and a cohort on an older binary gets no bundle that relies on it. `cache` is Fetch's `"default"` (unsaid), `"no-store"`, `"reload"`, `"no-cache"`, `"force-cache"` or `"only-if-cached"`, controlling the native cache for a resource's `GET` and the browser's cache on the web; any other value is a `TypeError`, and a module that names it needs `typescriptRuntime` 11. A `Response` has `status`, `ok`, `statusText`, `headers`, `bodyUsed`, `text()`, `json()`, `arrayBuffer()`, `clone()`, as Fetch's does: a body reads once, a clone of a read one is a `TypeError`, a fetched reply's headers and a redirect's take no change, and a response's own refuse `Set-Cookie`; no `body` getter, `Request`, `Blob` or `FormData`. `new Response(body, init)`, `Response.json` and `Response.redirect` make a reply as Fetch does (a demo's server in the app), from a string, `URLSearchParams`, bytes or a `ReadableStream`. A stream brings no Content-Type; `text()`, `json()` and `arrayBuffer()` read its `Uint8Array` chunks to the end, rejecting any other chunk with a `TypeError` and propagating stream errors. A locked or already-read stream is refused; `bodyUsed` tracks reads and cancellation, and `clone()` tees an unread, unlocked stream so each response reads independently. A module that names `.clone`, `statusText` or `bodyUsed` needs `typescriptRuntime` 7, as an older prelude makes one with no status and no body, reads a body again, or throws. `Response`, `ReadableStream`, `TransformStream`, `CompressionStream` and `DecompressionStream` require runtime 8 so stream bodies cannot reach an older prelude that stringifies them. Natively and in the wasm target's module realms, `Response.redirect` takes only an absolute URL (a relative one is a `TypeError`; the web JS executor resolves it as the browser does), and natively a fetched reply's `statusText` is `""`; in the wasm target's browser realms, which are not hardened, a `Response`'s headers take no guard, and an app that replaces an intrinsic there (a prototype's method, `Math`, a `WeakMap`'s `get`) changes what `Response` does, as it changes what any code in that realm does, while what an app gives a `Response` (an object's own properties, its iterator, its conversion hooks) is read as Fetch reads it on every host |
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
| `Intl.NumberFormat`, `Intl.DateTimeFormat`, `Intl.Collator`, `localeCompare`, `toLocaleString` | Android's published v5 bundle has no Intl. With an Intl-enabled bundle carrying receipt-bound English ICU data, Hermes's ICU backend supplies `Collator` and Ibex supplies `NumberFormat` and `DateTimeFormat`. Date formatting needs an explicit timestamp. No `Intl.PluralRules`, `RelativeTimeFormat`, `ListFormat`, `Segmenter` or `DisplayNames` (Apple's engine; Linux's is built `--intl`). `Intl.Locale` is the prelude's on every Hermes host (the engine has none): a tag parsed and canonicalized as Chrome does, its options and getters, and `getWeekInfo()` with Chrome's `{firstDay, weekend}` from CLDR's week data, by the tag's region, its `-u-rg-`, or its language's likely region (two-letter languages and a few others; another reads Monday and a Saturday-Sunday weekend), and `-u-fw-`. It has no `maximize`, `minimize` or other `get…()` list, does not canonicalize aliases (`iw` stays `iw`, `en-840` keeps `840`, and its week is then the default, Monday, where Chrome's is `en-US`'s), a formatter given a `Locale` object rather than its string uses the default locale, and `structuredClone` copies a `Locale` as `{}` where Chrome refuses it. Apple's `ja-JP` long date puts a space before the weekday (`10月6日 火曜日`, Chrome `10月6日火曜日`). Apple's engine has no `notation: "compact"`: the prelude formats its short display as Chrome does for a decimal in en, en-GB, en-IN, de, fr, fr-CA, es, es-MX, it, pt, pt-PT, nl, sv, da, nb, fi, pl, ru, uk, cs, tr, ja, zh, zh-TW, ko, hi, he, id, th and vi (and their regions); `compactDisplay: "long"`, another locale or a compact currency is printed in full and said once in the logs. Apple's engine rounds a tie to even and ignores `roundingMode` (`$0.82` for 0.825 dollars, Chrome `$0.83`); the prelude rounds the number first, as Chrome does: from its shortest digits, by the formatter's `roundingMode` (half away from zero unless it names another), which `resolvedOptions()` then reports; a compact it prints in full rounds so too, to its style's digits (`$0.83` for a compact 0.825 dollars). A `unit` keeps the engine's rounding, since that engine converts it to the locale's unit (`0.622mi` for 1 kilometer). Apple's engine has no `formatToParts` |
| `console` | To the runner's logs after each answer and reply, including refused calls; available through the agent's `logs` on native hosts. Outside a drive the web writes it to the browser's console too, and an Android development build to logcat under the tag `exact-app` (`adb logcat -s exact-app`). An Android release build and the other native hosts keep it for `logs` alone |

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

Native hosts keep a resource's `GET` reply when it says how long it is fresh
or carries a validator (LLP 1162 D4): in memory, 16 MiB per runtime, gone with
the process. An entry answers only a request equal in method, URL and every
header; a stale reply is revalidated when it has a validator. `refresh` and a
declared `refreshes` go to the network and replace the entry, and a successful
write to a URL drops that URL's entries. A browser uses its own cache; the
render server keeps none.

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

On Hermes each turn of an answer has 100 ms of its own time: its call, or its
resumption after a `fetch`, storage step or yield it awaited, until it awaits
the next. Its own time is the calling thread's CPU clock where the platform keeps
one, so a turn that waits on a blocked host call spends little of it; a turn may
still take at most ten budgets (1 s) of wall clock. A turn over either is refused
as an overrun (`Overrun`: `` `work` took 904.5 ms of its own (CPU) time (2283.7 ms
of wall), over the 100 ms budget for one turn; … ``), and the message names the
repair: `await scheduler.yield()` every few thousand items of a long loop
([the guide](contract-for-agents.md#data-requests-and-side-effects) shows one).
A resource so refused keeps its last value and reads as pending, and is asked
again at 250 ms, then at 1 s; a third overrun in a row fails it as any refusal
does. A mutation's or a stream message's overrun is not asked again.
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

On Apple, SQLite is the OS library, so its SQL version follows the OS. Other
native hosts bundle the pinned SQLite. A native production build links SQLite
only when the app grants `sqlite.open`
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
`'ENOTEMPTY'`, `'EBUSY'`), `'full'` (256 operations already wait behind the
one in flight: the module's storage queue is full, LLP 1097), else `'failed'`. Branch on the code, never the
message: `catch (e) { if (e.code === 'ENOENT') return empty; throw e; }`.

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
web profile. Apple native modules receive the same named data, cache and temporary
roots across launches. A fresh drive clears them; the picker's temporary documents
alone are cleared on each launch. A web drive with no `--storage` refuses app storage (`storage is unavailable
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
(`exactStream`) takes none, except an upload (below).

A WebSocket stream can stay open and carry nothing. Give it
`exactIdleTimeout`, in milliseconds (1 to 3600000), and the host ends it
when nothing has arrived for that long: `fetch('wss://…', { exactStream,
exactIdleTimeout: 60000 })` (LLP 1016.000 D11). The count starts when the
socket is asked, and starts again at its opening and at each message;
protocol pings and pongs do not count, and nothing the app sends does. The
mapper is then called once more, with `{ type: 'error', kind: 'Timeout',
message: 'nothing arrived for 60000 ms' }`, after what it had already
received (each message, in order), and the stream is over; the app opens it again by refreshing the
resource. The time is real time on every host, under the driver too, so
`clock +60000` ends no socket: test it with a short timeout against a peer
that stays silent, and wait with `clock until state …`. Only a WebSocket
stream takes it: on an event stream, a byte stream, a long poll or an upload
it is a `TypeError`, and the web's wasm host and a Rust module across the
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
before it is sent, nor is a download to a file (`exactBodyTo`). A Rust source's request takes `Request::held()` (not across the Rust
module seam, which refuses it, as it refuses a deadline). On the web build an
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
it. The web JS target, macOS, iOS and Linux read it; the web's wasm host, Rust
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
length, and the last has `loaded === total`. A newer one replaces one the
runtime has not taken, counted in its `coalesced`: progress is a snapshot. The reply is read whole within the ceiling
(`exactIndependentHttp.maxResponseBytes`, 1 MiB unless given) and ends the
stream as `{type: 'load', status, data}`, whatever its status, as
XMLHttpRequest's `load`. A failure ends it as any stream's `error` does, of kind
`Timeout` once its `exactTimeout` passes (an upload may take one; a reply that
lands as it passes stands). The platform's own limits follow that deadline a
second later; without one they are the upload's, as they are a fetch's
(URLSession's 60 s without data; the portable transport's 30 s for the whole
exchange, 15 s for the reply's head and 15 s for its body), so give a
deadline to an upload that may take longer. An empty body tells no progress.
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
module seam. The Rust module ABI is 7: long polls, two-way sockets
(LLP 1016.003), cache modes (LLP 1162 D4) and admitted secret names
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
- A remote notification's click opens its payload's `navigate` and nothing
  else ([Push](#push)).
- `setAppBadge(contents=)` sets the app icon's badge, best effort: a number, no
  number for a flag (`•` on the Mac's Dock tile, `1` on iOS), 0 to clear;
  `clearAppBadge()` clears it. The badge is the app's: an embedded host's
  sessions share one. It needs no grant. A negative count is refused
  `TypeError` on every host, under the agent too.
- `notificationStatus()` reads `{available, state, badge, apple}` (`state`
  `granted`, `prompt`, `denied` or `unknown`), read again when the app comes
  back to the front; `requestNotificationPermission()` asks while `prompt` and
  answers the status.

| host | post | `showTrigger` | click | badge |
| --- | --- | --- | --- | --- |
| web | `new Notification` in the page | while the page is open: the web has no trigger that outlives the page | while the page is open | `navigator.setAppBadge` where the browser has it, else `NotSupportedError` |
| macOS, iOS | `UNUserNotificationCenter`, shown with the app in front too | the system's, delivered with the app closed | warm, or the launch | the Dock tile's label; `setBadgeCount` |
| Linux, terminal, tvOS, Android, Windows | no implementation yet: each call is refused `NotSupportedError`, under the agent too, and the status reads `available: false` | | | |

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

`@exact/push` (LLP 1133.004 §9) is a visible push from the app's own server,
on iOS, iPadOS and macOS, by the Push API's names. Exact sends nothing and relays
nothing: the app sends its device token to its own server, and the server
sends to APNs.

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
- `pushSubscription()` reads `{apple: {deviceToken}}`, the token's bytes in
  lowercase hex, or null. It never asks: its first read in a launch registers
  once when permission is already granted, as Apple asks of every launch, and
  a new token is a new value, which the app sends to its server again. Read it
  in every launch so the server keeps the current token. While permission is
  `prompt` or `denied` it reads null and registers nothing; a change of
  permission in Settings is read again when the app comes back to the front.
- `pushPermissionState()` reads `{available, state}`, notifications'
  permission, which the two modules share.
- On a Mac build signed without the entitlement, and on tvOS, Android, the
  web, Linux, Windows and the terminal, each call is refused
  `NotSupportedError`, `pushSubscription()` reads null and the state
  `available: false`.

The server sends APNs an alert (`apns-push-type: alert`, `apns-topic` the
bundle id), APNs's own `aps` dictionary and one key of Exact's:

```json
{ "aps": { "alert": { "title": "Delayed", "body": "The 9:41 now leaves at 9:52" }, "sound": "default" },
  "navigate": "/trips/941" }
```

The system shows it, with the app in front too. Its tap opens the top-level
`navigate` string, warm or at launch, as a local notification's `navigate=`
opens: reduced by `location_of` to the app's own location whatever its scheme
or origin, never through the app's `protocol_handlers`. Nothing else of the
payload is read and no event is raised; without `navigate` the app only comes
forward. A server writes that string, so a route must not act on being opened.
No app code runs when a push arrives: `content-available` does nothing (the
`remote-notification` background mode is refused), and `mutable-content`
changes nothing (no service extension is built).

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

Under the agent nothing reaches APNs, and `pushSubscription()` reads null
while the permission is not granted, as on the phone. A subscribe while
`prompt` waits on the
`permission` ticket (`tap @t allow`), then on the subscription's value (`type
@t '{"apple":{"deviceToken":"ab12"}}'`); `prefer extension
push.pushSubscription=<json>` sets the value later, as a rotation.

Run on an iOS 27 simulator: the real prompt; a registration the simulator
answered with a token; a relaunch that only reads `pushSubscription()`
answering the same token; the entitlement in the built app; pushes sent with
`xcrun simctl push` shown and their taps opening `navigate`, with the app in
front, from the Home Screen and at launch. Not yet run: delivery through
APNs, the entitlement signed against a profile with Push Notifications, and a
production build. `packages/push/test/checklist.md` is those checks; until
they pass, nothing here is claimed for a device.

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

Under the driver no scheduled sound plays on any host (`output: "agent"`). The
runner initializes `state.sounds.output` to `"agent"`; Linux and Windows
leave it there even outside the driver. Android's state reader is available
only under the driver and retains `"agent"`; it does not expose AAudio's output
state or latency. Outside the driver, macOS, iOS and tvOS
replace it with an object whose `kind` is `"avaudioengine"` and whose `state` is `"closed"`,
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

The Android arm decodes declared WAVs at first play. It uses
[AAudio's speaker timestamp](https://developer.android.com/ndk/reference/group/audio#aaudiostream_gettimestamp)
to place frames; startup waits up to 100 ms for that timestamp, then uses the
stream's buffer size as a latency estimate if none is available. Late effects
start from their attack. The idle stream closes after its last frames reach the
speaker and releases its focus claim. A failed open or disconnected stream is
reported; a play at least one second after the failure tries again. Background
playback, MIDI output and timed sequence haptics on Android remain unavailable.

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

### Pick a file of any type

An `input type="file"` with no `accept` (or `accept="*/*"`) offers any file;
`multiple` allows more than one. Tap a visible input or call `showPicker(id)`
from an action to open it. `change` receives `Picked[]` with each file's name, type, size and
`app:/tmp/picked/` path. The copy at `path` is named `<id>.<ext>`, `<id>` from the
launch and a counter, never after the file: show or upload it under `name`. A
specific `accept` list is limited to media types and types in the app's
`file_handlers`. The live Linux picker is unavailable;
its agent substitute accepts any file. Image pixel and Canvas readback remain
deferred.

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

`storage.fs.compressImage(from, to, {maxDimension, maxBytes, type})` decodes the image
at `from`, scales it so its longer side is at most `maxDimension` pixels, and
writes a JPEG of at most `maxBytes` bytes at `to`, searching JPEG quality the way
Bluesky's composer does (from quality 51 down and up; when 51 and 26 both miss,
the size shrinks by 0.8, at most four sizes). Orientation is applied, and none
of the source's metadata is written (no EXIF, GPS or orientation); transparency
becomes white and the colour is sRGB. With `type: 'image/png'` it writes a PNG
instead, which keeps transparency: PNG has no quality, so it is encoded at the
fitted size and, while over `maxBytes`, the size shrinks by 0.8, at most four
times. It resolves `{path, type, size, width, height}`, `type` the one written
(LLP 1069.002 Amendment A1, A1.11):

```ts
export const grants = 'fs.read app:/tmp\nfs.write app:/tmp';
// files from `change` on an `input type="file"`; posts: 4000 px and 2 MB
const out = await storage.fs.compressImage(files[0].path, `app:/tmp/upload/${seed}.jpg`,
  { maxDimension: 4000, maxBytes: 2_000_000 });
const body = await storage.fs.readFile(out.path);
```

Both paths are `app:/` files (`fs.read` on `from`, `fs.write` on `to`); `to` is
replaced atomically, and left as it was on any failure. It always re-encodes, so
compare the pick's `size`, `width` and `height` first to skip it. It is one
storage operation in the module's queue; natively it runs off the JS thread. A bad option
(a `type` other than `'image/jpeg'`, the default, or `'image/png'` too)
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
and iOS they are security-scoped URLs held for the session; Linux opens no
picker panel (a drive's held pickers still answer), so outside a drive only
`open-file` delivers one there. The handles end with the
session, unless the app keeps what the person chose (below).

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
it wrote, from Rust or TypeScript. A path whose entry is gone
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
| Linux, Windows, Android, terminal | none (declared; Linux and Windows drivers refuse by name) | empty: these hosts paste text only |

The driver: `tap <target> drop <path…>` (files and folders), `tap <target>
dragenter <path…>` and `tap <target> dragleave`, `type <target> paste --file
<path…>` and `type <target> paste --image <png>`.

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
not); `appId` and `grants` are constants the build reads from the source without
running it (a
string, a template or `+` of constants, an array of them `.join('\n')`, or a
top-level `const` holding one). Numbers keep their type: `1 + 2` is `3`, while
`'a' + 1 + 2` is `'a12'`. Conditional expressions are refused.

A backend override (`--backend`, `EXACT_BACKEND_ORIGIN` or `.exact/backend-origin`)
needs one top-level `const origin`, set to a literal http(s) string or a setting
such as `import.meta.env.API_BASE_URL ?? 'https://api.example'`, and used in
`net.fetch ${origin}`. Settings are inlined before the override. A refused
selection names its selector and reports `backend-selection`, including in
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
same grants. Authored source stays unchanged. Keyed grants move to another
origin in a development build (`EXACT_UPDATE_TRUST` unset or `development`): an
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
compatibility id, never an update. `build_mixed_with` bakes a mixed app through
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
and Android (Android requires a rebuilt bundle, as above):
`en`/`en-US` and complete currency data are present, and unsupported locales
fall back to `en-US`; it does not select `intl-all-locales`. The verified
bundle supplies both VM archives, all three ICU data tiers and the matching
compiler, while the feature links the English tier. Apple does not install
Ibex's `INTL` group because Hermes retains OS-backed Intl there. iOS, tvOS and
Windows use their pinned v4 bundles and must not fall back to the former
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

**Limits.** `app.ts` imports pure-JavaScript packages only (Node's modules, the DOM, a computed `import()` and native addons are refused; a use that a `typeof` test guards is not, nor a read through the global object, such as `globalThis.process?.env`, whose value is only tested, compared, spread into an object, returned, or held in a `const` whose uses are guarded in turn). Code from a string throws on every host through `Function` and `eval`, and through their own `constructor`: each throws `SyntaxError: Parsing source code unsupported`, as Hermes does. The build warns (`code-from-string`) at the uses of the global `Function` or `eval` it can see by name, a package's or `app.ts`'s: a direct call or `new`, a member of `globalThis`, destructuring a global object's property, a parameter initializer, or a JSX component name (`<Function />`). A use reached another way builds without a warning and throws the same error when it runs. Either way, code that uses them works only if it catches that error and takes another path, as zod does. What reaches `Function.prototype.constructor` or a function's own `constructor` (`(function () {}).constructor`) still runs code from a string on the web, and not on Hermes: the warning is a tripwire, not a sandbox, and refuses nothing. Under the render server's CSP and in the JS server render that throws too. A package may not read `import.meta.env`, which is app.ts's to read and pass in; a read the bundle drops, in a function nothing calls, is not one. That check keeps a module portable between Hermes and the page; it is no security boundary: on the web the module runs in the page's realm, and a package is code the author chose, trusted as their own. A package's `/// <reference lib>` to an ES library newer than the data module's (`esnext.disposable`) loads nothing, so what that library declares (`Symbol.dispose`, which Hermes lacks) stays undeclared for the app.

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

| Scope | Swift (`ExactModule`) | Page module (`modules/web/index.js`) |
|---|---|---|
| A node marked `hatch="word"` | `element(_:)`, `elementEnded(_:)`; a frame's `elementConfiguration(_:_:)` | `element(e)`, `elementEnded(e)` |
| A route, a navigation stack, tabs (iOS; the web's elements) | `route`, `routeEnded`, `navigation`, `tabs`, `tabContainer` | `route`, `routeEnded`, `navigation`, `tabs` |
| The window toolbar (macOS) | `toolbar(_:)` | none |
| The app: its facts, by the web's names, and the root node's `data-*` words as `data` (each in `app.json` `data`, which `hatch --app` adds; Swift reads `data-booted` as `app.data[.booted]`) | `app(_:)`, `appEnded(_:)` | `app(a)`, `appEnded(a)` |
| The window the session presents into | `window(_:)`, `windowEnded(_:)` | `window(w)`, `windowEnded(w)` |

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
colours by name: `ExactTamber(view).color(.surfaceCard)` on Apple (the view a
module made, or one inside it; in the process, a view under any node, or
`ExactTamber(session:)` for a screen of the host's own), a `UIColor` or
`NSColor` that resolves the scheme, contrast and elevation it is drawn in;
`ExactTamber(view).color("surface.card")` on Android, a `Color` int resolved
for the view's scheme. Each reads its place's app tuple: a view under an
`-exact-mode` pin answers that mode. `observe { … }` runs after each batch that
moved what the place reads (a `setMode`, a pin above it, a live edit, the
scheme, the root font size), so re-apply a colour kept in a layer there. A
name the design lacks answers the fallback (`color(_:fallback:)`) and is
logged once. `bun exact.mjs tamber gen swift .` writes the names
(`.exact/tamber/tamber.swift`, never committed), which a module's build
compiles in. `apps/tamber-host` is the sample.

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

An app `exact new` made outside this checkout that answers in Rust writes its own Cargo workspace
(LLP 1153 D1.4). Read its crates from [Sparkline](../apps/sparkline)'s `data/`, `web/` and `apple/`:
each `Cargo.toml`, `build.rs` and `src/lib.rs`, not the TypeScript ones `.exact/shells/` holds. Sparkline's
paths into Exact are relative to this checkout: in the copies, write each as `<exact2>/…` before running
`bun exact.mjs update`, `contract` as `<exact2>/contract/cli` too. `update` points `exact-*` crates at this
checkout by name, and any other path only through that placeholder. The web build copies the source
`web/build.rs` passes to `contract::bake(plan, …)` into a module of its own, where build.rs's `use`s and
locals are not: write it as a path from the data crate (`my_app_data::MyApp`), or that path's `::default()`
or `::new()` with no arguments, with no comma after it (rustfmt adds one when it breaks the call across
lines). For a mixed app, the build copies the third argument to
`exact_js_bake::build_mixed_with`: a reference to the same data-crate value or constructor,
with no constructor arguments. TypeScript answers the other sources. A declared
`rust.module.package` supplies the module instead of a generated one.
A source built with arguments is exported by a crate of the app's own
instead, named in app.json's `rust.module.package` (Update Lab's `logic/`).

A source that wraps another implements `exact_runner::DataSourceWrapper`.
Every method is required, so adding a data-source method cannot silently use
a leaf's default in a wrapper. Forward each call or handle it explicitly.

A record is positional: `Value::record(vec![…])` takes its fields in the order
the shape declares them. `bun exact.mjs contract rust app.contract -o
/tmp/shapes.rs` writes each shape as a struct with its fields in that order.
It includes native-module call records only when an authored shape or a data
source's parameter or result type reaches them; `contract::module_rust`
generates the module bindings. For a build script,
`contract::compile_path_for_rust` returns the plan and call-record names to
pass to `contract::rust`; the names are compiler metadata, outside the binary
plan. Both map scoped names such as `Point__exact` to `PointExact`.
`Value::Number`, `Value::str`, `Value::Bool` and `Value::list` are the rest.

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
| `"fs.compressImage"` | `{"path", "destination", "maxDimension", "maxBytes"}`, and `"type"` (`"image/png"`; absent or `null` is JPEG), `"rotate"`, `"flip"`, `"crop"` | `{"path", "type", "size", "width", "height"}`, as `storage.fs.compressImage` answers |
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
`$XDG_DATA_HOME/exact/<app id>/secrets`; not encrypted), and the web in
`localStorage`; Windows keeps them in memory for now. The host reads them into a snapshot before boot, so a
read is synchronous, and a scripted drive never keeps them. A best time is not
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

Production Apple builds optimize Rust and ExactKit for size on iOS and tvOS,
and for speed on macOS. Embeds default to size on every Apple platform;
a game keeps speed.
`host.ios.optimize` (also used by tvOS) or `host.macos.optimize` in app.json
selects `"size"` or `"speed"`; `--optimize speed|size` overrides it. Size uses
Cargo's `size` profile and Swift `-Osize -internalize-at-link`; speed uses
`release` and `-O`. Games refuse size because their generated Cargo workspace
has no `size` profile.
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
methods appear only when configured. The header shows build/source/timestamp
and serving context. Optional `brand.logo` and `brand.wordmark` reuse app images
or a text wordmark with an asset font; the footer uses the gray Exact mark.
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
direct `for` (LLP 1010). Only a non-empty collection has edges: its geometric
window reaches the first or last supplied row. An empty collection fires neither
event. A plain list lays out all its children and has no collection window;
the Contract compiler refuses either handler there, on every target.

A row that is not an item, such as a line saying that older messages are loading
or that the thread starts here, is one more item of the `for`'s list, put first
by a `derive`. The `for`'s one root draws it by its key, and it is the edge row
that `reachstart` watches:

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

| `bun scripts/agent.mjs ios widget next-trains --configuration '{"rows":"one"}' "screenshot target/one.png" state` | A configured widget's render with that choice; `state` reports the `configuration` |

A configured widget's cells render its defaults, then each other value of each
parameter (the rest at their defaults) in full colour, light and dark;
`--configuration <json>` names parameters and values over the defaults, and a name or
value the configuration does not declare is refused with the ones it does.

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
