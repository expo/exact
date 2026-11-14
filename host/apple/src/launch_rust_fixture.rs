use super::*;

#[derive(Clone)]
pub(super) struct RustSource {
    pub(super) base: Source,
    pub(super) manifest: bool,
    pub(super) entry: Entry,
    pub(super) request: Request,
    pub(super) reads_session: bool,
}
impl DataSource for RustSource {
    fn launch(&self) -> Option<Entry> {
        Some(self.entry.clone())
    }
    fn preloads(&self) -> &[Baked] {
        if self.manifest {
            self.base.definitions
        } else {
            &[]
        }
    }
    fn grants(&self) -> &str {
        &self.base.grants
    }
    fn ready(&self) -> bool {
        !self.reads_session || self.base.activated.load(Ordering::Acquire) > 0
    }
    fn activate(&mut self) -> Result<(), DataError> {
        self.base.activate()
    }
    fn query(&mut self, name: &str, args: &[Value]) -> Result<Value, DataError> {
        self.base.query(name, args)
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if matches!(source, "rotate" | "first") {
            self.base.answer(store, source, args)
        } else if source == "public" {
            Ok(Answer::Later(
                self.base.definitions[0].fill(|_| None).unwrap(),
            ))
        } else {
            let mut request = self.request.clone();
            if self.reads_session {
                if let Some(token) = store.get("session") {
                    request = request.header("Authorization", token);
                }
            }
            Ok(Answer::Later(request))
        }
    }
    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.base.parse(store, source, args, outcome)
    }
}
pub(super) type Data =
    exact_logic::Swappable<exact_data::Mixed<(), exact_data::Placed<RustSource>>>;
pub(super) fn composed(source: RustSource) -> Data {
    exact_logic::Swappable::off(
        exact_data::Mixed::new(
            (),
            exact_data::Placed::new(source, Placement::Main),
            &[],
            &["read", "rotate", "public", "first", "timeline"],
        )
        .unwrap(),
    )
}
pub(super) fn source(
    server: &Server,
    run: impl Fn(&mut dyn LaunchContext) + Send + Sync + 'static,
) -> RustSource {
    let base = server.source(PreloadWhen::Start, false);
    let request = base.definitions[0].fill(|_| None).unwrap();
    RustSource {
        base,
        request,
        manifest: false,
        reads_session: false,
        entry: Entry::new("launch-data", run),
    }
}
pub(super) fn begin(launch: &LaunchPreloads, source: &RustSource, prewarm: bool) {
    let source = source.clone();
    launch.begin(prewarm, move || composed(source));
}
pub(super) fn signal(launch: &LaunchPreloads, source: &RustSource, opened: bool) {
    let source = source.clone();
    launch.signal(opened, move || composed(source));
}
pub(super) struct Drive {
    pub(super) bridge: Bridge<Data>,
    pub(super) hooks: Hooks,
    _wake: Box<Sender<()>>,
    pub(super) wakes: Receiver<()>,
}
impl Drive {
    pub(super) fn boot(source: &RustSource, plan: &[u8]) -> (Self, String) {
        Self::boot_endowed(source, plan, memory_endow)
    }
    pub(super) fn boot_endowed(
        source: &RustSource,
        plan: &[u8],
        endow: fn(&str, &str, bool) -> crate::store::Endowed,
    ) -> (Self, String) {
        let (send, wakes) = channel();
        let sender = Box::new(send);
        extern "C" fn wake(ctx: *mut std::ffi::c_void) {
            // SAFETY: Drive retains the sender until after the bridge is dropped.
            #[allow(unsafe_code)]
            let send = unsafe { &*ctx.cast::<Sender<()>>() };
            let _ = send.send(());
        }
        let hooks = Hooks {
            wake: Some(wake),
            wake_ctx: (&*sender as *const Sender<()>).cast_mut().cast(),
            ..Hooks::none()
        };
        let mut bridge = Bridge::with_links(Links {
            io: Some(IoLinks {
                preloads: linked,
                endow,
                start: start_io,
                ..IoLinks::LINKED
            }),
            ..Links::ALL
        });
        let len = bridge.boot_selected(
            plan.to_vec().leak(),
            || composed(source.clone()),
            hooks,
            100.,
            100.,
        );
        let batch = String::from_utf8(bridge.output_bytes(len as usize).to_vec()).unwrap();
        assert!(!batch.contains("\"error\":\""), "{batch}");
        (
            Self {
                bridge,
                hooks,
                _wake: sender,
                wakes,
            },
            batch,
        )
    }
    pub(super) fn complete(&mut self, source: &RustSource) {
        while source.base.parsed.load(Ordering::Acquire) == 0 {
            if self.wakes.recv_timeout(HANG).is_err() {
                panic!("no completion wake: {}", self.logs());
            }
            self.bridge.pump(0.);
        }
    }
    pub(super) fn logs(&mut self) -> String {
        let len = self.bridge.input_write(br#"{"op":"logs"}"#);
        let len = self.bridge.agent(len);
        String::from_utf8(self.bridge.output_bytes(len as usize).to_vec()).unwrap()
    }
}
pub(super) fn definitions(launch: &LaunchPreloads) {
    launch.0.set.get().unwrap().control.wait_definitions();
}
