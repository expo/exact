# Exact Documentation

Write an app once. It runs natively on the web, macOS, iOS, and Linux.

## Set up Exact

Give this prompt to your coding agent. It installs the toolchain, makes an app and starts its dev loop.

```prompt Start a hello world app
Start a new hello world project for me using Expo Exact.

1. Exact has no npm package: you work from a checkout of its repository. If this folder has `scripts/exact.mjs`, it is the checkout. If not, and it has an `exact` folder with `scripts/exact.mjs`, that is the checkout. Otherwise run `git clone https://github.com/expo/exact.git` here, which makes the checkout in an `exact` folder.
2. In the checkout, run `bun scripts/exact.mjs setup --platform web`. It needs Bun 1.4.2 or later and rustup, any version. If one is missing or too old, ask me before you install it. `setup` then downloads Chromium, Rust toolchains and crates. That is expected.
3. In the checkout, run `bun scripts/exact.mjs new ../hello`. It makes the app in a `hello` folder next to the checkout. Then `cd ../hello`.
4. Run `bun exact.mjs test web`. If it fails, show me the output and stop.
5. Start `bun exact.mjs web` in the background, or tell me to run it if you cannot. Wait for the line that starts with `http://` and give me that URL.
6. Then ask me which other platforms I want this app on: iOS, macOS, Android, Linux or Windows. For each platform I name, run `bun scripts/exact.mjs setup` in the checkout, then run `bun exact.mjs docs platform checks` in the app and follow its table to check the app and open it. Keep the dev loop from step 5: where the table starts one, use it instead, with the URL it printed. Tell me if this computer cannot build a platform.

Do not edit `app.contract`, `app.ts` or `app.json` yourself. A platform command may add its platform to `app.json`; keep that change.
```
