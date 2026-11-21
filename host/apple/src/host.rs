//! The runner wrapped for a presenter: receipts → batches, with layout and motion.
//!
//! @ref LLP 1008 §1
//!
//! Each commit projects receipts onto views, seeks motion to the app's clock,
//! and lays out the kernel roots. Eligible iOS list-row wrappers are omitted
//! from the view tree (LLP 1171 D4). Layout projects the registered Height,
//! then emits changed frames and scroll content sizes; other presentation
//! values follow. The mirror remembers what the presenter has been told.

#[path = "host_replies.rs"]
mod replies;
use crate::{batch::Batch, style};
use exact_kernel::motion::{motion_node, node_key, targets, MotionSync};
use exact_kernel::{
    CommitReceipt, Frame, Kernel, NodeKey, NodeRef, NodeType, Offer, Overflow, PropId, PropValue,
    TextMeasurer, ViewId,
};
use exact_motion::{Change, Engine, HoldToken, Property};

#[path = "activities.rs"]
mod activities;
#[path = "agent.rs"]
mod agent;
#[path = "arrange.rs"]
mod arrange;
#[path = "arrange_group.rs"]
mod arrange_group;
#[cfg(test)]
#[path = "arrange_group_tests.rs"]
mod arrange_group_tests;
#[cfg(test)]
#[path = "arrange_tests.rs"]
mod arrange_tests;
#[path = "canvas2d.rs"]
pub(crate) mod canvas2d;
#[path = "canvas_capture.rs"]
pub(crate) mod canvas_capture;
#[path = "canvas_performance.rs"]
mod canvas_performance;
#[path = "colors.rs"]
mod colors;
#[path = "commit.rs"]
mod commit;
#[path = "content_region/host.rs"]
mod content_region_host;
pub use content_region_host::LinkedRegions;
#[cfg(test)]
#[path = "control_text_tests.rs"]
mod control_text_tests;
#[path = "covers.rs"]
mod covers;
#[path = "first_frame.rs"]
mod first_frame;
#[cfg(test)]
#[path = "first_frame_tests.rs"]
mod first_frame_tests;
#[path = "flat.rs"]
mod flat;
#[path = "flights.rs"]
mod flights;
#[path = "fold.rs"]
mod fold;
#[path = "height.rs"]
mod height;
#[path = "height_drag.rs"]
mod height_drag;
#[cfg(test)]
#[path = "height_tests.rs"]
mod height_tests;
#[path = "holds.rs"]
mod holds;
#[path = "live.rs"]
mod live;
#[path = "midi.rs"]
mod midi;
#[path = "paint.rs"]
mod paint;
#[path = "paragraph.rs"]
mod paragraph;
#[path = "replacement.rs"]
mod replacement;
#[path = "root_tabs.rs"]
mod root_tabs;
#[cfg(test)]
#[path = "root_tabs_tests.rs"]
mod root_tabs_tests;
use paragraph::props_for;
#[path = "forma.rs"]
mod forma;
#[path = "persistence.rs"]
mod persistence;
#[path = "presence.rs"]
mod presence;
#[path = "region_seam.rs"]
pub mod region_seam;
#[path = "renew.rs"]
mod renew;
#[path = "route_overflow.rs"]
mod route_overflow;
#[path = "route_parts.rs"]
mod route_parts;
#[cfg(test)]
#[path = "transform_drag_tests.rs"]
mod transform_drag_tests;
pub use covers::{OwnerReports, Restyle};
use exact_plan::EventKind;
use exact_runner::{Carried, DataSource, Event, Outcome, RequestOut, Runner, RunnerError, Timed};
pub use height::{HeightOwnerChange, HeightOwnerDisposition, HeightOwnerError};
use height_drag::{HeightDrag, HeightHandle};
#[path = "activation.rs"]
mod activation;
#[cfg(test)]
#[path = "activation_tests.rs"]
mod activation_tests;
#[cfg(test)]
#[path = "box_motion_tests.rs"]
mod box_motion_tests;
#[cfg(test)]
#[path = "canvas_link_tests.rs"]
mod canvas_link_tests;
#[cfg(test)]
#[path = "filter_motion_tests.rs"]
mod filter_motion_tests;
#[path = "layout.rs"]
mod layout;
#[cfg(test)]
#[path = "turn_motion_tests.rs"]
mod turn_motion_tests;
use layout::{content_size, published_content_box};
#[path = "resize.rs"]
mod resize;
#[path = "scale.rs"]
mod scale;
#[path = "share_in.rs"]
mod share_in;
#[path = "snapshot.rs"]
pub(crate) mod snapshot;
pub(crate) use scale::LaunchLayout;
#[cfg(test)]
#[path = "spacing_tests.rs"]
mod spacing_tests;
#[cfg(test)]
#[path = "storage_tests.rs"]
mod storage_tests;
#[path = "svg.rs"]
pub(crate) mod svg;
#[path = "svg_lower.rs"]
mod svg_lower;
#[cfg(test)]
#[path = "svg_tests.rs"]
mod svg_tests;
#[path = "tables.rs"]
mod tables;
#[cfg(test)]
#[path = "tables_tests.rs"]
mod tables_tests;
#[path = "timeline_scroll.rs"]
mod timeline_scroll;
#[cfg(test)]
#[path = "timeline_scroll_tests.rs"]
mod timeline_scroll_tests;
#[path = "transform_drag.rs"]
mod transform_drag;
#[path = "transform_drag_wire.rs"]
mod transform_drag_wire;
use crate::store::Platform;
use exact_kernel::id::{IdMap, IdSet};
use std::collections::{BTreeMap, BTreeSet};
use transform_drag::TransformDrags;

