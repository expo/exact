```prompt Use this recipe in my app
Make the Exact app in this folder do what this recipe describes. Use https://github.com/expo/exact/blob/main/docs/{{SOURCE}} as the source for each step. The Exact checkout has the same page at docs/{{SOURCE}}.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. Read the recipe. If it leaves a choice that I did not make (which screen, which data, which platforms), propose one and ask me before you write code.
3. Do what the recipe says, in its order, with its tags, properties, functions and grants. Its component, state and URL names are examples. Change nothing else in the app.
4. Add a test in app.test.contract for each thing a person does that you changed. Copy the recipe's `contract-test` block where it has one. A drive operation the recipe shows (`prefer extension`, `call`, a ticket answer such as `tap @N accepted`) is not a test step: run it with `bun exact.mjs agent`.
5. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 6) and report it; do not keep trying.
6. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
7. Tell me what you changed, what the tests and checks reported, and what I must still do by hand.
```
