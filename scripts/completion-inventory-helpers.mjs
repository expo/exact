// Synchronous runner APIs and their value helpers, plus the platform adapters that use them.
export function helperClassifications(group) {
  return [
  group("delivery", ["durable operation"], "installed app/update stream; session application is synchronous", "durable",
    "The named public runner-crate source is the delivery API or its value/admission helper. runner/src/runner/delivery.rs exposes the app delivery state; host delivery synchronization applies persisted stream facts to the current session.", [
    ["runner:runner/src/delivery.rs", "Delivery::field Delivery::with_compat production refuse_analysis"],
  ]),
  group("UI facts", ["one-shot"], "current session, viewport, page or rendered geometry", "UI-owned",
    "The named public runner-crate source is the UI facts API or its value/admission helper. runner/src/runner/viewport.rs and page.rs settle facts of the live session; these entries carry no app/account operation identity.", [
    ["runner:runner/src/design.rs", "batch digest Edit::json Edit::parse notice Scrub::older shape snapshot structure tokens"],
    ["runner:runner/src/desktop.rs", "buttons dock_menu presents status_items"],
    ["runner:runner/src/geometry.rs",
      "GeometryAnswer::of GeometryAnswer::value GeometryEnv::new GeometryEnv::read GeometryLinks::ahead GeometryLinks::new GeometryLinks::with_range_frame " +
      "Scrolled::note Scrolled::place Scrolled::shown Scrolled::top_layer Scrolled::top_layer_frames"],
    ["runner:runner/src/geometry/modal.rs", "Scrolled::top_layer_backdrop"],
    ["runner:runner/src/head.rs", "Head::fields"],
    ["runner:runner/src/orientation.rs", "conflict_line Lock::json session Session::lock types"],
    ["runner:runner/src/page.rs", "Page::bits Page::field Page::from_bits Page::gpu_tier Page::in_background Page::navigation_type Page::visibility_state"],
    ["runner:runner/src/runner/resize.rs", "ResizeRect::parse ResizeRect::value"],
    ["runner:runner/src/runner/forma.rs", "Forma::color Forma::dark Forma::font Forma::get Forma::has Forma::inputs Forma::length Forma::measures_json Forma::number Forma::tuple Token::new"],
    ["runner:runner/src/store.rs", "Store::clock_ms Store::entropy_draws Store::observe_entropy Store::set_clock_ms"],
    ["runner:runner/src/time.rs", "agent_seed launching launching_with Place::field Place::launched Place::validate posix_locales WallTime::field WallTime::validate without_launch_time"],
    ["runner:runner/src/viewport_reads.rs", "Media::holds Reads::any Reads::key Reads::media Reads::of Reads::reads"],
    ["runner:runner/src/viewport.rs",
      "Contrast::keyword DisplayMode::keyword even_segments Fold::validate Gamut::keyword Geometry::field Geometry::of Geometry::restore Hover::keyword " +
      "Pointer::keyword Posture::from_bits Posture::from_keyword Posture::keyword Preferences::bits Preferences::color_scheme Preferences::from_bits Update::keyword " +
      "Viewport::field Viewport::sized Viewport::validate"],
    ["runner:runner/src/wakelock.rs", "bound bound_prop holders"],
  ]),
  group("picker results", ["one-shot"], "current picker/share request and its app file roots", "UI-owned",
    "The named public runner-crate source is the picker results API or its value/admission helper. host/painted/src/picker.rs configures app file roots and copies/selects paths for the current picker/share. Persistent copies are separate from the UI continuation receiving them.", [
    ["runner:runner/src/file_pickers.rs", "answer_paths arm Kind::capability Kind::command Kind::of Picker::from_args Picker::summary request"],
    ["runner:runner/src/runner/picker.rs", "accepts answer_paths delivered_type extension extension_for mime_for Picked::line Picked::payload Picked::payload_under PickerRequest::json"],
    ["runner:runner/src/save_file.rs", "arm chosen_name covered request SaveFile::from_args SaveFile::summary"],
  ]),
  group("fills", ["one-shot"], "collection/view generation, window and fill token", "UI-owned",
    "The named public runner-crate source is the fills API or its value/admission helper. runner/src/runner/collection.rs validates feedback/fill against the current collection; instance/collection/index.rs versions rows and into_view.rs versions reveal requests.", [
    ["runner:runner/src/instance/collection/api.rs",
      "CollectionFeedback::decode CollectionFeedback::decode_with_fill CollectionFeedback::encode CollectionFeedback::encode_with CollectionSnapshot::json " +
      "CollectionSnapshot::write_json ListAxis::name snapshots_json"],
    ["runner:runner/src/instance/collection/into_view.rs", "Align::parse Tree::into_view_json"],
    ["runner:runner/src/instance/collection/mod.rs", "set_bootstrap_extent set_lead_scale"],
    ["runner:runner/src/instance/collection/nest.rs", "Tree::kept_positions_json"],
    ["runner:runner/src/instance/collection/reorder_api.rs", "ReorderEnding::name ReorderGeometry::decode ReorderGeometry::encode ReorderPhase::name ReorderStep::from_key ReorderToken::list ReorderToken::serial"],
    ["runner:runner/src/instance/collection/shown.rs", "RowsShown::extend"],
    ["runner:runner/src/instance/collection/traversal.rs", "Tree::collection Tree::collection_data Tree::collection_mounted Tree::collections Tree::collections_bounded Tree::collections_shallow"],
  ]),
  group("sockets and listens", ["subscription"], "source stream/watch and pending subscription in a session", "UI-owned",
    "The named public runner-crate source is the sockets and listens API or its value/admission helper. runner/src/runner/sockets.rs and listen.rs associate reader facts with source requests; retirement of the source removes the UI observer.", [
    ["runner:runner/src/listen.rs", "admitting EndReason::name grant_prefix grant_refusal Listen::refusal Standing::name under unix_path"],
    ["runner:runner/src/request_socket.rs", "Request::check_protocols Request::protocols Request::protocols_refusal"],
    ["runner:runner/src/request.rs", "Answer::stream Outcome::messages Request::listen Request::listen_refusal Request::long_poll Request::long_poll_refusal Request::socket"],
    ["runner:runner/src/store/socket.rs",
      "Socket::buffered_amount Socket::close Socket::is_open Socket::new Socket::resource Socket::send Socket::send_binary Socket::send_text Socket::ticket " +
      "Store::end_socket_call Store::note_socket_overload Store::set_socket_call Store::set_socket_open Store::socket Store::socket_call Store::socket_open " +
      "Store::socket_overload"],
  ]),
  group("module announcements", ["subscription"], "bound source/module and its active readers in this session", "UI-owned",
    "The named public runner-crate source is the module announcements API or its value/admission helper. runner/src/runner/time.rs changed/apply_announced settle source watches; modules/events.rs raises events into the live module bindings. No durable outcome record is provided.", [
    ["runner:runner/src/local_network.rs", "fact launch launch_once prompts Reach::refusal Reach::static_risk report State::parse State::word"],
    ["runner:runner/src/runner/source.rs", "Interrupt::trigger Native::changed Native::on_changed"],
  ]),
  group("media and sensors", ["subscription"], "current MIDI/sound/sensor session, device handle or scheduled sample", "UI-owned",
    "The named public runner-crate source is the media and sensors API or its value/admission helper. runner/src/runner/modules/samples.rs and runner/src/midi.rs number live sensor/device work; sound.rs owns voice/haptic IDs. No persisted operation record is created.", [
    ["runner:runner/src/midi.rs",
      "Access::from_word Access::word batch_op coalesce hex length messages Midi::clock Midi::field Midi::new Midi::open Midi::ports Midi::received Midi::recorded " +
      "Midi::sent Midi::take Midi::waiting Port::named Port::state Ports::connected_input Ports::connected_output Ports::from_json Ports::none Sent::state Sent::tick " +
      "split"],
    ["runner:runner/src/runner/midi.rs", "MidiMessage::value"],
    ["runner:runner/src/sound.rs", "By::word Haptic::state Sounds::evicted Sounds::haptics Sounds::live Sounds::take Sounds::voices Sounds::waiting Voice::state"],
    ["runner:runner/src/transport.rs",
      "Answer::field Segment::beat_at Segment::time State::word Transport::answer Transport::beat_at Transport::ended_due Transport::issued_to Transport::new " +
      "Transport::playing Transport::segments Transport::sequence Transport::wake_due"],
    ["runner:runner/src/transport/clock.rs", "Follow::last_tick Follow::position Follow::tempo Follow::window Transport::clock_input Transport::follower"],
  ]),
  group("presence storage", ["durable operation"], "app/account secret generation and backend queue", "durable",
    "The named public runner-crate source is the presence storage API or its value/admission helper. runner/src/presence.rs Backend serializes secret operations by name/generation; module presence installation adopts its occupancy. This class applies to backend storage, not a retired unlock continuation.", [
    ["runner:runner/src/presence.rs",
      "Backend::cancel Backend::checked Backend::checking Backend::enqueue Backend::generation Backend::new Backend::occupancies Backend::occupancy " +
      "Backend::plain_value Backend::pump Backend::snapshot_entries Backend::take_log MemoryVault::memory Occupancy::from_word Occupancy::locked Occupancy::word"],
    ["runner:runner/src/store/presence.rs",
      "Store::adopt_occupancy Store::call_snapshot Store::has Store::locked Store::occupancy Store::opened_locked Store::presence_capable Store::presence_names " +
      "Store::presence_of Store::set_presence_capable Store::take_locked_read"],
  ]),
  group("presence storage", ["durable operation"], "app/account secret generation and backend queue", "durable",
    "runner/src/presence.rs Backend serializes secret operations by name/generation; module presence installation adopts its occupancy. This class applies to backend storage, not a retired unlock continuation.", [
    ["runner:runner/src/presence.rs", "Backend::pump#reconcile_at:1 Backend::refused#retry:1 Backend::wait_move#retry_at:1 Backend::write#retry_at:1"],
  ]),
  group("replies", ["one-shot", "subscription", "durable operation"], "pending ticket and source turn; UI target by default", "UI-owned",
    "The named public runner-crate source is the replies API or its value/admission helper. runner/src/runner/reply.rs:27-88 checks pending, background, tool and module-work tickets; reply.rs:309 preserves effectful reply writes. Ordinary resource/then ownership is inferred from PendingReq.target in commit.rs:960.", [
    ["runner:runner/src/request.rs", "Reply::message Reply::new Reply::send Reply::stream"],
    ["runner:runner/src/upload.rs", "Message::sent Message::upload_progress Message::upload_start Message::upload_started Request::upload_progress Request::upload_progress_refusal"],
  ], "durable when the ticket names background/tool work or retained effectful module work; see reply.rs:70-88 and module_work.rs. Not all tool handlers survive session teardown."),
  group("storage", ["durable operation"], "app store contents; inspection/adoption is synchronous", "durable",
    "The named public runner-crate source is the storage API or its value/admission helper. runner/src/runner/kept.rs adopts stored values and retries store readers; runner/src/runner.rs take_store_writes drains committed writes. Stored effects are distinct from UI settlement (reply.rs:309).", [
    ["runner:runner/src/request.rs", "Request::storage"],
    ["runner:runner/src/store.rs",
      "Store::check_exclusive Store::copied_bytes Store::forget Store::forget_durable Store::get Store::granted Store::module_mirror Store::names Store::new " +
      "Store::new_linked Store::observe_external_read Store::observe_topic Store::read Store::reads Store::runner_owned Store::set Store::set_durable " +
      "Store::set_socket_buffered Store::snapshot Store::socket_buffered Store::stage_answer_socket Store::stage_socket Store::staged_sockets " +
      "Store::take_answer_sockets Store::take_sockets Store::take_topics Store::take_writes Store::unparsed Store::with_grants"],
  ]),
  group("wakes", ["durable operation"], "app action/wake invocation until expiration or end", "durable",
    "The named public runner-crate source is the wakes API or its value/admission helper. runner/src/runner/modules/wakes.rs raises and expires action/wake invocations independently of a mounted view; declared/wake_list.rs allocates their references. Persistence after process exit is not established here.", [
    ["runner:runner/src/request.rs", "Request::wake"],
  ]),
  group("UI input", ["one-shot"], "current mounted view, action binding or interaction", "UI-owned",
    "The named public runner-crate source is the UI input API or its value/admission helper. runner/src/runner/event.rs dispatches through a mounted view and its action binding; no independent operation identity is supplied.", [
    ["runner:runner/src/runner/event.rs",
      "ActionBinding::retained_utf8_bytes Event::checked_payload Event::focus_payload Event::height_release_payload Event::key Event::pan_payload " +
      "Event::pan_release_payload Event::press Event::pressed Event::record Event::scroll_payload Event::transform_geometry_payload Event::transform_release_payload " +
      "KeyModifiers::held KeyModifiers::split"],
    ["runner:runner/src/runner/event/frame_navigation.rs", "FrameNavigation::parse FrameNavigation::value other"],
    ["runner:runner/src/runner/event/keyboard.rs", "KeyboardEvent::chord KeyboardEvent::parse KeyboardEvent::value"],
    ["runner:runner/src/runner/field.rs", "Event::edit_payload Event::field_payload FieldSelection::at_end FieldSelection::caret FieldSelection::clamped FieldSelection::parse SelectionDirection::name SelectionDirection::of utf16_len"],
    ["runner:runner/src/runner/host_kinds.rs", "Event::clipboard_payload Event::media_payload Event::of_host_kind Event::paste_files_payload Event::selection_change_payload Event::selection_payload"],
    ["runner:runner/src/runner/modules/declared/desktop.rs", "chord_refusal"],
    ["runner:runner/src/runner/pointer.rs",
      "AtomEvent::parse AtomEvent::value DropEvent::parse DropEvent::parse_point DropEvent::value Event::contextmenu_payload Event::pointer_payload MouseEvent::parse " +
      "MouseEvent::value PointerEvent::parse PointerEvent::value WheelEvent::parse WheelEvent::value"],
    ["runner:runner/src/runner/reorder_codec.rs", "Event::reorder_drop_bytes Event::reorder_drop_payload"],
  ]),
  group("timer fires", ["subscription"], "session timer generation/deadline or one frame task", "UI-owned",
    "runner/src/runner/native_timers.rs fire_native_timer checks timer index/generation; gates.rs changes generations when task ownership changes. Advance/frame can also pump replies and deferred thens.", [
    ["runner:runner/src/runner/live_time.rs", "refresh#due:1 refresh#due:2"],
    ["runner:runner/src/runner/waits.rs", "Runner::fire_timer#k:1"],
  ]),
  group("native callbacks", ["one-shot", "subscription"], "module instance and granted handle or anchored invocation", "UI-owned",
    "The named public runner-crate source is the native callbacks API or its value/admission helper. runner/src/runner/modules/invoke.rs:1-22 anchors callbacks to handles/pending sends; Running::drop answers AbortError on session replacement. Final handlers may temporarily retain their anchor.", [
    ["runner:runner/src/runner/modules/admit.rs", "CallOrigin::word"],
  ]),
  group("wakes", ["durable operation"], "app action/wake invocation until expiration or end", "durable",
    "runner/src/runner/modules/wakes.rs raises and expires action/wake invocations independently of a mounted view; declared/wake_list.rs allocates their references. Persistence after process exit is not established here.", [
    ["runner:runner/src/runner/modules/declared/wake_list.rs", "Runner::wake_entry_ended#not_before:1 Runner::wake_entry_ended#not_before:2"],
  ]),
  group("presence", ["one-shot"], "outstanding presence call and secret generation", "UI-owned",
    "The named public runner-crate source is the presence API or its value/admission helper. runner/src/runner/modules/presence.rs:224-249 drops a check whose call was let go. The authentication continuation is session-owned even though the keystore value is durable.", [
    ["runner:runner/src/runner/modules/presence.rs", "PresenceAsk::json"],
  ]),
  group("holds", ["one-shot"], "held request/node until answer, cancellation or node retirement", "UI-owned",
    "runner/src/runner/device.rs:1-12 retires holds with the requesting node or ticket and refuses duplicate answers; the agent hold is not an executor request.", [
    ["runner:runner/src/runner/modules/sheet.rs", "Runner::sheet_call#asking:1"],
  ]),
  group("shared tokens", ["one-shot"], "share token in the live router; copying may outlive the initiating view", "UI-owned",
    "The named public runner-crate source is the shared tokens API or its value/admission helper. runner/src/runner/picker/shared.rs:596-630 updates only a token accepted by router.share. Copied files can survive, but this completion targets the current router token.", [
    ["runner:runner/src/runner/picker/shared.rs", "Claimed::copy Claimed::copy_with Claims::claim Claims::new is_token location Pending::from_json received_line shared_path SharedFiles::value ShareTarget::from_json token"],
  ]),
  group("module lifecycle", ["one-shot"], "module binding, source instance or session configuration", "UI-owned",
    "The named public runner-crate source is the module lifecycle API or its value/admission helper. runner/src/runner/modules/bound.rs binds live module instances; runner/src/runner/kept.rs data_ready reconciles the current source/store before settlement.", [
    ["runner:runner/src/runner/retirement.rs", "ModuleIdentity::instance ModuleIdentity::retire ModuleInstance::failure"],
    ["runner:runner/src/runner/source.rs",
      "Interrupt::new Native::extension_handler Native::handler Native::host Native::host_call Native::hosted Native::hosted_call Native::set Native::sync Native::sync_form Native::synced PreloadWake::ask " +
      "PreloadWake::set Target::mutation Target::text"],
  ]),
  group("navigation", ["one-shot"], "current router, visit, tab or navigation operation", "UI-owned",
    "The named public runner-crate source is the navigation API or its value/admission helper. runner/src/runner/router.rs validates visits against the live router; replacement/retirement changes that owner.", [
    ["runner:runner/src/runner/router.rs", "launch_matches RouterChange::location routing"],
    ["runner:runner/src/runner/split.rs", "press_writes_router"],
  ]),
  group("timer fires", ["one-shot"], "session timer generation/deadline or one frame task", "UI-owned",
    "The named public runner-crate source is the timer fires API or its value/admission helper. runner/src/runner/native_timers.rs fire_native_timer checks timer index/generation; gates.rs changes generations when task ownership changes. Advance/frame can also pump replies and deferred thens.", [
    ["runner:runner/src/runner/waits.rs", "frame_at_or_after IdleObservation::allows virtual_frame WaitKinds::from_bits WaitKinds::union Waits::next_check"],
  ]),
  group("share results", ["one-shot"], "share-sheet request or command until reported completion/cancellation", "UI-owned",
    "The named public runner-crate source is the share results API or its value/admission helper. host/apple/src/abi/exports.rs exact_share/ exact_share_land and host/painted/src/presenter/requests.rs associate share UI with its pending command; this is separate from incoming router share tokens.", [
    ["runner:runner/src/share.rs", "arm request Share::from_args Share::from_json Share::summary"],
  ]),
  group("region and text", ["one-shot"], "text request/source, font recipe, paint context and current paragraph/cache residency", "UI-owned",
    "The named public runner-crate source is the region and text API or its value/admission helper. host/painted/src/text.rs and text/transfer.rs prepare and adopt text for its request, source and paint context; text cache and layout helpers retain those presentation values, not app outcomes.", [
    ["runner:runner/src/surface_record.rs", "answer decode decode_published keep_signature oversize surface_name"],
  ]),
  group("media and sensors", ["subscription"], "current MIDI/sound/sensor session, device handle or scheduled sample", "UI-owned",
    "runner/src/runner/modules/samples.rs and runner/src/midi.rs number live sensor/device work; sound.rs owns voice/haptic IDs. No persisted operation record is created.", [
    ["runner:runner/src/transport.rs", "Transport::fill#refill_due:1 Transport::fill#skip:1"],
    ["runner:runner/src/transport/clock.rs", "Follow::feed#held:1 Follow::feed#spp:1 Transport::follow#clock_to:1"],
  ]),
  group("presentation helpers", ["one-shot"], "current painted frame, geometry, style, ink cache or painter", "UI-owned",
    "host/web-js/module.mjs buildMarkdown emits alloc, pieces and output for one loaded Markdown capability. The exports allocate caller bytes, render Markdown pieces and expose the retained output buffer; they deliver no runner completion.", [
    ["web-js:host/web-js/module.mjs", "alloc output pieces"],
  ]),
  group("inspection", ["one-shot"], "synchronous session journal or diagnostics", "UI-owned",
    "runner/src/runner/lines.rs owns the current session's journal; canvas requests.rs forwards the reader's diagnostic line to it.", [
    ["runner:runner/src/runner/lines.rs", "Runner::journal Runner::journal_start Runner::journal_echo"],
    ["runner:runner/src/runner/console.rs", "Console::mark_unjournaled"],
    ["runner:runner/src/runner/faults.rs", "Runner::fetch_fault Runner::share_faults Runner::take_fault_logs"],
    ["painted:host/painted/src/executor.rs", "Executor::set_fetch_fault"],
    ["painted:host/painted/src/canvas/jni_extensions.rs", "Java_dev_exact_canvas_Native_journal"],
    ["painted:host/painted/src/canvas/requests.rs", "CanvasHost::journal"],
  ]),
  group("session lifecycle", ["one-shot"], "host/session startup and platform configuration", "UI-owned",
    "canvas/jni_start.rs starts or adopts the process's canvas session; canvas/tls.rs installs its Android trust provider.", [
    ["painted:host/painted/src/canvas/jni_start.rs", "Java_dev_exact_canvas_Native_adopt Java_dev_exact_canvas_Native_start Java_dev_exact_canvas_Native_startAsync"],
    ["painted:host/painted/src/canvas/tls.rs", "initialize Java_dev_exact_canvas_Native_trust"],
    ["runner:runner/src/launch_env.rs", "value"],
  ]),
  group("delivery", ["durable operation"], "installed app/update stream and its current reader", "durable",
    "painted delivery.rs resolves the installed reader root and refuses a plan outside the linked cohort; presenter/checks.rs schedules checks and acknowledges a drawn generation.", [
    ["painted:host/painted/src/canvas.rs", "CanvasHost::update_fd"],
    ["painted:host/painted/src/canvas/jni_start.rs", "Java_dev_exact_canvas_Native_assetRoot"],
    ["painted:host/painted/src/canvas/jni_extensions.rs", "Java_dev_exact_canvas_Native_drew"],
    ["painted:host/painted/src/canvas/requests.rs", "CanvasHost::drew"],
    ["painted:host/painted/src/delivery.rs", "reader_root set_reader_root within_linked"],
    ["painted:host/painted/src/presenter/checks.rs", "Checks::completed Checks::due Checks::foreground Checks::from_compat Checks::tick"],
    ["painted:host/painted/src/presenter/delivery.rs", "Presenter::reader_drew Presenter::set_reader_blesses Presenter::update_check_due Presenter::update_foreground Presenter::update_launched"],
  ]),
  group("navigation", ["one-shot"], "current router, route part or presented transition", "UI-owned",
    "runner/router.rs host_root commits the current router's host root; painted stage/authored.rs retains the route's transition parts until arrival.", [
    ["apple:host/apple/src/abi/exports.rs", "exact_host_root exact_route_part"],
    ["runner:runner/src/runner/router.rs", "Runner::host_root"],
    ["painted:host/painted/src/flights.rs", "Host::flying Host::start_flights"],
    ["painted:host/painted/src/presenter/arrive.rs", "Presenter::focus_landed"],
    ["painted:host/painted/src/presenter/reveal.rs", "Presenter::reveal_flights"],
    ["painted:host/painted/src/presenter/stage/authored.rs", "Presenter::drawn_at Presenter::entrance Presenter::hold_arriving Presenter::top_of"],
  ]),
  group("module announcements", ["subscription"], "bound module, app topic and its current readers", "UI-owned",
    "Store::announce_topic records changed app topics; exact_peer_changed delivers them to another session. Canvas extension activation checks the current plan's declared action before raising its event.", [
    ["apple:host/apple/src/abi/exports.rs", "exact_peer_changed"],
    ["runner:runner/src/store.rs", "Store::announce_topic Store::take_announced"],
    ["runner:runner/src/runner/modules/wakes.rs", "Runner::declares_action"],
    ["painted:host/painted/src/canvas/jni_extensions.rs", "Java_dev_exact_canvas_Native_activate"],
    ["painted:host/painted/src/canvas/requests.rs", "CanvasHost::extension_activated"],
  ]),
  group("region and text", ["one-shot"], "current layout region, paragraph or content measurement", "UI-owned",
    "exact_region_fallback and exact_text_paragraph_bases answer the current region and text request; painted stage.rs fit_content measures the current sheet.", [
    ["apple:host/apple/src/abi/exports.rs", "exact_region_fallback"],
    ["apple:host/apple/src/textflow.rs", "exact_text_paragraph_bases"],
    ["painted:host/painted/src/presenter/stage.rs", "Presenter::fit_content"],
  ]),
  group("UI facts", ["one-shot"], "current session's launch and presentation configuration", "UI-owned",
    "Apple ABI setters configure the current Bridge. chooses_scheme reads the plan's command use, without starting work.", [
    ["apple:host/apple/src/abi/exports.rs", "exact_flat_nodes exact_set_launch_direction exact_set_launch_window exact_set_natural_text"],
    ["runner:runner/src/uses.rs", "chooses_scheme"],
    ["web:host/web/src/abi.rs", "exact_chooses_scheme"],
  ]),
  group("runner helpers", ["one-shot"], "synchronous query, request value or caller-owned evaluator state", "UI-owned",
    "The named APIs inspect current owners/resources, record render dependencies or build request values. sandbox.rs and request_preload.rs decide admission; no helper delivers a completion.", [
    ["runner:runner/src/canvas_capture.rs", "Encoding::lossy_quality Encoding::new media_type_of Request::capture_canvas_as"],
    ["runner:runner/src/desktop.rs", "image_node label"],
    ["runner:runner/src/instance/owners.rs", "Tree::owners"],
    ["runner:runner/src/request_preload.rs", "secret_in_origin"],
    ["runner:runner/src/runner.rs", "Runner::plan_site_of"],
    ["runner:runner/src/runner/checkpoint.rs", "Runner::record_render_request"],
    ["runner:runner/src/runner/owned.rs", "Runner::bake_launch_answers Runner::instance_resources"],
    ["runner:runner/src/runner/source.rs", "Native::library_load_time Native::record_library_load"],
    ["runner:runner/src/sandbox.rs", "device line"],
    ["runner:runner/src/stdlib.rs", "format_time_two_digit"],
    ["runner:runner/src/uses.rs", "sources_can_be_remote"],
    ["runner:runner/src/vm.rs", "Owned::request Owned::send Owned::shared"],
  ]),
  group("UI input", ["one-shot"], "current focus, menu, accessibility node or reader hit target", "UI-owned",
    "painted presenter routes accessibility, keys and reader hit tests through the live host; top/status.rs owns the current status menu and its input.", [
    ["painted:host/painted/src/presenter/access.rs", "Presenter::access_range_keys Presenter::access_step_range Presenter::access_tables"],
    ["painted:host/painted/src/presenter/events.rs", "Presenter::key_event_at"],
    ["painted:host/painted/src/presenter/scrollers.rs", "Presenter::reader_module"],
    ["painted:host/painted/src/presenter/tabs.rs", "Presenter::type_ahead_key"],
    ["painted:host/painted/src/presenter/top.rs", "Presenter::surface_type_ahead"],
    ["painted:host/painted/src/presenter/top/invoke.rs", "Presenter::context_menu_at"],
    ["painted:host/painted/src/presenter/top/status.rs", "Presenter::dismiss_status_menu Presenter::open_status_menu Presenter::status_menu_key Presenter::status_menu_left Presenter::status_menu_pointer Presenter::status_menu_wheel Presenter::status_press Presenter::take_status_exit"],
  ]),
  group("presentation helpers", ["one-shot"], "current painted frame, geometry, menu or paint resource", "UI-owned",
    "paint/shared.rs retains the host's current flight frames; top/status.rs projects and paints the live menu. These helpers create no runner request.", [
    ["painted:host/painted/src/bars.rs", "Host::place_status_items"],
    ["painted:host/painted/src/paint_motion.rs", "Host::presented_filter"],
    ["painted:host/painted/src/paint/shape.rs", "PaintedBox::contains"],
    ["painted:host/painted/src/paint/shared.rs", "SharedState::update"],
    ["painted:host/painted/src/presenter/alert.rs", "Presenter::alert_anchor_scroll Presenter::backdrop"],
    ["painted:host/painted/src/presenter/top.rs", "Presenter::surface_clock"],
    ["painted:host/painted/src/presenter/top/status.rs", "Presenter::place_status_items Presenter::status_icon Presenter::status_items Presenter::status_menu Presenter::status_menu_pixels Presenter::status_popover Presenter::status_quit_removed Presenter::with_exit"],
  ]),
  ];
}
