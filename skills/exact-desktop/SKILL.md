---
name: exact-desktop
description: Build a macOS desktop app with Exact (expo/exact) whose native shell hosts something Contract does not draw itself — an embedded web editor or canvas in an `iframe`, a local companion service, arbitrary filesystem paths, native file pickers, a full-window title bar, menu-bar shortcuts, or a self-contained `.app` bundle. Use when an Exact app needs an iframe, postMessage, a loopback server, `native.later`, `viewport-fit="cover"`, `aria-keyshortcuts`, `-exact-menu`, `host.macos.resources`, or packaging and installing a Mac build. Patterns come from Exact Draw, an Excalidraw desktop app.
---

# exact-desktop

Exact owns the native shell: window, sidebar, lists, menus, shortcuts. A web guest in an
`iframe` owns a surface Contract does not draw (an editor, a canvas). A companion process
owns what the data module's grants cannot reach. Use `exact-app` for the shell itself and
`exact-performance` for its lists and polling. `<exact>` is the Exact checkout.

Record what you verify, per host. A passing web run never establishes native behaviour.

## Choose the boundary first

| Need | Exact gives | Otherwise |
| --- | --- | --- |
| Files the person picks, this session | `showOpenFilePicker`, `showDirectoryPicker`: session-local `doc:/` handles (`bun exact.mjs docs documents the person chose`) | — |
| App-owned files, SQLite | `app:/data`, `app:/cache` through `grants` | — |
| Real paths kept across launches, directory walks, file watching | — | A companion service, with a Swift module returning the chosen path |
| A multi-file web app (scripts, chunks, fonts) | An `iframe` of a page under `assets/` or `deck/` | A loopback server, when the page also needs a live API |
| A platform API with no view | A native module `app.ts` asks (`vocab native-module`) | — |

A `doc:/` handle has no real path and cannot be handed to another process.

## Embedding a web guest

```contract
column id="canvas-host" flex=1 min-width=0 min-height=0 position="relative"
  for editorUrl in [workspace.editorUrl] key=editorUrl
    iframe src=editorUrl message=updateAppearance sandbox="allow-scripts allow-same-origin" flex=1 min-height=0 testId="editor"
```

- **Key the frame by its session URL.** A new authenticated URL then makes a new frame
  instead of re-pointing a live one; ordinary state updates keep the URL and the guest.
  Exact Draw's cold launch showed WebKit error -999 until it did this.
- **Keep the guest mounted.** Remove the sidebar from layout around it rather than
  rebuilding the frame's container. A frame kept with the same `src` retains its guest.
- **Guest to shell is `postMessage`.** The iframe's `message` action receives a string;
  validate it in the action before it reaches state. Shell to guest is the `src` query.
- **Origins differ by host.** On the web the parent is the app's page on a loopback port
  that differs between `web`, `agent web` and `test web`. On macOS with a `sandbox`, and
  always on iOS, the parent is a wrapper at `https://exact.invalid`, so the first request
  is a cross-site navigation. A server that checks `Origin` or `Sec-Fetch-Site` against a
  fixed list refuses its own page. Admit the page by a token in the `src` the data module
  builds, and keep origin checks for the API calls the page makes.
- **A bundled page on Apple** is `exact-app://app/<path>`: no service worker, no cookies.
  Keep its data in `localStorage`, IndexedDB or OPFS.
- **Test the guest, not the tag.** `expect tree has "editor"` passes while the guest shows
  an error page. Inspect the guest's tree (`agent macos tree`, `tree --ax`) and a
  screenshot, with the sidebar open and hidden, in light and dark.

## A companion service

- Bind it to loopback, name its origin in `export const grants = 'net.fetch http://127.0.0.1:<port>'`,
  and authenticate every API call with a per-session token.
- In `app.ts`, let `code: 'bake'` errors rethrow, answer a failed read with the last good
  answer plus an `error` field, and drop the session on 401 so the next call reconnects.
- Poll with a revision: `/workspace?since=<rev>` answers `unchanged`, and the source
  returns the previous object. Gate or slow the poll task; a mutation's `refreshes` covers
  the app's own writes.
