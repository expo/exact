surfaces: web
agent: Codex
task: use the declared Rust backup source on the JS target

## Rough

The JS builder recognized a Rust source passed to `contract::bake`, but missed
the Rust source passed to `build_mixed_with`. Fieldnotes silently backed up
through its TypeScript implementation. Both build paths now use the same source
selection before generating the Rust ABI module.

## Lean in

The existing module adapter carries Rust storage requests and leaves the other
sources with TypeScript. The app needs no new module crate or manifest setting.

## Needed

- storage: provided; Rust keeps its own grants while sharing the notebook and
  revision with TypeScript.

## Checkpoints

- web build: smooth; the generated module exports Fieldnotes' Rust backup source.
- driving the app: smooth; backup, restore, grant refusal and recovery after a
  storage error pass on the JS target.
