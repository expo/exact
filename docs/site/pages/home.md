# Exact Documentation

Write an app once. It runs natively on the web, macOS, iOS, and Linux.

## Set up Exact

Give this prompt to your coding agent. It installs the toolchain, makes an app and starts its dev loop.

```prompt Start a hello world app
Start a new hello world project for me using Expo Exact.

1. Exact has no npm package. If this folder has an `exact` folder, use it. If not, run `git clone https://github.com/expo/exact.git` here. It makes an `exact` folder.
2. In `exact`, run `bun scripts/exact.mjs setup --platform web`. It needs Bun 1.4.2 or later and rustup, any version. If one is missing or too old, ask me before you install it. `setup` then downloads Chromium, Rust toolchains and crates. That is expected.
3. Run `bun scripts/exact.mjs new ../hello`. It makes the app in a `hello` folder next to `exact`. Then `cd ../hello`.
4. Run `bun exact.mjs test web`. If it fails, show me the output and stop.
5. Start `bun exact.mjs web` in the background, or tell me to run it if you cannot. Wait for the line that starts with `http://` and give me that URL.
6. Then ask me which other platforms I want this app on: iOS, macOS, Android, Linux or Windows. For each platform I name, run `bun scripts/exact.mjs setup` in `exact`, then run `bun exact.mjs docs platform checks` in the app and follow its table to check the app and open it. Tell me if this computer cannot build a platform.

Do not edit `app.contract`, `app.ts` or `app.json`.
```
