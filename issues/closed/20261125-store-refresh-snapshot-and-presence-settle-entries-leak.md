# Store::refresh_snapshot and presence::settle_entries leak mandatory user-presence secrets in plaintext and desynchronize has()/get()

**Status:** Closed
**Resolution:** Enforced presence occupancy and value-byte invariants in presence::settle_entries, Store::occupancy, Store::adopt_occupancy, and Store::refresh_snapshot so mandatory secrets never retain plaintext bytes and optional occupancies reconcile with bytes.
**Systems:** runner (store, user-presence secrets)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Back doors)

Verification: verified by reading.

At `9c0431fd5`, `Store::refresh_snapshot` (`runner/src/store.rs:649-663`) and `presence::settle_entries` (`runner/src/store/presence.rs:50-71`) allow plaintext bytes of mandatory user-presence secrets to enter and remain readable in `Store::values`, and desynchronize occupancy (`has()`/`locked()`) from stored bytes (`get()`/`read()`):

- `Store::refresh_snapshot` (`runner/src/store.rs:649-663`) builds `next` from every granted entry in `snapshot` (including plain bytes for presence-policy names), then copies `self.values`'s existing `exact.presence.<name>` entries (already populated at boot as `"absent"` or `"plain"` by `Store::from_names` -> `settle_entries`) into `next` (`:658-662`), and calls `presence::settle_entries(&mut next, &self.presence)` (`:663`).
- In `presence::settle_entries` (`runner/src/store/presence.rs:57-68`):
  ```rust
  let said = values.get(&key).and_then(|w| Occupancy::from_word(w));
  let occupancy = match said {
      Some(o) => o,
      None if values.contains_key(name) && !line.optional => Occupancy::Pending,
      None if values.contains_key(name) => Occupancy::Plain,
      None => Occupancy::Absent,
  };
  values.insert(key, occupancy.word().as_bytes().to_vec());
  if occupancy.locked() {
      values.remove(name);
      locked = true;
  }
  ```
  Because `said` is `Some(Occupancy::Absent)` (not `None`), `settle_entries` takes `Some(o) => o`, never transitions a mandatory (`!line.optional`) presence name with incoming bytes to `Occupancy::Pending`, and because `Occupancy::Absent.locked()` is `false`, never removes `name`'s raw bytes from `next`.
- `Store::read` (`runner/src/store.rs:515-521`) only errors when `self.occupancy(name).is_some_and(|o| o.locked())`, and otherwise returns `self.values.get(name)`. Meanwhile `Store::has` (`runner/src/store/presence.rs:128-131`) returns `o != Occupancy::Absent`.
- Furthermore, in `Runner::store_changed` (`runner/src/runner/kept.rs:654-676`), `self.checkpoint()` (`runner/src/runner/commit.rs:81`) runs `self.adopt_presence()` *before* `restore_store(snapshot)` calls `refresh_snapshot`, so `self.settle(false)` runs immediately against the leaked plaintext bytes. Even on later commits when `adopt_presence()` runs again, `Backend::occupancies()` still reports its boot-reconciled `Occupancy::Absent`, and `Store::adopt_occupancy` (`runner/src/store/presence.rs:215-218`) returns early on `before == occupancy && (occupancy != Occupancy::Plain || bytes)` without stripping the leaked bytes from `self.values`.

### Trigger
1. An app declares a presence-protected secret (`secret.keep api_key user-presence`, or optional `secret.keep tok user-presence optional`).
2. Boot initializes `exact.presence.api_key = "absent"` (or `exact.presence.tok = "absent"` / `"plain"`).
3. Another window, app extension, watch import, or older app build writes a plain value for `api_key` (or writes/forgets `tok`) in the shared secret store, and the host invokes `Runner::store_changed(snapshot)` or `Runner::adopt_store(snapshot)` (on Apple: `Host::refresh_store_after` at `host/apple/src/activation.rs:139, 142` passing `Platform::snapshot` -> `read_parts` at `host/apple/src/store.rs:297-311, 819-829`, which reads every name in `secrets.names()`).

