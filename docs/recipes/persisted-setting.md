# Recipe: a remembered setting

A setting the app keeps across launches — a tip percentage, °C or °F — is a root
`state` that ends in `persist` (LLP 1116 D5). There is no data module, grant or
JSON to write: the runner reads the store before the first frame and writes it
after every commit that changes the state.

```contract
component App
  state tipPercent = 18 persist
  state units = "C" persist
  action tip(p: number)
    tipPercent = p
  action unit(u: string)
    units = u
  view
    column padding=16 gap=12
      text "Tip" role="heading" aria-level=2
      row role="radiogroup" aria-label="Tip"
        for p in [15, 18, 20, 25] key=p
          button role="radio" aria-checked=(tipPercent == p) press=tip(p) testId=`tip-${p}` flex=1
            text `${p}%`
      text "Temperature" role="heading" aria-level=2
      row role="radiogroup" aria-label="Temperature"
        button role="radio" aria-checked=(units == "C") press=unit("C") testId="unit-c" flex=1
          text "°C"
        button role="radio" aria-checked=(units == "F") press=unit("F") testId="unit-f" flex=1
          text "°F"
```

Each test in `app.test.contract` gets its own store. A leading `stored` line
puts a JSON value in it before the first frame, so you can test an upgrade from
an earlier build. On `reload` the app reads whatever the test has saved.

```contract-test
test "an upgrade keeps the earlier choice"
  stored units "F"
  expect state units == "F"
  tap "unit-c"
  tap "tip-20"
  reload
  expect state units == "C"
  expect state tipPercent == 20

test "a fresh install starts from the declared values"
  expect state tipPercent == 18
  expect state units == "C"
```

Run it with `bun exact.mjs test web` (or `test ios`, `test linux`). To see it by
hand, drive twice with the same store: `bun exact.mjs agent web --storage s1 "tap
tip-20"`, then `bun exact.mjs agent web --storage s1 --keep-storage state` shows `tipPercent:
20`. A drive without `--storage` starts from the declared values and keeps nothing
past the drive.

What to know:

- **The first frame.** A kept value of the state's type replaces the declared one.
  A missing value keeps the declared one, and so does a value of another shape
  (the state's type changed since it was kept); `logs` names that one.
- **Where.** The page's `localStorage`, `UserDefaults` on Apple, a file on Linux, keyed by the
  state's name: renaming the state forgets it.
- **Only small settings.** A number, string or bool, or an option or list of one,
  at most 16 KB as JSON. `persist` on a record or a list of records is refused
  (`type-persist-type`): keep app data in a source. A child component's state
  cannot persist (`type-persist-child`): keep the setting in the root and pass it
  down as a prop.
- **A derive is never persisted.** Persist what it reads (`syntax-persist-derive`).
- **A choice of one value** is a `radiogroup` of radios, each `aria-checked` when it is
  the value: the platform's segmented control, the arrows moving it on the web. A
  `tablist` is for switching panels.
