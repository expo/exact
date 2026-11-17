# Unloading a JavaScript module retains streams from its old engine

**Status:** Closed
**Resolution:** Already fixed by da9cd31c6, whose drop_runtime clears the stream registry on unload; a socket test now covers unloading with a live stream, reloading, and delivering the old reply.
**Systems:** js/src/lib.rs, js/src/resume.rs
**Severity:** P3
**Author:** Codex, repository review
**Date:** 2026-11-12

## Failure

At `f6be99aff76bc8bd881b0ec8daaa2c10ace0b9a2`, `Module::unload()` clears parked calls but leaves `self.streams` intact (`js/src/lib.rs:748–791`). This violates its documented promise to forget every answer in flight. A subsequent `load()` starts a fresh engine whose call and fetch IDs begin at 1, while the Rust registry still maps those IDs to the old streams.

The normal socket fixture demonstrates both effects: `in_flight()` remains 1 after unloading, and a reply for the old stream runs a newly created stream's mapper after reloading. `resume_fetch` finds the old stream by its key and calls `message` without an incarnation check (`js/src/resume.rs:47–56`). The old entry's call ID now names a call in the new engine.

This report concerns a consumer reusing the public `Module` through `unload()` and `load()`. It does not establish that current host reload paths reuse the same object; those that construct a fresh `Module` avoid this defect.

## Reproduction

Save the following as `/tmp/exact-review-20261112/js-unload-streams/main.rs`. It uses the repository's valid socket fixture and delivers one ordinary message; no network connection is opened.

```rust
use exact_js::Module;
use exact_plan::Value;
use exact_runner::{Answer, DataSource, Message, Outcome, Store, Target};
fn main() {
 let bytes=std::fs::read("target/debug/build/exact-js-5072cb4bd4d56047/out/socket.hbc").unwrap();
 let grants="net.websocket wss://echo.test\n";
 let mut m=Module::loaded(bytes,"test.socket",grants).unwrap();
 m.set_budget_ms(f64::INFINITY);
 let p=contract::compile("component App\n  resource first = feed(\"a\") as shape string\n  resource second = feed(\"side\") as shape string\n  view\n    text first\n").unwrap();
 m.bind(&p);
 let mut s=Store::new(grants,vec![]);
 let a=[Value::str("a")];
 let first=m.answer_for(Target::Resource(0), &mut s,"feed",&a).unwrap();
 let Answer::Later(first)=first else {panic!("stream")};
 println!("before unload in_flight={} fetch={:?}",m.in_flight(),first.fetch);
 m.unload();
 println!("after unload in_flight={}",m.in_flight());
 m.load().unwrap();
 println!("after reload in_flight={} calls_open={:?}",m.in_flight(),m.calls_open());
 let b=[Value::str("side")];
 let next=m.answer_for(Target::Resource(1), &mut s,"feed",&b).unwrap();
 let Answer::Later(next)=next else {panic!("stream")};
 println!("new stream fetch={:?}",next.fetch);
 let reply=m.parse_fetch_for(Target::Resource(0), first.fetch, &mut s,"feed", &a, Outcome::Message(Message {data:"hello".into(), ..Default::default()}));
 println!("old stream reply={reply:?}");
}

```

Compiled and run against the existing review artifacts, without a Cargo rebuild:

```sh
rustc --edition=2021 /tmp/exact-review-20261112/js-unload-streams/main.rs \
  -L dependency=target/debug/deps \
  --extern exact_js=target/debug/deps/libexact_js-13cc590fe137e0bc.rlib \
  --extern exact_plan=target/debug/deps/libexact_plan-7f4ed158b855da05.rlib \
  --extern exact_runner=target/debug/deps/libexact_runner-faa2c5b2aa381b98.rlib \
  --extern contract=target/debug/deps/libcontract-6c4999c7ae79fb84.rlib \
  -o /tmp/exact-review-20261112/js-unload-streams/repro
/tmp/exact-review-20261112/js-unload-streams/repro
```

Observed:

```text
before unload in_flight=1 fetch=Some(1)
after unload in_flight=1
after reload in_flight=1 calls_open=Some(0)
new stream fetch=Some(1)
old stream reply=Ok(Some(Now(Str("message hello 0"))))
```

Expected: unloading leaves no stream in flight, and the old stream's reply cannot invoke a new engine's mapper.

## Direction

Clear the stream registry when its engine is unloaded, and reset other engine-scoped bookkeeping consistently. Add coverage that unloads with a live stream, reloads, starts a different resource, and attempts to deliver the old reply.

**Deduplication.** Checked open and closed filesystem issues and the all-state GitHub issue snapshot; no matching report was found. `20261112-review-reload-does-not-reopen-carried-streams.md` concerns a fresh runner carrying a stream's cached value without reopening it. This issue concerns stale stream entries surviving reuse of one JavaScript executor.
