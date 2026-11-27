//! The document portal's file transfer,
//! [`org.freedesktop.portal.FileTransfer`](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileTransfer.html),
//! which hands files across a Flatpak sandbox's edge: a drag's source
//! registers its files and offers the key (LLP 1094.000 D13), and a
//! sandboxed receiver redeems a key for paths it may open (LLP 1094.001 D8).
//! Every call is blocking, for a worker thread, and each wait in it bounded
//! by [`super::TIMEOUT`]; a registration ends by its deadline as well.

use super::bus::Bus;
use super::wire::{self, Message, Value};
use std::collections::HashMap;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

const DOCUMENTS: &str = "org.freedesktop.portal.Documents";
const DOCUMENTS_PATH: &str = "/org/freedesktop/portal/documents";
const FILE_TRANSFER: &str = "org.freedesktop.portal.FileTransfer";
const DBUS: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
/// The most descriptors one `AddFiles` call sends, as GTK sends them: the
/// session bus's limit for one message.
const BATCH: usize = 16;

/// The document portal on the session bus, as a drag offers and redeems keys.
#[derive(Clone)]
pub struct Portal {
    inner: Arc<Inner>,
}

struct Inner {
    address: Option<String>,
    state: Mutex<State>,
    /// The connection keys are registered on, once made.
    sender: Mutex<Option<Arc<Sender>>>,
    /// How many sender connections were made, which numbers each.
    senders: AtomicU64,
    /// The connection probes and redemptions go on, so that their failures
    /// end no key.
    asker: Mutex<Option<Bus>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(sender) = lock(&self.sender).take() {
            sender.close();
        }
    }
}

/// The connection keys are registered on, kept until it is lost, since a
/// key dies with its sender's connection. Calls are written under one lock
/// and each waits for its own reply, by its own deadline; a thread of its
/// own reads every reply and hands it to its call, so one that comes after
/// its call stopped waiting is still read, and the transfer a start made
/// all the same is stopped. Stops request no reply and are written by a
/// thread of their own, from a queue, so neither the reader nor a
/// registration waits on a write for one.
struct Sender {
    number: u64,
    out: Mutex<UnixStream>,
    passes_fds: bool,
    serial: AtomicU32,
    waiting: Mutex<HashMap<u32, Waiting>>,
    stops: Mutex<mpsc::Sender<String>>,
    lost: AtomicBool,
}

/// What a call's serial waits for: a call's reply, to hand over; or the
/// reply to a start whose caller stopped waiting, kept since then, whose
/// transfer is stopped when it comes.
enum Waiting {
    Call(mpsc::Sender<Result<Vec<Value>, String>>),
    Late { since: Instant },
}

/// A transfer the portal registered: its key, and the number of the
/// connection that holds it, on which alone it can be stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    /// The key a receiver redeems.
    pub key: String,
    /// The number of the sender connection that holds the transfer.
    pub connection: u64,
}

/// How long the reply to a start whose caller stopped waiting is still
/// listened for, counted from when the caller stopped; the mark is dropped
/// at the next message or call after that, and a transfer such a start
/// makes later is left until the connection ends.
const LATE: Duration = Duration::from_secs(120);
/// How long a stop's write may wait for the writer and the socket, and how
/// long a stop whose write ran out waits before it is tried again.
const STOP_WRITE: Duration = Duration::from_secs(2);
const STOP_PACE: Duration = Duration::from_secs(1);

/// What the probes found. `available` holds only for the latest probe
/// asked for since the portal's owner last changed: each probe and owner
/// change advances `generation`, and a reply from an older one is ignored.
/// Without a watch on the owner, nothing is available.
#[derive(Default)]
struct State {
    available: bool,
    generation: u64,
    probing: bool,
    watching: bool,
}

