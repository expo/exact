//! What the presenter asks of the reader (LLP 1115.001 D5). The presenter
//! queues what it asks of the platform and wakes the reader by the
//! descriptor it listens to; the reader takes the queue here
//! (`Native.requests`) and answers by id (`Native.answer`). The queue, its
//! words and its kinds are the presenter's (`presenter/requests.rs`).

use super::CanvasHost;
use exact_runner::DataSource;
use serde_json::Value as Json;

impl<D: DataSource + Default> CanvasHost<D> {
    /// What the presenter asks of the reader since it last took them, as
    /// words ([`crate::presenter::Presenter::take_requests`]), with what the
    /// reader is told of the app's state when that changed (whether Back
    /// has somewhere to go, whether a field wants the keyboard). The state
    /// is not read during a scroll's own frames, which run no command and
    /// commit only rows: it is told with the frame after.
    ///
    /// The reader calls this after every turn of the host, which is where
    /// the journal is written out when asked for (`canvas/journal.rs`).
    pub fn requests(&mut self) -> Option<Vec<i32>> {
        if !self.launch.done() {
            if !self.p.module_pending() {
                self.launch.activated();
            }
            self.launch.turn(&crate::launch_marks::items(
                &self.p.host().runner().outstanding(),
            ));
        }
        self.launch.record_done();
        self.echo_journal();
        if self.p.host.sound.ready() {
            let files = self
                .p
                .host
                .runner()
                .plan()
                .sounds
                .iter()
                .map(|s| self.p.host.runner().plan().str(s.src).to_owned())
                .collect();
            let body = self
                .p
                .host
                .sound
                .batch(files, self.origin_ns, super::monotonic_ns());
            self.p.request(crate::presenter::Request::Sound(body));
        }
        super::clock::publish(
            self.started,
            super::agent::clock(&self.p),
            super::agent::offset(),
        );
        self.p.surfaces.publish_names();
        if let Some(names) = crate::canvas::jni::jni_extensions::take_bound() {
            self.bind_extensions(&names);
        }
        if !self.scroll_hold.take_request_skip() {
            // The reader takes its requests once it has the turn's frame:
            // after first pixel (a displayed frame activates it), the
            // hatches connect, once (LLP 1075.003.000.001 §2.1).
            if self.p.first_pixel_told() {
                self.p.connect_hatches();
            }
            self.p.tell_shell();
            self.p.tell_reader_hatches();
        }
        self.p.take_requests()
    }

    /// What the reader's hatches ask (`hatches::reader::act`), run at once,
    /// between turns: its answer, -1 when refused. A view hosted or let go
    /// paints again.
    pub fn hatch_act(&mut self, node: u32, kind: i32, text: Option<&str>) -> i64 {
        self.p.reader_hatch_act(node, kind, text)
    }

    /// An app module's event, only for the session and allocation whose view the reader still holds.
    pub fn module_event(
        &mut self,
        id: u32,
        key: exact_kernel::NodeKey,
        session: u64,
        kind: &str,
        payload: String,
    ) {
        let now = self.now();
        self.force |= self
            .p
            .reader_module_event(id, key, session, kind, payload, now);
    }

    /// The reader's iframe finished a document (LLP 1020 §10).
    pub fn frame_loaded(&mut self, id: u32, key: exact_kernel::NodeKey, session: u64) {
        self.force |= self.p.reader_frame_loaded(id, key, session);
    }

    /// The reader's iframe took the platform's focus or lost it.
    pub fn frame_focused(
        &mut self,
        id: u32,
        key: exact_kernel::NodeKey,
        session: u64,
        focused: bool,
    ) {
        self.p.reader_frame_focused(id, key, session, focused);
    }

    /// The reader's iframe's `FrameNavigation` record (LLP 1020.002 D2).
    pub fn frame_event(
        &mut self,
        id: u32,
        key: exact_kernel::NodeKey,
        session: u64,
        event: &str,
        record: &str,
    ) {
        self.force |= self.p.reader_frame_event(id, key, session, event, record);
    }

