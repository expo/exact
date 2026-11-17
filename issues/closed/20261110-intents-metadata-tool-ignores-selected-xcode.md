# The App Intents metadata pass runs from `/Applications/Xcode.app` whichever Xcode the build uses

**Status:** Closed
**Resolution:** The metadata pass takes the processor from `xcrun --find appintentsmetadataprocessor`, which honours `DEVELOPER_DIR` and `xcode-select` as the module's `swiftc` does, and its `--toolchain-dir` from that path.
**Systems:** host/apple/intents.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`buildIntents` compiles the generated intents module with the selected Xcode (`xcrun --sdk <sdk> swiftc`, `host/apple/intents.mjs:218`) and then runs the metadata processor from a fixed path (`:17`, `:226`):

```js
const toolchain = '/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain';
…
runApple(resolve(toolchain, 'usr/bin/appintentsmetadataprocessor'), ['--toolchain-dir', toolchain, '--module-name', 'ExactIntents', '--sdk-root', sdk,
  '--xcode-version', xcode, …
```

Every other tool of the build is found through `xcrun`, which honours `DEVELOPER_DIR` and `xcode-select` (`useXcode` in `host/apple/devices.mjs:20-28` keeps an explicit `DEVELOPER_DIR`). This one is not. On a Mac whose build Xcode is not at `/Applications/Xcode.app` (a beta beside the release, a versioned `Xcode_27.1.app` on a build service), the pass either does not start or runs another Xcode's processor against the selected Xcode's SDK (`--sdk-root`), its version (`--xcode-version`) and its compiler's `.swiftconstvalues`.

## The failure

An app whose plan has a tool with an Apple face (`compat.inputs.faces`, a door named `apple`), built with:

```
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer bun host/apple/build.mjs --ios <app>-apple
```

Observed, with no `/Applications/Xcode.app`: the build fails when it spawns `/Applications/Xcode.app/…/appintentsmetadataprocessor`, after the intents module has compiled. With both installed: the release Xcode's processor reads constants the beta's compiler wrote. Expected: the processor and `--toolchain-dir` of the Xcode the build compiles with, as `xcrun --find appintentsmetadataprocessor` names it.

## Verification

By reading. It was not run: the Mac used here has one Xcode, at the default path, where the constant and `xcrun` agree:

```
$ xcode-select -p
/Applications/Xcode.app/Contents/Developer
$ xcrun --find appintentsmetadataprocessor
/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/appintentsmetadataprocessor
```

The constant has no other use and is not checked against `DEVELOPER_DIR` anywhere.

## Direction

Take the processor from `xcrun --find appintentsmetadataprocessor` and the toolchain directory from that path (three levels up).

## Related

No issue or QUEUE entry names `appintentsmetadataprocessor`. `useXcode` also names `/Applications/Xcode.app`, but only as the fallback when `xcode-select` points at the Command Line Tools and no `DEVELOPER_DIR` is set.
