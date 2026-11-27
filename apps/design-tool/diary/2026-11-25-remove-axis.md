surfaces: web
agent: Codex (GPT-6.2-Astra)
task: show an axis removal in a held proposal

### Rough

Removing a referenced value reported its name and line but omitted the token
that held the condition. Removing the last value produced a syntax error.

### Lean in

The held proposal already carries the operation. Its label can use the axis
name directly.

### Needed

Forma now reports the owners and conditions that prevent an axis removal.

### Checkpoints

The standalone server and web loop share the transaction and undo path.