impl Portal {
    /// The session's portal, probed as it is first asked for and again
    /// whenever the portal's owner on the bus changes.
    pub fn session() -> &'static Portal {
        static SESSION: OnceLock<Portal> = OnceLock::new();
        SESSION.get_or_init(|| {
            let portal = Portal::at(std::env::var("DBUS_SESSION_BUS_ADDRESS").ok());
            portal.watch();
            portal
        })
    }

    /// The portal on the bus at `address`, not yet probed.
    fn at(address: Option<String>) -> Portal {
        Portal {
            inner: Arc::new(Inner {
                address: address.filter(|a| !a.is_empty()),
                state: Mutex::new(State::default()),
                sender: Mutex::new(None),
                senders: AtomicU64::new(0),
                asker: Mutex::new(None),
            }),
        }
    }

    /// Whether the latest probe succeeded while the owner is watched: a drag
    /// offers the key while it has.
    pub fn available(&self) -> bool {
        let state = self.state();
        state.available && state.watching
    }

    /// Probe again on a thread of its own, unless the portal is available or
    /// a probe is under way, and watch the owner again if the watch ended: a
    /// lift asks after a failure.
    pub fn reprobe(&self) {
        let (watching, idle) = {
            let state = self.state();
            if state.available && state.watching {
                return;
            }
            (state.watching, !state.probing)
        };
        if !watching {
            self.watch();
        } else if idle {
            self.probe_apart();
        }
    }

    /// Register `count` files for one transfer the receiver may write
    /// through and that ends at its first retrieval, each opened by `open`
    /// as a path alone (`O_PATH`) a batch at a time, all by `deadline`. The
    /// files are opened apart from the connection, so a slow file system
    /// holds up no other registration; each batch is added on the
    /// connection that started the transfer. A transfer whose files could
    /// not all be added in time is stopped.
    pub fn send(
        &self,
        count: usize,
        open: impl Fn(usize) -> Result<std::fs::File, String>,
        deadline: Instant,
    ) -> Result<Transfer, String> {
        let address = self.address()?;
        let options = || {
            Value::dict(vec![
                ("writable", Value::Bool(true)),
                ("autostop", Value::Bool(true)),
            ])
        };
        // A connection found lost as the transfer starts, which ended any transfer it had begun, is replaced once
        let mut sender = self.sender(address, deadline)?;
        let started = match sender.call("StartTransfer", &[options()], &[], deadline, true) {
            Err(_) if sender.lost() => {
                sender = self.sender(address, deadline)?;
                sender.call("StartTransfer", &[options()], &[], deadline, true)
            }
            started => started,
        }?;
        let [Value::Str(key)] = started.as_slice() else {
            return Err("the document portal gave no transfer key".into());
        };
        let transfer = Transfer {
            key: key.clone(),
            connection: sender.number,
        };
        for first in (0..count).step_by(BATCH) {
            let added = (first..count.min(first + BATCH))
                .map(|n| match Instant::now() < deadline {
                    true => open(n),
                    false => Err("its time ran out".into()),
                })
                .collect::<Result<Vec<_>, String>>()
                .and_then(|opened| {
                    let fds: Vec<_> = opened.iter().map(AsFd::as_fd).collect();
                    let indices = (0..fds.len() as u32).map(Value::Fd).collect();
                    let body = [
                        Value::str(&transfer.key),
                        Value::Array("h".into(), indices),
                        Value::dict(vec![]),
                    ];
                    sender
                        .call("AddFiles", &body, &fds, deadline, false)
                        .map(drop)
                });
            if let Err(e) = added {
                sender.stop(&transfer.key);
                return Err(e);
            }
        }
        Ok(transfer)
    }

    /// End `transfer`, whose key did not reach its receiver, on the
    /// connection that holds it, by a stop that connection's own thread
    /// writes, so this only queues it; nothing when that connection is
    /// gone, which ended it.
    pub fn stop(&self, transfer: &Transfer) {
        let sender = lock(&self.inner.sender).clone();
        if let Some(sender) = sender.filter(|s| s.number == transfer.connection) {
            sender.stop(&transfer.key);
        }
    }

    /// The sender connection, made by `deadline` when there is none or the
    /// last was lost; it is made outside the lock, so no other call waits
    /// on its setup.
    fn sender(&self, address: &str, deadline: Instant) -> Result<Arc<Sender>, String> {
        let live = |kept: &Option<Arc<Sender>>| kept.as_ref().filter(|s| !s.lost()).cloned();
        if let Some(sender) = live(&lock(&self.inner.sender)) {
            return Ok(sender);
        }
        let number = self.inner.senders.fetch_add(1, Ordering::Relaxed) + 1;
        let made = Sender::connect(address, deadline, number)?;
        let mut kept = lock(&self.inner.sender);
        if let Some(sender) = live(&kept) {
            made.close();
            return Ok(sender);
        }
        *kept = Some(made.clone());
        Ok(made)
    }

    /// The paths the portal gives this process for the files of transfer
    /// `key`: their own to a process outside a sandbox, ones it is granted
    /// in the document portal's mount to a sandboxed one.
    pub fn retrieve(&self, key: &str) -> Result<Vec<PathBuf>, String> {
        let reply = self.ask(|bus| {
            bus.call(
                DOCUMENTS,
                DOCUMENTS_PATH,
                FILE_TRANSFER,
                "RetrieveFiles",
                &[Value::str(key), Value::dict(vec![])],
            )
        })?;
        match reply.as_slice() {
            [Value::Array(_, files)] => Ok(files
                .iter()
                .filter_map(|f| match f {
                    Value::Str(path) => Some(PathBuf::from(path)),
                    _ => None,
                })
                .collect()),
            _ => Err("the document portal gave no files".into()),
        }
    }

    /// Ask the portal for its `FileTransfer` version, starting it if the bus
    /// can, and keep the answer unless a later probe or an owner change
    /// replaced this one.
    fn probe(&self) {
        let generation = {
            let mut state = self.state();
            state.generation += 1;
            state.probing = true;
            state.generation
        };
        let answered = self
            .ask(|bus| {
                bus.call(
                    DOCUMENTS,
                    DOCUMENTS_PATH,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    &[Value::str(FILE_TRANSFER), Value::str("version")],
                )
            })
            .is_ok();
        let mut state = self.state();
        if state.generation == generation {
            state.available = answered;
            state.probing = false;
        }
    }

    /// [`Portal::probe`] on a thread of its own.
    fn probe_apart(&self) {
        let portal = self.clone();
        let _ = std::thread::Builder::new()
            .name("exact-portal-probe".into())
            .spawn(move || portal.probe());
    }

    /// The portal's owner changed, or its watch starts: the last success
    /// no longer holds, and a probe under way will be ignored.
    fn owner_changed(&self) {
        let mut state = self.state();
        state.generation += 1;
        state.available = false;
        state.probing = false;
    }

    /// Watch the portal's owner on the bus and probe, on a thread of its
    /// own: each change clears the last success and probes again. A watch
    /// that cannot start or that ends leaves nothing available, and the
    /// next lift starts another.
    fn watch(&self) {
        {
            let mut state = self.state();
            if state.watching {
                return;
            }
            state.watching = true;
        }
        self.owner_changed();
        let portal = self.clone();
        let started = std::thread::Builder::new()
            .name("exact-portal-watch".into())
            .spawn(move || {
                let watching = portal.address().and_then(|address| {
                    let mut bus = Bus::connect(address, super::TIMEOUT)?;
                    bus.subscribe(&format!(
                        "type='signal',sender='{DBUS}',path='{DBUS_PATH}',interface='{DBUS}',member='NameOwnerChanged',arg0='{DOCUMENTS}'"
                    ))?;
                    Ok(bus)
                });
                if let Ok(mut bus) = watching {
                    portal.probe_apart();
                    bus.wait_forever();
                    while bus.signal().is_ok() {
                        portal.owner_changed();
                        portal.probe_apart();
                    }
                }
                let mut state = portal.state();
                state.watching = false;
                state.available = false;
            });
        if started.is_err() {
            self.state().watching = false;
        }
    }

    /// Run `call` on the connection probes and redemptions share,
    /// connecting first when there is none.
    fn ask<T>(&self, call: impl FnOnce(&mut Bus) -> Result<T, String>) -> Result<T, String> {
        let address = self.address()?;
        let mut asker = lock(&self.inner.asker);
        if asker.is_none() {
            let mut bus = Bus::connect_passing(address, super::TIMEOUT, None)?;
            bus.ignore_signals();
            *asker = Some(bus);
        }
        let bus = asker.as_mut().expect("connected above");
        let answer = call(bus);
        if bus.lost() {
            *asker = None;
        }
        answer
    }

    fn address(&self) -> Result<&str, String> {
        self.inner
            .address
            .as_deref()
            .ok_or_else(|| "there is no session bus".into())
    }

    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.inner.state)
    }
}

