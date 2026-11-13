# Queued file writes read a reused buffer after the call

**Status:** Closed
**Resolution:** A data module's `writeFile`, `atomicWriteFile` and `appendFile` copy their bytes at the call (js/src/storage.js), so a write queued behind another operation writes what it was called with however the app reuses its buffer.
**Systems:** js/src/prelude.js
**Severity:** P2
**Author:** Codex, repository review
**Date:** 2026-11-12

## Failure

At `f6be99aff76bc8bd881b0ec8daaa2c10ace0b9a2`, `storageCall()` copies the argument list but retains its objects (`js/src/prelude.js:852–856`). When another storage operation is in flight, the native binding does not read a write's byte buffer until that operation finishes. Reusing the buffer after calling `writeFile()` can therefore change the file being written. The same call reads the original bytes when the queue is empty.

The file methods forward their arguments to this queue without copying the data (`js/src/prelude.js:937–938`). `atomicWriteFile()` and `appendFile()` use the same path. This can corrupt ordinary code that submits successive writes from a reusable buffer, even to different files.

The browser filesystem explicitly preserves call-time bytes before waiting for its lock (`host/web/storage-fs.js:384–388`). The native prelude also snapshots transaction parameters at the call (`js/src/prelude.js:916–924`), but does not do so for these file writes.

## Reproduction

This valid data module submits one byte, then reuses its buffer. The `queued` case first starts an unrelated directory creation:

```js
function probe(source, args, store, storage) {
 var first=args[0] === "queued" ? storage.fs.mkdir("app:/data/ready") : Promise.resolve();
 var bytes=new Uint8Array([65]);
 var write=storage.fs.writeFile("app:/data/value",bytes);
 bytes[0]=66;
 return Promise.all([first,write]).then(function(){
  return storage.fs.readFile("app:/data/value");
 }).then(function(value){return JSON.stringify(Array.from(new Uint8Array(value)));});
}
globalThis.exact={abi:1,appId:"review.storage",grants:"fs.read app:/data\nfs.write app:/data\n",answer:probe};
```

Save it as `/tmp/exact-review-20261112/queued-storage-bytes/app.js` and compile it with the pinned Hermes compiler:

```sh
/Users/ccheever/.cargo/hermes-lean-sys/hermes-vanilla-d412d3bd8512-v5/bc71c7b398f1d41a5390761b75ce72e7be234aa48cb1ddec947224188ed6a4f2/bin/hermesc \
  -O -emit-async-break-check -Xes6-block-scoping -emit-binary \
  -out /tmp/exact-review-20261112/queued-storage-bytes/app.hbc \
  /tmp/exact-review-20261112/queued-storage-bytes/app.js
```

Save this caller as `/tmp/exact-review-20261112/queued-storage-bytes/main.rs`. It configures ordinary app storage, services the module's continuations, and reads the resulting file directly:

```rust
use exact_js::Module;
use exact_plan::Value;
use exact_runner::{Answer,DataSource,Store};
fn main(){
 let bytes=std::fs::read("/tmp/exact-review-20261112/queued-storage-bytes/app.hbc").unwrap();
 let grants="fs.read app:/data\nfs.write app:/data\n";
 let mut m=Module::new(bytes,"review.storage",grants);
 let dir=std::path::PathBuf::from(format!("/tmp/exact-review-20261112/queued-storage-bytes/storage-{}",std::process::id()));
 m.configure_storage(dir.join("data"),dir.join("cache"),dir.join("tmp")).unwrap();
 m.set_budget_ms(f64::INFINITY);
 m.bind(&contract::compile("component App\n  resource probe = probe(\"direct\") as shape string\n  view\n    text probe\n").unwrap());
 m.load().unwrap();
 let mut s=Store::new(grants,vec![]);
 for mode in ["direct","queued"] {
 let args=[Value::str(mode)];
 let mut a=m.answer(&mut s,"probe",&args).unwrap();
 for _ in 0..10 {
  match a {
   Answer::Now(v)=>{println!("{mode}: reply={}; written={:?}",v.as_str().unwrap(), std::fs::read(dir.join("data/value")).unwrap());break},
   Answer::Later(r)=>{let outcome=m.continuation(r.continuation.unwrap()).unwrap()();a=m.parse(&mut s,"probe",&args,outcome).unwrap();}
  }
 }
 }
}
```

Run it against the existing review artifacts:

```sh
rustc --edition=2021 /tmp/exact-review-20261112/queued-storage-bytes/main.rs \
  -L dependency=target/debug/deps \
  --extern exact_js=target/debug/deps/libexact_js-13cc590fe137e0bc.rlib \
  --extern exact_runner=target/debug/deps/libexact_runner-faa2c5b2aa381b98.rlib \
  --extern exact_plan=target/debug/deps/libexact_plan-7f4ed158b855da05.rlib \
  --extern contract=target/debug/deps/libcontract-6c4999c7ae79fb84.rlib \
  -o /tmp/exact-review-20261112/queued-storage-bytes/repro
/tmp/exact-review-20261112/queued-storage-bytes/repro
```

Actual Hermes executor output:

```text
direct: reply=[65]; written=[65]
queued: reply=[66]; written=[66]
```

Both cases should write `[65]`, the submitted byte. The unrelated preceding operation should not change which contents are written. No product code was changed and no Cargo rebuild was needed.

## Direction

Copy write data before adding an operation to the storage queue. Cover buffer reuse with a preceding operation in flight for `writeFile`, `atomicWriteFile`, and `appendFile`.

**Deduplication.** Checked open and closed filesystem issues and the all-state GitHub issue snapshot for queued storage arguments, buffer reuse, and write-time copying; no matching report was found.
