surfaces: terminal
agent: Codex, GPT-6.2 Astra
task: verify a harness session that answers outside its declared shape

### Rough

The harness test expected a bad session answer to stop the runner at boot.
The resource now reports the failure while the app keeps its placeholder.

### Lean in

The test can read the failure code through Contract and the field name through
the runner's diagnostics.

### Needed

None.

### Checkpoints

- Checked the harness data source through the runner.