impl Sender {
    /// A sender connection, numbered `number`, connected by `deadline`, its
    /// replies read by a thread of its own and its stops written by another.
    fn connect(address: &str, deadline: Instant, number: u64) -> Result<Arc<Sender>, String> {
        let mut bus = Bus::connect_passing(address, super::TIMEOUT, Some(deadline))?;
        let out = bus.writer().map_err(|e| e.to_string())?;
        let (stops, queued) = mpsc::channel::<String>();
        let sender = Arc::new(Sender {
            number,
            out: Mutex::new(out),
            passes_fds: bus.passes_fds(),
            serial: AtomicU32::new(bus.serial()),
            waiting: Mutex::new(HashMap::new()),
            stops: Mutex::new(stops),
            lost: AtomicBool::new(false),
        });
        let reading = sender.clone();
        let read = std::thread::Builder::new()
            .name("exact-portal-sender".into())
            .spawn(move || {
                bus.wait_forever();
                while let Ok(m) = bus.next() {
                    reading.answered(m);
                }
                // A reader that ends, on the bus's end or on bytes that are no message, ends the connection
                bus.shutdown();
                reading.lose();
            });
        // The stops' thread holds the connection weakly, so it ends with the connection's last holder
        let stopping = Arc::downgrade(&sender);
        let stop = std::thread::Builder::new()
            .name("exact-portal-stops".into())
            .spawn(move || write_stops(&stopping, &queued));
        if let Err(e) = read.and(stop) {
            sender.close();
            return Err(e.to_string());
        }
        Ok(sender)
    }

