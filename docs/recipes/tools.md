# Recipe: tools an agent or the system calls

A `tool` line offers a root action to callers other than the person: Siri and Shortcuts (App
Intents) and a browser's agent ([WebMCP](https://webmachinelearning.github.io/webmcp/)), if it
has a face in `app.json`.

```contract
routes nav
  home "/" render=build

component App
  state items: string[] = []
  state lastFailed = false
  tool listItems reads "The shopping list, one item per line." = listItems
    returns items.join("\n")
  tool addItem writes additive closed "Add an item. Fails for an empty name." = addItem
    returns `${items.length} on the list`
    fails when lastFailed
    available when items.length < 20
  tool clearList destroys "Remove every item." = clearList
    available when items.length > 0
  action listItems
  action addItem(name: string, aisle: enum<"produce", "dairy", "other">, quantity: number)
    let refused = name.trim() == ""
    lastFailed = refused
    if not refused
      items = items.concat([`${quantity} × ${name.trim()} (${aisle})`])
  action clearList
    items = []
  view
    main
      text "Shopping" role="heading" aria-level=1
      for i in items key=i
        text i
```

```json
{ "intents": { "tools": {
  "listItems": { "title": "List Items" },
  "addItem": { "title": "Add Item", "parameters": {
    "name": { "title": "Name" }, "aisle": { "title": "Aisle" },
    "quantity": { "title": "Quantity", "type": "integer" } } },
  "clearList": { "title": "Clear List" } } } }
```

- **After the name:** `reads`, `writes` or `destroys`; then `additive` (after `writes`: adds,
  never overwrites), `idempotent` (a repeat changes nothing), `closed` (touches only
  the app's data).
- **Clauses**, in this order: `returns` (a faced tool's is a string, number or bool, else
  `intents-face-type`), `fails when` (true after the action: `outcome: failed`), `available when`
  (false: a call is refused, `reason: unavailable`; WebMCP also withdraws the tool, an App
  Intent does not). An `enum` outside its list: `reason: arguments`.
- **The face:** a parameter's `entity` names a kind of `intents.entities`. `doors` defaults
  to `apple`, `android` (AppFunctions: not built yet) and `web`; a `destroys` tool's is
  `apple` alone.
- **On the web** a page registers its faced tools with `document.modelContext` once its script
  runs. A route with no `activate=` that would otherwise wait for the reader is `idle` in an app
  with a faced tool on the web door, so the tools are registered after load; a route that says
  `activate=interaction` registers them only when its script starts (the pitfall's triggers).
  `document.modelContext` exists only in a secure context: `https`, `localhost` or `127.0.0.1`,
  not a LAN address over `http`. Chrome: `chrome://flags/#enable-webmcp-testing`, or
  `--enable-features=WebMCPTesting`; without it the page runs as before.
- **Drive:** `bun exact.mjs agent web 'call addItem {"name":"Milk","aisle":"dairy","quantity":2}'
  state`; `state.intents` lists the tools and each call's `outcome`. `call` is not a test step.
