# Contract content mapper for TypeScript 7.1

Language server support for `.contract` files, through TypeScript 7.1's content
mappers: hover with types, go to definition, references, rename, completion of
enum values, and the compiler's errors. TypeScript's own language server
answers; there is no second server. It is opt-in: nothing in Exact runs it, or
any of what it asks the compiler for, unless an app and an editor turn it on
(LLP 1035.005.003).

Each answer comes from the `contract` binary. `contract symbols --editor` gives the
definitions, references, element positions and types; `contract build --json
--errors-only` gives the errors; `contract vocab --json --all` gives the tags
and attributes; `contract sources` says which app a file belongs to. The mapper writes them as a virtual `.tsx` file with span
mappings back to the contract, and analyses nothing itself.

## Use

Content mappers need TypeScript 7.1, the Go compiler that was `tsgo` in the
previews. It installs as `typescript` and its command is `tsc`:

```sh
bun add --dev typescript@7.1.0-beta
```

TypeScript starts the mapper with `bun`, which must be on the `PATH` of
whatever starts TypeScript (a terminal, or VS Code).

In the app's `tsconfig.json`:

```json
{
  "contentMappers": [
    { "package": "@exact/contract-content-mapper", "extensions": [".contract"], "options": { "contract": "/path/to/contract" } }
  ],
  "include": ["*.contract", "app.ts"]
}
```

`options.contract` is the compiler binary; without it the mapper runs
`EXACT_CONTRACT`, then this checkout's `target/release` or `target/debug`
build, then `contract` on the `PATH`.

TypeScript resolves the package from the app's `node_modules`. From this
checkout, link it there:

```sh
mkdir -p node_modules/@exact
ln -s /path/to/exact/editors/content-mapper node_modules/@exact/contract-content-mapper
```

### Command line

```sh
bunx tsc --noEmit --runExternalCode
```

A mapper is a program the project names, so `tsc` runs it only with
`--runExternalCode`. The compiler's errors print in TypeScript's format:

```
app.contract(25,7): error contract9100: lower-unknown-tag: unknown tag `buton`; did you mean `button`?
```

An editor other than VS Code starts the same server with `bunx tsc --lsp --stdio`
and `runExternalCode: true` in its initialization options.

### VS Code

1. Install the TypeScript 7 extension, `TypeScriptTeam.native-preview`.
2. In the workspace's `.vscode/settings.json`:

   ```json
   {
     "js/ts.experimental.useTsgo": true,
     "js/ts.contentMappers.enabled": true,
     "js/ts.tsdk.path": "node_modules/typescript/lib"
   }
   ```

3. Open a `.contract` file. VS Code asks "This workspace has a TypeScript 7
   tsdk configured (7.1.0-beta). Would you like to use it?", then whether the
   project's content mapper may run. Allow both; hover shows nothing until
   then.
4. Run **TypeScript: Restart TS Server** after changing the mapper or the
   `contract` binary.

The extension bundles TypeScript 7.0.2, which has no content mappers, so
`js/ts.tsdk.path` points it at the app's own 7.1 and VS Code asks to use it.
Once the extension bundles 7.1, that setting and its prompt are not needed.

The TypeScript 7 extension sends a `.contract` file to the server only after an
extension registers `.contract` with it. `editors/vscode` does that, so install
it too (its README says how); without it a contract is served once a TypeScript
file of the same project is open.

## Parts

- `server.js`: the JSON-RPC process TypeScript starts.
- `mapper.js`: one transform, the parts below in turn.
- `names.js`: definitions, references, rename and types.
- `sources.js`: a source call to the function in `app.ts` that answers it.
- `vocab.js`: tags and attributes as JSX against the vocabulary.
- `errors.js`: the compiler's errors as the mapper's diagnostics.

## Test

```sh
cargo build -p contract
EXACT_TSC71=/path/to/typescript-7.1/bin/tsc EXACT_CONTRACT=$PWD/target/debug/contract bun test editors/content-mapper/
```

The tests drive `tsc --lsp` and compare its answers with `contract symbols` on
the repository's apps. They skip when `EXACT_TSC71` is not set.

## Limits

- Completion offers an enum attribute's values and nothing else: not attribute
  names, and nothing while the file has a syntax error.
- A file an app uses is built with that app, the nearest `app.contract` beside or
  above it whose sources include the file: this file as the editor
  holds it (`build --stdin-as`), the app's other files as saved.
- A source is not renamed from a contract: its name is also the key of the
  function in `app.ts` that answers it.
- An import's alias (`use Row as TheirRow`) has no definition or rename.
- Which tags an attribute is allowed on is prose in `contract vocab`, so hover
  shows an attribute on any tag.
- A shape named as a vocabulary type is (`MouseEvent`, `MediaMetadata`) shares
  that name in the generated text, and its hover may be wrong.
- The vocabulary is read once for each compiler binary; restart the TypeScript
  server after rebuilding `contract`.
- The tests need TypeScript 7.1 and skip without `EXACT_TSC71`, so the
  repository's own test run does not exercise them yet.
