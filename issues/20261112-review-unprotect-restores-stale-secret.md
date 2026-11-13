# Unprotect can restore an older secret and discard the latest value on reboot

**Status:** Open
**Systems:** runner/src/presence.rs, native presence secret storage
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, a failed deletion during a protected secret write can make a
later successful `store.unprotect` lose the latest value on the next boot.
The failure requires one transient storage deletion error; every subsequent
storage operation in the reproduction succeeds.

`Backend::write` adds the new generation, logs a failed deletion of the old
generation, and advances its current generation (`runner/src/presence.rs:706-724`).
`unprotect` then writes the current value to the plain store and deletes only
that current generation, returning `Done::Ok` (`presence.rs:915-948`). The old
locked generation survives. On boot, `reconcile` selects it and deletes the
newer plain copy (`presence.rs:346-369`), restoring stale data and protection
that the person had successfully removed.

Unprotect should not report success until every remaining locked generation
is deleted. A transient failure must retain a retryable operation and the
current value, so reconciliation cannot choose an obsolete generation.

## Reproduction

The following standalone program uses the public backend and its supplied
`MemoryVault`. Its wrapper fails exactly one `delete_locked` call, when
replacing the first protected value. It then unprotects successfully and
opens a new backend over the same vault.

```rust
use exact_runner::presence::*;
use std::sync::{Arc, atomic::{AtomicBool,Ordering}};
#[derive(Clone)]
struct VaultWithOnceFailedDelete { inner: MemoryVault, fail: Arc<AtomicBool> }
impl Vault for VaultWithOnceFailedDelete {
 fn plain(&self,n:&str)->Result<Option<Vec<u8>>,String>{self.inner.plain(n)}
 fn put_plain(&mut self,n:&str,v:Option<&[u8]>)->Result<(),String>{self.inner.put_plain(n,v)}
 fn record(&self,n:&str)->Result<Option<Vec<u8>>,String>{self.inner.record(n)}
 fn put_record(&mut self,n:&str,v:Option<&[u8]>)->Result<(),String>{self.inner.put_record(n,v)}
 fn generations(&self,n:&str)->Result<Vec<u64>,String>{self.inner.generations(n)}
 fn add_locked(&mut self,n:&str,g:u64,p:&Presence,v:&[u8])->Result<(),String>{self.inner.add_locked(n,g,p,v)}
 fn delete_locked(&mut self,n:&str,g:u64)->Result<(),String>{
  if self.fail.swap(false,Ordering::SeqCst){Err("temporary delete failure".into())}
  else {self.inner.delete_locked(n,g)}
 }
 fn read_locked(&mut self,n:&str,g:u64)->Result<Option<Vec<u8>>,String>{self.inner.read_locked(n,g)}
}
fn boot(v:&VaultWithOnceFailedDelete)->Backend {
 Backend::new(Box::new(v.clone()),vec![("a".into(),Some(Presence{policy:Policy::UserPresence,optional:true,words:vec![]}))])
}
fn main(){
 let mut v=VaultWithOnceFailedDelete{inner:MemoryVault::default(),fail:Arc::new(AtomicBool::new(false))};
 v.put_plain("a",Some(b"old token")).unwrap();
 let mut b=boot(&v);
 let env=Env{can_lock:true,moves:true,now_ms:0.0,faults:Faults::NONE};
 b.enqueue("a",Op::Protect,Some(1)); println!("protect: {:?}",b.pump(&env));
 v.fail.store(true,Ordering::SeqCst);
 b.enqueue("a",Op::Write(Some(b"new token".to_vec())),None); b.pump(&env);
 println!("after write: generations={:?}",v.generations("a").unwrap());
 b.enqueue("a",Op::Unprotect,Some(2)); println!("check: {:?}",b.pump(&env));
 b.checked(2,Checked::Passed); println!("unprotect: {:?}",b.pump(&env));
 println!("before reboot: plain={:?}, generations={:?}",v.plain("a").unwrap().map(|s|String::from_utf8(s).unwrap()),v.generations("a").unwrap());
 drop(b); let mut b=boot(&v);
 println!("after reboot: occupancy={:?}, plain={:?}",b.occupancy("a"),v.plain("a").unwrap());
 b.enqueue("a",Op::Unlock,Some(3)); b.pump(&env); b.checked(3,Checked::Passed);
 println!("unlock: {:?}",b.pump(&env));
 assert_eq!(v.read_locked("a",1).unwrap(),Some(b"old token".to_vec()));
}
```

Compile against this checkout's existing runner artifact (substitute its
filename if different):

```sh
rustc --edition=2021 main.rs -L dependency=target/debug/deps --extern exact_runner=target/debug/deps/libexact_runner-faa2c5b2aa381b98.rlib -o /tmp/presence-repro
/tmp/presence-repro
```

Observed:

```text
unprotect: [Done { id: 2, done: Ok }]
before reboot: plain=Some("new token"), generations=[1]
after reboot: occupancy=Some(Locked), plain=None
unlock: old token
```

The last line is the decoded `Done::Value` byte vector. Expected: the new
value remains plain after reboot. The fixture and raw output are retained
at `/tmp/exact-review-20261112/presence-old-generation/`.

**Deduplication.** Checked open and closed filesystem issues and the all-state
GitHub issue snapshot. `issues/closed/20261102-presence-protect-reports-success-before-cleanup.md`
concerns protect reporting success while its plain copy remains. This report
concerns an older locked generation surviving a later write and unprotect,
then replacing the newer plain value at the next boot.
