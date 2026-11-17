surfaces: web
agent: Codex, GPT-6.1 Sol
task: cancel a pending browser sign-in and retry

## Rough

- Closing a popup left Sign in disabled until the ten-minute bound. COOP
  makes closure unknowable. The screen needed its own Cancel control.
- WebKit's fixture preparation fails while keeping the DPoP CryptoKey:
  “The object can not be cloned.” It never reaches the auth popup. The
  app recovery test runs on Chromium; transport tests run on both engines.

## Lean in

- Assigning `none` to the sign-in mutation releases its request and clears
  pending, so Cancel needs no new runtime API or data source.

## Needed

- sign-in cancellation: provided by mutation assignment

## Checkpoints

- Chromium: close the popup, press Cancel, retry without preparing again,
  approve and complete the PKCE code exchange.
- Chromium and WebKit: COOP callback, immediate callback, closure reaching
  the bound, supersession and forgotten request cleanup.
