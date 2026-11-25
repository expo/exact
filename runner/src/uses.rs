//! What a plan uses beyond the core, derived from its bytes.
//!
//! @ref LLP 1047 D1 (the core, linked and loaded capabilities)
//! @ref LLP 1047 D2 (the use-set is a pure function of the plan)
//! @ref LLP 1047 D6 (a host refuses a plan that uses what it doesn't link)
//!
//! No author declares a capability: a plan's rows say which it needs. This
//! lives in the runner, not the plan crate, because a use is a kernel prop or
//! style row, and the plan crate declares no kernel vocabulary (LLP 1004 D2).
//! A binding whose value the plan computes counts as a use of whatever it
//! might select, so the set is never smaller than what a run can reach.

use exact_kernel::{PropId, StyleId};
use exact_plan::{BindingKind, EventKind, Opcode, Plan, Stdlib, StrId};
use std::fmt;

mod apple;
pub use apple::{
    apple_grant_uses, apple_uses, data_url, image_sources, lays_out_grid, ImageSource,
};

/// A capability beyond the core, linked into an artifact only when its plan
/// uses it (LLP 1047 §4's roster, as each one moves behind its seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// `markup="markdown"` text: its source styled as pieces.
    Markdown,
    /// Springs and holds: a `transition` that can be a `-exact-spring()`, and the
    /// gestures that hold a value (a swipe, a height, transform or reorder
    /// drag). CSS plays every other transition.
    Motion,
    /// Lists the host windows: a `virtualized` list,
    /// or a handler for a list's edges.
    Collections,
    /// Drags a host tracks at every commit: height, transform and reorder
    /// handles. A drag holds a value, so it uses motion too.
    Drag,
    /// GPU canvas surfaces: a canvas with a surface, or a resource reading a
    /// surface's record; the host answers both (the GPU module is loaded).
    Surfaces,
    /// The router (LLP 1038): a plan that declares `routes`.
    Router,
    /// `formatDate` and `formatNumber` (LLP 1054.000.003 D8): a plan whose
    /// code calls one.
    Format,
    /// `backgroundMaterial` (LLP 1053.000 D4): the materials table, and on
    /// the web its approximation's CSS.
    Materials,
    /// `backdrop-filter` (LLP 1053.000 D1): its grammar and named refusals.
    Backdrop,
    /// `share(…)` (LLP 1069.003): a plan whose code runs the command.
    Share,
    /// `saveFile` and the three file pickers (LLP 1069.010): a plan whose
    /// code runs one, or that hears a `drop` of files (studio diary R19).
    Documents,
    /// `input type="file"` and `showPicker` (LLP 1069.002): a plan with a
    /// file input, or whose code runs the command.
    Picker,
    /// Timelines (LLP 1057.003): `-exact-drag-timeline`, the scroll and view
    /// timeline rows, `animation-timeline`, `animation-range` and
    /// `timeline-scope`, their grammar, lowering, name lookup and placement.
    /// A timeline follows a held value or a scroll, so it uses motion too.
    Timelines,
    /// `text-transform` (LLP 1064 D5): its Unicode case mapping, for a plan
    /// that binds the row.
    TextTransform,
    /// `filter` and `clip-path` (LLP 1055.000 D10, D14): their grammars, for
    /// a plan that binds either row.
    Effects,
    /// CSS animations (LLP 1055 D5): the `animation` shorthand's and
    /// `@keyframes`' grammars, for a plan that declares keyframes or binds
    /// `animation`, `-exact-exit-animation` or a route's
    /// `::view-transition-new` or `::view-transition-old`.
    Animations,
    /// Gradient grammars (LLP 1066, LLP 1077 D2), for a plan that binds
    /// `background-image` or `mask-image`.
    Gradients,
    /// CSS grid's track, placement and keyword grammars, for a plan that
    /// binds any of the six grid rows. On Apple, the grammars and the grid
    /// algorithm together, for a plan that binds one or whose layout can
    /// reach the algorithm ([`apple_uses`]).
    Grid,
    /// `frame` and `measure` (LLP 1051.000): a plan whose actions read
    /// geometry. Native hosts answer from the kernel; the web links a
    /// synchronous import the page answers.
    Geometry,
    /// `env(viewport-segment-*)` lengths (LLP 1078 D3): the segment grammar,
    /// resolution and wire decode, for a plan whose strings name a segment.
    /// The fold's `exactViewport` fields are the core's.
    Segments,
    /// `data-*` words (LLP 1075.003 §3.3): a plan that binds `dataset`. The
    /// web writes each word as its own attribute.
    Dataset,
    /// A navigation root's tabs (LLP 1075.003 §3.7): a plan that binds
    /// `aria-controls`. The web's document shows each tab's stack.
    Tabs,
    /// A native grouped list: a `list` whose appearance is `auto`, which
    /// Contract marks with `listStyle` (LLP 1084 D2). A native host draws it
    /// with the platform's list; the web draws its authored nodes, so the web
    /// links nothing for it (LLP 1047.001 D2).
    GroupedLists,
    /// The host's I/O: the requests a data source makes (fetch, sockets,
    /// files, SQLite) and the kept secrets and answers it stores. A plan
    /// uses it for kept answers and surface records. An app's grants
    /// link it too, which the bake reads (LLP 1047.001). The browser does
    /// its own, so the web links nothing for it.
    Io,
    /// What a native engine plays that CSS plays on the web: a `transition`,
    /// a `-exact-layout-transition` or a shared element's flight, and the
    /// `transition` shorthand's grammar. A native host's motion engine moves
    /// them; without them it only settles values (LLP 1047.001). The browser
    /// plays them, so the web counts it linked and links only the grammar
    /// (`exact_web::link::link`, the JS target's motion module).
    Transitions,
    /// iOS's photo picker (LLP 1047.001.000 E): a file input whose `accept`
    /// is only images and videos, or is computed. Linking it costs every
    /// launch PhotosUI's images, so a document input does not; the picker
    /// itself is [`Capability::Picker`].
    MediaPicker,
    /// The sign-in sheet behind `openAuthSession` (LLP 1047.001.000 E):
    /// never set by a plan, since a data source's requests are the source's;
    /// the bake links it for an app whose grants name `auth.session`, and a
    /// manifest's `link` may name it.
    AuthSession,
    /// A text's numerals rolling as they change, per glyph
    /// (`-exact-content-transition`, LLP 1077 D15): on Apple, SwiftUI's
    /// numeric text, whose framework an app links only for this (LLP
    /// 1115.003 D3). A plan uses it when it binds the row.
    Numerals,
    /// A date or time control (`input type="date"`, `"time"` or
    /// `"datetime-local"`): on Android its picker is Compose Material 3's,
    /// which an app links only for it (LLP 1115.001.000 D2). Every other
    /// host shows its platform's own picker and links nothing for it.
    Dates,
    /// SQL storage, selected by a data module's `sqlite.open` grants or by
    /// the manifest's `link`, since requests live below the plan.
    Sqlite,
    /// A tip over an element (`tip`, LLP 1035.006.000): on Apple, TipKit's
    /// popover, whose frameworks (TipKit, SwiftUI) an app links only for it.
    /// Hosts with no tip system show none and link nothing for it.
    Tips,
    /// `box-shadow` and `text-shadow` grammars (LLP 1077 D3, D4).
    Shadows,
    /// `corner-shape`'s grammar (LLP 1077 D1).
    Corners,
    /// `rotate`'s angle and axis grammars (LLP 1077 D8).
    Rotate,
    /// `-webkit-text-stroke`'s shorthand grammar (LLP 1077 D7).
    TextStroke,
    /// `-exact-symbol-palette`'s grammar (LLP 1077 D10).
    SymbolPalette,
    /// Locked secrets (LLP 1133.010 D16), selected by `secret.keep` grants
    /// with a presence policy, including names whose protection is optional.
    Presence,
    /// A package's native modules, views and events (LLP 1047.001 D10,
    /// D11): a plan whose `modules`, `views` or `module_events` table is not
    /// empty. Native only ([`apple_uses`]); the web loads its packages
    /// itself.
    Modules,
    /// Tool calls (LLP 1111; LLP 1047.001 D11): a plan whose `tools` table
    /// is not empty. Native only ([`apple_uses`]).
    Tools,
    /// SVG decoding (LLP 1047.001 D10): an SVG element, or an image whose
    /// file can be an SVG, which the host tells by its bytes. Native only.
    Svg,
    /// Loading and showing image files: an `image` whose source can be a
    /// file, or a Markdown atom's icon. Native only.
    Images,
    /// The app module's native views and hatches (LLP 1024, LLP
    /// 1075.003.000): a module tag, a `hatch`, or a package's view, and an
    /// app whose module artifact connects the app-level hatches, which the
    /// bake reads. Native only.
    NativeViews,
    /// Content regions (long documents): never set by a plan, since the
    /// entry registers the region. An entry that registers one requires it
    /// in its set, so a manifest's `link` names it. Native only.
    ContentRegions,
    /// Trace recording: a plan that runs `startTrace_EXPERIMENTAL` or
    /// `stopTrace_EXPERIMENTAL`, or a manifest's `link` for an embedder that
    /// calls the public API. Native only.
    Traces,
    /// The optional native controls: switches, checkboxes, radios, sliders,
    /// pop-up selects, date pickers, progress and segmented controls. A
    /// plain button and a number field's stepper are the core's. Native
    /// only.
    Controls,
    /// Shared-element flights (LLP 1013.000): a plan that binds
    /// `sharedElement`, the name a flight follows from a leaving view to an
    /// arriving one. Native only.
    Flights,
    /// `position: sticky` (LLP 1083 D4): a plan whose `position` can be
    /// `sticky`. Native only.
    Sticky,
    /// Notifications: posting, the delegate that shows one in front and
    /// opens a click, declared actions and push registration. Selected by a
    /// `device.notifications` grant, which push needs too. Native only.
    Notifications,
    /// Host events (LLP 1133.001.000 D2): a system call on the app's or the
    /// scene's delegate that a package takes, held until it does. Selected
    /// by a grant a listed event needs signed (`device.cloudkit`). Native
    /// only.
    HostEvents,
    /// Live Activities: a plan that starts, updates or ends one, or reads
    /// them (`exactActivities`). Native only.
    Activities,
    /// Wakes, background transfers and monitored regions (LLP 1133.017,
    /// LLP 1133.021 D14), which can launch the app with no scene. Selected by
    /// a background grant (`device.background-sync`,
    /// `device.periodic-background-sync`, `device.geolocation-background`)
    /// or a background transfer's or a geofence's names. Native only.
    Wakes,
    /// The display held awake (LLP 1125 D11): a `wakelock` that can be
    /// `screen`. Native only.
    WakeLock,
    /// Video and audio elements' players, and an image's picture in
    /// picture, which the video arm carries (LLP 1098; LLP 1042 §9): a
    /// `video` or `audio` node, or a plan that runs
    /// `requestPictureInPicture`. Native only.
    Video,
    /// Iframes' web views (LLP 1020): an `iframe` node. Native only.
    WebViews,
    /// The voice table's output and the runner's audio (LLP 1096; LLP 1125
    /// D3): a plan that [`runs_sounds`] or reads `exactTransport`. Native
    /// only.
    Sound,
    /// MIDI ports and the MIDI queue's output (LLP 1125 D6–D8): a plan that
    /// [`keeps_midi`]. Native only.
    Midi,
    /// Route stacks, tabs and their bars as the platform's navigation, with
    /// route modals (LLP 1008 §9, LLP 1115.000): a plan that binds a
    /// navigation root's or a route's props (`navigationBack`,
    /// `navigationKey`, `navigationPresentation` and the rest). Native only.
    Navigation,
    /// Selecting non-editable text (LLP 1115.000 D11.6): a `user-select`
    /// that can be `text` or `all`, which iOS needs to make text
    /// selectable. Native only; the Mac's set always names it, since
    /// AppKit's document text is selectable unsaid.
    TextSelection,
    /// A widget's or a Live Activity's entry (LLP 1126 D2, D8): the app's
    /// publication of its generations to the App Group and the observation
    /// of the store the extension writes; an entry boots only where it is
    /// linked. An entry's plan (`snapshot_entry`) selects it, and so do Live
    /// Activities, whose sends name the published generation. Native only.
    Widgets,
    /// The content index (LLP 1133.023 D4, D9): the mirror
    /// `@exact/search-index` writes, Spotlight's index of it, and the entries
    /// an entity parameter offers. Selected by that package's names. Native
    /// only.
    ContentIndex,
    /// HTML's `dialog` shown as UIKit's sheet, or as the author's box in the
    /// top layer when its author paints it (LLP 1115.000 D11.5, LLP 1035.001
    /// D11): a node that can be a `dialog`. iOS and tvOS only: the Mac's
    /// top-layer stack, which a sheet route shares, presents a `dialog` in its
    /// core ([`Uses::MAC_CORE`]).
    Dialogs,
    /// A drag out of the app (LLP 1094.000 D3): a node with `draggable`,
    /// whose host lifts it to the system with its data.
    DragOut,
    /// A drag into the app on UIKit (LLP 1094.001 D4): a node hearing `drop`,
    /// `dragenter` or `dragleave`, whose host takes drags from other apps.
    /// The Mac takes them in its core ([`Uses::MAC_CORE`]), the web in its
    /// documents glue ([`Capability::Documents`]).
    DragIn,
}

