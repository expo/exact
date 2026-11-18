```prompt Give a component a timer
Give a bounded child component in the Exact app in this folder its own timer and gate. Use https://github.com/expo/exact/blob/main/docs/recipes/timer-in-a-component.md as the source for each step. The Exact checkout has the same page at docs/recipes/timer-in-a-component.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not name the component that needs the timer, find it from what the app does and ask me before you write code.
3. Check that the component is used outside data-sized repetition: under `if` or `match`, in a tab, or in a navigation stack. Declare the `task`, its gate state and its stepping action in that component. Tasks under a data-sized `for` or in a virtualized list stay out; use one shared task in the enclosing screen for those rows.
4. Each instance owns its task and stops it when destroyed. A hidden instance stays alive: pass activity as a prop and include it in `when` if the timer should stop while hidden. Use `key=` when changing a value should restart the timer.
5. Keep the existing tests passing without changing what they check. Add a test in app.test.contract that starts the timer (`tap "<id>" drag 0 0 hold <ms>` for a held control, or a `tap`), expects state while it runs, and after it stops runs `clock +<ms>` and expects the state unchanged.
6. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 7) and report it; do not keep trying.
7. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
8. Tell me which component owns the task, what gates it, what the tests and checks reported, and the launch command for each target platform.
```
