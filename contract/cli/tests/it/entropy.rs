//! LLP 1069.005 D2: an answer that drew secure randomness is a device read
//! whose value bake leaves out of the plan — unlike a store read, whose
//! empty-store answer is compiled as the fresh install's placeholder — so
//! no random value is shared by every install, and the device asks it.

use exact_kernel::Kernel;
use exact_runner::{Answer, DataError, DataSource, Runner, Store, Value};

const SRC: &str = r#"
component App
  resource id = token() as shape string
  resource session = session() as shape string
  resource uuid = uuid() as shape string
  resource digest = digest() as shape number
  view
    column
      text id testId="id"
      text session testId="session"
      text uuid testId="uuid"
      text "{digest}" testId="digest"
"#;

/// `token` draws (as `crypto.randomUUID` does on every executor);
/// `session` reads the device's store.
#[derive(Default)]
struct Device {
    draws: u32,
}

impl DataSource for Device {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        _: &[Value],
    ) -> Result<Answer, DataError> {
        match source {
            "token" => {
                self.draws += 1;
                store.observe_entropy();
                Ok(Answer::Now(Value::str(&format!("draw {}", self.draws))))
            }
            "session" => {
                store.observe_external_read();
                Ok(Answer::Now(Value::str("signed out")))
            }
            // D5: a Rust source's own helpers, marked as TypeScript's are.
            "uuid" => exact_data::crypto::random_uuid(store).map(|id| Answer::Now(Value::str(&id))),
            "digest" => {
                let bytes = exact_data::crypto::digest(exact_data::crypto::Sha::Sha256, b"abc");
                Ok(Answer::Now(Value::Number(bytes[0] as f64)))
            }
            other => Err(DataError::UnknownSource(other.into())),
        }
    }
}

#[test]
fn bake_compiles_no_value_that_drew_randomness() {
    let plan = contract::bake(contract::compile(SRC).unwrap(), Device::default()).unwrap();
    let row = |name: &str| {
        plan.resources
            .iter()
            .find(|r| plan.str(r.name) == name)
            .unwrap()
            .clone()
    };
    assert!(row("id").reader && row("id").initial.len == 0);
    assert!(row("session").reader && row("session").initial.len > 0);
    // D5: `exact_data::crypto` marks a Rust source's draw the same way; a
    // digest is pure and compiled.
    assert!(row("uuid").reader && row("uuid").initial.len == 0);
    assert!(!row("digest").reader && row("digest").initial.len > 0);

    // The device answers it, with its own draw.
    let runner = Runner::boot(
        plan,
        Device { draws: 41 },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(runner.resource("id"), Some(&Value::str("draw 42")));
    assert!(runner.resource_draws_entropy("id") && runner.resource_reads_store("id"));
    assert!(!runner.resource_draws_entropy("session"));
}

const WRITES: &str = r#"
shape Flag
  ok: bool

component App
  resource id = token() as shape string
  resource session = session() as shape string
  resource both = both() as shape string
  resource live = connect(id) as shape string
  resource watched = watch(session) as shape string
  mutation signedIn as shape Flag

  action signIn
    send signedIn = signIn()

  view
    column
      text id testId="id"
      text session testId="session"
      text both testId="both"
      text live testId="live"
      text watched testId="watched"
"#;

/// Counts each source's asks; `signIn` keeps a secret.
#[derive(Default)]
struct Writer {
    asks: std::collections::HashMap<String, u32>,
}

impl DataSource for Writer {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        *self.asks.entry(source.into()).or_default() += 1;
        match source {
            "token" => store.observe_entropy(),
            // Draws; once `enabled`, reads the secret too, and answers the same.
            "maybe" => {
                store.observe_entropy();
                if args[0] == Value::Bool(true) {
                    store.get("session");
                }
                return Ok(Answer::Now(Value::str("token")));
            }
            "session" => {
                store.get("session");
            }
            "both" => {
                store.observe_entropy();
                store.get("session");
            }
            "connect" | "watch" | "follow" | "trail" => {}
            "signIn" => {
                // A new value each time, so each write changes the store.
                store.set("session", &format!("ada {}", self.asks["signIn"]))?;
                return Ok(Answer::Now(Value::record(vec![Value::Bool(true)])));
            }
            other => return Err(DataError::UnknownSource(other.into())),
        }
        Ok(Answer::Now(Value::str(source)))
    }

    fn grants(&self) -> &str {
        "secret.keep session\n"
    }
}

/// Each source's asks, in `WRITES`' order.
fn asks(r: &mut Runner<Writer>) -> [u32; 5] {
    ["token", "session", "both", "connect", "watch"]
        .map(|s| r.data().asks.get(s).copied().unwrap_or(0))
}

/// After `signIn` keeps a secret: who was asked again.
fn sign_in(r: &mut Runner<Writer>) -> [u32; 5] {
    let before = asks(r);
    r.act("signIn", vec![]).unwrap();
    let after = asks(r);
    std::array::from_fn(|i| after[i] - before[i])
}