impl Capability {
    /// Every capability, in bit order.
    pub const ALL: [Capability; 63] = [
        Capability::Markdown,
        Capability::Motion,
        Capability::Collections,
        Capability::Drag,
        Capability::Surfaces,
        Capability::Router,
        Capability::Format,
        Capability::Materials,
        Capability::Backdrop,
        Capability::Share,
        Capability::Documents,
        Capability::Picker,
        Capability::Timelines,
        Capability::TextTransform,
        Capability::Effects,
        Capability::Animations,
        Capability::Gradients,
        Capability::Grid,
        Capability::Geometry,
        Capability::Segments,
        Capability::Dataset,
        Capability::Tabs,
        Capability::GroupedLists,
        Capability::Io,
        Capability::Transitions,
        Capability::MediaPicker,
        Capability::AuthSession,
        Capability::Numerals,
        Capability::Dates,
        Capability::Sqlite,
        Capability::Tips,
        Capability::Shadows,
        Capability::Corners,
        Capability::Rotate,
        Capability::TextStroke,
        Capability::SymbolPalette,
        Capability::Presence,
        Capability::Modules,
        Capability::Tools,
        Capability::Svg,
        Capability::Images,
        Capability::NativeViews,
        Capability::ContentRegions,
        Capability::Traces,
        Capability::Controls,
        Capability::Flights,
        Capability::Sticky,
        Capability::Notifications,
        Capability::HostEvents,
        Capability::Activities,
        Capability::Wakes,
        Capability::WakeLock,
        Capability::Video,
        Capability::WebViews,
        Capability::Sound,
        Capability::Midi,
        Capability::Navigation,
        Capability::TextSelection,
        Capability::Widgets,
        Capability::ContentIndex,
        Capability::Dialogs,
        Capability::DragOut,
        Capability::DragIn,
    ];

