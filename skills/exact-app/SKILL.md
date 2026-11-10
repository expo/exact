---
name: exact-app
description: Build or change an app or website written with Exact (expo/exact) — a Contract view (`app.contract`), a TypeScript data module (`app.ts`), authored tests (`app.test.contract`) and a manifest (`app.json`) that ship natively to the web, iOS, macOS, Linux, Windows and Android. Use whenever a directory has `app.contract` or `exact.mjs`, when the user says "Exact app", "Contract", "exact new", or asks to scaffold, extend, test, verify or debug one. Use it before writing any Contract: the language looks like JSX and CSS but refuses React habits.
---

# exact-app

Exact has no releases or npm package. An app uses a checkout of the repository by path:
the app's generated `AGENTS.md` block names it, and `EXACT2` overrides it. Below,
`<exact>` is that checkout and every `bun exact.mjs …` runs from the app's directory.

## Read first, then look up

1. Read `<exact>/docs/start-here.md` whole (a website: `docs/start-here-site.md`). It is
   the only required reading and holds a complete screen, its data module and its test.
2. Look up everything else by name. Do not read the long guides front to back.

| Need | Command |
| --- | --- |
| Every function, tag, part, test step, idiom | `bun exact.mjs contract vocab` (the card, `docs/card.md`) |
| One name, or several at once | `bun exact.mjs contract vocab list-rows search swipe` |
| One section of a guide, with its path and line | `bun exact.mjs docs <heading words>` |
| A tested starting app | `bun exact.mjs pattern` lists; `pattern <name>` writes |
| A flag | `bun exact.mjs <verb> --help` |
| A whole app to grep | `<exact>/apps/shelf` |

A recipe (`docs/recipes/<name>.md`) answers one common question; `vocab <name>` prints it.

## The model

**Write the web, ship the platform.** Structure, layout and names are HTML's and CSS's.
What you leave unsaid is the platform's: controls, colour, type, bars, dark mode, motion,
gestures. What you write wins, so writing less gives a more native app.

- Say what a thing is: a `header` with a heading and buttons, a `list` of rows, a
  `tablist`, a `dialog role="alertdialog"`. Each host draws its own control.
- Colour and type name a role (`-exact-secondary-label`, `font="-exact-footnote"`), never
  a hex. A brand is an `app.tamber`. `bun <exact>/scripts/no-tells.mjs .` lists literals.
- Paint on a `button`, field or bar (`background-color`, `border`, `font-size`) makes it
  your box, not the platform's control. Emphasis is `buttonStyle="filled"` or a part from
  `exact:content` (`SummaryCard`, `EmptyState`, `RoundButton`, …).
- Every screen, tab, detail and sheet is a route (`routes`, `push`, `link href=path(…)`).
  A screen chosen by `if page == …` loses Back, reload and deep links, and the build
  warns (`lower-screen-by-state`).

## Where work belongs

| Work | Home |
| --- | --- |
| View structure, UI state, event binding, derived labels | `app.contract` |
| Network, storage, joins, grouping, a stored or locale order, algorithms | `app.ts` |
| A platform API with no view | a native module `app.ts` asks (`vocab native-module`) |
| What only the platform's own view can do | an access hatch (`bun exact.mjs hatch <word>`) |
| Identity, grants, icons, host settings, your own verbs | `app.json` |

Only the root component declares `resource`, `mutation` and `task`; children take `props`.

## Contract is not JSX

Expressions are TypeScript's where they compile; the differences are refused with the
repair named. The ones that cost the most:

| Habit | Contract |
| --- | --- |
| `items[0]`, `null`, truthiness | `first(items)` / `items.at(0)` is an option; `match` it. `if name != ""` |
| `[...xs, x]`, `{...old, title}` | `xs.concat([x])`, `Shape(old, title=value)` |
| `items.map(x => <Row/>)` | `for x in xs key=x.id` with the row in its body |
| `press={() => save(id)}` | `press=save(id)`; the event appends its payload after captured arguments |
| `fontSize`, `resizeMode`, `numberOfLines` | `font-size`, `object-fit`, `line-clamp` |
| `width=20px` | `width=20` (pixels) or `width="20px"`; quote every unit and keyword |
| `a-b` | `a - b`: names hold hyphens |
| `background="…"` | `background-color`; the shorthand is refused |
| `sort`, `reduce`, `find`, `Math.random()` | `sortBy(xs, x => key)`; the rest in the data module (`vocab <name>` says what) |
| Reading a state after assigning it in the same action | Writes land when the action ends: `let n = …`, assign `n`, read `n` |
| An effect that runs because a value changed | Nothing does. An event runs an action; a `task` runs on a timer or gate |

## The data module

- Each source answers exactly its declared `shape`. An extra field fails the resource or
  voids the mutation's reply. Type replies as `Result<'source'>` so the build names it.
- A failed read throws, so Exact keeps the previous answer; a refused write returns a
  shaped reply (`{ ok: false, error }`) for the action to show.
- The build asks each source once and bakes the answer into the first frame. Storage, the
  network and native modules refuse there with `code: 'bake'`: rethrow it
  (`if (e.code === 'bake') throw e`), or the error is baked into every launch.
- No clock or randomness in `app.ts`; time arrives as an argument
  (`time.epochAtZero + performanceNow()` from the reserved `exactTime()` source).
- It reaches only what `export const grants` names (`net.fetch <origin>`, `fs.read …`).
- List every resource a write changes: `mutation saved … refreshes items, counts`.

## The loop

```sh
bun exact.mjs contract build app.contract --json   # [] and exit 0: no errors or warnings
bun exact.mjs test web                             # the app's *.test.contract
bun exact.mjs agent web tree "tap add" state logs "screenshot .exact/out.png"
bun exact.mjs verify                               # tests, layout checks light/dark at two widths, a relaunch
bun exact.mjs verify ios                           # once at the end for a phone app
```

- Read every diagnostic before the next edit. A warning builds but is not what you meant;
  fixing it is part of the change.
- `verify`, `test` and `agent` build their own copy. Run no dev server and no
  `contract build` before them. `linux` and `android` drives need their build run first.
- Give every control a `testId` and drive by it on every host, never by coordinates.
- In tests, `clock data` lands replies and `clock settle` ends transitions. Never wait in
  real time.
- `"screenshot .exact/x.png compare ref/x.png"` reports differences as text, node by node.
- Scratch files go in `.exact/`, committed helper scripts in `tools/`. Other new files can
  make the build stale.
- Do not edit generated files: `exact.mjs`, `app.contract.d.ts`, `rust-toolchain.toml`,
  `.cargo/config.toml`, `Cargo.toml`'s `[patch.crates-io]`, the `exact:` block in `AGENTS.md`.

## When it compiles and misbehaves

Search `<exact>/docs/agent-pitfalls.md` for the symptom; it lists verified footguns by
area (layout, lists, native navigation, actions, input, driving). Three that recur:

1. A `header` holds a heading, one subtitle text and buttons (or a search field, a
   `tablist`). Anything else is not in the iOS bar.
2. `width="100%"` with padding overflows, as in CSS. Drop the width; a `flex=1` field
   takes `min-width=0`.
3. A scroller or list needs a bound: `flex=1 min-height=0` inside a bounded column, or a
   `height`.

## Done

- `contract build --json` prints `[]`, `verify` is clean on the app's platform, and the
  requested interaction was driven with `state`, `logs` or a screenshot as evidence.
- A web pass proves nothing about a native host. Build and drive each host you claim.
- A fix loop gets three rounds; then report the diagnostic and evidence.
- When a doc was wrong, a workaround was needed or a command confused you, record it:
  `bun exact.mjs feedback note "<what happened>"` (`feedback status` says whether notes
  are sent; skip under `never`).

Related skills: `exact-performance` for speed, `exact-desktop` for a Mac app that embeds
web content, a local service or native code.
