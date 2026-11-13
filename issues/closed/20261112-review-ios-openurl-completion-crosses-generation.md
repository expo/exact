# An old iOS openURL completion can change a replacement runner

**Status:** Closed
**Resolution:** `ExactSession.openURLAnswer(args)` replaces `answerOpenURL`: it stamps the generation when the open is asked and its completion, which both adapters hand to the platform for every scheme, changes the result node only while that generation runs.
**Systems:** host/apple
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the iOS adapter passes `UIApplication.open` a completion
that captures the session and command arguments, without the session's
generation (`host/apple/Sources/ExactIOS/main.swift:58-76`). This applies to
web URLs, declared custom schemes and the app's Settings URL.

`ExactSession.answerOpenURL` checks only that the session is not destroyed.
It resolves the result target's HTML id in the **current** runner and sends
that node a `change` containing `opened` or `failed`
(`host/apple/Sources/ExactKit/OpenURL.swift:39-43`). The runner lookup has
no request identity: `runner/src/commands.rs:44-58` simply finds the current
node by id. A successful plan replacement keeps the session and increments
its generation (`host/apple/Sources/ExactKit/Session.swift:992-999`).

The resulting sequence is:

1. Generation A runs `openURL(url, "result")`; UIKit retains its completion.
2. A plan reload or activation replaces the runner with generation B, which
   also has a node named `result`.
3. UIKit completes A's open. B's `change` handler runs even though B did not
   issue the command.

The old result can change the replacement app's state, for example advancing
a flow that waits for an external open to succeed. The destroyed-session
guard does not protect a live session whose runner was replaced. The macOS
adapter's open path answers synchronously, so this report concerns iOS.

**Verification.** Source trace through the complete iOS adapter, the complete
`answerOpenURL` implementation, the runner's lookup and the session commit
boundary. No UIKit launch or asynchronous platform-open reproduction was
run. A controlled regression can retain an injected opener's completion,
replace the session's plan, and complete the old open; the replacement's
handler must remain untouched.

**Direction.** Capture the issuing generation before passing the completion
to UIKit and refuse delivery after that generation ends. Keep the check at
the completion boundary for all supported schemes, including Settings.
Cover old completions after replacement and destruction, and a current
generation's successful and failed opens.

**Duplicates checked.** Searched open and closed filesystem reports and all
275 cached GitHub issue titles and bodies for openURL completion, reload and
generation routing. The presence and iframe callback reports concern other
callback registries; neither covers this application-delegate completion.