    /// The name an entry, a refusal and a report use.
    pub const fn name(self) -> &'static str {
        match self {
            Capability::Markdown => "markdown",
            Capability::Motion => "motion",
            Capability::Collections => "collections",
            Capability::Drag => "drag",
            Capability::Surfaces => "surfaces",
            Capability::Router => "router",
            Capability::Format => "format",
            Capability::Materials => "materials",
            Capability::Backdrop => "backdrop",
            Capability::Share => "share",
            Capability::Documents => "documents",
            Capability::Picker => "picker",
            Capability::Timelines => "timelines",
            Capability::TextTransform => "text_transform",
            Capability::Effects => "effects",
            Capability::Animations => "animations",
            Capability::Gradients => "gradients",
            Capability::Grid => "grid",
            Capability::Geometry => "geometry",
            Capability::Segments => "segments",
            Capability::Dataset => "dataset",
            Capability::Tabs => "tabs",
            Capability::GroupedLists => "grouped_lists",
            Capability::Io => "io",
            Capability::Transitions => "transitions",
            Capability::MediaPicker => "media_picker",
            Capability::AuthSession => "auth_session",
            Capability::Numerals => "numerals",
            Capability::Dates => "dates",
            Capability::Sqlite => "sqlite",
            Capability::Tips => "tips",
            Capability::Shadows => "shadows",
            Capability::Corners => "corners",
            Capability::Rotate => "rotate",
            Capability::TextStroke => "text_stroke",
            Capability::SymbolPalette => "symbol_palette",
            Capability::Presence => "presence",
            Capability::Modules => "modules",
            Capability::Tools => "tools",
            Capability::Svg => "svg",
            Capability::Images => "images",
            Capability::NativeViews => "native_views",
            Capability::ContentRegions => "content_regions",
            Capability::Traces => "traces",
            Capability::Controls => "controls",
            Capability::Flights => "flights",
            Capability::Sticky => "sticky",
            Capability::Notifications => "notifications",
            Capability::HostEvents => "host_events",
            Capability::Activities => "activities",
            Capability::Wakes => "wakes",
            Capability::WakeLock => "wake_lock",
            Capability::Video => "video",
            Capability::WebViews => "web_views",
            Capability::Sound => "sound",
            Capability::Midi => "midi",
            Capability::Navigation => "navigation",
            Capability::TextSelection => "text_selection",
            Capability::Widgets => "widgets",
            Capability::ContentIndex => "content_index",
            Capability::Dialogs => "dialogs",
            Capability::DragOut => "drag_out",
            Capability::DragIn => "drag_in",
        }
    }

    /// The capability `name` names ([`Capability::name`]), as a manifest's
    /// `link` list does (LLP 1047 D8).
    pub fn from_name(name: &str) -> Option<Capability> {
        Capability::ALL.into_iter().find(|c| c.name() == name)
    }

    const fn bit(self) -> u64 {
        1 << self as u32
    }
}

