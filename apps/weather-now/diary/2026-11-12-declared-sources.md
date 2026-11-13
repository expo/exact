surfaces: web, macos
agent: Claude
task: a weather screen that reads Open-Meteo with no data module (expo/exact#359)

### Rough

A screen that only shows JSON from one endpoint needed an `app.ts`: a grant, a `fetch`, a status check and a
mapping into the shape, and with it Hermes in the native build. exact2 now has sources declared in Contract
(LLP 1162.001), and this app is their first consumer: both of its reads are declared, so it has no `app.ts`.

The JSON's keys are the shape's field names (`temperature_2m`), since a declared answer is read field by field
and renaming is a `derive`'s. A search with no match leaves `results` out of the reply, so `Found` keeps it
`option<Place[]>` and the view reads `none` as an empty list.

### Lean in

`when query.trim().length >= 2` keeps the search from asking for one letter; `current(found)` tells "no places"
from "still asking". `failed(now)` and `refresh now` are the whole of the error and retry.

### Needed

- a read from an HTTP endpoint with no data module: provided (declared sources)
- an origin from `app.json`'s `env`: not provided; this app names Open-Meteo's origins literally

### Checkpoints

- web (JS target): smooth; the forecast and a search against Open-Meteo itself, and `app.test.contract`
- macOS: smooth; the same drive and test (an ad-hoc signature, `EXACT_IDENTITY=-`, since the keychain
  refused this machine's identity)
- macOS release, against the same app with an `app.ts` answering both sources: the executable is
  18,174,192 bytes to 21,278,256 (3.1 MB, 14.6% smaller), the `.app` 19.2 MB to 22.5 MB, and its strings
  name Hermes 0 times to 136
