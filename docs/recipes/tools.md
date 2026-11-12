# Recipe: tools an agent or the system calls

A `tool` line offers a root action to callers other than the person: Siri and Shortcuts (App
Intents) and a browser's agent ([WebMCP](https://webmachinelearning.github.io/webmcp/)). Only a
tool with a face in `app.json` is offered to them. Android's door (AppFunctions) is not built
yet, so `android` in `doors` reaches no caller.

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
    "name": { "title": "Name", "description": "What to buy." }, "aisle": { "title": "Aisle" },
    "quantity": { "title": "Quantity", "type": "integer" } } },
  "clearList": { "title": "Clear List" } } } }
```

- **After the name:** `reads`, `writes` or `destroys`; then `additive` (after `writes`: adds,
  never overwrites), `idempotent` (a repeat changes nothing more), `closed` (touches only
  the app's data). The caller reads the description and passes the action's parameters.
- **Clauses**, in this order: `returns` (a faced tool's is a string, number or bool, else
  `intents-face-type`), `fails when` (true after the action: `outcome` is `failed`),
  `available when` (while false a call is refused, `reason: unavailable`, and the web
  withdraws the tool). An `enum` value outside the list is refused (`reason: arguments`).
- **The face:** a parameter's `title` and `description` are what callers show; its `entity`
  names a kind of `intents.entities`. `doors` defaults to `apple`, `android` and `web`; a
  `destroys` tool's is `apple` alone.
- **On the web** a page whose route says no `activate=` registers its tools with
  `document.modelContext` after load; `activate=interaction` waits
  ([pitfall](../agent-pitfalls.md#websites)). It needs `https`, `localhost` or `127.0.0.1`,
  and in Chrome `--enable-features=WebMCPTesting`. Hints and schemas:
  [Tools](../contract-for-agents.md#tools-what-a-call-answers).
- **Drive:** `bun exact.mjs agent web 'call addItem {"name":"Milk","aisle":"dairy","quantity":2}'
  state`; `state.intents` lists the tools and each call's `outcome`. `call` is not a test step.