    /// Call the document portal's `member` with `body` and `fds`, waiting
    /// for its reply by `deadline`; `start` when it starts a transfer, which
    /// is stopped if its reply comes after the wait.
    fn call(
        &self,
        member: &str,
        body: &[Value],
        fds: &[std::os::fd::BorrowedFd<'_>],
        deadline: Instant,
        start: bool,
    ) -> Result<Vec<Value>, String> {
        if !fds.is_empty() && !self.passes_fds {
            return Err("the session bus passes no file descriptors".into());
        }
        let (reply, answer) = mpsc::channel();
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        {
            let mut waiting = lock(&self.waiting);
            forget_late(&mut waiting);
            waiting.insert(serial, Waiting::Call(reply));
        }
        let call = wire::message(
            wire::METHOD_CALL,
            serial,
            None,
            DOCUMENTS,
            DOCUMENTS_PATH,
            FILE_TRANSFER,
            member,
            body,
            fds.len() as u32,
        );
        if let Err(e) = self.write(&call, fds, deadline) {
            lock(&self.waiting).remove(&serial);
            return Err(e);
        }
        let ended = || Err("the session bus connection ended".into());
        match answer.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(reply) => reply,
            Err(mpsc::RecvTimeoutError::Disconnected) => ended(),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // The reader hands a reply over under this lock: either it already has, or it will find the call
                // gone, a start's reply then stopping its transfer
                let mut waiting = lock(&self.waiting);
                match waiting.remove(&serial) {
                    Some(_) => {
                        if start {
                            let since = Instant::now();
                            waiting.insert(serial, Waiting::Late { since });
                        }
                        Err("the document portal did not answer in time".into())
                    }
                    None => answer.try_recv().unwrap_or_else(|_| ended()),
                }
            }
        }
    }

    /// Queue a stop of transfer `key`, which its own thread writes.
    fn stop(&self, key: &str) {
        let _ = lock(&self.stops).send(key.to_owned());
    }

    /// End transfer `key` with a call that requests no reply, written
    /// within [`STOP_WRITE`].
    fn write_stop(&self, key: &str) -> Result<(), String> {
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        let call = wire::message(
            wire::METHOD_CALL,
            serial,
            None,
            DOCUMENTS,
            DOCUMENTS_PATH,
            FILE_TRANSFER,
            "StopTransfer",
            &[Value::str(key)],
            0,
        );
        self.write(&wire::without_reply(call), &[], Instant::now() + STOP_WRITE)
    }

    /// Hand reply `m` to its call, under the lock a call that stopped
    /// waiting takes; a late start's reply has its transfer stopped, and
    /// another reply no call waits for goes nowhere.
    fn answered(&self, m: Message) {
        let mut waiting = lock(&self.waiting);
        forget_late(&mut waiting);
        let Some(serial) = m
            .reply_serial
            .filter(|_| matches!(m.kind, wire::METHOD_RETURN | wire::ERROR))
        else {
            return;
        };
        let reply = match m.kind {
            wire::METHOD_RETURN => Ok(m.body),
            _ => Err(match m.body.first() {
                Some(Value::Str(text)) => format!("{}: {text}", m.error_name),
                _ => m.error_name,
            }),
        };
        match waiting.remove(&serial) {
            Some(Waiting::Call(to)) => {
                let _ = to.send(reply);
            }
            Some(Waiting::Late { .. }) => {
                if let Ok([Value::Str(key)]) = reply.as_deref() {
                    self.stop(key);
                }
            }
            None => {}
        }
    }

    /// Write `bytes`, with `fds` beside the first of them, by `deadline`: a
    /// message cut off part way, or a failure that is not a wait running
    /// out, loses the connection. Only the send timeout is set, which the
    /// reader, on the same socket, never sets.
    fn write(
        &self,
        bytes: &[u8],
        fds: &[std::os::fd::BorrowedFd<'_>],
        deadline: Instant,
    ) -> Result<(), String> {
        let out = lock_until(&self.out, deadline)?;
        if self.lost() {
            return Err("the session bus connection ended".into());
        }
        let mut at = 0;
        while at < bytes.len() {
            let left = deadline.saturating_duration_since(Instant::now());
            let wrote = match left.is_zero() {
                true => Err(std::io::ErrorKind::TimedOut.into()),
                false => out
                    .set_write_timeout(Some(left))
                    .and_then(|()| match (at, fds) {
                        (0, [_, ..]) => super::bus::send_passing(&out, bytes, fds),
                        _ => (&*out).write(&bytes[at..]),
                    }),
            };
            match wrote {
                Ok(0) => return Err(self.lose_with(&out, "the bus took no more")),
                Ok(n) => at += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(match at {
                        0 => "the bus did not take the call in time".into(),
                        _ => self.lose_with(&out, "the bus took only part of the call in time"),
                    });
                }
                Err(e) => return Err(self.lose_with(&out, &e.to_string())),
            }
        }
        Ok(())
    }

    fn lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// Mark the connection lost and end its stream, so its reader ends and
    /// every call still waiting hears so: `why`, for the caller.
    fn lose_with(&self, out: &UnixStream, why: &str) -> String {
        self.lost.store(true, Ordering::Relaxed);
        let _ = out.shutdown(std::net::Shutdown::Both);
        why.to_owned()
    }

    /// The reader ended: the connection is lost, and the calls waiting on
    /// it hear so.
    fn lose(&self) {
        self.lost.store(true, Ordering::Relaxed);
        lock(&self.waiting).clear();
    }

    /// End the connection, whose reader then ends.
    fn close(&self) {
        let _ = lock(&self.out).shutdown(std::net::Shutdown::Both);
    }
}