/// D2, "Asking again": a draw does not make a resource ask again, so a write
/// to the store asks again the resources whose own ask read it, and not one
/// that only drew randomness (expo/exact#54: Convex's socket, which draws its
/// session id, was opened again by every sign-in). A resource whose arguments
/// read a reader is asked when they change: `watch(session)` keeps its
/// arguments, as `session` answers the same.
#[test]
fn a_store_write_asks_again_what_read_the_store_not_what_only_drew() {
    let plan = contract::bake(contract::compile(WRITES).unwrap(), Writer::default()).unwrap();
    let mut r = Runner::boot(
        plan,
        Writer::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    // token (drew), session (read), both (drew and read), connect(id), watch(session).
    assert_eq!(sign_in(&mut r), [0, 1, 1, 0, 0]);
    // Still the device's: bake keeps no value for either.
    assert!(r.resource_draws_entropy("id") && r.resource_reads_store("id"));
    assert!(r.resource_reads_store("live"));
}

/// The same after a reload: what each reader's last ask read is carried.
#[test]
fn a_reload_carries_which_readers_only_drew() {
    let plan = contract::bake(contract::compile(WRITES).unwrap(), Writer::default()).unwrap();
    let r = Runner::boot(
        plan.clone(),
        Writer::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let mut reloaded = Runner::boot_carrying(
        plan,
        Writer::default(),
        Kernel::with_monospace(),
        &r.carry(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(asks(&mut reloaded), [0; 5], "every answer was carried");
    assert_eq!(sign_in(&mut reloaded), [0, 1, 1, 0, 0]);
}

const LATER: &str = r#"
shape Flag
  ok: bool

component App
  state enabled = false
  resource id = maybe(enabled) as shape string
  resource live = follow(id) as shape string
  derive key = `${id}!`
  resource trailed = trail(key) as shape string
  mutation signedIn as shape Flag

  action enable
    enabled = true
  action signIn
    send signedIn = signIn()

  view
    column
      text live testId="live"
      text trailed testId="trailed"
"#;

/// An upstream answer that starts reading the store is asked again by a store
/// write from then on, and while it gives the same value its dependents keep
/// their arguments and ask nothing, whether they read it directly or through
/// a derive.
#[test]
fn a_store_write_asks_no_dependent_whose_arguments_stay() {
    let plan = contract::bake(contract::compile(LATER).unwrap(), Writer::default()).unwrap();
    let mut r = Runner::boot(
        plan,
        Writer::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    // `follow` takes `id` directly, `trail` through a derive.
    let asks = |r: &mut Runner<Writer>| {
        ["maybe", "follow", "trail"].map(|s| r.data().asks.get(s).copied().unwrap_or(0))
    };
    let before = asks(&mut r);
    r.act("signIn", vec![]).unwrap();
    assert_eq!(asks(&mut r), before, "each only drew");
    r.act("enable", vec![]).unwrap();
    let enabled = asks(&mut r);
    assert_eq!(
        enabled,
        [before[0] + 1, before[1], before[2]],
        "the same answer: both keep their arguments"
    );
    r.act("signIn", vec![]).unwrap();
    assert_eq!(asks(&mut r), [enabled[0] + 1, enabled[1], enabled[2]]);
}

/// `token()` asks with a GET; its reply's turn draws randomness and answers
/// outside its shape, so the reply's commit is refused and a later one fails
/// the resource.
#[derive(Default)]
struct Drawn {
    asks: u32,
}

impl DataSource for Drawn {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        self.asks += 1;
        Ok(Answer::Later(exact_runner::Request::get(
            "https://example.test/token",
        )))
    }
    fn parse(
        &mut self,
        store: &mut Store,
        _: &str,
        _: &[Value],
        _: exact_runner::Outcome,
    ) -> Result<Answer, DataError> {
        store.observe_entropy();
        Ok(Answer::Now(Value::Number(7.0)))
    }
    fn grants(&self) -> &str {
        "secret.keep session\n"
    }
}

/// A refused reply keeps what its turn read, and a turn that only drew
/// randomness read nothing the store holds: no store write asks it again.
#[test]
fn a_refused_reply_that_only_drew_is_asked_again_by_no_store_write() {
    let src = "component App
  resource id = token() as shape string else empty()
  view
    text id
";
    let mut r = Runner::boot(
        contract::compile(src).unwrap(),
        Drawn::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let ticket = r.take_requests()[0].ticket;
    let reply = exact_runner::Outcome::Response(exact_runner::Response {
        status: 200,
        headers: Vec::new(),
        body: Vec::new(),
    });
    r.fulfill(ticket, reply).unwrap();
    r.store_changed(vec![("session".into(), b"ada".to_vec())])
        .unwrap();
    assert_eq!(r.data().asks, 1);
}