/// A set of capabilities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Uses(u64);

impl Uses {
    /// The core alone.
    pub const NONE: Uses = Uses(0);

    /// The native roster's entries (LLP 1047.001 D10), which only an Apple
    /// set names: [`apple_uses`] and the Apple bake select them, and no other
    /// host links anything for them.
    pub const NATIVE: Uses = Uses::NONE
        .with(Capability::Modules)
        .with(Capability::Tools)
        .with(Capability::Svg)
        .with(Capability::Images)
        .with(Capability::NativeViews)
        .with(Capability::ContentRegions)
        .with(Capability::Traces)
        .with(Capability::Controls)
        .with(Capability::Flights)
        .with(Capability::Sticky)
        .with(Capability::Notifications)
        .with(Capability::HostEvents)
        .with(Capability::Activities)
        .with(Capability::Wakes)
        .with(Capability::WakeLock)
        .with(Capability::Video)
        .with(Capability::WebViews)
        .with(Capability::Sound)
        .with(Capability::Midi)
        .with(Capability::Navigation)
        .with(Capability::TextSelection)
        .with(Capability::Widgets)
        .with(Capability::ContentIndex)
        .with(Capability::Dialogs)
        .with(Capability::DragIn);

    /// The native roster's entries the Mac's core presents itself, which a
    /// macOS set never names: a `dialog` is an entry of the top-layer stack
    /// a sheet route presents in too, and a drop is the core's
    /// (`FileDragMac.swift`).
    pub const MAC_CORE: Uses = Uses::NONE
        .with(Capability::Dialogs)
        .with(Capability::DragIn);

    /// What every Android APK ships whatever its plan uses, so a later
    /// generation may use it: the root's drag adapters, `DragIn.kt` and
    /// `DragOut.kt`, and the `doc:` table the drop path mints a dropped or
    /// pasted file's handle in (LLP 1069.010, LLP 1094.001 D5, LLP 1094.000
    /// D7).
    pub const ANDROID_CORE: Uses = Uses::NONE
        .with(Capability::Documents)
        .with(Capability::DragIn)
        .with(Capability::DragOut);

    /// This set and `capability`.
    pub const fn with(self, capability: Capability) -> Uses {
        Uses(self.0 | capability.bit())
    }

    fn with_style(self, style: StyleId) -> Uses {
        style.capability().map_or(self, |name| {
            self.with(Capability::from_name(name).expect("schema style capability"))
        })
    }

    /// Whether the set holds `capability`.
    pub const fn has(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    /// Include capabilities selected by a data module's grants.
    pub fn with_grants(self, grants: &str) -> Uses {
        let mut uses = self;
        if !crate::io_grants(grants).is_empty() {
            uses = uses.with(Capability::Io);
        }
        for line in grants.lines() {
            if exact_grants::line_presence(line).is_some() {
                uses = uses.with(Capability::Presence);
            }
            // The listener's and the data executor's own lines, outside the
            // I/O grammar, admit work the executor runs.
            if exact_grants::process::is_process_line(line.trim()) {
                uses = uses.with(Capability::Io);
            }
            uses = match line.split_whitespace().next() {
                Some("sqlite.open") => uses.with(Capability::Sqlite),
                // The sign-in sheet's answer reaches its source through the
                // executor's pump.
                Some("auth.session") => uses.with(Capability::AuthSession).with(Capability::Io),
                Some("net.listen" | "env.launch") => uses.with(Capability::Io),
                _ => uses,
            };
        }
        uses
    }

    /// What this set holds that `linked` doesn't.
    pub const fn beyond(self, linked: Uses) -> Uses {
        Uses(self.0 & !linked.0)
    }

    /// Whether the set is the core alone.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The capabilities in the set, in bit order.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL.into_iter().filter(move |c| self.has(*c))
    }
}

/// The names, comma-separated: `markdown, router`.
impl fmt::Display for Uses {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, capability) in self.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            f.write_str(capability.name())?;
        }
        Ok(())
    }
}

