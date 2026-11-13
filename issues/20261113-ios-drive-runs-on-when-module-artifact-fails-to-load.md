# `agent ios` drives an app whose module artifact failed to load, and its rebuild drops what the builder added to the bundle

**Status:** Open
**Systems:** scripts/agent-build.mjs (`buildFirst`), scripts/agent.mjs (simulator carrier), host/apple/Sources/ExactKit/NativeModule.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-ios-module-dynamic-framework-not-embedded.md, issues/20261113-ios-app-cannot-declare-entitlements-or-plist-keys.md

## What is wrong

When a drive finds the build older than its sources, an app's `exact.mjs` rebuilds with
`host/apple/build.mjs --ios <crate> --no-install` (`buildArgs`, `scripts/agent-build.mjs:118`)
and the drive installs that bundle. A Swift edit, a change to the design, or a changed setting
in the environment is enough. The rebuilt bundle is Exact's alone: a builder whose app needs a
dynamic framework or an entitlement Exact cannot place (the two related issues) has added them
to the bundle by a script after each build, and the drive's rebuild installs a bundle without
them.

The app then starts with a module artifact that cannot load. `NativeTable.load`
(`host/apple/Sources/ExactKit/NativeModule.swift:185-196`) turns a failed `dlopen` into the state
`unavailable`, and every native call answers that state; screens that wait on those calls come up
empty. The drive does not treat this as a failure of the launch, so the builder sees empty
screens and has to find the cause in the logs.

## The failure

An app ported from React Native, at `94b8671f7`, on the iOS simulator: "`bun exact.mjs agent ios`
after a Swift edit, a tamber change, or a changed environment variable reinstalls a bundle
without the module frameworks: every native call fails with `dlopen … libexact_modules.dylib`
and screens quietly come up empty. Cost several builders many drives. It also empties the app's
data container (lost a recording in progress)."

Not checked: why the data container was emptied. Nothing in the drive's install path at
`49ae6357` uninstalls the app or erases its container; a reinstall whose signature or
entitlements differ from the installed bundle's is a possible cause.

## Fix

Fail the launch of a development drive when the module artifact the app was built with does not
load, quoting the `dlopen` error, rather than driving an app whose every native call answers
`unavailable`. The rebuild losing the builder's additions goes away with the related issues;
until then the refusal should name the frameworks the artifact could not find.
