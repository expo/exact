# Recipe: open a page over the app

Install with `bun exact.mjs add @exact/in-app-browser`. In `app.contract`:

```contract
use openBrowser, BrowserResult from "@exact/in-app-browser"

component App
  mutation browser as shape BrowserResult
  action checkout
    send browser = openBrowser(url="https://example.com/checkout")
  view
    column
      button press=checkout testId="checkout"
        text "Check Out"
      text (match browser { case some(r) => `${r.action}`, case none => "" }) testId="result"
```

Pass an absolute HTTP(S) URL. iOS shows `SFSafariViewController` over the app and
answers `{action: "dismissed"}` when it closes. Android opens a Custom Tab (or
falls back to the browser) and answers `{action: "opened"}` on launch; a link an
installed app claims (YouTube's, with YouTube installed) opens that app, as any
link on Android does. macOS
opens the default browser; the web opens a tab/window; both answer `"opened"`.
Linux and tvOS refuse with `NotSupportedError`. No grant is needed.

**Under the agent nothing opens.** A send waits on a `browser` ticket in
`state.pending`. `clock settle` stops at the hold and prints its target (`@N`);
`state pending` prints the numeric `ticket`. Run this first:

```sh
bun exact.mjs agent web "tap checkout" "clock settle" "state pending"
```

Copy the target it prints into the next operation: `tap @N dismissed` or
`tap @N opened`. In the same drive, answer its first ticket as follows:

```sh
bun exact.mjs agent web "tap checkout" "clock settle" "state pending" "tap @1 dismissed" "clock settle" state logs
```

This command starts a new drive of the example, whose first ticket is `@1`.
The final settle fills the mutation with that action. The web drive's `logs`
records `in-app-browser: dismissed https://example.com` (or `opened`).
In `app.test.contract`, answer the held ticket by capability name:

```contract-test
test "checkout dismisses the browser"
  tap "checkout"
  tap "@browser" dismissed
  clock data
  expect text "result" == "dismissed"
```

A drive confirms the open request and substituted reply; it cannot see Safari's
sheet. To inspect the real sheet automatically on a simulator, use XCUITest
against a normal app launch without the agent.

On iOS 27 in English the dismiss control's accessibility label is `Close`: the
package leaves Safari's default
[dismiss style](https://developer.apple.com/documentation/safariservices/sfsafariviewcontroller/dismissbuttonstyle-swift.property)
unless a call sets one. If its accessibility tap leaves the sheet up, tap a coordinate at the centre of
that button's frame, then assert the sheet is gone and the prior app screen is
back. See the package's `test/checklist.md` for platform checks.
