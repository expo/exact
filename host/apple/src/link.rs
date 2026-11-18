//! What this archive links beyond the core (LLP 1047.001 D2, D5).
//!
//! @ref LLP 1047 D6 (a host refuses a plan that uses what it doesn't link)
//! @ref LLP 1047.001 D2 (one composition per app), D5 (refused before anything changes)
//!
//! An app archive is one composition (LLP 1031: an embedder links one), so
//! the set is the process's: the generated entry names it before the first
//! runtime exists, and every boot — the first, a reload, a delivered
//! candidate — admits its plan against it after decoding.

pub use exact_runner::{Capability, Uses};
use exact_runner::{DataSource, DeviceLinks, RunnerLinks};
use std::sync::OnceLock;

static LINKED: OnceLock<Uses> = OnceLock::new();

/// Every capability: what an entry that names none links (a hand-written
/// `host!`, a development build).
pub const ALL: Uses = {
    let mut uses = Uses::NONE;
    let mut i = 0;
    while i < Capability::ALL.len() {
        uses = uses.with(Capability::ALL[i]);
        i += 1;
    }
    uses
};

/// Name this archive's linked set. The first call wins: the entry makes it
/// at the first `exact_create`, before any boot.
pub fn set(linked: Uses) {
    let _ = LINKED.set(linked);
}

/// This archive's linked set; every capability until an entry names one.
pub fn linked() -> Uses {
    LINKED.get().copied().unwrap_or(ALL)
}

/// The kernel's style grammars and grid layout, which a native host links
/// by use (LLP 1047.001 D3, D10), and the families' kernel code the host's
/// core paths reach ([`families`]): each `Some` is registered at boot.
#[derive(Clone, Copy)]
pub struct Grammars {
    backdrop: Option<fn()>,
    segments: Option<fn()>,
    timelines: Option<fn()>,
    wide_colors: Option<fn()>,
    grid: Option<fn()>,
    svg: Option<fn()>,
    effects: Option<fn()>,
}

impl Grammars {
    /// Every grammar.
    pub const ALL: Grammars = Grammars::of(ALL);

    /// The grammars `uses` needs. Wide colours are linked always: the web
    /// decides them from the plan's strings and the data crate's source,
    /// which a native entry does not read.
    pub const fn of(uses: Uses) -> Grammars {
        Grammars {
            backdrop: if uses.has(Capability::Backdrop) {
                Some(exact_kernel::style::link_backdrop_filter)
            } else {
                None
            },
            segments: if uses.has(Capability::Segments) {
                Some(exact_kernel::style::link_segments)
            } else {
                None
            },
            timelines: if uses.has(Capability::Timelines) {
                Some(exact_kernel::timeline::link)
            } else {
                None
            },
            wide_colors: Some(exact_kernel::style::link_wide_colors),
            grid: if uses.has(Capability::Grid) {
                Some(exact_kernel::layout::link_grid_layout)
            } else {
                None
            },
            svg: if uses.has(Capability::Svg) {
                Some(families::link_svg)
            } else {
                None
            },
            effects: if uses.has(Capability::Effects) {
                Some(families::link_effects)
            } else {
                None
            },
        }
    }

    /// Register them.
    pub fn link(self) {
        for link in [
            self.backdrop,
            self.segments,
            self.timelines,
            self.wide_colors,
            self.grid,
            self.svg,
            self.effects,
        ]
        .into_iter()
        .flatten()
        {
            link();
        }
    }
}

/// The families' kernel code the host's core paths call (LLP 1047.001 D10):
/// registered at boot when the composition links the family ([`Grammars`]),
/// so an archive without it names none of that code. Unregistered, each
/// answers what a plan without the family needs, which is all admission lets
/// such an archive run.
pub(crate) mod families {
    use exact_kernel::svg::filter::{Filter, FilterList};
    use exact_kernel::{style::ColorValue, Kernel, NodeRef, ViewId};
    use std::{collections::HashSet, sync::OnceLock};

