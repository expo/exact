# Which packages an app uses is read from its raw text, so strings and comments add and drop packages

**Status:** Closed
**Resolution:** The bake reads `app.ts`'s imports from its syntax tree (Oxc, through `rolldown/utils`) and the compiler from JavaScript tokens, so a string, template, regular expression or comment neither adds nor hides a package; the text search for an interface's use (`usesInterface`) was already gone, replaced by the compiled plan's `extensions` in 5659b182.
**Systems:** contract/cli/src/sources/data.rs (the data-code import walk), js/bake/src/typescript.mjs (`usedPackages`, `usesInterface`, and their callers `appleImplementations`, `androidImplementations`, `packageConfig`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Grok 4.7's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

Two questions about an app are answered by scanning source text rather than parsing it.

**What `app.ts` imports.** The compiler's walk (`imported`, `contract/cli/src/sources/data.rs:73`)
puts each package it finds in the plan as a used module. The bake's walk (`usedPackages`,
`js/bake/src/typescript.mjs:1096`) picks the native implementations to link. Both first delete
comments without reading strings: from `/*` to the next `*/`, and from `//` to the end of the
line unless `//` follows `:`, a quote or a backslash (`data.rs:132-163`, `typescript.mjs:1108`).
Both then take any `import … from "x"` they see, inside a string or not (`data.rs:201-242`;
the regex at `typescript.mjs:1110`). So:

- A string holding `/*` before an import and one holding `*/` after it delete the import.
  Neither walk sees the package; it is not in the plan, so its rows are missing, and its
  native implementation is not linked.
- A string holding `a//b` deletes the rest of its line, with any import after it there.
- A string holding `import { takePhoto } from "@exact/camera"` adds `@exact/camera` to the
  plan and links its implementation.

**Which interfaces the app uses.** `appleImplementations`, `androidImplementations` and
`packageConfig` keep an installed interface when `usedPackages` names it or `usesInterface`
is true (`typescript.mjs:1129-1130`, `:1168-1169`, `:1181-1185`). `usesInterface` (`:1159`)
is true when the raw text of the walked files, `app.ts` and every `.contract` file under the
app (`appText`, `:1085`), contains the package name in quotes, or any of its view tags as a
word, comments included. A comment that says `exact-camera` links `@exact/camera`'s
`Camera.swift`, `Viewfinder.swift` and its Android Maven artifacts. A comment that says
`exact-map` makes `packageConfig` require `@exact/maps`'s configuration, and the web build
stops with `maps-tiles-missing` (`host/web-js/build.mjs:269`). Inside this repository every
`@exact` package is installed in the root `node_modules`, so every app sees all 59.

## To see it

Confirmed by running the bake's functions in Bun against scratch apps (the compiler's walk
by reading code; it follows the same rules):

- `app.ts` of `const note = "/*";`, `import { rows } from "@exact/ledger";`,
  `const end = "*/";`: `usedPackages(app).names` is empty.
- `app.ts` of `const hint = 'import { takePhoto } from "@exact/camera"';` and
  `const u = "a//b"; import { x } from "@exact/real";`: the names are `@exact/camera` alone.
- `app.ts` of `// a plain list, not the exact-camera view`, with the repository's
  `node_modules` above it: `appleImplementations(app, 'ios')` returns camera's two Swift files
  and `androidImplementations(app)` its three Maven artifacts. With `// a list, not the
  exact-map view` instead, `packageConfig(app, 'web')` throws `maps-tiles-missing`.

## What a fix is

Read the import specifiers with a scanner that skips strings, template literals and comments
as JavaScript does (ECMA-262 §12), and have both walks agree on it; the two are meant to
match (`data.rs:1-6`), and today they share each other's errors. Decide an interface's use
from that import set,
from Contract `use` lines, and from the view tags the compiler lowers, rather than by searching
text. Test each case above.
