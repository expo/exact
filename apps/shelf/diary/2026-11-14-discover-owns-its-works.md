surfaces: web, macos, linux
agent: Claude
task: move Discover's subject and works out of the root (LLP 1035.005.004, #476)

### Rough
Discover's state, its resource and the two actions that change them lived in
the root, prefixed by nothing and gated on `nav.tab == "discover"` there,
because a child could not declare a resource. The `always` property on
`subject` named root state by name; with `subject` in Discover there is no
root name for it, and the property is gone.

### Lean in
`Discover` declares `subject`, `works`, `chooseSubject` and `reload` itself.
The root passes it `shown=(nav.tab == t.name)`, the library's `owned` keys
and `addWork` as `add`; `works` asks only while its tab shows and keeps its
answer while another tab does. `refresh works` reaches only this instance.

### Needed
- a child's own requests: provided by exact2 in this change

### Checkpoints
- `expect perf query works` counts 0 before the first Discover tap, 1 on it,
  0 while Library shows and 1 when Discover shows again.
