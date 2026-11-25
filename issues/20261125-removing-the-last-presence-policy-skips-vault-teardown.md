# Removing the last presence policy skips vault teardown, permanently orphaning locked Keychain items and blocking Watch sync for that secret

**Status:** Open
**Systems:** host/apple (presence_secrets, store, watch_sync); host/painted (presence_secrets)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `presence_secrets::keychain_backend` (`host/apple/src/presence_secrets.rs:299-317`), `presence_secrets::agent_backend` (`host/apple/src/presence_secrets.rs:98-108`), and `store::endowed::<false>` (`host/apple/src/store.rs:402-403`, selected by `host/apple/src/link.rs:638-642`) all skip presence vault reconciliation whenever no grant line retains a presence policy, leaving old locked items, index entries, and `exact.presence/<name>` KV records permanently orphaned and permanently breaking Apple Watch sync for that secret (`host/apple/src/watch_sync.rs:384-388`).

### Triggering sequence
1. Version 1 of an app declares a single (or last) presence-protected secret (e.g. `secret.keep token biometry-any` or `secret.keep token user-presence optional`) and locks a value on the user's device. This persists:
   - a locked Keychain item `token#1` under `<app_id>.presence` (or `exact.presence.agent/token#1` under the agent; `$HOME/exact-presence/<app_id>/token#1` on Android),
   - an index entry `exact.presence.items/token#1` in KV (on Apple), and
   - an intent record `exact.presence/token` in KV.
2. Version 2 updates the grant to a plain secret (`secret.keep token`, e.g. to drop the lock or enable Apple Watch sync) and boots on the upgraded device.
3. Even when `Capability::Presence` is linked (`endowed::<true>`, e.g. dev builds, agent drives, or `Links::ALL`), `keychain_backend` (`host/apple/src/presence_secrets.rs:306`) and `agent_backend` (`host/apple/src/presence_secrets.rs:101`) immediately return `None` because `!lines.iter().any(|(_, p)| p.is_some())`. In a release Apple build that strips `Capability::Presence` (`runner/src/uses.rs:482-485`, `host/apple/src/link.rs:638-642`), `store::endowed::<false>` (`host/apple/src/store.rs:402-403`) sets `presence = None` without touching KV or Keychain.
4. When Watch sync runs `watch_sync::transact` (`host/apple/src/watch_sync.rs:383-388`) for `token`, `store.watch_presence("token")` (`host/apple/src/store.rs:554-559`) reads `kv.get("exact.presence", "token")`, finds the stale record, and returns `Err(Refusal::Invalid)`. Calling `store.set` or `store.forget` on `token` in v2 goes through `Platform::write_import` (`host/apple/src/store.rs:535-540`), which never clears `exact.presence` or `exact.presence.items`, so the failure is permanent until uninstall/reinstall.

### Actual vs. expected result
- **Actual:** `Backend::reconcile`'s policy-removal teardown branch (`(None, _) => !generations.is_empty() || record.is_some()`, `runner/src/presence.rs:326-345`) is dead code whenever the app removes its *last* presence policy. The locked item, index entry, and `exact.presence/<name>` KV record remain orphaned forever; Watch sync permanently refuses the secret with `Refusal::Invalid`; and if a future version re-adds a presence policy on `token`, the stale v1 locked item can resurrect.
- **Expected:** LLP 1133.010 D4 (`llp/1133.010-local-auth.rfc.md:381-383`: *"presence to plain: the locked item is not read; `has` reads false, the record is cleared, and the next `set` replaces it"*) and `runner/src/presence.rs:326-341` (*"A line that lost its policy, or changed it: the old items are forgotten unread, and the record with them"*) establish that downgrading a secret from presence to plain must clear its locked items and `exact.presence/<name>` record at boot, unblocking plain use and Watch sync.

### Verification & sibling check
- **Verified by reading** `host/apple/src/presence_secrets.rs`, `host/apple/src/store.rs`, `host/apple/src/link.rs`, `host/apple/src/watch_sync.rs`, `runner/src/presence.rs`, `runner/src/uses.rs`, and `llp/1133.010-local-auth.rfc.md`.
- **Sibling defect found in `host/painted` (Android / Linux / Windows):** `host/painted/src/presence_secrets.rs:198-202` has the exact same early return `if !lines.iter().any(|(_, p)| p.is_some()) { return None; }`, permanently orphaning `$HOME/exact-presence/<app_id>/<name>#<gen>` files, `exact.presence.agent` entries, and `exact.presence/<name>` KV records on Android/Linux when the last presence policy is removed.

### Suggested fix & regression test
- In `presence_secrets::keychain_backend`, `presence_secrets::agent_backend`, and sibling `host/painted/src/presence_secrets.rs::backend`: when `!lines.iter().any(|(_, p)| p.is_some())`, check whether any stale entries exist in `kv.keys(RECORDS)` or `kv.keys(INDEX)`/`kv.keys(LOCKED)` (or the Android presence directory); if so, run `Backend::new` over the plain `lines` (plus any orphaned names still present in `RECORDS`/`INDEX`) to delete locked items and KV records, then return `None`.
- In `host/apple/src/store.rs:402-403` (`endowed::<false>`, where `ExactPresence` is stripped from a release build that no longer links `Capability::Presence`): clear stale `exact.presence` KV records (and `exact.presence.items`, plus `<app_id>.presence` Keychain items if cleaned via Security or on-demand teardown) when `bindings.kv` still holds stale presence records for names that no longer have a presence policy, so `watch_sync` is never permanently blocked.
- **Regression test:** Add a test in `host/apple/src/presence_secrets_tests.rs` (and `host/apple/src/watch_sync/tests.rs` / `host/painted/src/presence_secrets_tests.rs`) that writes a locked secret under `secret.keep token biometry-any`, reboots with `secret.keep token` (no presence policies remaining), and asserts the locked Keychain item, `exact.presence.items`, and `exact.presence/token` KV record are all removed and `watch_sync::transact` accepts `token`.

### Siblings checked

Checked host/apple/src/presence_secrets.rs (both agent_backend and keychain_backend affected); checked host/apple/src/store.rs (endowed::<false> affected); checked host/painted/src/presence_secrets.rs:198-202 (exact same early return bug on Android/Linux/Windows).
