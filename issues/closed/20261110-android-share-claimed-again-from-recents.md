# On Android a share that started the app is claimed again when the app is reopened from Recents

**Status:** Closed
**Resolution:** The activity claims a share or launch URL only from an intent that arrives now (`Shares.arrived`): one marked `FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`, or started with a saved state, is a plain launch, at a cold start and in a live process.
**Systems:** host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt, host/android/canvas/src/main/java/dev/exact/canvas/Shares.kt
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`. The activity claims a share from whatever intent it was made with, in two places: at a cold start, as the host's launch location (`ExactActivity.kt:553`, `environment`), and when an activity is made again in a live process with no saved state (`ExactActivity.kt:112`, `received`):

```kotlin
Shares.request(this, intent)?.let { android.system.Os.setenv("EXACT_SHARE", it, true) }
…
if (again && state == null) received(intent)
```

Neither asks whether the intent is new. When a share starts the app, its `ACTION_SEND` intent, extras included, is the task's base intent. Back at the app's first screen finishes such an activity (AOSP `ActivityClientController.onBackPressed` keeps only one that Home launched with a main intent). When the person then picks the task in Recents, Android starts the base intent again (`ActivityTaskSupervisor.startActivityFromRecents`) and marks it `FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`, "set for you by the system if this activity is being launched from history" ([Intent](https://developer.android.com/reference/android/content/Intent#FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY)). `Shares.isShare` (`Shares.kt:25`) reads only the action.

**The failure.** Fieldnotes, which declares a `share_target`, is not running. Share a text to it from another app: it opens at its share target and the note is saved. Press Back until the app closes. Open Fieldnotes from Recents.

- observed (by reading): the share is claimed a second time, and the app opens at the share target with the same text, whether the process lived (`received`) or not (`EXACT_SHARE`). Any stream is opened again, or counted as dropped if its grant is gone;
- expected: the app opens where a launch opens it. A share is one location, delivered when it arrives (LLP 1133.024 D2).

A process the system killed in the background is started again with the same intent and a saved state, and `environment` claims the share there too.

**Verification.** Read only: the lines above and AOSP `android14-release`. Not run: no Android device was available. The device check: the steps above; the second launch should open the app's first screen.

**Direction.** Treat an intent with `FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`, and a cold start that has a saved state, as a plain launch in `environment` and in `onCreate`'s `received`: no share from it.

**Related.** `QUEUE.md`, "Android's share target, what is owed", lists the by-hand share from another app as not done; it does not name this. A launch URL (`EXACT_LAUNCH_URL`, `ExactActivity.kt:551`) is read the same way, which only opens the same place again.