    type Sources = fn(&Kernel, ViewId) -> HashSet<ViewId>;
    type Blended = fn(&Kernel, &NodeRef<'_>) -> bool;
    type BoxFilter = fn(&FilterList, ColorValue, bool) -> Option<Filter>;

    static SVG: OnceLock<(Sources, Blended)> = OnceLock::new();
    static EFFECTS: OnceLock<BoxFilter> = OnceLock::new();

    /// SVG: the nodes a scene reads, and whether a node is drawn into one.
    pub fn link_svg() {
        let _ = SVG.set((
            exact_kernel::svg::scene_sources,
            exact_kernel::svg::in_blended_scene,
        ));
    }

    /// Box effects: a box's `filter` resolved to its program.
    pub fn link_effects() {
        let _ = EFFECTS.set(exact_kernel::svg::scene::box_filter);
    }

    /// The nodes `root`'s scene reads; none without SVG.
    pub(crate) fn scene_sources(kernel: &Kernel, root: ViewId) -> HashSet<ViewId> {
        SVG.get()
            .map_or_else(HashSet::new, |svg| (svg.0)(kernel, root))
    }

    /// Whether `node` is drawn into an SVG's pixels; never without SVG.
    pub(crate) fn in_blended_scene(kernel: &Kernel, node: &NodeRef<'_>) -> bool {
        SVG.get().is_some_and(|svg| (svg.1)(kernel, node))
    }

    /// A box's `filter` as its program; none without box effects.
    pub(crate) fn box_filter(list: &FilterList, text: ColorValue, dark: bool) -> Option<Filter> {
        EFFECTS.get().and_then(|filter| filter(list, text, dark))
    }
}

/// The host's I/O (LLP 1047.001): the app's bindings and the stores they
/// keep, and the executor that runs a source's requests over them. Linked
/// when the plan keeps answers or the app's grants name I/O.
#[derive(Clone, Copy)]
pub struct IoLinks {
    /// The process-start work, independent of any session.
    pub preloads: fn() -> Option<Box<dyn crate::launch_preloads::LaunchWork>>,
    /// Bind the app's grants: [`crate::store::endowed`].
    pub endow: fn(&str, &str, bool) -> crate::store::Endowed,
    /// Read the stores only on an explicitly requested refresh.
    pub snapshot: StoreSnapshot,
    /// Start the executor: [`crate::executor::start_io`].
    pub start: StartIo,
    /// Write a canvas capture's image under the app's `fs.write` grants.
    pub write_file: AppFileWrite,
    /// Copy an app file a share names for the sheet, under the app's
    /// `fs.read` grants: [`crate::picker::share_copy`].
    pub share_copy: ShareCopy,
    /// Tell the app's widgets its store changed: [`crate::store::widget_changed`].
    pub widget_changed: fn(&str),
}

/// How [`IoLinks`] copies an app file a share names (`{"path","to"}`) under
/// the grants, from the app's roots (none where it has no storage), and
/// answers the share's JSON.
pub type ShareCopy = fn(&str, &str, Option<&[std::path::PathBuf; 3]>) -> String;

/// How [`IoLinks`] writes an image at an app path among the app's roots
/// (`app:/data`, `app:/cache`, `app:/tmp`), under the `fs.write` lines of
/// the grants narrowed by a source's scope, or names the refusal.
pub type AppFileWrite = fn(
    &[std::path::PathBuf; 3],
    &str,
    Option<&str>,
    &str,
    &[u8],
) -> Result<(), (&'static str, String)>;

/// Read a linked app's stores through its endowed bindings, or say why the
/// read cannot be used.
pub type StoreSnapshot =
    fn(Option<&ibex2::host::Bindings>, &str) -> Result<Vec<(String, Vec<u8>)>, String>;

/// How [`IoLinks`] starts the executor: over the bindings, for the grants,
/// waking the presenter through the hook.
pub type StartIo = fn(
    Option<ibex2::host::Bindings>,
    &str,
    Option<(crate::executor::WakeFn, *mut std::ffi::c_void)>,
) -> Box<dyn crate::executor::Io>;

impl IoLinks {
    /// The platform's.
    pub const LINKED: IoLinks = IoLinks {
        preloads: crate::launch_preloads::linked,
        endow: crate::store::endowed::<true>,
        snapshot: crate::store::snapshot_for,
        start: crate::executor::start_io,
        write_file: crate::host::canvas_capture::write,
        share_copy: crate::picker::share_copy,
        widget_changed: crate::store::widget_changed,
    };
}

/// A turn's family work (LLP 1047.001 D10): after its commits, which
/// `Host::finish` runs in this order, the documents a source's answers
/// watch (I/O), then, after the resize rounds, the SVG scenes the batch
/// owes; and through the turn, what Core Animation plays of the motion.
/// Each is `None` when the composition does not link its family, so the
/// archive names none of its code.
pub struct TurnLinks<D: DataSource> {
    /// Reconcile the watched documents with the answers' (`doc_watch`).
    pub documents: Option<fn(&mut crate::Host<D>)>,
    /// Send the SVG scenes laid out, moved or restyled this turn.
    pub svg: Option<fn(&mut crate::Host<D>, &mut crate::batch::Batch)>,
    /// Hand Core Animation the motion it can play (`svg_lower`): boxes',
    /// SVG paths' and live filters'. Linked when anything can move, as the
    /// engine is; where nothing moves there is nothing to hand over.
    pub motion: Option<MotionLowering<D>>,
}

/// [`TurnLinks::motion`]'s parts, each where the turn reaches it.
pub struct MotionLowering<D: DataSource> {
    /// At boot, over the engine's first sync of the tree: which motion Core
    /// Animation plays and which the engine samples.
    pub boot: fn(&mut crate::Host<D>, &exact_kernel::motion::MotionSync),
    /// The same for each commit's sync.
    pub commit: fn(&mut crate::Host<D>, &exact_kernel::motion::MotionSync),
    /// Before sampling: hand over the transitions Core Animation plays.
    pub present: fn(&mut crate::Host<D>),
    /// After a turn's layout: the boxes' lowered animations it owes.
    pub turn: fn(&mut crate::Host<D>, &mut crate::batch::Batch),
}

impl<D: DataSource> Clone for MotionLowering<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D: DataSource> Copy for MotionLowering<D> {}

/// Whether anything in a plan that `uses` can move: a spring or a hold, a
/// drag, keyframes, a timeline, or a transition CSS would play.
const fn moves(uses: Uses) -> bool {
    uses.has(Capability::Motion)
        || uses.has(Capability::Drag)
        || uses.has(Capability::Animations)
        || uses.has(Capability::Timelines)
        || uses.has(Capability::Transitions)
}

impl<D: DataSource> Clone for TurnLinks<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D: DataSource> Copy for TurnLinks<D> {}

impl<D: DataSource> TurnLinks<D> {
    /// What `uses` needs.
    pub const fn of(uses: Uses) -> TurnLinks<D> {
        TurnLinks {
            documents: if uses.has(Capability::Io) {
                Some(crate::Host::<D>::sync_documents)
            } else {
                None
            },
            svg: if uses.has(Capability::Svg) {
                Some(crate::Host::<D>::emit_svg_scenes)
            } else {
                None
            },
            motion: if moves(uses) {
                Some(MotionLowering {
                    boot: crate::Host::<D>::lower_motion,
                    commit: crate::Host::<D>::lower_committed_motion,
                    present: crate::Host::<D>::hand_over_transitions,
                    turn: crate::Host::<D>::emit_box_motion,
                })
            } else {
                None
            },
        }
    }
}

/// What a boot links (LLP 1047.001 D3): the runner's capabilities, its
/// device capabilities, the kernel's grammars and the host's I/O. An entry makes it in a
/// `const` from its set (`host!`), so the linker sees only what it names;
/// the public boots, which tests and tools use, link everything.
pub struct Links<D: DataSource> {
    /// The runner's capabilities (the router, lists, canvases, …).
    pub runner: RunnerLinks,
    /// The device capabilities (share, documents, pickers, notifications;
    /// authentication, which follows the grants, always).
    pub device: DeviceLinks<D>,
    /// The presenter found a packaged ActivityKit arm; false until it declares one.
    pub activities: bool,
    /// The kernel's style grammars.
    pub grammars: Grammars,
    /// The host's I/O; `None`, none (no request runs, nothing is kept).
    pub io: Option<IoLinks>,
    /// What the motion engine plays: everything, or settled values only
    /// when the plan animates nothing.
    pub engine: &'static exact_motion::EngineLinks,
    /// The runner's optional parts: native modules, tool calls and presence
    /// secrets (LLP 1047.001 D11).
    pub modules: exact_runner::ModuleLinks<D>,
    /// A turn's family work after its commits.
    pub turn: TurnLinks<D>,
}

impl<D: DataSource> Clone for Links<D> {
    fn clone(&self) -> Self {
        Links {
            runner: self.runner,
            device: self.device,
            activities: self.activities,
            grammars: self.grammars,
            io: self.io,
            engine: self.engine,
            modules: self.modules,
            turn: self.turn,
        }
    }
}

impl<D: DataSource> Links<D> {
    /// Every capability.
    pub const ALL: Links<D> = Links {
        runner: RunnerLinks::ALL,
        device: DeviceLinks::ALL,
        activities: false,
        grammars: Grammars::ALL,
        io: Some(IoLinks::LINKED),
        engine: &exact_motion::EngineLinks::ALL,
        modules: exact_runner::ModuleLinks::ALL,
        turn: TurnLinks::of(ALL),
    };

    /// What `uses` needs; evaluated in a `const`, so nothing else is named.
    pub const fn of(uses: Uses) -> Links<D> {
        let all = RunnerLinks::ALL;
        Links {
            runner: RunnerLinks {
                surface_answer: if uses.has(Capability::Surfaces) {
                    all.surface_answer
                } else {
                    None
                },
                router: if uses.has(Capability::Router) {
                    all.router
                } else {
                    None
                },
                lists: if uses.has(Capability::Collections) {
                    all.lists
                } else {
                    None
                },
                canvas: if uses.has(Capability::Surfaces) {
                    all.canvas
                } else {
                    None
                },
                format: if uses.has(Capability::Format) {
                    all.format
                } else {
                    None
                },
                geometry: if uses.has(Capability::Geometry) {
                    all.geometry
                } else {
                    None
                },
                grants: if uses.has(Capability::Io) {
                    all.grants
                } else {
                    None
                },
                keyframes: if uses.has(Capability::Animations) {
                    all.keyframes
                } else {
                    None
                },
                text_axes: if uses.has(Capability::Transitions) || uses.has(Capability::Animations)
                {
                    all.text_axes
                } else {
                    None
                },
            },
            device: DeviceLinks {
                auth: DeviceLinks::<D>::ALL.auth,
                share: if uses.has(Capability::Share) {
                    DeviceLinks::<D>::ALL.share
                } else {
                    None
                },
                documents: if uses.has(Capability::Documents) {
                    DeviceLinks::<D>::ALL.documents
                } else {
                    None
                },
                picker: if uses.has(Capability::Picker) {
                    DeviceLinks::<D>::ALL.picker
                } else {
                    None
                },
            },
            activities: false,
            grammars: Grammars::of(uses),
            io: if uses.has(Capability::Io) {
                Some(IoLinks {
                    endow: if uses.has(Capability::Presence) {
                        crate::store::endowed::<true>
                    } else {
                        crate::store::endowed::<false>
                    },
                    ..IoLinks::LINKED
                })
            } else {
                None
            },
            engine: if moves(uses) {
                &exact_motion::EngineLinks::ALL
            } else {
                &exact_motion::EngineLinks::SETTLE
            },
            modules: exact_runner::ModuleLinks {
                modules: if uses.has(Capability::Modules) {
                    exact_runner::ModuleLinks::<D>::ALL.modules
                } else {
                    exact_runner::ModuleLinks::<D>::NONE.modules
                },
                tools: if uses.has(Capability::Tools) {
                    exact_runner::ModuleLinks::<D>::ALL.tools
                } else {
                    exact_runner::ModuleLinks::<D>::NONE.tools
                },
                presence: if uses.has(Capability::Presence) {
                    exact_runner::ModuleLinks::<D>::ALL.presence
                } else {
                    exact_runner::ModuleLinks::<D>::NONE.presence
                },
                // As `RunnerLinks::text_axes`, which makes their engine.
                text_axes: if uses.has(Capability::Transitions) || uses.has(Capability::Animations)
                {
                    exact_runner::ModuleLinks::<D>::ALL.text_axes
                } else {
                    exact_runner::ModuleLinks::<D>::NONE.text_axes
                },
            },
            turn: TurnLinks::of(uses),
        }
    }
}

/// The capabilities `plan` uses that `linked` lacks, by name; `None` when
/// it links them all. A `data:` image that is an SVG uses SVG, as the bake
/// counts it, so a plan applied after the bake is held to the same rule; a
/// bundled asset's bytes are the bake's alone to read.
pub fn missing(plan: &exact_plan::Plan, linked: Uses) -> Option<String> {
    let mut uses = exact_runner::uses::apple_uses(plan);
    let data_svg = exact_runner::uses::image_sources(plan)
        .iter()
        .any(|source| {
            source
                .bundled()
                .and_then(|name| name.strip_prefix("data:"))
                .is_some_and(|url| {
                    exact_runner::uses::data_url(url).is_none_or(|b| exact_svg_file::sniff(&b))
                })
        });
    if data_svg {
        uses = uses.with(Capability::Svg).with(Capability::Effects);
    }
    let missing = uses.beyond(linked);
    (!missing.is_empty()).then(|| missing.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_holds_every_capability() {
        assert!(Capability::ALL.iter().all(|c| ALL.has(*c)));
    }

    #[test]
    fn io_links_the_presence_vault_only_when_selected() {
        struct Empty;
        impl DataSource for Empty {
            fn query(
                &mut self,
                _: &str,
                _: &[exact_plan::Value],
            ) -> Result<exact_plan::Value, exact_runner::DataError> {
                unreachable!()
            }
        }

        let grants = "secret.keep token user-presence optional";
        let memory = std::sync::Arc::new(ibex2::secrets::MemoryStore::new());
        crate::store::PLATFORM.with(|p| *p.borrow_mut() = Some(memory));
        for linked in [Uses::NONE.with(Capability::Io), ALL] {
            let io = Links::<Empty>::of(linked).io.unwrap();
            let endowed = (io.endow)(grants, "com.example.link.presence", false);
            assert!(endowed.unbound.is_none(), "{:?}", endowed.unbound);
            assert_eq!(endowed.presence.is_some(), linked.has(Capability::Presence));
        }
        crate::store::PLATFORM.with(|p| *p.borrow_mut() = None);
    }

    /// D5: a plan that uses what the archive doesn't link is named and
    /// refused; one within it is admitted.
    #[test]
    fn a_plan_beyond_the_linked_set_names_what_is_missing() {
        let grouped = contract::compile(
            "component A\n  view\n    list appearance=\"auto\"\n      section\n        text \"a\"\n",
        )
        .unwrap();
        let plain = contract::compile("component A\n  view\n    text \"a\"\n").unwrap();
        let without = Capability::ALL
            .into_iter()
            .filter(|c| *c != Capability::GroupedLists)
            .fold(Uses::NONE, Uses::with);
        assert_eq!(missing(&grouped, without).as_deref(), Some("grouped_lists"));
        assert_eq!(missing(&grouped, ALL), None);
        assert_eq!(missing(&plain, Uses::NONE), None);
        // A grid and a table both lay out by the grid algorithm (D10).
        let without = Capability::ALL
            .into_iter()
            .filter(|c| *c != Capability::Grid)
            .fold(Uses::NONE, Uses::with);
        for view in [
            "view display=\"grid\"\n      text \"a\"",
            "table\n      tr\n        td\n          text \"a\"",
        ] {
            let plan = contract::compile(&format!("component A\n  view\n    {view}\n")).unwrap();
            assert_eq!(missing(&plan, without).as_deref(), Some("grid"), "{view}");
            assert_eq!(missing(&plan, ALL), None);
        }
    }

    /// D5 for the native roster: on hello's binary, which links nothing
    /// beyond the core, a plan that uses an entry is refused by its name.
    #[test]
    fn hellos_binary_refuses_each_native_entry_by_name() {
        let tools = contract::compile(
            "component A\n  state n = 0\n  tool bump writes \"Add one.\" = bump\n  action bump\n    n = n + 1\n  view\n    text \"a\"\n",
        )
        .unwrap();
        let mut view = contract::compile("component A\n  view\n    text \"a\"\n").unwrap();
        view.add_view("swatch-box", ("swatch", "@fixture/swatch"), true, &[], "");
        let compile = |view: &str, act: &str| {
            contract::compile(&format!(
                "component A\n  state on = true\n  action go\n    {act}\n  view\n    {view}\n"
            ))
            .unwrap()
        };
        let svg = compile(
            "svg viewBox=\"0 0 10 10\"\n      circle cx=5 cy=5 r=4",
            "on = false",
        );
        let image = compile("image \"assets/a.png\"", "on = false");
        let hatch = compile("view hatch=\"avatar\"", "on = false");
        let control = compile("input type=\"checkbox\"", "on = false");
        let traced = compile("button \"b\" press=go", "startTrace_EXPERIMENTAL()");
        let filtered = compile("text \"a\" filter=\"blur(2px)\"", "on = false");
        for (plan, name) in [
            (&tools, "tools"),
            (&view, "modules, native_views"),
            (&svg, "effects, svg"),
            (&image, "images"),
            (&hatch, "native_views"),
            (&control, "controls"),
            (&traced, "traces"),
            (&filtered, "effects"),
        ] {
            assert_eq!(missing(plan, Uses::NONE).as_deref(), Some(name));
            assert_eq!(missing(plan, ALL), None);
        }
    }
}
