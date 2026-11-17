# The plist writer leaves dictionary keys unescaped and throws a bare TypeError on `null`

**Status:** Closed
**Resolution:** The plist writer escapes keys as it escapes strings and refuses null, a non-finite number or an unsafe integer by its key path, and an Apple build refuses such a `moduleConfig` value by its `app.json` path before it compiles anything.
**Systems:** host/apple/assets.mjs (`plistFile`), host/apple/build.mjs (`infoPlist`, `macInfoPlist`, `modulePlist`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`plist` (`host/apple/assets.mjs:546-552`) escapes `&` and `<` in string values and nothing in keys:

```js
return `<dict>\n${Object.entries(value).map(([k, v]) => `${indent}<key>${k}</key>${plist(v, indent + '  ')}`)…
```

It has no case for `null`: `typeof null` is `'object'`, so `null` falls through to `Object.entries(null)`. And an integer too large for JavaScript's plain notation is written as `<integer>1e+21</integer>`.

Most dictionaries it writes have fixed keys. Two take the author's: `ExactModuleConfig`, which is `app.json`'s `moduleConfig` as written (`host/apple/build.mjs:433`, `:556`; the schema, `scripts/app.schema.json:25-31`, asks only that each module's settings be an object), and a module's declared Info.plist dictionary, keyed by the app's strings keys (`modulePlist`, `build.mjs:405-414`).

## The failure

```json
"moduleConfig": { "sentry": { "dsn": null } }
```

Observed: after the Rust and Swift have compiled, the build ends with `Object.entries requires that input parameter not be null or undefined`, naming neither the manifest nor the key.

```json
"moduleConfig": { "analytics": { "q&a": "on" } }
```

Observed: the Mac build exits 0, and the bundle's `Info.plist` is not a property list. Expected in both cases: the key written escaped, and a value a property list cannot hold refused by name (`moduleConfig.sentry.dsn`).

## Verification

Run, with each line added to `apps/hello/app.json` and `bun host/apple/build.mjs hello-apple --bundle`:

```
"dsn": null      Object.entries requires that input parameter not be null or undefined      exit 1
"q&a": "on"      exit 0; plutil -lint Hello.app/Contents/Info.plist:
                 (Encountered unknown ampersand-escape sequence at line 14)
```

And `plistFile(dict)` called directly, each result checked with `plutil -lint`:

```
a key with &:        (Encountered unknown ampersand-escape sequence at line 6)
a key with <:        (Encountered unexpected character b on line 6 while looking for close tag)
a value of 1e21:     (Hex digit in non-hex <integer> on line 7)
```

String values with `]]>` or a control character lint clean and read back unchanged. Not run: an iOS build with such a manifest.

## Direction

Escape keys as values are escaped, and throw an error that names the key path for `null`, `undefined` and a number that is not finite or not representable. `readManifest` can refuse `null` in `moduleConfig` before any build step.

## Related

No issue or QUEUE entry names `plistFile` or `moduleConfig`.