/// Why the host refused.
#[allow(missing_docs)]
#[derive(Debug)]
pub enum HostError {
    Plan(exact_plan::PlanError),
    Runner(RunnerError),
    Layout(String),
    Delivery(String),
    /// The plan uses capabilities this archive doesn't link, by name
    /// (LLP 1047.001 D5): `Unlinked("grouped_lists")`.
    Unlinked(String),
    RuntimeIdExhausted,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Mirror {
    kind: Option<&'static str>,
    props: exact_kernel::SortedMap<&'static str, String>,
    /// The style the presenter last heard, one allocation per text ([`paragraph::StyleTexts`]).
    style: std::rc::Rc<str>,
    children: Box<[ViewId]>,
    frame: Option<(f32, f32, f32, f32)>,
    content: Option<(f32, f32)>,
    fit_height: Option<f32>,
    flow: Box<[exact_kernel::FlowShape]>,
    /// Boxed: only a text field has one, and inline it was a quarter of the mirror.
    field_content: Option<Box<crate::batch::ContentBox>>,
    field_cell: bool,
    block: Option<(f32, f32)>,
    /// Whether the block is the kernel's (a popover's, the viewport): a
    /// host that gives the popover its own is sent the used size again.
    implied: bool,
}

/// What a source's `captureCanvas` is answered with where canvases are not
/// linked: the plan has none to capture.
pub(crate) const UNLINKED_SURFACES: &str =
    "Unlinked(\"surfaces\"): this binary links no canvas, which a capture reads; a plan with a canvas links them";

/// One runner, one presenter.
pub struct Host<D: DataSource> {
    pub(crate) preloads: Option<Box<dyn crate::launch_preloads::LaunchWork>>,
    pub(crate) launch_store: Option<std::sync::Arc<crate::launch_preloads::StoreWatch>>,
    /// Tell the app's widgets its store changed: the I/O link's, which keeps
    /// the store they read; none without I/O.
    widget_changed: Option<fn(&str)>,
    boot_requests: Vec<RequestOut>,
    preload_content: Vec<String>,
    preload_accepted: bool,
    runner: Runner<D>,
    admitting_frame: bool,
    font_hook: Option<(crate::measure::FontsFn, *mut std::ffi::c_void, usize)>,
    /// Whether the plan declares a `head`, and the title the presenter was
    /// last told (LLP 1048.003 D1): the window or scene title.
    has_heads: bool,
    head_title: Option<String>,
    /// The active head's `edited` (LLP 1069.010 D6), sent with the title.
    head_edited: bool,
    language: Option<String>,
    /// Boxed: a mirror is 128 bytes, and the table has up to twice as many slots as views.
    mirror: IdMap<ViewId, Box<Mirror>>,
    style_texts: paragraph::StyleTexts,
    keys: IdMap<NodeKey, ViewId>,
    inline_runs: IdMap<ViewId, (ViewId, Vec<EventKind>)>,
    flat: flat::Flat,
    /// Hatch words: those journaled as viewless here, and those the plan does not give this platform.
    hatches: paragraph::HatchWords,
    /// SVG scenes and lowered CSS animations (LLP 1055 D4, D7).
    svg: svg::SvgState,
    /// 2D canvases whose replays the presenter has not caught up with: a
    /// frame request waits for them (`exact_canvas_held`, LLP 1056 D5).
    canvas_held: IdSet<ViewId>,
    canvas_kept: [Vec<exact_runner::CanvasList>; 2],
    /// Image handles the presenter was asked to decode and may hold,
    /// decoded or failed (LLP 1056 D9).
    canvas_images: BTreeSet<String>,
    /// Canvas draws run in a turn of their own (`exact_canvas_draw`), not in
    /// the turns main waits on (LLP 1072 §8.5).
    canvas_deferred: bool,
    /// This session's display scale for Canvas 2D (LLP 1056 D4).
    canvas_scale: f64,
    dirty_paragraphs: BTreeSet<ViewId>,
    pending_layout: IdSet<NodeKey>,
    layout_withheld: bool,
    /// A layout failed or a commit's did not run: `inert_layout` lays out.
    layout_owed: bool,
    /// Committing where only the commits move the tree: an advance
    /// ([`Host::advanced`]), a list's feedback, an event's action. Any other
    /// entry changes what layout reads besides the tree.
    inert_entry: bool,
    /// Routes whose `navigationDetent` names `fit-content` (LLP 1075.003
    /// §9.11), each with its width, resolved padding, border and bottom cover when last measured.
    fit_routes: Vec<(ViewId, Option<[f32; 10]>)>,
    /// Each sticky node's constraint as the presenter last heard it (LLP 1083).
    stickies: IdMap<ViewId, exact_kernel::StickyConstraint>,
    /// The scrollers scroll and view timelines read (LLP 1057.003 D5).
    timeline_scrolls: timeline_scroll::TimelineScrolls,
    /// Each multi-column record as the presenter last heard it (LLP 1093 D7).
    fragments: IdMap<ViewId, String>,
    /// Each table's and table part's record as the presenter last heard it
    /// (`tables.rs`).
    tables: IdMap<ViewId, String>,
    ranks: IdMap<ViewId, i64>,
    roots: Vec<ViewId>,
    /// Last published common collection snapshot; refreshed only after layout.
    collections_json: String,
    collection_heads: Option<Vec<exact_runner::CollectionSnapshot>>,
    /// Section indices and mounted cells in the last batch for Mac split lists.
    #[cfg(target_os = "macos")]
    section_tables: std::collections::BTreeMap<ViewId, exact_runner::list_api::SectionTable>,
    engine: Engine,
    paint: paint::Paint,
    holds: BTreeMap<u64, HoldToken>,
    /// Canvas captures the presenter is encoding, or refused (`canvas_capture.rs`).
    canvas_capture_waiting: Vec<(RequestOut, Option<String>)>,
    canvas_captures: BTreeMap<u64, canvas_capture::Pending>,
    height_owner: Option<NodeKey>,
    height_handles: BTreeMap<NodeKey, HeightHandle>,
    height_auto_owned: bool,
    height_drag: Option<HeightDrag>,
    transform_drags: TransformDrags,
    /// The one Arrange contact, from its catch until its source settles.
    arrange: Option<arrange::Arrange>,
    /// A grouped session (LLP 1094), from its lift until its ghost lands.
    group: Option<arrange_group::GroupArrange>,
    presence: presence::Presence,
    flights: flights::Flights,
    /// The content region, which only [`LinkedRegions`] makes.
    content_region: Option<Box<dyn std::any::Any>>,
    /// The content-region seam (LLP 1047.001 §9.8).
    regions: &'static dyn region_seam::ContentRegions<D>,
    height_projection: Vec<(NodeKey, f32)>,
    height_sampling: Vec<(NodeKey, f32)>,
    height_presented: Vec<exact_kernel::PresentedHeight>,
    height_transitions: BTreeMap<NodeKey, Option<exact_kernel::Dimension>>,
    height_transition_epoch: Option<u64>,
    height_targets_dirty: bool,
    #[cfg(test)]
    height_target_passes: usize,
    #[cfg(test)]
    pub(crate) layout_calls: usize,
    viewport: (f32, f32),
    route_overflow: route_overflow::Journal,
    now_ms: f64,
    /// Where the app's kept secrets, and the runner's kept answers, go after
    /// a commit (LLP 1018 D6); `None` keeps them in the runner only (a test,
    /// or grants that do not parse).
    secrets: Option<Box<dyn crate::store::KeptStore>>,
    pending_store_writes: Vec<exact_runner::StoreWrite>,
    /// A store write landed since the last batch was written (`Batch::stored`).
    stored: bool,
    pending_store_stamps: std::collections::BTreeMap<String, exact_data::secrets::Stamp>,
    /// What the disk held for each owed name when its write first failed.
    pending_store_bases: std::collections::BTreeMap<String, Option<Vec<u8>>>,
    reported_store_failures: BTreeSet<String>,
    data_activated: bool,
    /// The source's activation, run ahead of `activate_data` by `prepare_data`.
    source_prepared: Option<Result<(), String>>,
    store_refresh_pending: bool,
    store_activation_pending: bool,
    store_retry_names: Vec<String>,
    /// The session's wake (`listen`), which a pending activation leaves with the data source.
    preload_wake: exact_runner::PreloadWake,
    doc_watch: Option<crate::doc_watch::DocWatch>,
    /// A turn's family work after its commits, in its written order.
    pub(crate) turn: crate::link::TurnLinks<D>,
    /// Deadline at the last successful finish with no pending resize notifications;
    /// only a receipt-free collection report can reuse its settled passes.
    finished_deadline: Option<Option<f64>>,
    quiet_collection: Option<([u64; 8], String)>,
    resize_pending: bool,
    #[cfg(test)]
    finish_passes: usize,
    #[cfg(test)]
    pub(crate) batch_writes: usize,
    /// The update store's last line this host journaled, so a sync after a check writes it once.
    update_line: Option<String>,
    delivery: Option<&'static crate::delivery::Hooks>,
    /// The reported colours' generation this session last presented; `None`
    /// before its first report, which corrects without motion (LLP 1095 D9).
    colors_seen: Option<u64>,
    /// The root font size and the window's modes as a batch last said them (`Host::facts_moved`).
    facts_told: Option<(f64, Vec<(String, String)>)>,
    /// The navigation roots' tabs as a batch last said them (`Host::navigation_shapes`).
    root_tabs_told: Option<String>,
    route_inspectors_told: Vec<(ViewId, exact_runner::RouteInspector)>,
    /// The holds as a batch last said them (`Host::wakelock_moved`, `Host::orientation_moved`, `Host::finish_moved`).
    holds_told: colors::Told,
    /// The top-layer boxes covered while closed (`closed.rs`).
    closed: layout::closed::Closed,
}

pub(crate) use crate::plan_bytes::PlanBytes;

impl<D: DataSource> Host<D> {
    /// Boot from plan bytes with the app's text measurer and viewport (points):
    /// decode (a validation pass), boot the runner, lay out, and produce the
    /// first batch, which creates and places the whole tree.
    pub fn boot(
        plan_bytes: &[u8],
        data: D,
        measurer: Box<dyn TextMeasurer>,
        width: f32,
        height: f32,
    ) -> Result<(Host<D>, String), HostError> {
        Host::boot_stored(
            plan_bytes,
            data,
            measurer,
            width,
            height,
            None,
            Vec::new(),
            None,
        )
    }
    /// Boot carrying an earlier host's state (the dev reload, LLP 1007 §6):
    /// slots by name where their types still fit, settled resources where
    /// their arguments still match, the clock.
    pub fn boot_with(
        plan_bytes: &[u8],
        data: D,
        measurer: Box<dyn TextMeasurer>,
        width: f32,
        height: f32,
        carried: Option<&Carried>,
    ) -> Result<(Host<D>, String), HostError> {
        Host::boot_stored(
            plan_bytes,
            data,
            measurer,
            width,
            height,
            carried,
            Vec::new(),
            None,
        )
    }
    /// Boot with the app's kept secrets (LLP 1018 D6): `snapshot` is what
    /// the platform's store holds under the granted names, read before this
    /// call (a carried boot takes the carried store instead); `secrets` is
    /// where the commits' writes go after each commit: secrets on this
    /// thread, kept answers on their writer's (`store::flush_kept`).
    #[allow(clippy::too_many_arguments)]
    pub fn boot_stored(
        plan_bytes: &[u8],
        data: D,
        measurer: Box<dyn TextMeasurer>,
        width: f32,
        height: f32,
        carried: Option<&Carried>,
        snapshot: Vec<(String, Vec<u8>)>,
        secrets: Option<Platform>,
    ) -> Result<(Host<D>, String), HostError> {
        let (mut host, batch) = Host::boot_stored_after_decode(
            PlanBytes::Copied(plan_bytes),
            data,
            measurer,
            scale::LaunchLayout::NONE,
            exact_runner::Viewport::sized(width as f64, height as f64),
            carried,
            snapshot,
            secrets.map(|p| Box::new(p) as Box<dyn crate::store::KeptStore>),
            None,
            None,
            None,
            "/",
            false,
            None,
            crate::link::Links::ALL,
            None,
            |_| Ok(()),
        )?;
        host.commit_boot();
        Ok((host, batch))
    }
    /// Boot with one action over the accepted plan before its first layout.
    /// The Apple ABI uses this for synchronous font registration without decoding the plan twice.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn boot_stored_after_decode(
        plan_bytes: PlanBytes<'_>,
        data: D,
        measurer: Box<dyn TextMeasurer>,
        layout: scale::LaunchLayout,
        viewport: exact_runner::Viewport,
        carried: Option<&Carried>,
        snapshot: Vec<(String, Vec<u8>)>,
        secrets: Option<Box<dyn crate::store::KeptStore>>,
        compat: Option<&str>,
        delivery: Option<&'static crate::delivery::Hooks>,
        candidate_delivery: Option<exact_runner::Delivery>,
        launch: &str,
        start_on_frame: bool,
        region: Option<crate::content_region::ContentRegionRegistration>,
        links: crate::link::Links<D>,
        font_hook: Option<(crate::measure::FontsFn, *mut std::ffi::c_void)>,
        prepare: impl FnOnce(&mut Runner<D>) -> Result<(), HostError>,
    ) -> Result<(Host<D>, String), HostError> {
        Self::boot_stored_after_decode_mode(
            plan_bytes,
            data,
            measurer,
            layout,
            viewport,
            carried,
            snapshot,
            secrets,
            compat,
            delivery,
            candidate_delivery,
            launch,
            start_on_frame,
            region,
            None,
            None,
            links,
            font_hook,
            prepare,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn boot_stored_after_decode_mode(
        plan_bytes: PlanBytes<'_>,
        mut data: D,
        measurer: Box<dyn TextMeasurer>,
        layout: scale::LaunchLayout,
        viewport: exact_runner::Viewport,
        carried: Option<&Carried>,
        snapshot: Vec<(String, Vec<u8>)>,
        secrets: Option<Box<dyn crate::store::KeptStore>>,
        compat: Option<&str>,
        delivery: Option<&'static crate::delivery::Hooks>,
        candidate_delivery: Option<exact_runner::Delivery>,
        launch: &str,
        start_on_frame: bool,
        region: Option<crate::content_region::ContentRegionRegistration>,
        native: Option<crate::content_region::NativeProjectionLimits>,
        previous: Option<&Self>,
        links: crate::link::Links<D>,
        font_hook: Option<(crate::measure::FontsFn, *mut std::ffi::c_void)>,
        prepare: impl FnOnce(&mut Runner<D>) -> Result<(), HostError>,
    ) -> Result<(Host<D>, String), HostError> {
        if let Some(json) = compat {
            exact_runner::delivery::refuse_analysis(json)
                .map_err(|why| HostError::Delivery(why.into()))?;
            let expected = exact_runner::Delivery::default().with_compat(json).store != '0';
            if expected != delivery.is_some() {
                return Err(HostError::Delivery("the baked store level does not match the linked delivery adapter; regenerate the app entry".into()));
            }
        }
        let plan = plan_bytes.decode().map_err(HostError::Plan)?;
        let entry_props = match plan_bytes {
            PlanBytes::Entry(_, _, props, configuration)
                if props.is_some() || configuration.is_some() =>
            {
                Some(snapshot::slots(&plan, props, configuration).map_err(HostError::Layout)?)
            }
            _ => None,
        };
        let carried = entry_props.as_ref().or(carried);
        // Before anything is built from it (LLP 1047.001 D5).
        if let Some(names) = crate::link::missing(&plan, crate::link::linked()) {
            return Err(HostError::Unlinked(names));
        }
        // This archive links its grammars; every public boot links all (LLP 1047.001 D3; 1053.000 §2).
        links.grammars.link();
        let mut kernel = scale::kernel(measurer);
        layout.prepare(&mut kernel)?;
        let facts = candidate_delivery.unwrap_or_else(|| {
            let mut facts = exact_runner::Delivery::default();
            if let Some(json) = compat {
                facts = facts.with_compat(json);
                if let Some(hooks) = delivery {
                    (hooks.status_into)(&mut facts);
                }
            }
            facts
        });
        // An `app:/data` image shows from the first frame, before storage
        // is configured and whether or not anything was picked (D7).
        crate::picker::know_roots(data.app_id());
        if let Some(store) = &secrets {
            store.seed_snapshot(
                carried
                    .filter(|c| !c.fresh_state)
                    .map_or(snapshot.as_slice(), |carried| &carried.store),
            );
        }
        if let Some(shared) = secrets.as_ref().and_then(|store| store.shared_secrets()) {
            data.configure_secrets(shared)
                .map_err(|error| HostError::Layout(format!("shared secrets: {error:?}")))?;
        }
        // A display presents its pictures: `presented` waits for the first's.
        let was = exact_runner::page::set_awaits_present(start_on_frame);
        let platform = if cfg!(target_os = "macos") {
            exact_runner::page::Platform::Macos
        } else if cfg!(target_os = "tvos") {
            exact_runner::page::Platform::Tvos
        } else if cfg!(target_os = "watchos") {
            exact_runner::page::Platform::Watchos
        } else {
            exact_runner::page::Platform::Ios
        };
        let runner = platform.boot(|| {
            Runner::boot_with_delivery_linked(
                links.runner,
                links.modules,
                plan,
                data,
                kernel,
                carried,
                snapshot,
                facts,
                viewport,
                launch,
            )
        });
        exact_runner::page::set_awaits_present(was);
        let mut runner = runner.map_err(HostError::Runner)?;
        runner.read_only_store(crate::watch_sync::read_only_names());
        // Its own roles from here, not those of the session it replaces (a
        // caller's scope is that one's; LLP 1121 stage 5).
        let _roles = runner.bind_roles();
        runner.share_app_state(activation::app_state());
        runner.set_device_links(links.device);
        if links.activities {
            runner.link_activities().map_err(HostError::Runner)?;
        }
        // @ref LLP 1079 D1 — a development build measures its work, a
        // release build from the first ask (`Bridge::measure_work`).
        crate::abi::measure_at_boot(&mut runner, compat);
        runner.admit_live(compat);
        if let Some(action) = region.and_then(|r| r.activate) {
            runner.act(action, Vec::new()).map_err(HostError::Runner)?;
        }
        let content_region = links
            .regions
            .boot(runner.kernel_mut(), region, native)
            .map_err(HostError::Layout)?;
        // Install the candidate catalog before first text measurement.
        prepare(&mut runner)?;
        let font_hook =
            font_hook.map(|(callback, ctx)| (callback, ctx, runner.runtime_font_count()));
        let has_heads = runner
            .plan()
            .nodes
            .iter()
            .any(|n| n.node_type == NodeType::Head as u8);
        let preloads = links.io.and_then(|io| (io.preloads)());
        let (boot_requests, preload_content) = preloads
            .as_ref()
            .map(|p| p.settle(&mut runner))
            .transpose()?
            .unwrap_or_default();
        let mut host = Host {
            launch_store: None,
            widget_changed: links.io.map(|io| io.widget_changed),
            preloads,
            boot_requests,
            preload_content,
            preload_accepted: false,
            has_heads,
            head_title: None,
            head_edited: false,
            language: None,
            hatches: paragraph::HatchWords::of(runner.plan()),
            flat: flat::Flat::new(layout.flat_nodes, runner.plan()),
            runner,
            admitting_frame: false,
            font_hook,
            mirror: IdMap::default(),
            style_texts: paragraph::StyleTexts::default(),
            keys: IdMap::default(),
            inline_runs: IdMap::default(),
            // LLP 1126 D10: watchOS has no Core Animation presenter for box motion or path morphs.
            svg: svg::SvgState::new(svg::render_server()),
            canvas_held: IdSet::default(),
            canvas_kept: Default::default(),
            canvas_images: BTreeSet::new(),
            canvas_deferred: false,
            canvas_scale: layout.canvas_scale.unwrap_or(canvas2d::DEFAULT_SCALE),
            dirty_paragraphs: BTreeSet::new(),
            pending_layout: IdSet::default(),
            layout_withheld: false,
            layout_owed: false,
            inert_entry: false,
            fit_routes: Vec::new(),
            stickies: IdMap::default(),
            timeline_scrolls: Default::default(),
            fragments: IdMap::default(),
            tables: IdMap::default(),
            ranks: IdMap::default(),
            roots: Vec::new(),
            collections_json: "[]".into(),
            collection_heads: Some(Vec::new()),
            #[cfg(target_os = "macos")]
            section_tables: Default::default(),
            engine: {
                // Settled values only when the plan animates nothing (LLP 1047.001).
                let mut engine = Engine::linked(links.engine);
                let _ = engine.set_start_on_frame(start_on_frame, 0.0);
                // LLP 1126 D10: watchOS keeps box motion sampled for its SwiftUI driver.
                engine.set_lowered_properties(&svg::lowered(svg::render_server()));
                engine
            },
            paint: paint::Paint::default(),
            holds: BTreeMap::new(),
            canvas_capture_waiting: Vec::new(),
            canvas_captures: BTreeMap::new(),
            height_owner: None,
            height_handles: BTreeMap::new(),
            height_auto_owned: false,
            height_drag: None,
            transform_drags: TransformDrags::new()?,
            arrange: None,
            group: None,
            presence: presence::Presence::default(),
            flights: flights::Flights::default(),
            content_region,
            regions: links.regions,
            height_projection: Vec::new(),
            height_sampling: Vec::new(),
            height_presented: Vec::new(),
            height_transitions: BTreeMap::new(),
            height_transition_epoch: None,
            height_targets_dirty: true,
            #[cfg(test)]
            height_target_passes: 0,
            #[cfg(test)]
            layout_calls: 0,
            viewport: (viewport.width as f32, viewport.height as f32),
            route_overflow: Default::default(),
            now_ms: 0.0,
            data_activated: false,
            source_prepared: None,
            store_refresh_pending: false,
            store_activation_pending: false,
            store_retry_names: Vec::new(),
            preload_wake: Default::default(),
            doc_watch: None,
            turn: links.turn,
            finished_deadline: None,
            quiet_collection: None,
            resize_pending: false,
            #[cfg(test)]
            finish_passes: 0,
            #[cfg(test)]
            batch_writes: 0,
            secrets,
            pending_store_writes: Vec::new(),
            stored: false,
            pending_store_stamps: Default::default(),
            pending_store_bases: Default::default(),
            reported_store_failures: BTreeSet::new(),
            update_line: None,
            delivery,
            colors_seen: None,
            facts_told: None,
            root_tabs_told: None,
            route_inspectors_told: Vec::new(),
            holds_told: Default::default(),
            closed: Default::default(),
        };
        let mut batch = Batch::new();
        host.native_prepare_candidate().map_err(HostError::Layout)?;
        let order = host.preorder();
        host.close_top_layer(&order);
        host.cover_root_tablist();
        host.prepare_flat(&mut batch);
        host.present_first_tree(previous, &order, &mut batch);
        host.emit_title(&mut batch);
        host.emit_language(&mut batch);
        colors::emit_roles(&mut batch);
        for s in host.runner.take_surface_updates() {
            batch.surface(&s);
        }
        // @ref LLP 1038 D7 — drain once, after all commits in this batch.
        if let Some(change) = host.runner.take_router_change() {
            batch.router(&change);
        }
        for c in host.runner.take_commands() {
            batch.command(&c.name, &c.args, c.source);
        }
        batch.timed(&mut host.runner, true);
        // The engine hears the whole tree once: values, no transitions; an
        // `animation` starts now, as a browser starts one on a new element.
        let mut sync = MotionSync::default();
        for id in &order {
            if let Some(node) = host.runner.kernel().node(*id) {
                host.runner.kernel().motion_sync_node(node.key, &mut sync);
            }
        }
        let applied = sync.apply(&mut host.engine);
        debug_assert!(applied.is_ok(), "kernel rows are always valid engine input");
        if let Some(motion) = host.turn.motion {
            (motion.boot)(&mut host, &sync);
        }
        host.boot_paint(&order);
        if let Some(drag) = host.turn.drag {
            (drag.reconcile_heights)(&mut host, &mut batch, true);
        }
        host.layout(&mut batch).map_err(HostError::Layout)?;
        if let Some(canvas) = host.turn.canvas {
            (canvas.limits)(&mut host);
            (canvas.turn)(&mut host, &mut batch);
        }
        // A failed first layout is a refused boot, not a partially committed
        // host. In particular, no candidate secret writes escape before this
        // point on a dev reload.
        if let Some(drag) = host.turn.drag {
            (drag.emit_transforms)(&mut host, &mut batch);
        }
        host.present(&mut batch, true);
        if let Some(old) = previous {
            host.finish_replaced_motion(old, &order, &mut batch);
        }
        let (mut batch, _) = host.resize_rounds(batch, None);
        host.state_onto(&mut batch);
        let batch = host.write(batch, None);
        Ok((host, batch))
    }
    /// [`crate::link::TurnMotion`]'s boot: what Core Animation can play of
    /// the motion the engine's first sync started, and the rest resampled.
    pub(crate) fn lower_motion(&mut self, sync: &MotionSync) {
        let runner = &self.runner;
        svg_lower::eligibility(
            runner.kernel(),
            &mut self.engine,
            sync,
            self.svg.box_motion,
            self.svg.live_filters,
            &|v| !runner.handlers_of(v).is_empty(),
        );
        svg_lower::hand_over_paths(runner.kernel(), &mut self.engine, self.svg.live_filters);
        for view in svg_lower::resample(
            runner.kernel(),
            &mut self.engine,
            self.svg.live_filters,
            &|v| !runner.handlers_of(v).is_empty(),
        ) {
            self.svg.element(self.runner.kernel(), view);
        }
    }

    /// Release effects only after the containing app accepted every candidate.
    pub(crate) fn commit_boot(&mut self) {
        if let Some(note) = self.delivery.and_then(|hooks| (hooks.take_note)()) {
            self.runner.log(note);
        }
        self.persist();
        self.preload_accepted = true;
        if let Some(preloads) = &self.preloads {
            preloads.committed(std::mem::take(&mut self.preload_content), self.runner.seq());
        }
    }
    /// The runner.
    pub fn runner(&self) -> &Runner<D> {
        &self.runner
    }
    /// The runner, mutably: an auth session's arm (LLP 1069.006).
    pub fn runner_mut(&mut self) -> &mut Runner<D> {
        &mut self.runner
    }

    /// `tap @t` or `type @t` (LLP 1069.007 D4): the agent answers a held
    /// device request, consumed here; the reply names the requesting node,
    /// where the Swift capability arm delivers it (a picker's `change` or
    /// `cancel`, LLP 1069.002 D9). Also the picker's `showPicker` hold and
    /// `pickedPath`, whose reply adds the file to copy into. A `share` hold
    /// delivers nothing but the journal line the runner writes (LLP 1069.003
    /// D6). `appRoots` names the three roots, which the
    /// Swift host resolves an image's `app:/` source against (LLP 1069.002
    /// D7); `shareFile` a share's confined copy (LLP 1125 D12). `None` else.
    pub fn answer_hold(&mut self, request: &str) -> Option<String> {
        let op = exact_runner::json::field_str(request, "op");
        if op.as_deref() == Some("shareFile") {
            let copy = Some(crate::picker::share_copy as crate::link::ShareCopy);
            return Some(crate::picker::share_file(
                request,
                self.runner.data_ref(),
                copy,
            ));
        }
        if op.as_deref() == Some("appRoots") {
            return Some(crate::picker::app_roots_reply(self.runner.data_ref()));
        }
        if op.as_deref() == Some("sensorEvents") {
            return Some(self.sensor_events(request));
        }
        // The documents a person chose, and those the app keeps (LLP 1069.010 D1, 1069.010.000).
        if let Some(reply) = crate::documents::answer(op.as_deref(), request, &self.runner) {
            return Some(reply);
        }
        let picked = op.as_deref() == Some("pickedPath");
        exact_runner::agent::answer(&mut self.runner, request).map(|(reply, _)| {
            if picked {
                crate::picker::with_files(reply)
            } else {
                reply
            }
        })
    }

    /// A command that shows system UI, about to run (`share`, LLP 1069.003;
    /// `saveFile`, LLP 1069.010 D3): the runner's ruling on it
    /// ([`exact_runner::commands::request`]).
    pub fn command(&mut self, request: &str) -> String {
        exact_runner::commands::request(&mut self.runner, request)
    }

    /// What a reload keeps (`Runner::carry`).
    pub fn carry(&self) -> Carried {
        self.runner.carry()
    }

    /// The motion engine: presentation values as the presenter shows them.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// What the update store has to say, into the runner (LLP 1030 D7) —
    /// after a check, after an activation: the `delivery` resource is
    /// answered again and the batch carries the commit; the check's line
    /// goes to the journal once, so the agent's `logs` reads it beside the app's own.
    pub fn sync_delivery(&mut self) -> String {
        let mut delivery = self.runner.delivery().clone();
        if let Some(hooks) = self.delivery {
            (hooks.status_into)(&mut delivery);
        }
        let line = self.delivery.and_then(|h| (h.last_line)());
        if line.is_some() && line != self.update_line {
            self.update_line = line.clone();
            self.runner.log(line.unwrap_or_default());
        }
        match self.runner.set_delivery(delivery) {
            Ok(Some(receipt)) => {
                let at_ms = self.now_ms;
                self.commit(&[Timed { at_ms, receipt }], None)
            }
            Ok(None) => self.commit(&[], None),
            Err(e) => self.commit(&[], Some(format!("delivery: {e:?}"))),
        }
    }

    fn configure_storage(source: &mut D) -> Result<(), exact_runner::DataError> {
        let Some(([data, cache, temporary], _)) = crate::picker::app_dirs(source.app_id())? else {
            return Ok(());
        };
        // An authored test's store starts empty every run (`agent --test`):
        // emptied at the boot that read it, so this is a no-op unless that failed.
        crate::picker::empty_fresh_tree(source.app_id())?;
        // What `app:/` names for the picker and an image's source (LLP
        // 1069.002 D4, D7); the last launch's picks go.
        crate::picker::set_roots(data.clone(), cache.clone(), temporary.clone());
        source.configure_storage(data, cache, temporary)
    }

    /// The work behind a continuation, dispatched on this thread after the
    /// commit that handed it out (LLP 1027.002 D3).
    pub fn dispatch_work(&mut self, token: u64) -> exact_runner::Dispatch {
        self.runner.dispatch_work(token)
    }

    /// Take the source's announced topics, waking the host (LLP 1016.002).
    pub fn listen(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        if let Some(store) = &self.launch_store {
            store.listen(wake.clone());
        }
        self.preload_wake.set(wake.clone());
        self.route_overflow.wake = Some(wake.clone());
        self.runner.listen(wake);
    }

    /// [`Host::listen`] to `io`, and read sockets' buffered bytes (D7) and
    /// long polls' backoff waits (D8) from it; watch the documents the answers watch, as `owner`'s (LLP 1069.010.000 D1).
    pub fn listen_io(&mut self, io: &dyn crate::executor::Io, owner: u64) {
        self.doc_watch = Some(crate::doc_watch::DocWatch::new(
            owner,
            self.runner.native_slot(),
        ));
        // Its first watches, read under the grants by the turn's documents work.
        if let Some(documents) = self.turn.documents {
            documents(self);
        }
        self.listen(io.waker());
        self.runner.set_socket_reader(io.socket_reader());
        self.runner.set_retry_reader(io.retry_reader());
        io.set_fetch_fault(self.runner.fetch_fault());
        self.runner.set_listen_reader(io.listen_reader());
        self.runner.set_work_clock(io.work_clock());
    }
    /// The source's native slot, where the host installs the app module.
    pub fn native_slot(&self) -> exact_runner::Native {
        self.runner.native_slot()
    }

    /// A long native call's work: the source's native handler, off this thread.
    pub fn native_work(&mut self, request: &exact_runner::Request) -> exact_runner::Dispatch {
        self.runner.native_work(request)
    }

    /// Work a source held at dispatch that the last commit released.
    pub fn release_work(&mut self) -> Vec<(u64, exact_runner::Dispatch)> {
        self.runner.release_work()
    }

    /// The hosts the app may reach (LLP 1016 D6): the data crate's grants and
    /// the origins its declared sources carry (LLP 1162.001 D3).
    pub fn grants(&mut self) -> String {
        self.runner.grants()
    }

    /// The app's `app:/data`, `app:/cache` and `app:/tmp`, as storage
    /// configures them, for a request whose body is one of its files (LLP
    /// 1108 D6 R2); `None` with no app id, or a drive with no scratch store.
    pub fn app_roots(&mut self) -> Option<[std::path::PathBuf; 3]> {
        crate::picker::app_dirs(self.runner.data().app_id())
            .ok()
            .flatten()
            .map(|(roots, _)| roots)
    }

    /// The kept answers the writer failed to write since the last call.
    pub fn kept_failures(&self) -> Vec<String> {
        self.secrets
            .as_ref()
            .map(|s| s.kept_failures())
            .unwrap_or_default()
    }

    /// Surface work for the presenter (`refusal`: the host refused it). With
    /// canvases unlinked a plan has none, so a source's `captureCanvas` is
    /// refused by the link's name (LLP 1047.001 D5) and other work goes to
    /// the presenter.
    pub fn surface_work(
        &mut self,
        presenter: &mut crate::batch::Batch,
        r: &RequestOut,
        refusal: Option<&str>,
    ) {
        if let Some(canvas) = self.turn.canvas {
            return (canvas.surface_work)(self, presenter, r, refusal);
        }
        if let Some(exact_runner::SurfaceRequest::Canvas { .. }) = r.request.surface.as_deref() {
            return self.refuse_request(r.ticket, UNLINKED_SURFACES, r.request.is_ordered());
        }
        presenter.surface_work(r, refusal);
    }

    /// The requests the runner handed out since the last take (LLP 1016 D2):
    /// the bridge gives them to the executor; nothing reaches the presenter.
    pub fn take_requests(&mut self) -> Vec<RequestOut> {
        let mut requests = std::mem::take(&mut self.boot_requests);
        requests.extend(self.runner.take_requests());
        requests
    }

    /// Admission failures remain on the runner's current tickets, not a queue.
    pub fn refuse_request(&mut self, ticket: u64, reason: &'static str, ordered: bool) {
        self.runner.refuse_request(ticket, reason, ordered);
    }

    /// Take one admission failure through the usual settlement path.
    pub fn take_request_refusal(&mut self, allow_ordered: bool) -> Option<(u64, Outcome)> {
        self.runner.take_request_refusal(allow_ordered)
    }

    /// Ordered refusals hold later ordered dispatch until they settle or are forgotten.
    pub fn has_ordered_request_refusals(&self) -> bool {
        self.runner.has_ordered_request_refusals()
    }

    /// Whether another pump must settle an admission failure.
    pub fn has_request_refusals(&self, allow_ordered: bool) -> bool {
        self.runner.has_request_refusals(allow_ordered)
    }

    /// Strict common LE viewport feedback followed by any runner edge action.
    /// Stale or malformed facts leave layout and the motion clock untouched.
    /// Only a runner receipt enters the ordinary native view commit path.
    pub fn collection_feedback(&mut self, bytes: &[u8], now_ms: f64) -> String {
        if !(0.0..=exact_runner::MAX_CLOCK_MS).contains(&now_ms) {
            return self.finish(
                Batch::new(),
                Some("invalid collection feedback time".into()),
            );
        }
        match self.runner.collection_feedback_bytes(bytes) {
            Ok(mut result) => {
                if result.receipts.is_empty() && result.error.is_none() {
                    return self.finish_collection();
                }
                self.now_ms = self.now_ms.max(now_ms);
                for timed in &mut result.receipts {
                    timed.at_ms = self.now_ms;
                }
                let error = result.error.map(|e| format!("{e:?}"));
                self.in_inert_entry(|host| host.commit(&result.receipts, error))
            }
            Err(error) => self.finish(Batch::new(), Some(format!("{error:?}"))),
        }
    }

    /// The agent's `tap <list> into <key>` (LLP 1070.000 §5).
    pub fn scroll_into_view(
        &mut self,
        view: ViewId,
        key: &str,
        block: &str,
        inline: &str,
    ) -> String {
        match self.runner.scroll_into_view_at(view, key, block, inline) {
            Ok(receipt) => {
                let at_ms = self.now_ms;
                self.commit(&[Timed { at_ms, receipt }], None)
            }
            Err(error) => self.finish(Batch::new(), Some(format!("scrollIntoView: {error:?}"))),
        }
    }

    /// A presenter's line for the runner's journal (LLP 1012 §3): a refused
    /// intent — a focus that could not be delivered, a route key that names
    /// no route, a presentation the owner refused — with its reason.
    pub fn log(&mut self, line: &str) {
        self.runner.log(line);
    }

    /// Deliver an event at the app's clock (milliseconds); the batch makes
    /// the presenter equal to the tree after the commit, laid out, with any
    /// motion the change started. A refusal is reported in the batch's
    /// `error`, and the presenter is untouched (as the kernel was).
    pub fn dispatch_at(&mut self, view: ViewId, event: Event, now_ms: f64) -> String {
        self.now_ms = now_ms.max(self.now_ms);
        // At the event's time: an action's `performanceNow()` is the host's (LLP 1096 D3).
        let a = self.runner.dispatch_at(view, event, self.now_ms);
        let error = a.error.map(|e| format!("{e:?}"));
        self.in_inert_entry(|host| host.commit(&a.receipts, error))
    }

    /// The platform's own Back from visit `id` (LLP 1115 D5), at `now_ms`.
    pub fn host_back(&mut self, id: u64, now_ms: f64) -> String {
        self.own_navigation(now_ms, |runner| runner.host_back(id))
    }

    /// Return to the visit selected in the platform's Back-history menu.
    pub fn host_back_to(&mut self, source: u64, destination: u64, now_ms: f64) -> String {
        self.own_navigation(now_ms, |runner| runner.host_back_to(source, destination))
    }

    /// Make `location` a rooted window's only visit (LLP 1030.004 D2).
    pub fn host_root(&mut self, location: &str, now_ms: f64) -> String {
        self.own_navigation(now_ms, |runner| runner.host_root(location))
    }

    /// Follow a declared route without an authored navigate handler.
    pub fn host_go(&mut self, location: &str, now_ms: f64) -> String {
        self.own_navigation(now_ms, |runner| runner.host_go(location))
    }

    /// Native Forward through the root's handler or the runner's own `go`.
    pub fn host_forward(&mut self, root: ViewId, now_ms: f64) -> String {
        self.now_ms = now_ms.max(self.now_ms);
        let a = self.runner.host_forward_at(root, self.now_ms);
        self.commit(&a.receipts, a.error.map(|e| format!("{e:?}")))
    }

    /// Advance the clock to `now_ms` and run `navigate` on the runner, as one
    /// batch.
    pub(crate) fn own_navigation(
        &mut self,
        now_ms: f64,
        navigate: impl FnOnce(&mut Runner<D>) -> Result<Option<CommitReceipt>, RunnerError>,
    ) -> String {
        let mut a = self.runner.advance_timed(now_ms.max(self.now_ms));
        self.now_ms = a.now_ms.max(self.now_ms);
        if a.error.is_none() {
            match navigate(&mut self.runner) {
                Ok(receipt) => a.receipts.extend(receipt.map(|receipt| Timed {
                    at_ms: self.now_ms,
                    receipt,
                })),
                Err(e) => a.error = Some(e),
            }
        }
        self.commit(&a.receipts, a.error.map(|e| format!("{e:?}")))
    }

    /// [`Host::dispatch_at`] at the clock's last value.
    pub fn dispatch(&mut self, view: ViewId, event: Event) -> String {
        self.dispatch_at(view, event, self.now_ms)
    }

    /// Move the clock; every timer due fires at its own time; one batch for
    /// all of them, the engine hearing each commit at the time it was made.
    /// A timer's refusal stops the clock there: the commits before it are in
    /// the batch, the refusal in `error`, and `clock` says where the runner stands.
    pub fn advance(&mut self, now_ms: f64) -> String {
        if self.engine.starts_on_frame() {
            self.now_ms = now_ms.max(self.now_ms);
        }
        let a = self.runner.advance_timed(now_ms);
        self.advanced(a)
    }

    /// [`Host::advance`], stopping after a timer that sends as well: an
    /// agent's jump ([`exact_runner::Runner::advance_until_request`]).
    /// While a node declares a layout transition, in steps (`presence.rs`).
    pub fn advance_until_request(&mut self, now_ms: f64) -> String {
        self.advance_in_layout_steps(now_ms)
    }

    /// Whether this host's display drives frame tasks (LLP 1073 D4): off when
    /// the agent's clock takes over, so its advances fire virtual frames.
    /// Set whether the display drives frame tasks.
    pub fn present_frames(&mut self, on: bool) {
        self.runner.present_frames(on);
    }

    fn advanced(&mut self, a: exact_runner::Advanced) -> String {
        // A frame task puts the runner at the display's target; with the
        // first-frame rule the host's clock stays the wall's (LLP 1003.001 D5).
        if !self.engine.starts_on_frame() {
            self.now_ms = a.now_ms.max(self.now_ms);
        }
        if let Some(canvas) = self.turn.canvas {
            (canvas.frame)(self);
        }
        let error = a.error.map(|e| format!("{e:?}"));
        self.in_inert_entry(|host| host.commit(&a.receipts, error))
    }

    /// An image loaded: its intrinsic size, in points (`None` when it failed
    /// or was cleared). Lays out again; the batch carries the frames that
    /// moved — the image's, and everything its size pushed.
    pub fn set_intrinsic(&mut self, view: ViewId, size: Option<(f32, f32)>) -> String {
        self.presence.unauthored = true;
        self.height_targets_dirty = true;
        let mut batch = Batch::new();
        batch.controls = self.option_part(view);
        let error = match self.runner.kernel_mut().set_intrinsic_size(view, size) {
            Ok(()) => self.layout_resized(view, &mut batch).err(),
            Err(e) => Some(format!("intrinsic: {e:?}")),
        };
        self.finish(batch, error)
    }

    /// Several images' intrinsic sizes at once — the symbols a batch just
    /// created — under one layout. A refusal for one view (it is gone, or
    /// not an image) leaves the others set and is the batch's error.
    pub fn set_intrinsics(&mut self, sizes: &[(ViewId, Option<(f32, f32)>)]) -> String {
        self.presence.unauthored = true;
        self.height_targets_dirty = true;
        let mut batch = Batch::new();
        let mut error = None;
        for &(view, size) in sizes {
            batch.controls |= self.option_part(view);
            if let Err(e) = self.runner.kernel_mut().set_intrinsic_size(view, size) {
                error.get_or_insert(format!("intrinsic: {e:?}"));
            }
            self.inline_image_resized(view, &mut batch);
        }
        if let Err(e) = self.layout(&mut batch) {
            error.get_or_insert(e);
        }
        self.finish(batch, error)
    }

    /// The viewport changed: lay out again; the batch carries the frames
    /// A surface changed its current public record, or was disposed.
    pub fn surface_record(&mut self, name: &str, json: Option<&str>) -> String {
        let (receipts, error) = match self.runner.set_surface_record(name, json) {
            Ok(Some(receipt)) => (
                vec![Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => (vec![], None),
            Err(error) => (vec![], Some(format!("surface {name}: {error:?}"))),
        };
        self.commit(&receipts, error)
    }

    /// The viewport's new size, in one batch.
    pub fn resize(&mut self, width: f32, height: f32) -> String {
        self.resize_at(width, height, self.now_ms)
    }

    /// A placement root's resolved margins and whether its width is the author's
    /// (`Kernel::placement_box`), as JSON `[left, top, right, bottom, authored, minimum]`;
    /// an error names the view.
    pub fn placement_box(&mut self, view: ViewId) -> String {
        let kernel = self.runner.kernel();
        let Some(key) = kernel.node(view).map(|n| n.key) else {
            return serde_json::json!({"error": format!("no view {view}")}).to_string();
        };
        match kernel.placement_box(key) {
            Some(b) => serde_json::to_string(&b).unwrap(),
            None => {
                serde_json::json!({"error": format!("view {view} is not laid out")}).to_string()
            }
        }
    }

    /// The date (LLP 1027.000.000): re-answer `exactTime` in one commit.
    pub fn set_time(&mut self, epoch_at_zero: f64, utc_offset: f64) -> String {
        let answer = self.runner.set_time(epoch_at_zero, utc_offset);
        self.told(answer, "time")
    }

    /// The user's display preferences (LLP 1061 D4): re-answer
    /// `exactViewport` in one commit; the same preferences commit nothing.
    pub fn set_preferences(&mut self, preferences: exact_runner::Preferences) -> String {
        let answer = self.runner.set_preferences(preferences);
        self.told(answer, "preferences")
    }

    /// The page's visibility, connectivity and share sheet (LLP 1069.000
    /// D2): re-answer `exactPage` in one commit; the same facts commit nothing.
    pub fn set_page(&mut self, page: exact_runner::Page) -> String {
        let released = self.page_presentable(page);
        let answer = self.runner.set_page(page);
        if released && matches!(answer, Ok(None)) {
            // What a release started (an exit's end, a lowered spec) goes now.
            let mut batch = Batch::new();
            self.present(&mut batch, false);
            return self.finish(batch, None);
        }
        self.told(answer, "page")
    }

    /// `prefer presented` (LLP 1069.000 D6): holds `exactPage().presented`
    /// at a value, or with `None` gives it back. It moves no animation.
    pub fn prefer_presented(&mut self, preferred: Option<bool>) -> String {
        let answer = self.runner.prefer_presented(preferred);
        self.told(answer, "presented")
    }

    /// The viewer's locale and zone, beside the date: one commit when it changes.
    pub fn set_place(
        &mut self,
        locale: &str,
        time_zone: &str,
        seed: Option<f64>,
        format_locale: Option<&str>,
    ) -> String {
        let answer = self
            .runner
            .set_place_formatted(locale, time_zone, seed, format_locale);
        self.told(answer, "place")
    }

    /// A told fact's answer: its commit, an empty batch when the fact is
    /// unchanged, or the refusal named by `what`.
    fn told(&mut self, answer: Result<Option<CommitReceipt>, RunnerError>, what: &str) -> String {
        match answer {
            Ok(Some(receipt)) => self.commit(
                &[Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => self.finish(Batch::new(), None),
            Err(e) => self.finish(Batch::new(), Some(format!("{what}: {e:?}"))),
        }
    }

    /// Content has settled: the tree and the motion engine give back what
    /// they hold beyond the live nodes ([`exact_kernel::Kernel::trim`]).
    pub fn trim(&mut self) {
        self.runner.kernel_mut().trim();
        self.engine.trim();
    }

    /// The active head's title and edited mark after it moves; the app owning
    /// the window or scene shows them (LLP 1048.003 D1, LLP 1069.010 D6).
    fn emit_title(&mut self, batch: &mut Batch) {
        if !self.has_heads {
            return;
        }
        let head = self.runner.head();
        if head.title != self.head_title || head.edited != self.head_edited {
            batch.title(head.title.as_deref(), head.edited);
            self.head_title = head.title;
            self.head_edited = head.edited;
        }
    }

    fn emit_language(&mut self, batch: &mut Batch) {
        let language = self.runner.resolved_locale();
        if self.language.as_deref() != Some(language) {
            batch.language(language, self.runner.direction());
            self.language = Some(language.into());
        }
    }
}

/// Whether a commit left every node as it was.
fn unchanged(r: &CommitReceipt) -> bool {
    !r.layout_invalidated
        && r.created.is_empty()
        && r.destroyed.is_empty()
        && r.touched.is_empty()
        && r.exits.is_empty()
        && r.handoffs.is_empty()
        && r.display_changed.is_empty()
        && r.timelines.is_empty()
        && r.renewed.is_empty()
        && r.revealed.is_empty()
}

/// A frame in its parent's space (a root's is absolute).
fn relative(frame: Frame, parent: Option<Frame>) -> (f32, f32, f32, f32) {
    match parent {
        Some(p) => (frame.x - p.x, frame.y - p.y, frame.width, frame.height),
        None => (frame.x, frame.y, frame.width, frame.height),
    }
}

/// The presenter's kind for a node: its type, in the schema's names.
fn kind_for(node: &NodeRef<'_>) -> &'static str {
    if node.node_type == NodeType::TextInput
        && node.props.str(PropId::SemanticTag) == Some("textarea")
    {
        return "textarea";
    }
    match node.node_type {
        NodeType::View => "view",
        NodeType::List => "list",
        NodeType::NativeView => "native",
        NodeType::Svg => "svg",
        // Never a view: its `svg`'s scene draws it (LLP 1055 D4).
        NodeType::SvgGroup
        | NodeType::SvgPath
        | NodeType::SvgPolyline
        | NodeType::SvgPolygon
        | NodeType::SvgCircle
        | NodeType::SvgLine
        | NodeType::SvgRect
        | NodeType::SvgEllipse
        | NodeType::SvgViewport
        | NodeType::SvgDefs
        | NodeType::SvgLinearGradient
        | NodeType::SvgRadialGradient
        | NodeType::SvgStop
        | NodeType::SvgUse
        | NodeType::SvgSymbol
        | NodeType::SvgClipPath
        | NodeType::SvgText
        | NodeType::SvgTSpan
        | NodeType::SvgMarker
        | NodeType::SvgMask
        | NodeType::SvgPattern
        | NodeType::SvgForeignObject
        | NodeType::SvgFilter
        | NodeType::SvgFe => "svg-element",
        NodeType::ScrollView => "scroll",
        NodeType::Text => "text",
        NodeType::Image => "image",
        NodeType::TextInput => "input",
        NodeType::Pressable => "button",
        NodeType::Control => "control",
        NodeType::Canvas => "canvas",
        NodeType::WebView => "iframe",
        NodeType::Video => "video",
        // A head takes no space; its title is the window's (LLP 1048.003 D1).
        NodeType::Head => "view",
    }
}

/// @ref LLP 1116.001 D1.4 — Swift's run-size multiplier for a fitted paragraph; none at full size.
pub(super) fn fit_scale(node: &NodeRef<'_>) -> Option<String> {
    let scale = node.fit_scale();
    (node.node_type == NodeType::Text && scale != 1.0).then(|| style::num(scale))
}

fn handler_name(e: EventKind) -> Option<&'static str> {
    match e {
        // The runner raises these itself, after collection feedback and
        // after layout: nothing for the presenter to listen for.
        EventKind::Reachstart | EventKind::Reachend | EventKind::Resize => None,
        _ => Some(e.name()),
    }
}