- Do long work (directory traversal) in the service's worker, never in an answer's turn.
- Encode paths before using them in a `testId`: a raw `/` reads as a view or part target.
  Keep the real path as the `aria-label`.

## Native pieces (macOS)

- **Module:** `modules/apple/*.swift` (or `macos/modules/*.swift` for the Mac alone), one `ExactModule` subclass
  named by `let exactModule`. `call` runs on main within the answer's 100 ms; anything
  that shows a panel or waits is `later`, replied once from any thread. An `NSOpenPanel`
  delegate inherits `NSObject`, so make it a helper class, not the module.
- **Availability:** `native?.available` is false in the build's bake and on hosts with no
  module. Answer a shaped fallback, and hide controls that need it
  (`if nativePicker.available`).
- **Full-window content:** `viewport-fit="cover"` on the root gives full-size content and
  a transparent title bar when the window has no toolbar. Lay a title row beside the
  traffic lights with `env(titlebar-area-x, 0px)`, `env(titlebar-area-width, 100%)` and
  `env(titlebar-area-height, 0px)` (`vocab titlebar`); the area is zero in full screen and
  undefined off the Mac, so keep the fallbacks. An app with routes has a toolbar instead.
- **Shortcuts and menus:** one declaration on the action's button connects the click, the
  key and the menu item, and works while the web guest has focus:

  ```contract
  button press=toggleSidebar aria-label=(sidebarOpen ? "Hide sidebar" : "Show sidebar") aria-keyshortcuts="Meta+b" -exact-menu="view" testId="toggle-sidebar"
    image "symbol:menu"
  ```

  The button must be on screen. Write no key listener and no shortcut registry.
- **Context menus:** `contextmenu=aim(id)` with `contextPopover="menu"` on the row and one
  `column id="menu" popover="auto" role="menu"` serve every row; a Mac menu natively, a
  popover on the web.
- **Icons:** `image "symbol:<role>"` covers the roles on the card. For one it lacks, a
  small Contract `svg` component draws on every host; `symbol:sf/<name>` is Apple-only.
- **Shell colours that follow the guest:** set `color-scheme` on the root from state so
  native text, fields and selection follow the guest's theme rather than the system's.

## Running and packaging

| Goal | Command |
| --- | --- |
| Build and launch as a real `.app` | `bun exact.mjs mac --bundle`, then run the bundle's `Contents/MacOS` executable |
| Drive the native app | `bun exact.mjs agent macos tree "tap <testId>" "screenshot .exact/x.png window"` |
| Install into `~/Applications` | `bun <exact>/scripts/exact.mjs install <app>` |
| Sign, notarise, package | `bun <exact>/scripts/exact.mjs release <app>` |
| Check prerequisites | `bun <exact>/scripts/exact.mjs setup --check` |

- A native drive refuses a stale build and names the rebuild. Run `mac --bundle` after
  each edit before `agent macos` or a shell test.
- Ship helper files (a runtime, the bundled service, editor assets) with
  `app.json`'s `host.macos.resources: [{ "from": "native-server", "to": "Resources/server" }]`.
  The native module starts the helper on demand, waits for a ready message, records
  startup errors and ends it on teardown; the helper also exits when its parent does.
  That lifecycle is app code.
- An assembled `.app` that needs a terminal launcher is not distributable. Verify a
  Finder launch from the installed location, and that quitting leaves no helper running.
- Put the pinned Bun first on `PATH`; package scripts otherwise find an older system Bun.
  Browser drives use `CHROME` when Playwright's cached browser is missing.
- After moving to a new Exact checkout: `exact new <app> --update`, then
  `bun exact.mjs lock`.

## Keep the experience journal

Exact Draw keeps `docs/exact-experience.md`: for each finding, what was tried, the
observed result, the workaround, and the framework improvement it suggests, each marked
as reproduced, source-confirmed, a hypothesis or an agent's mistake, with the platform
and Exact revision. Keep one in any app that pushes on Exact's edges, and mark superseded
workarounds so they are not read as current advice. It is separate from
`bun exact.mjs feedback note`, which is the submission path.
