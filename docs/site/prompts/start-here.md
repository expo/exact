```prompt Build an app with Exact
Build a new app with Exact in this folder. Use https://github.com/expo/exact/blob/main/docs/start-here.md as the source for each step: it is the only required reading. The exact2 checkout has the same page at docs/start-here.md.

1. If I did not say what the app does or which platforms it runs on (web, macOS, iOS, Android, Windows, Linux), ask me. For a website, follow docs/start-here-site.md instead. Find the exact2 checkout (the folder with scripts/exact.mjs); if you cannot find it, ask me where it is.
2. Run `bun <exact2>/scripts/exact.mjs setup --check`. If it names a missing tool for a platform the app runs on, run `bun <exact2>/scripts/exact.mjs setup`; ignore the others.
3. Make the app: `bun <exact2>/scripts/exact.mjs new <name>`, then `cd <name>`. For an iOS app, start `bun exact.mjs ios --prebuild` now: the first build takes minutes. Run `bun exact.mjs pattern` and start from the nearest pattern.
4. Replace the placeholder screen and its tests: the first screen in app.contract, its data in app.ts, and a test in app.test.contract for each thing a person does.
5. Look up only what you need: `bun exact.mjs docs <heading words>`, `bun exact.mjs contract vocab <name>`, and the recipe the page's table names.
6. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. Stop after three rounds of fixes; record and report any remaining or pre-existing findings (step 7), and any target you could not check.
7. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
8. Tell me what you built, what each platform check reported, and the launch command for each platform from the Platform checks section below.
```