    /// A media element's event or state (LLP 1042.001 D7), only for the
    /// session and allocation whose element the reader still holds.
    #[allow(clippy::too_many_arguments)]
    pub fn media(
        &mut self,
        id: u32,
        key: exact_kernel::NodeKey,
        session: u64,
        event: &str,
        payload: &str,
        state: &str,
    ) {
        let now = self.now();
        self.force |= self
            .p
            .reader_media(id, key, session, event, payload, state, now);
    }

    /// Authored updates for an audio player whose reader is stopped.
    #[cfg(target_os = "android")]
    pub fn background_audio(
        &self,
        id: u32,
        key: exact_kernel::NodeKey,
        session: u64,
    ) -> Option<String> {
        self.p
            .background_audio(id, key, session)
            .map(|props| props.to_string())
    }

    /// The reader's answer to request `id`, or its cancellation (`None`).
    pub fn answer(&mut self, id: u32, answer: Option<&str>) {
        self.p.answer_request(id, answer);
    }

    /// How a presence check the reader made ended (LLP 1133.010 D13).
    pub fn presence_reply(&mut self, answer: &str) {
        self.p
            .host_mut()
            .runner_mut()
            .presence_reply(answer.as_bytes());
        self.force = true;
    }

    /// What the reader reported of the live activities it shows
    /// (`Activities.kt`, LLP 1126 D13): a send's answer, or the list.
    pub fn activities(&mut self, json: &str) {
        if let Some(error) = self.p.activities(json) {
            self.p.host_mut().log(&error);
        }
        self.force = true;
    }

    /// What the activity asked for the session's orientation lock
    /// (`Orientation.kt`, LLP 1133.029 D6), and a line its resolution
    /// journals: a conflict, or a type this device does not show.
    pub fn orientation_applied(&mut self, applied: &str, conflict: bool, line: Option<&str>) {
        if let Some(line) = line {
            self.p.host_mut().runner_mut().log(line);
        }
        self.p.orientation_applied(applied.to_owned(), conflict);
        self.force = true;
    }

    /// Android's visibility and execution eligibility, composed by `Exact.executionEligible`
    /// from Activity and promoted service ownership. Release gated streams before returning.
    pub fn page_hidden(&mut self, hidden: bool, eligible: bool) {
        self.force |= self
            .p
            .page_execution(hidden, eligible, super::agent::seekable());
    }

    /// Whether the data module is active (`Native.dataActive`): 1, 0 while
    /// its image loads, -1 when it failed to activate. With `unshown`, a
    /// host no reader takes a frame from activates it now (LLP 1133.017 D7).
    pub fn data_active(&mut self, unshown: bool) -> i32 {
        if unshown {
            self.p.activate_unshown();
            self.force = true;
        }
        self.p.data_state()
    }

    /// The reader drew a frame (`Native.drew`): whether it should say so
    /// again after its next one ([`crate::presenter::Presenter::reader_drew`]).
    pub fn drew(&mut self) -> bool {
        self.p.reader_drew()
    }

    /// A package's call of the app's handler (`ExtensionContext.invoke`, LLP
    /// 1133.016 D15 item 1): its number, whose answer `Native.invoked` reads.
    pub fn module_invoke(&mut self, body: &str) -> i32 {
        let (id, answer) = crate::canvas::jni::jni_extensions::invoking();
        self.p.host_mut().runner_mut().invoke_module(body, answer);
        self.force = true;
        id
    }

    /// A topic the reader's extensions announced (`ExtensionContext.changed`,
    /// LLP 1016.002): its readers are asked again.
    pub fn extension_changed(&mut self, topic: &str) {
        self.p.host().runner().native_slot().changed(topic);
    }

