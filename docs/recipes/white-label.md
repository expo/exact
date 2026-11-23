# Recipe: one app under several brands

A brand with its own store app (id, name, icon, colours, launch screen, URL
scheme) is its own app, not a variant (LLP 1030.008 §6): a directory per brand
from `exact new <dir> --name … --bundle-id … --with ios,mac,android`, in a Bun
workspace whose packages hold the screens (Contract, and a `.forma` of their
tokens) and the data code (TypeScript, with a `.d.ts`).
Each brand depends on them as `"workspace:*"`. Its `app.contract`:

```contract
use BrandScreen, Greeting from "@fixture/brand-ui"

routes nav
  home "/"
    open "/open"

component Ruby
  resource greeting = greeting("Ruby") as shape Greeting
  action follow(url: string)
    nav = go(nav, url)
  view
    main testId="root" navigationKey=`${top(nav).id}` navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        BrandScreen(entry=e, name="Ruby", logo="assets/ruby-logo.png", greeting=greeting)
```

`app.ts`, then `app.forma`:

```ts
export * from '@fixture/core';
export const grants = '';
```

```forma
forma ruby

theme
  AccentColor = light-dark(#c0392b, #e5483a)
  brand-ui.card = light-dark(#fbe9e7, #3a1410)
```

`app.json` adds these, the launch pictures into the `host` entries `exact new`
wrote:

```json
"icons": [{ "src": "assets/icon.png", "sizes": "1024x1024", "type": "image/png", "purpose": "any" }],
"background_color": "#C0392B",
"color_scheme_dark": { "background_color": "#5B1A14" },
"protocol_handlers": [{ "protocol": "ruby", "url": "/open?u=%s" }],
"host": { "ios": { "launchImage": { "src": "assets/ruby-launch.png", "width": 120 } },
  "android": { "launchImage": { "src": "assets/ruby-launch.png", "width": 160 } } }
```

- **The root is each brand's**: `routes` live in the root file (LLP 1038 D2),
  and a package's component asks only its own package's sources (LLP
  1035.005.004 D7), so the screen takes the answers as props (`entry: Entry`,
  `name`, `logo`, `greeting: Greeting`).
- **Grants are each app's**: a package's are never read, and `app.ts`'s own
  export wins over the star. The shared code exports no `appId`; each app takes
  its `app.json` id, and a native build refuses one naming another brand.
- **Tokens**: `brand-ui.card` is declared in the package's `brand-ui.forma`;
  each `app.forma` replaces it by full name.
- **Only a brand's own `assets/` ship**; its logo reaches the screen as a path.
- **A shared edit** reaches each brand's next `ios`, `mac` or `android` build.
  A macOS or iOS simulator drive names the file and builds first; an Android
  drive names the command that rebuilds.
- **Stores**: each brand's id takes its own signing profile and store record,
  under one Apple team and Play account or its own; Exact needs nothing more.
  Exact archives ([EAS iOS](eas-ios.md)); uploading is the app's. Two teams,
  two Play accounts and store submission have not been run with this layout.
