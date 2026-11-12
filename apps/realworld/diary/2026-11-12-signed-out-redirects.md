surfaces: web
agent: Claude Code (Opus 5.5)
task: send a signed-out reader on a route that needs one to sign in

### Rough

- `/settings` showed an empty form and `/editor` an editor to a signed-out reader: nothing checked. Your Feed was checked by an action a 100 ms tick ran, since `me` answering runs nothing.
- The check is now a task the session arms (`when signedOut and not publicRoute key=current.id`, `after(1, toSignIn)`): it fires once `me` has answered, with no tick. Every route not named in `publicRoute` needs a reader, so a route added later is protected.
- A deep link to `/settings` has one browser history entry, so after the redirect Back leaves the site, as React's `<Navigate replace>` does. Your Feed's first entry can only be the tab's root, so it becomes `/` and `/login` is pushed after it: Back is the global feed.
- The test driver's `back` finds no route beneath sign-in, since the home page is drawn outside the stack's rows; `tap "app" history -1` (the browser's Back) reaches it.

### Lean in

- `at "/settings"` with `fail fetch` tests a signed-out launch with no request to the hosted API.

### Needed

- protected routes: by hand (a task and `replace`; the router has no guards, LLP 1038)

### Checkpoints

- 2026-11-12: `app.test.contract` passes on the web (five tests), and `app.test.mjs` still passes. A signed-in reader was not driven: that needs the hosted API.
