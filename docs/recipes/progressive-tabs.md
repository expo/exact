# Recipe: keep rows on a warm tab return

Open two terminals at the repo root. Start the server in A.

```sh
bun docs/recipes/progressive-tabs/server.mjs
```

In B use an iPhone UDID from `xcrun simctl list devices available` for `YOUR_UDID`.

```sh
app=docs/recipes/progressive-tabs
sim=YOUR_UDID
bun host/apple/build.mjs "$app" --ios --sim "$sim" --run
```

When B returns to the prompt, replace `PID` with the number after `dev.exact.progressivetabs:`.

```sh
pid=PID
out="$app/.exact"
save() {
  bun scripts/agent.mjs trace --pid "$pid" --app "$app" --plan "$app/target/bake" --out "$out/$1.json"
}
```

Close other apps using this server. When A prints `Held` with a path, type its reply line in A and press Return.

1. Tap **Explore** (magnifying glass). Reply `/rows/Stocks 200`. “Stocks rows” appears while history is held. Reply `/history/Stocks 200` for “Stocks history”.
2. Tap **Home** (house), then **Explore**. Both sections stay visible. When A holds both GETs again, run `save warm` in B. The keyed [entry](progressive-tabs/app.contract) kept its answers while `when shown` was closed. `current(rows)` is false on return. Using it as the guard shows “Loading rows…”.
3. Reply `/rows/Stocks 503`. When the rows error appears, run `save failed` in B. Reply `/history/Stocks 503`. Both sections keep their data and show errors.
4. Tap **Switch filter**. Lend shows “Loading rows…” and no history. Reply `/rows/Lend 200` for “Lend rows”, then `/history/Lend 200` for “Lend history”.

Each resource keeps one answer. Contract cannot read its arguments, so [Rust](progressive-tabs/data/src/lib.rs) returns the requested filter and the view tests `rows.filter == filter`. Nonempty filters exclude the empty answer. Include account or window in the call, answer and guard if they scope the data. `Cache-Control: no-store` prevents HTTP caching here. Offline rows need app storage.

## Compare native GETs

Relaunch in B and set `pid` to the new PID it prints.

```sh
xcrun simctl launch --terminate-running-process "$sim" dev.exact.progressivetabs
```

Tap **Use ordered history**, then **Explore**. Reply `/rows/Stocks 200`. Rows stay pending. Reply `/history/Stocks 503` to release them. Run `save ordered` in B. History is declared first. Ordinary reads overlap and settle in request order. In step 1 `request.independent_http(4096)` lets history’s GET and parser reorder. It caps the reply at 4 KiB (range 1 byte–64 MiB). An oversized reply fails and keeps the last answer. Keep writes and dependent reads ordered.

## Read the traces in B

```sh
jq '.state | {epoch,pending,instances,tab:.slots.nav.tab}' "$out/warm.json"
jq -r '.journal.lines[]' "$out/failed.json" "$out/ordered.json"
jq -c '.frames.records[] | select(.seq) | {t,seq,apply,commit}' "$out/failed.json"
```

Match press and failure epochs to frame `seq` ranges. iOS samples the display link. Touch delivery, wire arrival and physical presentation are unmarked. [Worked trace, phone, Linux and Chromium](../contract-for-agents.md#progressive-tab-traces)
