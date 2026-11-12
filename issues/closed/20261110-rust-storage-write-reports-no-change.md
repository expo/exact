# A Rust source's storage write reports no storage change, so a widget is not reloaded after it

**Status:** Closed
**Resolution:** Rust storage jobs notify once for successful changes to app-readable files, including a sibling source's read grants, while execution retains the writer's grants.
**Systems:** data/host/src/lib.rs (`exact_data_host::Storage`), data/host/src/native.rs, widgets, language parity (LLP 1027.001)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`DataSource::take_storage_changed` answers whether there were "successful storage effects since the last host commit" (`runner/src/runner/source.rs:330`). The Apple host posts the widget-store change from it after each commit (`host/apple/src/persistence.rs:8`), which is how a widget reloads after the app writes its files.

For a TypeScript source the module counts its own writes (`js/src/background.rs:51`). A Rust source's writes are run by `Storage`: it takes the source's portable storage request, runs it on a worker (`data/host/src/lib.rs:468`, `data/host/src/native.rs:549`) and hands back the outcome. `Storage::take_storage_changed` only asks the wrapped source (`data/host/src/lib.rs:163`), which made no write itself and answers false. Nothing records that the executor wrote (at `dd6f5dcd`).

LLP 1027.001 §1 asks that an operation moved between the two languages keep its observable behaviour. Here the same `fs.writeFile` reloads a widget when TypeScript makes it and not when Rust does.

No app in `apps/` or `examples/` has both a widget and a Rust source behind `Storage`, so none shows it today.

## The failure

A source whose answer is `storage::request("fs.writeFile", json!({"path":"app:/data/note","text":"hello"}))`, with `fs.read app:/data` and `fs.write app:/data` granted, under a configured and activated `Storage`:

```rust
let Answer::Later(request) = storage.answer(&mut store, "write", &[]).unwrap() else { panic!() };
let Dispatch::Run(Work::Now(work)) = storage.dispatch(request.continuation.unwrap(), &store) else { panic!() };
storage.parse(&mut store, "write", &[], work()).unwrap();
storage.take_storage_changed()
```

Observed: the file holds `hello` and `take_storage_changed()` is false. Expected: true.

## Verification

Ran it as a test in `data/host/src` (`cargo test -p exact-data-host --lib`): the file is written and the assertion on `take_storage_changed()` fails. No app with a widget was built.

## Direction

Have the storage job set a shared flag when a writing operation succeeds, as `Placed` shares one with its owner thread (`data/src/placed.rs:256`), and have `Storage::take_storage_changed` take it beside the source's.

## Related

`issues/20261110-wrote-reports-no-storage-change.md` is a native module's route to the same flag. `issues/closed/20261021-widget-gallery-reads-no-storage.md` and `issues/closed/20261019-widget-refresh-native-hook-stale-layout-scale.md` concern TypeScript and native-module writes; neither covers a Rust source.
