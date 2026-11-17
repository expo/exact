# A phone drive that fails on Apple's local-network prompt or a locked iPhone does not say so

**Status:** Open
**Systems:** host/apple/devices.mjs (`phoneBridge`), host/apple/Sources/ExactKit/AgentSocketStartup.swift, scripts/agent-stdio.mjs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`agent ios --device` starts a TCP listener on the Mac's first non-internal IPv4 address
(`phoneBridge`, `devices.mjs:605`) and launches the app with that address in
`EXACT_AGENT_CONNECT`; the app dials out to it (`AgentSocketStartup`). Reaching a LAN
address is local-network access, so the first launch of a bundle id on a phone raises
Apple's local-network prompt. Two things follow, and neither failure names the cause:

- While the prompt is unanswered the dial fails, and the launch ends with
  `agent startup: deadline expired before handshake: No route to host`. While the prompt is
  up, every other app's driver launch on that phone fails with `deadline expired before
  handshake (waiting for foreground)` (`AgentSocketStartup.swift:17`): that text means
  the app never became active, which the prompt in front of it explains. The prompt stays on the home screen after the app is
  closed, and even after it is uninstalled. The driver cannot answer it.
- Against a locked iPhone, a run can print only `the app never became ready; ` and exit 1
  (`agent-stdio.mjs:122`: the message joins `hostLines`, which are empty here). A second run
  printed devicectl's `FBSOpenApplication` "Locked" error, so the reason is sometimes there
  to read.

`docs/reference.md:3511` says "allow local networking, and keep the app visible", but
not that the first launch raises a prompt the driver cannot answer, or that `No route to
host` and `waiting for foreground` are how it shows.

## The failure

On Exact `9e53578f`, a real iPhone, `agent ios --device`: the first launch of an app with a
new bundle id raised the local-network prompt and failed `No route to host`; later launches
of other apps failed `(waiting for foreground)` until the prompt was answered on the phone.
Separately, against a locked phone, one run with several ops printed only `the app never
became ready; `.

## Fix

- Name the likely cause in both startup failures: for `EHOSTUNREACH` and for a deadline
  spent waiting for the foreground on a phone, say that Apple's local-network prompt may be
  up on the phone and must be allowed there.
- Before launching, ask devicectl whether the phone is locked (or read the launch's own
  error), and fail with "the iPhone is locked: unlock it and run again" instead of an empty
  `never became ready`. When `hostLines` is empty, the message should say that the app
  printed nothing.
- Longer term, as the reporter asked: a carrier that needs no local-network access, such as
  a connection over the paired device's tunnel, or a documented first step that clears the
  prompt once per bundle id.

## Landed; device verification owed

The driver now preserves launch diagnostics, identifies an explicitly reported
device lock, gives qualified Local Network guidance for `No route to host` and
`waiting for foreground`, and says when the app printed nothing. The reference
documents the first-launch prompt and the need to answer it on the phone.
Diagnostic tests cover these cases. A signed iPhone run with an unanswered prompt
and a locked screen is still owed; this Mac lane has no signed phone.