/// The capabilities `plan` uses (LLP 1047 D2).
pub fn uses(plan: &Plan) -> Uses {
    let mut uses = Uses::NONE;
    if plan.tools.iter().any(|t| {
        serde_json::from_str::<serde_json::Value>(plan.str(t.face)).is_ok_and(|face| {
            face["doors"]
                .as_array()
                .is_some_and(|doors| doors.iter().any(|d| d == "apple"))
                && face["parameters"].as_object().is_some_and(|params| {
                    params
                        .values()
                        .any(|p| p.get("type").is_some_and(|t| t == "file"))
                })
        })
    }) {
        uses = uses.with(Capability::Picker);
    }
    if !plan.keyframes.is_empty() {
        uses = uses.with(Capability::Animations);
    }
    if plan.router.is_some() {
        uses = uses.with(Capability::Router);
    }
    // Kept answers and surface records both need the host's store, and a
    // declared source its network (LLP 1162.001 D3).
    if plan.resources.iter().any(|r| r.reader || r.keep)
        || !plan.fetches.is_empty()
        || plan.slots.iter().any(|s| s.is_persisted())
    {
        uses = uses.with(Capability::Io);
    }
    // A segment length is a value, not a row: any string of the plan's
    // naming one (a literal, a template's piece) can reach a dimension row,
    // so the set is never smaller than what a run can reach.
    if plan.strings.iter().any(names_segment) {
        uses = uses.with(Capability::Segments);
    }
    if !plan.surfaces.is_empty()
        || plan.resources.iter().any(|r| {
            matches!(
                plan.str(r.source),
                crate::surface_record::SOURCE | crate::canvas_performance::SOURCE
            )
        })
    {
        uses = uses.with(Capability::Surfaces);
    }
    let can_be = |binding: &exact_plan::BindingsRow, used: &dyn Fn(&str) -> bool| {
        constant_str(plan, plan.code(binding.expr)).is_none_or(used)
    };
    for binding in plan.bindings.iter() {
        match binding.kind {
            BindingKind::Prop => match PropId::from_wire(binding.id) {
                Some(PropId::Markup) if can_be(&binding, &|v| v == "markdown") => {
                    uses = uses.with(Capability::Markdown);
                }
                Some(PropId::HeightDragFor | PropId::TransformDragFor | PropId::ReorderFor) => {
                    uses = uses.with(Capability::Motion).with(Capability::Drag);
                }
                // A drag's data is admitted for its host whether or not the
                // node drags now, and `draggable=false` is the web's too.
                Some(PropId::Draggable) => uses = uses.with(Capability::DragOut),
                // A shared element flies between its two places (LLP 1013.000).
                Some(PropId::SharedElement) => uses = uses.with(Capability::Transitions),
                Some(PropId::Virtualized)
                    if constant_bool(plan, plan.code(binding.expr)).is_none_or(|on| on) =>
                {
                    uses = uses.with(Capability::Collections);
                }
                Some(PropId::BackgroundMaterial) => uses = uses.with(Capability::Materials),
                Some(PropId::DecorationStyles) => {
                    for style in
                        decoration_rows(plan, &binding).unwrap_or_else(|| StyleId::ALL.to_vec())
                    {
                        uses = uses.with_style(style);
                    }
                }
                Some(PropId::ListStyle) => uses = uses.with(Capability::GroupedLists),
                Some(PropId::Dataset) => uses = uses.with(Capability::Dataset),
                Some(PropId::AccessibilityControls) => uses = uses.with(Capability::Tabs),
                Some(PropId::Tip) if can_be(&binding, &|v| !v.is_empty()) => {
                    uses = uses.with(Capability::Tips);
                }
                Some(PropId::Type) if can_be(&binding, &|v| v == "file") => {
                    uses = uses.with(Capability::Picker);
                }
                Some(PropId::Type)
                    if can_be(&binding, &|v| {
                        matches!(v, "date" | "time" | "datetime-local")
                    }) =>
                {
                    uses = uses.with(Capability::Dates);
                }
                _ => {}
            },
            BindingKind::Style => {
                if let Some(style) = StyleId::from_bit(u32::from(binding.id)) {
                    uses = uses.with_style(style);
                }
                if StyleId::from_bit(u32::from(binding.id)) == Some(StyleId::BackdropFilter) {
                    uses = uses.with(Capability::Backdrop);
                }
                if matches!(
                    StyleId::from_bit(u32::from(binding.id)),
                    Some(
                        StyleId::DragTimeline
                            | StyleId::AnimationTimeline
                            | StyleId::AnimationRange
                            | StyleId::TimelineScope
                            | StyleId::ScrollTimelineName
                            | StyleId::ScrollTimelineAxis
                            | StyleId::ViewTimelineName
                            | StyleId::ViewTimelineAxis
                            | StyleId::ViewTimelineInset
                    )
                ) {
                    uses = uses.with(Capability::Timelines).with(Capability::Motion);
                }
                if StyleId::from_bit(u32::from(binding.id)) == Some(StyleId::TextTransform) {
                    uses = uses.with(Capability::TextTransform);
                }
                if StyleId::from_bit(u32::from(binding.id)) == Some(StyleId::ContentTransition) {
                    uses = uses.with(Capability::Numerals);
                }
                if StyleId::from_bit(u32::from(binding.id)) == Some(StyleId::Transition)
                    && can_be(&binding, &|v| v.contains("spring"))
                {
                    uses = uses.with(Capability::Motion);
                }
            }
        }
    }
    for handler in &plan.handlers {
        match handler.event {
            // A pan's release velocity is the engine's tracker where the
            // platform measures none (LLP 1057 §10.6; LLP 1047 stage 3).
            EventKind::Swiperight | EventKind::Swipeleft | EventKind::Panrelease => {
                uses = uses.with(Capability::Motion)
            }
            EventKind::Heightrelease
            | EventKind::Transformgeometry
            | EventKind::Transformrelease
            | EventKind::Reorderdrop
            | EventKind::Reordertargetchange => {
                uses = uses.with(Capability::Motion).with(Capability::Drag);
            }
            // Dropped and pasted files become `doc:` handles; the web's drag
            // is heard beside its drop (files.js), and UIKit's is a module's.
            EventKind::Drop | EventKind::Dragenter | EventKind::Dragleave => {
                uses = uses.with(Capability::Documents).with(Capability::DragIn)
            }
            EventKind::Paste => uses = uses.with(Capability::Documents),
            _ => {}
        }
    }
    if plan.handlers.iter().any(|h| {
        matches!(
            h.event,
            EventKind::Reachstart | EventKind::Reachend | EventKind::Reorderdrop
        )
    }) {
        uses = uses.with(Capability::Collections);
    }
    let (format, geometry, lowercase) = stdlib_calls(plan);
    if format {
        uses = uses.with(Capability::Format);
    }
    // `toLowerCase` and `toUpperCase` map through the same case tables (LLP 1088 D2).
    if lowercase {
        uses = uses.with(Capability::TextTransform);
    }
    if runs_command(plan, &["share"]) {
        uses = uses.with(Capability::Share);
    }
    if runs_command(
        plan,
        &[
            "saveFile",
            "showOpenFilePicker",
            "showDirectoryPicker",
            "showSaveFilePicker",
            "requestPermission",
        ],
    ) {
        uses = uses.with(Capability::Documents);
    }
    if runs_command(plan, &["showPicker"]) {
        uses = uses.with(Capability::Picker);
    }
    if geometry {
        uses = uses.with(Capability::Geometry);
    }
    if shows_media_picker(plan) {
        uses = uses.with(Capability::MediaPicker);
    }
    uses
}