/// Write each stop `queued` for the connection `stopping` holds, in order:
/// one whose write ran out of time before it began is tried again after
/// [`STOP_PACE`], however many stops come meanwhile, which wait in the
/// queue, on that same connection; the stops left are dropped when the
/// connection ends, which ended their transfers.
fn write_stops(stopping: &std::sync::Weak<Sender>, queued: &mpsc::Receiver<String>) {
    for key in queued {
        loop {
            let Some(sender) = stopping.upgrade().filter(|s| !s.lost()) else {
                return;
            };
            match sender.write_stop(&key) {
                Ok(()) => break,
                Err(_) if sender.lost() => return,
                Err(_) => {}
            }
            // The connection is not held while the stop waits, so its last holder can end it
            drop(sender);
            std::thread::sleep(STOP_PACE);
        }
    }
}

/// Drop the marks of starts whose replies have not come within [`LATE`].
fn forget_late(waiting: &mut HashMap<u32, Waiting>) {
    waiting.retain(|_, w| !matches!(w, Waiting::Late { since } if since.elapsed() >= LATE));
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// `mutex`'s guard, taken by `deadline`, or an error once it passes.
fn lock_until<T>(mutex: &Mutex<T>, deadline: Instant) -> Result<MutexGuard<'_, T>, String> {
    loop {
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(e)) => return Ok(e.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(std::sync::TryLockError::WouldBlock) => return Err("its time ran out".into()),
        }
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