    /// A person's act on what an extension posted on the platform's own UI
    /// (a notification's tap or action, `Native.activate`): its event,
    /// raised with a person's origin as Apple's notification delegate raises
    /// it (LLP 1133.004 D6, D7). `args` is a JSON array. An action's
    /// `action`, the declared record its post carried, raises only while the
    /// plan declares it, as Apple's categories are its current declarations.
    pub fn extension_activated(
        &mut self,
        extension: &str,
        event: &str,
        args: &str,
        action: Option<&str>,
    ) {
        let runner = self.p.host_mut().runner_mut();
        if let Some(action) = action {
            let record = serde_json::from_str(action).unwrap_or(Json::Null);
            if !runner.declares_action(extension, &record) {
                let name = record["action"].as_str().unwrap_or("");
                runner.log(format!(
                    "{extension}: the action \"{name}\" is not one this version of the app declares, and raised nothing"
                ));
                self.force = true;
                return;
            }
        }
        if let Err(e) =
            runner.raise_module_event(extension, event, args, exact_runner::CallOrigin::App)
        {
            runner.log(e);
        }
        self.force = true;
    }

    /// A person's press of the compiled control `kind`, installed with
    /// `digest`: a Quick Settings tile's, or with `foreground` the app's in
    /// front that the tile opened (`Native.press`, LLP 1126.004 D2, D5).
    /// `{"call": id}`, or `{"error"}`.
    pub fn press_control(
        &mut self,
        kind: &str,
        digest: &str,
        foreground: bool,
        cold: bool,
        hidden: bool,
    ) -> String {
        let how = exact_runner::CallHow {
            door: "android".into(),
            cold,
            hidden,
            digest: None,
            foreground,
        };
        let runner = self.p.host_mut().runner_mut();
        let reply = match runner.press_control(kind, digest, how) {
            Ok(id) => serde_json::json!({ "call": id }),
            Err(e) => {
                runner.log(format!("control:{kind}: {e}"));
                serde_json::json!({ "error": e })
            }
        };
        self.force = true;
        reply.to_string()
    }

    /// Press `id` ends: the tile that started it was superseded, the app it
    /// opened left the front, or the phone locked (`Native.cancelPress`).
    pub fn cancel_press(&mut self, id: u64) {
        let request = serde_json::json!({ "op": "cancel", "id": id }).to_string();
        let runner = self.p.host_mut().runner_mut();
        if let Some(Err(e)) = runner.call_agent("cancel", &request) {
            runner.log(format!("control: press {id}: {e}"));
        }
        self.force = true;
    }

    /// Whether press `id` still runs (`Native.pressLive`).
    pub fn press_live(&mut self, id: u64) -> bool {
        self.p.host_mut().runner_mut().press_live(id)
    }

    /// A line the reader journals (`Native.journal`).
    pub fn journal(&mut self, line: String) {
        self.p.host_mut().runner_mut().log(line);
        self.force = true;
    }

    /// The extensions the APK carries (`exact.app.ExactExtensions`, LLP
    /// 1133.001 D5), one a line: they answer their names from now on,
    /// outside the agent; under it, those `EXACT_AGENT_BIND` names
    /// (`--bind`) as in production, and the rest told they run under the
    /// agent, which answer only an interface that is its own substitute
    /// (LLP 1124 D7, LLP 1133.006 D8).
    pub fn bind_extensions(&mut self, names: &str) {
        let under_agent = crate::canvas::jni::jni_extensions::under_agent;
        let (under, real): (Vec<String>, Vec<String>) = (names.lines())
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .partition(|n| under_agent(n));
        // An interface's `owned` and `images.owned` are by target (LLP
        // 1133.019 D11 item 4).
        self.p.say_target("android");
        self.p.bind_modules(real, under);
        self.p.bind_pictures();
    }

    /// A package's picture for request `id` (`Native.picture`): its pixels,
    /// or the implementation's error.
    pub fn picture_answer(&mut self, id: u32, answer: Result<tiny_skia::Pixmap, String>) {
        self.force |= self.p.picture_answered(id, answer);
    }

    /// An extension announced picture `source` changed or revoked it
    /// (`ExtensionContext.changedPicture`, `revokePicture`).
    pub fn picture_changed(&mut self, source: &str, revoked: bool) {
        self.p.picture_changed(source, revoked);
        self.force = true;
    }
}
