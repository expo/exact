# An iOS app's module links a dynamic framework from modules/apple/*.xcframework but the bundle does not carry it

**Status:** Closed
**Resolution:** Already fixed by 2bf3a9b89, which embeds and signs each dynamic slice of `modules/apple/*.xcframework` under the bundle's `Frameworks/` with the run path, and gives roster probes the slices' directories; `scripts/apple-frameworks.test.mjs` covers iOS and tvOS simulator and device, macOS, development and production.
**Systems:** host/apple/build.mjs (`frameworkSlice`, module arms, roster probe), host/apple/assets.mjs, scripts/app.schema.json
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20260928-module-artifact-cannot-link-libraries.md

## What is wrong

An app's `modules/apple/*.xcframework` is linked into the module artifact
(`libexact_modules.dylib`). For a slice that is a framework, `frameworkSlice`
(`host/apple/build.mjs:1101-1114`) gives the linker `-F <slice> -framework <Name>` and nothing
else: the framework is not copied into the bundle's `Frameworks/`, not signed, and no
`@executable_path/Frameworks` run path is added. At launch, `dlopen` of the module artifact
fails on the missing framework (`NativeTable.load`,
`host/apple/Sources/ExactKit/NativeModule.swift:189-190`) and every native call answers
`unavailable`.

Packages get this right: `embedLibrary` (`scripts/package-libraries.mjs:328-333`) places a
package's framework under `Frameworks/` with the run path. And macOS has a way for the app's
own: `host.macos.resources` may place a tree under `Frameworks/`, whose Mach-O the build signs
(`host/apple/assets.mjs:690-693`, `:459-465`). iOS has neither: `host.ios.resources` takes only
`Resources/` and refuses Mach-O code (`assets.mjs:690`, `scripts/app.schema.json:747`), and no
other `app.json` key places a file under an iOS bundle's `Frameworks/`. So an iOS
app that needs a third-party dynamic framework copies it into the built bundle and re-signs
after every build.

## The failure

Two builders on macOS at `9e53578f2` and `94b8671f7` (an app ported from React Native):

- "We had to copy three items into `<bundle>/Frameworks` after `ios --device --no-install` and
  codesign again, after every build." On macOS `host.macos.resources` did the same job for the
  same libraries.
- "A module's dynamic framework is linked but not embedded: every build needs a script that
  copies it into the bundle and re-signs (simulator and device). `--release` fails the same way
  in its roster probe." The roster probe (`host/apple/build.mjs:1176-1184`) loads the built
  module, which cannot find the framework; this was not reproduced here.

## Fix

Embed and sign each framework slice of `modules/apple/*.xcframework` on both hosts, with the run
path, as `embedLibrary` does for a package's, and point the roster probe at the same
`Frameworks/` (the reporter's first ask). Failing that, let `host.ios.resources` target `Frameworks/`
and hold Mach-O, signed as macOS's is.
