# Android biometric Cancel leaves authentication pending

**Status:** Open
**Systems:** Local authentication Android implementation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, call `authenticate(reason="Unlock", biometricOnly=true)`
on Android and press the prompt's Cancel button. The dialog closes, but
the send never answers `USER_CANCEL` and its cancellation signal remains
in the module's `signals` set. A caller waiting for authentication can
remain disabled or show its pending state indefinitely.

`packages/local-auth/android/LocalAuth.kt:138–140` installs the negative
button with an empty listener:

```kt
builder.setNegativeButton(text, executor) { _, _ -> }
```

The only completion paths, at lines 146–147, are
`onAuthenticationSucceeded` and `onAuthenticationError`. The mapping of
error 13 to `USER_CANCEL` at lines 107–108 does not handle the button:
the platform's `BiometricPrompt.onDialogDismissed` sends a negative-button
press to its registered `OnClickListener`, without calling the
authentication error callback. This is explicit in the
[Android framework implementation](https://android.googlesource.com/platform/frameworks/base/+/1963708d366eaf301a6b6c29d9509d4b1dc7eff7/core/java/android/hardware/biometrics/BiometricPrompt.java).

This finding is verified by tracing the complete module and the platform
callback dispatch. No device or biometric prompt was opened. The same
path applies when `android.allowedAuthenticators` selects biometrics
without a device credential, including the authenticator entry used by
presence-protected keys with that policy.

Make the negative listener complete with `USER_CANCEL` and remove its
signal. Share a completion guard with success, error, and destruction so
each prompt answers once. Validate a negative-button press separately
from Back/system cancellation, then authenticate again in the same
session and verify no stale prompt remains registered.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot; no existing report covers this completion path.
