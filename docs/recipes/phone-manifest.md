# Recipe: a phone app's manifest

`app.json` names the app and what it ships; `$schema` gives an editor every key.
A portrait phone app with a light appearance and HTTP access (a name in `env`
needs its `const` in `app.ts`, below, or the bake refuses):

```json
{
  "$schema": "../exact2/scripts/app.schema.json",
  "name": "Euxy",
  "app": { "id": "com.yourcompany.euxy", "name": "Euxy" },
  "orientation": "portrait",
  "userInterfaceStyle": "light",
  "env": ["API_BASE_URL"],
  "host": {
    "ios": { "appTransportSecurity": { "allowsArbitraryLoads": true } },
    "android": {}
  }
}
```

- `orientation` locks iPhone and Android; iPad keeps all orientations for multitasking.
- `userInterfaceStyle` fixes iOS, macOS and web appearance; Android follows the device.
  A node's `color-scheme="light"` fixes its subtree on every graphical host.
- `host.ios.appTransportSecurity.allowsArbitraryLoads` allows HTTP connections,
  including `fetch` to named hosts admitted by `net.fetch` grants. Omit it for HTTPS.
  `allowsArbitraryLoadsInWebContent` allows HTTP only in web views, not `fetch`.
  Android's `fetch` and images use Rust sockets: the manifest's cleartext policy
  does not govern them. Android web views follow that policy and refuse HTTP.
- Exact already writes `UILaunchScreen` and
  `CADisableMinimumFrameDurationOnPhone=true` on iOS; no manifest key is needed.
  The latter permits ProMotion rates; it does not promise 120 Hz.
- `host.ios` and `host.android` name the native platforms the app ships.
- Set `app.id` and `app.name` before building (`exact new --name --bundle-id`).

`env` declares public settings inlined at build. In `app.ts`, read each directly
in a top-level `const`, then derive from it:

```ts
const BUILT = import.meta.env.API_BASE_URL ?? '';
const API = BUILT.trim() || 'http://127.0.0.1:8080';
```

Values come from the environment, `.env.local`, then `.env` beside `app.json`.
A missing value needs a literal `??` default. Declare every name read, and read
every name declared. A setting is public on every host: never a secret
([a REST server](rest-backend.md)).
