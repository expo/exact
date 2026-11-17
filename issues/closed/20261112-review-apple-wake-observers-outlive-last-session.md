# Reopening Apple's last session installs duplicate wake delivery observers

**Status:** Closed
**Resolution:** `ExactExtensions` keeps the token of its one `exact.wakes.due` observer and adds it only when it has none, so a module made after the last one ended reuses it and a due offer reaches the live module's adapter once.
**Systems:** host/apple/modules/ExactExtensions.swift, host/apple/modules/ExactNativeModule.swift
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `ExactExtensions.created` adds an `exact.wakes.due` block
observer whenever appending a module makes `modules.count == 1`
(`ExactExtensions.swift:117-128`). It discards the observer token.
`destroy` removes the module from that array but does not remove the
observer (`:281-299`). These methods run at the native module's actual
creation and destruction (`ExactNativeModule.swift:1007-1022`).

After the last session is destroyed, creating another session installs a
second observer. The old observer still exists: it captures the static
module registry, so it now finds the new module. On one due notification,
both observers set the same offer's `taken` field and call the same
adapter's `due`. Neither checks whether another observer already took the
offer. Each further transition from no modules to one adds another
delivery.

A package can therefore receive the same background wake several times,
repeat its work and post several completion notifications for one token.
This can occur in an embedding process that replaces its sessions, or a
desktop app that stays running after closing its last session and opens
another. The stale registrations also accumulate for the process lifetime.

Verification is a complete source trace of registration, destruction and
notification delivery. No background task or native app was launched.
A focused regression should create A, destroy A, create B, post one
in-memory due offer and assert B's fake adapter is called once. Repeat the
cycle and test multiple simultaneously live sessions too.

Own a single process observer independently of the current module count,
or retain and remove its token when the last module ends. The registration
state must agree with whether the observer still exists.

Duplicate check: searched open and closed filesystem issues, `QUEUE.md`
and all 275 cached GitHub issue titles and bodies for extension wake
observers and duplicate delivery. The separate
`20261112-review-apple-wake-completes-during-offer.md` concerns synchronous
completion ordering within one offer; fixing it does not remove these
duplicate observers.
