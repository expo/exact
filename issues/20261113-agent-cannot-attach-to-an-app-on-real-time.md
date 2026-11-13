# The agent cannot attach to an app on real time

**Status:** Open
**Systems:** scripts/agent.mjs, scripts/agent-launch.mjs, host/apple/Sources/ExactKit/Agent.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261022-agent-cannot-drag-to-dismiss-the-keyboard-or-film-real-time.md

## What is wrong

The driver launches the app it drives and holds its clock from the first frame. It cannot
attach to an app that is already running on the platform's clock. Timing work, such as the
time from a server push to the frame that shows it, needs the app on real time from its
launch, so the driver cannot be used for it.

`--timing platform` lets the platform keep the clock, and since aead4f85 an input lands
inside a live frame capture (`perf frames capture start`, then `perf frames health during
"<op>"`; `docs/contract-for-agents.md`, the frame health section). But the session is still
one the driver launched, with its storage and secret presence substituted
(`docs/reference.md:1426-1436`). Only `trace --pid` reaches a running app
(`scripts/agent-launch.mjs:591`), and it reads a trace, not `tree` or `state`.

## The failure

Exact `9e53578f`, macOS: a builder measuring push-to-frame latency asked for the driver to
attach to an app running on real time and read `tree` and `state` without holding its clock.
They steered the app through a side channel of their own and read its state through requests
it made.

## Fix

A read-only attach (`tree`, `state`, `layout`, `logs`, `perf frames live`) to a running
development build that leaves its clock alone, reached as `trace --pid` reaches one.

## Decision needed

The existing live carrier requires launch-time opt-in; a normal development
process exposes no carrier for these reads. Read-only attachment requires a
decision about how that process admits a client, selects a session and limits
operations (LLP 1012 §8; LLP 1142 D8). Frame capture itself is no longer the
reported defect. This issue remains open for attachment.