/// Whether a file input of `plan` can take iOS's photo picker: its `type`
/// can be `file` and its `accept`, as the host reads it, is a non-empty list
/// of only `image/…` and `video/…` types, or is computed. `showPicker` names
/// an input, so the inputs decide.
fn shows_media_picker(plan: &Plan) -> bool {
    let media = |accept: &str| {
        let types: Vec<String> = accept
            .split(',')
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        !types.is_empty()
            && types
                .iter()
                .all(|t| t.starts_with("image/") || t.starts_with("video/"))
    };
    plan.nodes.iter().any(|node| {
        let prop = |id: PropId| {
            node.bindings
                .iter()
                .map(|b| plan.binding(b))
                .find(|b| b.kind == BindingKind::Prop && b.id == id as u16)
        };
        let file = prop(PropId::Type)
            .is_some_and(|b| constant_str(plan, plan.code(b.expr)).is_none_or(|v| v == "file"));
        file && prop(PropId::Accept)
            .is_some_and(|b| constant_str(plan, plan.code(b.expr)).is_none_or(media))
    })
}

/// Whether the plan says `portal` anywhere (LLP 1013.002): only then does
/// placement run, on any host.
pub fn portals(plan: &Plan) -> bool {
    (plan.bindings.iter()).any(|b| b.kind == BindingKind::Prop && b.id == PropId::Portal as u16)
}

/// Whether node `node` is a zoomable scroller (LLP 1057.001.000 D1): either
/// range row written, literal or bound. The web loads its zoom module, and the
/// wasm host names its absence, only for a plan with one.
pub fn zoomable(plan: &Plan, node: exact_plan::NodesId) -> bool {
    plan.node(node)
        .bindings
        .iter()
        .map(|b| plan.binding(b))
        .any(|b| {
            b.kind == BindingKind::Style
                && matches!(
                    StyleId::from_bit(b.id as u32),
                    Some(StyleId::MinZoom | StyleId::MaxZoom)
                )
        })
}

/// Whether `plan` has a zoomable scroller.
pub fn zooms(plan: &Plan) -> bool {
    (0..plan.nodes.len()).any(|i| zoomable(plan, exact_plan::NodesId(i as u32)))
}

/// Whether `plan` can show an SVG island (LLP 1055.000 D10, D14): a `mask`
/// or `filter` element, or `filter` bound on an SVG element (its functions
/// need no element). A host whose island module is loaded on demand (LLP
/// 1047 D1's loaded tier; Apple's `libexact_svg.dylib`) opens it off the
/// main thread at boot when this holds, and never for a plan without one.
pub fn svg_islands(plan: &Plan) -> bool {
    use exact_kernel::NodeType;
    svg_filters(plan)
        || plan
            .nodes
            .iter()
            .any(|node| NodeType::from_wire(node.node_type) == Some(NodeType::SvgMask))
}

/// Whether `plan` can show an SVG filter: a `filter` element, or `filter`
/// bound on an SVG element. A host that draws a filtered picture on the GPU
/// (Apple's `SvgFilterMetal`) makes its pipelines off the main thread at
/// boot when this holds: they compile on first use, and the first picture
/// is drawn in the commit that shows it.
pub fn svg_filters(plan: &Plan) -> bool {
    use exact_kernel::NodeType;
    plan.nodes
        .iter()
        .any(|node| match NodeType::from_wire(node.node_type) {
            Some(NodeType::SvgFilter) => true,
            Some(t) if t == NodeType::Svg || t.is_svg_element() => node.bindings.iter().any(|b| {
                let b = plan.binding(b);
                b.kind == BindingKind::Style
                    && StyleId::from_bit(u32::from(b.id)) == Some(StyleId::Filter)
            }),
            _ => false,
        })
}

