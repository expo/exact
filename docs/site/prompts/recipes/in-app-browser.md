```prompt Open a web page over my app
Make the Exact app in this folder open a web page over the app, in the platform's own in-app browser. Use https://github.com/expo/exact/blob/main/docs/recipes/in-app-browser.md as the source for each step. The Exact checkout has the same page at docs/recipes/in-app-browser.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say which page opens and which control opens it, ask me before you write code. The page needs an absolute HTTP(S) URL.
3. Install the package: `bun exact.mjs add @exact/in-app-browser`. No grant is needed.
4. In app.contract, send `openBrowser` with the URL from the control's action, into a mutation of shape `BrowserResult`. Give the control a `testId`.
5. Use the answer as each platform gives it. iOS answers `{action: "dismissed"}` when its sheet closes. Android, macOS and the web answer `"opened"` when the page opens, so do not wait for a close there. Linux and tvOS refuse with `NotSupportedError`: if one is a target, tell me.
6. Under the agent nothing opens: the send waits on a `browser` ticket. Drive it as the recipe shows: `bun exact.mjs agent web "tap checkout" "clock settle" "state pending"`, with the control's `testId` for `checkout`, prints the ticket's target (`@N`). Run the drive again with `tap @N dismissed` or `tap @N opened`, another `clock settle`, then `state` and `logs` appended, and read the action and the log line.
7. In app.test.contract, add a test that ends on the screen with the control, so verify checks that screen. Do not press the control in it: a ticket answer is a drive operation, not a line of app.test.contract.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me the page and the control, what the drive, the tests and the checks reported, and what I must still look at by hand: a drive cannot see Safari's sheet or a Custom Tab. The package's `test/checklist.md` has the platform checks.
```
