# A Rust module seam drops topic watches, entropy, and durable writes

**Status:** Closed
**Resolution:** The call reply (ABI 11) carries the topics an answer watched, its draws of randomness and each write's `durable`, and `call_reply` applies them to the host store as an embedded source's store records them.
**Systems:** logic, runner, data
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A Rust data module answers on a mirror store. The call reply encodes a read flag, plain name and value writes, and socket operations. `call_reply` applies that flag with `observe_external_read` and each write with `store.set` or `store.forget`. Topics, entropy draws, and `StoreWrite.durable` stay on the mirror.

`data` sources do use those effects. `envelope` watches a topic and writes with `set_durable`. The crypto helpers call `observe_entropy`. The JS seam forwards topics, entropy, and the durable bit. The runner replaces a resource's watches from `take_topics` and sets `entropy_reader` from `entropy_draws`, both on the host store after the reply.

After a module replacement, a source that watched a topic is not asked again when the topic changes. A random answer is not marked `entropy_reader`, so a bake can compile that draw into the plan. A durable write is persisted again. The Apple store skips a second write only when `durable` is set, so the lease can fail.

## Where

`logic/abi/src/lib.rs`, the answer and parse arm and `call_reply`. `runner/src/store.rs`, `StoreWrite.durable`, `take_topics`, and `entropy_draws`. `js/web/src/lib.rs`, the JS seam that forwards the same effects. `data/src/envelope.rs` and `data/src/crypto.rs`.

## How to reproduce

Answer from a Rust data module that calls `observe_topic`, `observe_entropy`, or `set_durable`. Change the topic, bake the plan, or retry the durable name. The watch is absent, the draw is compiled, or the host writes a value the module already leased.

## Direction

Carry topics, an entropy count, and `StoreWrite.durable` on the call reply and apply them on the host store, as the JS seam does. Until that exists, refuse those effects instead of dropping them.
