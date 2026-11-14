# Recipe: app.json, assets and modules

`app.json` names the app and its contents; `$schema` lists every key.

```json
{
  "$schema": "../exact2/scripts/app.schema.json",
  "app": { "id": "com.yourcompany.euxy", "name": "Euxy" },
  "name": "Euxy",
  "modules": ["waveform-view"],
  "audio_session": "playback",
  "icons": [{ "src": "assets/icon-1024.png", "purpose": "any maskable" }],
  "protocol_handlers": [{ "protocol": "euxy", "url": "/p/%s" }]
}
```

- **`assets/` and `deck/`** ship except `.DS_Store` files. Contract paths start at the app
  (no leading slash): `image "assets/logo.png"`, a `sound` and `playSound`, a `font` block's
  `400 = "assets/Inter-Regular.ttf"` (then `font-family="Inter"`).
- **`modules/apple/*.swift`** (or `*.xcframework`) is the app's native module;
  `modules/web/index.js` answers on the web ([native module](native-module.md)). `modules`
  lists the hyphenated tags of native views ([native views](native-views.md)): a tag the
  list lacks is `bake-unknown-module`.
- **`audio_session`**: `ambient` (default), `playback`, or `playback-mix` for sound over
  other apps' audio. [Sessions](../reference.md#sounds).
- **`app.id` and `app.name`** are set before the first build (`exact new --name --bundle-id`,
  [porting and release](porting-and-release.md)); `app.ts` takes the id from here.
- **`host.ios.launchImage`** `{ "src": "assets/splash.png", "width": 276 }`: the launch
  screen's PNG, `width` points wide, on `background_color`.
- **`host.ios.icon`** `"art/App.icon"`: an Icon Composer `.icon`; dark icons are `color_scheme_dark.icons`.
- **`protocol_handlers`**: `euxy://p/abc` is `/p/euxy%3A%2F%2Fp%2Fabc` (whole at `%s`).
  Cold, it opens in its route's tab (declare `p "/p/:d"`); running, `go` pushes it on the
  selected tab, `open` on its route's (`vocab navigate`).
- **Other keys:** `userInterfaceStyle` (`dark`/`light`), `categories` (store, Game Mode),
  `data` (`data-*` words a hatch reads), `tamber` (design file),
  `typescript.sources` (outside directories `app.ts` imports), `commands` (the app's
  `exact.mjs` verbs).
- **`strings`**: `{ "base": "en" }` sets the base locale for `strings/<locale>.json`.
- **`env`**: settings inlined at build, so never a secret
  ([a REST server](rest-backend.md)). **`host.web.head`**: [a website](website.md).
- **`host.ios.appClip`** `{}`: an App Clip too (`id` `<app id>.Clip` by default; each
  `applinks:` domain is its `appclips:` one).
- **Apple metadata:** grants derive keys ([permissions](permissions.md)), including
  `device.photos-add purpose.savePhoto` for `NSPhotoLibraryAddUsageDescription`.
  `host.ios` and `host.macos` dictionaries `infoPlist` and `entitlements` refuse
  every key; see [bundle metadata](../reference.md#apple-bundle-metadata).
- **Binary inputs** (`app.json`, assets, Swift, grants, `appId`, canvas surfaces, fonts,
  strings) require `bun exact.mjs ios`. Other `.contract`, `app.ts` and imported edits
  reach a running app in seconds; Rust data inputs rebuild ([driving iOS](ios-driving.md)).
