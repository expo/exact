# Recipe: an icon the person chooses

An app may ship more than one Home Screen icon and let the person pick one in
its settings (LLP 1163.000). Declare the alternates in `app.json`, in the order
the picker shows them, each made from icons as the app's own are:

```json
"icons": [{ "src": "assets/icon.png" }],
"alternate_icons": [
  { "name": "Paper", "icons": [{ "src": "assets/icon-paper.png" }] }
]
```

A `name` is letters and digits, starting with a letter. Each alternate needs a
square opaque icon of at least 1024 px (or its own `host.ios.icon` project),
and the app a primary. A catalog `.appiconset` no entry names is refused.
Install the package with `bun exact.mjs add @exact/app-icon`:

```contract
use appIcon, setAppIcon, AppIconStatus from "@exact/app-icon"

component App
  resource shown = appIcon() as shape AppIconStatus else empty()
  mutation chosen as shape AppIconStatus refreshes shown
  action choose(name: string)
    send chosen = setAppIcon(icon=name)
  action choosePrimary
    send chosen = setAppIcon()
  view
    list appearance="auto" listStyle="inset-grouped"
      if shown.available
        section
          header
            text "App Icon"
          button press=choosePrimary role="radio" aria-checked=(shown.icon == none) testId="icon-primary"
            image "assets/icon.png" alt="" width=29 height=29 border-radius=7
            text "Default" flex=1
          if shown.icons.includes("Paper")
            button press=choose("Paper") role="radio" aria-checked=(shown.icon == some("Paper")) testId="icon-paper"
              image "assets/icon-paper.png" alt="" width=29 height=29 border-radius=7
              text "Paper" flex=1
```

- **Only from a tap.** iOS shows its own alert for every change, so call
  `setAppIcon` from the action a person's choice runs, never from a timer, a
  resource's answer or launch. The runner refuses a tool's call
  (`NotAllowedError`, `person-required`) but cannot tell the rest apart.
- **Hide the picker where `available` is false**: every host but iOS and
  iPadOS. `setAppIcon` refuses `NotSupportedError` there, and `NotFoundError`
  for a name the binary does not ship.
- **`icon` is iOS's own answer.** It may name an alternate an update removed;
  `setAppIcon()`, with no icon, always restores the primary.
- **A new roster is a new binary**: the names are in the iOS compatibility id.
  New art for the same name is a native build too, never an update.

Under the agent nothing is shown. The roster is the word `app-icon.icons`, and
each change waits on an `app-icon` ticket standing for iOS's alert:

```sh
bun exact.mjs agent ios 'prefer extension app-icon.icons=["Paper"]' "tap icon-paper" "clock settle" "state pending"
```

Answer it with `tap @N ok`. To see iOS's alert itself, add `--bind app-icon`
to drive the real implementation on a simulator.