### Actual vs. Expected
- **Actual:**
  - **Mandatory presence secret (`!line.optional`):** `self.values` holds both `exact.presence.api_key = "absent"` and `api_key = <plaintext>`. `store.read("api_key")` and `store.get("api_key")` return the plaintext secret without any user-presence check, and `store.snapshot()` / `store.call_snapshot()` (`runner/src/store/presence.rs:245-253`) leak the raw bytes to child executors, while `store.has("api_key")` and `store.locked("api_key")` return `false`.
  - **Optional presence secret (`line.optional`):** A peer plain write when local occupancy was `"absent"` leaves `has("tok") == false` while `get("tok") == Some(v)` (and a subsequent `store.set("tok", v)` short-circuits at `runner/src/store.rs:543` before calling `written_plain` at `:548`); a peer forget when local occupancy was `"plain"` removes value bytes while keeping `exact.presence.tok = "plain"`, leaving `has("tok") == true` while `get("tok") == None`.
- **Expected (LLP 1133.010 D4 & D5; `runner/src/store.rs:646-648`; `runner/src/store/presence.rs:1-2, 46-48`):** A mandatory presence name must never hold or expose plaintext bytes in `Store` (`Pending` when plain bytes appear, bytes stripped); `Occupancy::Absent` must never coexist with value bytes (reconciling to `Plain` for optional lines with incoming bytes, or dropping bytes if truly absent); and `Occupancy::Plain` with no value bytes after a peer forget must reconcile to `Occupancy::Absent`.

### Verification & Siblings Checked
Verified by reading at `9c0431fd5`. Note that the existing test `a_locked_or_pending_name_holds_no_bytes_from_any_snapshot` (`runner/src/store/presence.rs:339-344`) only tests `refresh_snapshot` when occupancy is already `Locked`, missing the `Absent` and `Plain` cases.
- **Sibling 1 (`Store::from_names`, `runner/src/store.rs:286-302`):** Calls the same `presence::settle_entries`, so any boot/carry/mirror snapshot supplying `exact.presence.<name>="plain"` on a mandatory `!line.optional` line, or `exact.presence.<name>="absent"` alongside `<name>=<bytes>`, retains raw plaintext bytes and lets `read()`/`get()`/`call_snapshot()` return them.
- **Sibling 2 (`Store::adopt_occupancy`, `runner/src/store/presence.rs:215-218`):** `if before == occupancy && (occupancy != Occupancy::Plain || bytes) { return false; }` fails to strip stray bytes when `before == occupancy == Occupancy::Absent` (or `Locked`/`Pending`).
- Checked `host/web-js/rt.js:913-920` (`Store.peer`): web-js does not host locked presence secrets.

### Suggested Fix & Regression Test
In `presence::settle_entries` (`runner/src/store/presence.rs:50-71`), enforce the occupancy/bytes invariants regardless of whether `said` is `Some` or `None`:
1. If `!line.optional`: treat `Some(Occupancy::Plain)` as invalid (`Pending` if bytes are present, else `Absent`); if `said` is `None | Some(Occupancy::Absent)` and `values.contains_key(name)`, transition to `Occupancy::Pending`; whenever `occupancy != Occupancy::Plain` (i.e. `Locked`, `Pending`, or `Absent`), remove `name` from `values`.
2. If `line.optional`: keep `Locked`/`Pending` (and strip `name`); otherwise derive `Plain` iff `values.contains_key(name)`, else `Absent` (so peer plain writes and peer forgets reconcile cleanly).
3. In `Store::adopt_occupancy` (`runner/src/store/presence.rs:216`), also proceed when `occupancy != Occupancy::Plain && bytes` so non-plain occupancies always drop stray bytes.
4. Add unit tests in `runner/src/store/presence.rs` covering `refresh_snapshot` and `Store::new` for: mandatory name starting `Absent` receiving peer plain bytes (must become `Pending`, hold no bytes, error `Locked` on `read`), mandatory name with spoofed `"plain"` occupancy, optional name starting `Absent` receiving peer plain bytes (must reconcile to `Plain`, `has()==true`), and optional name starting `Plain` after peer forget (must reconcile to `Absent`, `has()==false`).

### Siblings checked

Confirmed sibling defect in Store::from_names (runner/src/store.rs:286-302, shares settle_entries and trusts Some(Plain) on mandatory lines and bytes alongside Some(Absent)) and in Store::adopt_occupancy (runner/src/store/presence.rs:215-218, early-returns without stripping stray bytes when before == occupancy == Absent). Checked host/apple/src/activation.rs, host/apple/src/store.rs, and host/web-js/rt.js:913-920.
