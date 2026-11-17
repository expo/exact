# Recipe: data the app ships

Data the app ships and only reads (a catalogue, a word list, a city table) is a JSON file
`app.ts` imports: it is bundled with the data module on every host, with no grant and no
path to get wrong.

```contract
shape City
  id: string
  name: string
  country: string

component Cities
  resource cities = loadCities() as shape City[] else empty()
  view
    list appearance="none"
      for c in cities key=c.id
        row gap=8 padding="8px 16px"
          text c.name testId=`city-${c.id}`
          text c.country color="-exact-secondary-label"
```

`cities.json`, beside `app.ts`:

```json cities.json
[{ "id": "1", "name": "Lyon", "country": "France" }, { "id": "2", "name": "Osaka", "country": "Japan" }]
```

```ts
import type { Answer, Sources } from './app.contract.d.ts';
import cities from './cities.json';

export const grants = '';

const sources: Sources = { loadCities: () => cities };
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
```

```contract-test
test "the shipped cities show"
  clock data
  expect text "city-1" == "Lyon"
  expect text "city-2" == "Osaka"
```

- An edit to the JSON ships as an update, as `app.ts` does; nothing to copy at build.
- A file read by path at run time (large, binary, or opened by a native module) is
  app.json's `host.ios.resources` (and `.macos`), read with `fs.read bundle:/Resources/…`
  ([files and grants](files-and-grants.md)). Its `from` is a directory of its own
  (`data-files/`), never one Exact owns: `assets/` `deck/` `gpu/` `fonts/` `strings/`
  `web/` `apple/` `linux/` `src/` `data/` `modules/` `node_modules/` `target/` `dist/`.
