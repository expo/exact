surfaces: web
agent: Codex
task: align web grant admission and migrate the notebook browser drive

## Rough

The browser drive passed a grant string to SQLite after the adapter switched
to normalized grant sets. Its Unicode-note setup failed with a grant denial.
Both setup calls now use the Rust parser's normalized SQLite grant.

## Lean in

The same drive covers the worker client, Rust backup, generation reload and
1,000 maximum-size notes through the shipped browser storage service.

## Needed

- storage: provided; browser adapters consume the shared parser's grant sets.

## Checkpoints

- browser drive: passes save, Rust backup, restore, generation reload and Unicode
  search, including backups during scrolling at normal and fourfold CPU throttle.