/// Whether any code range calls a `format` entry, whether any reads
/// geometry (`frame`, `measure`, `elementFromPoint`), and whether any calls `toLowerCase` or `toUpperCase`: each
/// validated body walked whole, so no call a run can reach is missed.
fn stdlib_calls(plan: &Plan) -> (bool, bool, bool) {
    let (mut format, mut geometry, mut lowercase) = (false, false, false);
    plan.each_code(&mut |code| {
        for i in crate::vm::instructions(plan.read_code(plan.code(code)).0).flatten() {
            if i.op == Opcode::Call {
                match Stdlib::from_wire(i.args[0] as u8) {
                    Some(
                        Stdlib::FormatShortcut
                        | Stdlib::FormatDate
                        | Stdlib::FormatNumber
                        | Stdlib::ToFixed
                        | Stdlib::FormatDecimal
                        | Stdlib::FormatDay,
                    ) => format = true,
                    // The core formats `en-US`; another locale's time is
                    // the capability's (LLP 1054.000.003 D6).
                    Some(Stdlib::FormatTime) if plan.format.is_some() => format = true,
                    Some(
                        Stdlib::Frame
                        | Stdlib::RangeFrame
                        | Stdlib::ScrollMetrics
                        | Stdlib::Measure
                        | Stdlib::ElementFromPoint
                        | Stdlib::ElementsFromPoint
                        | Stdlib::QuerySelectorAll,
                    ) => geometry = true,
                    Some(Stdlib::ToLowerCase | Stdlib::ToUpperCase) => lowercase = true,
                    _ => {}
                }
            }
        }
    });
    (format, geometry, lowercase)
}

/// Whether any code range runs a host command named one of `names`.
/// Whether `plan` declares a sound or a sequence, or runs one of the voice
/// table's commands (LLP 1096; LLP 1125 D3): what links the JS target's
/// table and its output.
pub fn runs_sounds(plan: &Plan) -> bool {
    !plan.sounds.is_empty()
        || !plan.sequences.is_empty()
        || runs_command(
            plan,
            &["playSound", "playSounds", "stopSounds", "setSounds"],
        )
}

/// Whether `plan` asks for MIDI ports or hears an input (LLP 1125 D6–D8):
/// it reads `exactMIDI`, runs `sendMIDI` or `requestMIDIAccess`, declares a
/// `midimessage` handler, or plays a sequence with `clockOut=`. What loads a
/// host's MIDI arm (rulings #10); a sequence's notes alone do not, since
/// without access no note is sent.
pub fn uses_midi(plan: &Plan) -> bool {
    plan.resources
        .iter()
        .any(|r| plan.str(r.source) == crate::midi::SOURCE)
        || runs_command(plan, &["sendMIDI", "requestMIDIAccess"])
        || plan.nodes.iter().any(|n| {
            n.handlers
                .iter()
                .any(|h| plan.handler(h).event == exact_plan::EventKind::Midimessage)
        })
        || plan.sequences.iter().any(|q| {
            constant_str(plan, plan.code(q.clock_out)) != Some("")
                || constant_str(plan, plan.code(q.clock)) != Some("")
        })
}

/// Whether the plan floats an image in picture in picture (LLP 1042 §9),
/// which the video arm carries on Apple.
pub fn uses_picture_in_picture(plan: &Plan) -> bool {
    runs_command(plan, &["requestPictureInPicture"])
}

/// A remote or computed image, media source, video poster or iframe URL
/// requires the Mac App Store sandbox's `network.client` (LLP 1030.005 D2).
pub fn sources_can_be_remote(plan: &Plan) -> bool {
    plan.bindings.iter().any(|binding| {
        binding.kind == BindingKind::Prop
            && matches!(
                PropId::from_wire(binding.id),
                Some(PropId::Src | PropId::ImageSource | PropId::Poster)
            )
            && constant_str(plan, plan.code(binding.expr)).is_none_or(exact_plan::has_http_scheme)
    })
}

/// Whether the runner keeps a MIDI queue the agent shows (`state.midi`):
/// for a plan that [`uses_midi`] or declares a sequence, whose notes and
/// clock go on it. The JS target bundles midi.js then.
pub fn keeps_midi(plan: &Plan) -> bool {
    uses_midi(plan) || !plan.sequences.is_empty()
}

/// Whether the plan's code runs `setLocale`: only then does a language the
/// app chose, kept in the store, choose its strings at the next launch.
pub fn chooses_locale(plan: &Plan) -> bool {
    runs_command(plan, &["setLocale"])
}

/// Whether the plan runs `setScheme`: then the scheme it chose is kept
/// across launches.
pub fn chooses_scheme(plan: &Plan) -> bool {
    runs_command(plan, &["setScheme"])
}

fn runs_command(plan: &Plan, names: &[&str]) -> bool {
    let mut runs = false;
    plan.each_code(&mut |code| {
        runs = runs
            || crate::vm::instructions(plan.read_code(plan.code(code)).0).any(|i| {
                i.is_ok_and(|i| {
                    i.op == Opcode::Command
                        && plan
                            .strings
                            .get(i.args[0] as usize)
                            .is_some_and(|_| names.contains(&plan.str(StrId(i.args[0] as u32))))
                })
            });
    });
    runs
}

/// The boolean a binding always evaluates to, when its code is one constant:
/// `Bool`, then `Return`.
fn constant_bool(plan: &Plan, code: &[u8]) -> Option<bool> {
    match plan.read_code(code).0 {
        [op, value, ret] if *op == Opcode::Bool as u8 && *ret == Opcode::Return as u8 => {
            Some(*value != 0)
        }
        _ => None,
    }
}

/// The string a binding always evaluates to, when its code is one constant:
/// `Str`, then `Return`.
/// The style rows a decoration table binding names (LLP 1114.007 D1): the
/// compiler writes the table as a literal JSON string, and a table that is
/// not a literal can name any row, so it is `None`.
pub(super) fn decoration_rows(
    plan: &Plan,
    binding: &exact_plan::BindingsRow,
) -> Option<Vec<StyleId>> {
    let json = constant_str(plan, plan.code(binding.expr))?;
    let Ok(serde_json::Value::Object(table)) = serde_json::from_str(json) else {
        // The host refuses a table that does not decode, parsing no row.
        return Some(Vec::new());
    };
    Some(
        table
            .values()
            .filter_map(serde_json::Value::as_array)
            .flatten()
            .filter_map(|row| {
                row.get(0)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|id| u32::try_from(id).ok())
                    .and_then(StyleId::from_bit)
            })
            .collect(),
    )
}

pub(crate) fn constant_str<'a>(plan: &'a Plan, code: &[u8]) -> Option<&'a str> {
    match plan.read_code(code).0 {
        [op, a, b, c, d, ret] if *op == Opcode::Str as u8 && *ret == Opcode::Return as u8 => {
            let id = u32::from_le_bytes([*a, *b, *c, *d]);
            plan.strings.get(id as usize).map(|_| plan.str(StrId(id)))
        }
        _ => None,
    }
}

/// Whether a string names a viewport segment variable (`viewport-segment-`
/// anywhere in it): a byte scan, so the core carries no string searcher
/// for it.
fn names_segment(s: &str) -> bool {
    const NAME: &[u8] = b"viewport-segment-";
    s.as_bytes().windows(NAME.len()).any(|w| w == NAME)
}

#[cfg(test)]
mod grant_tests {
    use super::{Capability, Uses};

    #[test]
    fn presence_grants_link_the_backend_including_optional_names() {
        for policy in [
            "user-presence",
            "biometry-any",
            "biometry-current-set",
            "device-credential",
        ] {
            for optional in ["", " optional"] {
                let grants =
                    format!("secret.keep token {policy}{optional}\ndevice.local-auth purpose.lock");
                let uses = Uses::NONE.with_grants(&grants);
                assert!(uses.has(Capability::Io));
                assert!(uses.has(Capability::Presence), "{grants}: {uses}");
            }
        }
        for grants in ["", "secret.keep token", "device.local-auth purpose.lock"] {
            assert!(!Uses::NONE.with_grants(grants).has(Capability::Presence));
        }
    }

    #[test]
    fn persisted_slots_link_storage_without_resources() {
        use exact_plan::PersistScope;
        use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};
        for persist in [PersistScope::None, PersistScope::App, PersistScope::Window] {
            let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
            let boolean = b.primitive(TypeKind::Bool);
            let mut initial = Asm::new();
            initial.bool(false);
            let initial = b.code(initial);
            let slot = b.slot("liked", boolean, initial);
            b.set_slot_persist(slot, persist);
            b.node(
                exact_kernel::NodeType::View as u8,
                None,
                None,
                0,
                &[],
                &[],
                None,
            );
            assert_eq!(
                super::uses(&b.finish().unwrap()).has(Capability::Io),
                persist != PersistScope::None
            );
        }
    }

    #[test]
    fn kept_surfaces_link_storage_without_a_data_reader() {
        use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};

        for keep in [false, true] {
            let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
            let number = builder.primitive(TypeKind::Number);
            let hud = builder.record("Hud", &[("count", number)]);
            let world = builder.str("world");
            let mut argument = Asm::new();
            argument.str(world);
            let argument = builder.code(argument);
            let resource = builder.resource("hud", "exactSurface", &[argument], hud, None);
            builder.surface("world", &[]);
            if keep {
                builder.set_resource_keep(resource);
            }
            builder.node(
                exact_kernel::NodeType::View as u8,
                None,
                None,
                0,
                &[],
                &[],
                None,
            );
            let plan = builder.finish().unwrap();
            assert!(!plan.resources[resource.0 as usize].reader);
            let uses = super::uses(&plan);
            assert!(uses.has(Capability::Surfaces));
            assert_eq!(uses.has(Capability::Io), keep);
        }
    }

    #[test]
    fn the_listener_the_processes_and_the_sign_in_sheet_link_io() {
        for grants in [
            "net.listen unix:/tmp/exact-listen-fixture",
            "process.spawn /usr/bin/git",
            "auth.session https://example.test",
        ] {
            assert!(
                Uses::NONE.with_grants(grants).has(Capability::Io),
                "{grants}"
            );
        }
    }

    #[test]
    fn sqlite_grants_select_sql_storage_apart_from_other_io() {
        let fetch = Uses::NONE.with_grants("net.fetch https://example.test");
        assert!(fetch.has(Capability::Io));
        assert!(!fetch.has(Capability::Sqlite));
        let files = Uses::NONE.with_grants("fs.read app:/data\nfs.write app:/data");
        assert!(files.has(Capability::Io));
        assert!(!files.has(Capability::Sqlite));
        let sql = Uses::NONE.with_grants("  sqlite.open app:/data/notes.db\n");
        assert!(sql.has(Capability::Io));
        assert!(sql.has(Capability::Sqlite));
        assert_eq!(sql.beyond(fetch).to_string(), "sqlite");
        assert_eq!(Uses::NONE.with_grants(""), Uses::NONE);
        assert!(Uses::NONE
            .with_grants("auth.session https://example.test")
            .has(Capability::AuthSession));
        assert!(!Uses::NONE
            .with_grants("auth.callback app:/oauth")
            .has(Capability::AuthSession));
    }
}
