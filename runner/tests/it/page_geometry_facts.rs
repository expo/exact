//! The geometry facts where the page lays out and the kernel does not, as on the web's wasm
//! target (@ref LLP 1123 D6): `exactGeometry` answers from the page's drawn box, its
//! `generation` the number of the page's layouts, and a layout the page reports that moves what a
//! fact reads reaches it in a commit of its own, up to D1's bound of eight.

use exact_kernel::{Kernel, ViewId};
use exact_plan::Value;
use exact_runner::geometry::{GeometryAnswer, GeometryLinks, PageBox, PageSpace, Scrolled};
use exact_runner::{DataError, DataSource, Delivery, ModuleLinks, Runner, RunnerLinks};
use std::cell::RefCell;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("queried {name}")
    }
}

/// A drawn box: its size, and its frame in the document, the viewport and the node.
type Drawn = ([f64; 2], [Option<[f64; 4]>; 3]);

thread_local! {
    /// The box the page draws for `hero`; `None` for no box.
    static DRAWN: RefCell<Option<Drawn>> = const { RefCell::new(None) };
    /// The spaces the page was asked for.
    static ASKED: RefCell<Vec<PageSpace>> = const { RefCell::new(Vec::new()) };
}

fn drawn(size: [f64; 2], frames: [Option<[f64; 4]>; 3]) {
    DRAWN.with(|d| *d.borrow_mut() = Some((size, frames)));
}

fn page_box(_: ViewId, space: PageSpace) -> Option<PageBox> {
    ASKED.with(|a| a.borrow_mut().push(space));
    let (size, frames) = DRAWN.with(|d| *d.borrow())?;
    let at = match space {
        PageSpace::Owner => 0,
        PageSpace::Window => 1,
        PageSpace::Node(_) => 2,
    };
    Some(PageBox {
        size,
        frame: frames[at],
    })
}

fn nothing(_: &Kernel, _: &Scrolled, _: ViewId) -> GeometryAnswer {
    GeometryAnswer::UNAVAILABLE
}

fn measure_nothing(_: &mut Kernel, _: &Scrolled, _: ViewId) -> GeometryAnswer {
    GeometryAnswer::UNAVAILABLE
}

/// A page's links: every action read unavailable, the facts' box [`page_box`].
static PAGE: GeometryLinks = GeometryLinks::new(
    nothing,
    measure_nothing,
    |_, _, _, _| None,
    |_, _, _, _| Vec::new(),
    |_, _, _, _| false,
)
.with_page_box(page_box);

const SHAPES: &str = "shape Rect
  x: number
  y: number
  width: number
  height: number
shape Size
  width: number
  height: number
shape Hero
  available: bool
  generation: number
  size: option<Size>
  frame: option<Rect>
  observations: option<Rect>
  margins: option<Rect>
  scroll: option<Rect>
";

/// A runner on a page for an app reading `hero`'s geometry in `space`.
fn page(space: &str) -> Runner<NoData> {
    let component = format!(
        "component App
  state grows = 0
  action grow
    grows = grows + 1
  resource g = exactGeometry(\"hero\"{space}) as shape Hero
  view
    column
      column id=\"hero\" width=120 height=40
      column id=\"other\" width=10 height=grows
"
    );
    Runner::boot_with_delivery_linked(
        RunnerLinks {
            geometry: Some(&PAGE),
            ..RunnerLinks::ALL
        },
        ModuleLinks::ALL,
        contract::compile(&format!("{SHAPES}{component}")).unwrap(),
        NoData,
        Kernel::with_monospace(),
        None,
        Vec::new(),
        Delivery::default(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// The answer `g` holds: available, generation, size, frame and the rest.
fn g(r: &Runner<NoData>) -> Vec<Value> {
    match r.shown_resource("g") {
        Some(Value::Record(fields)) => fields.to_vec(),
        other => panic!("g is a record: {other:?}"),
    }
}

fn rect(fields: &[f64]) -> Value {
    Value::Option(Some(std::rc::Rc::new(Value::Record(
        fields.iter().copied().map(Value::Number).collect(),
    ))))
}

#[test]
fn the_page_answers_the_fact_and_numbers_it_by_its_layouts() {
    // Before the page drew anything: unavailable at the page's layout 0. The page draws
    // `hero` and reports its layout: answered in a commit of its own, at generation 1, with its
    // size and its frame in the document; the rest `none` until the web's owners report them. A
    // layout that moves nothing it reads commits nothing, and one that moves it does
    DRAWN.with(|d| *d.borrow_mut() = None);
    let mut r = page("");
    assert_eq!(g(&r)[..2], [Value::Bool(false), Value::Number(0.0)]);
    drawn([120.0, 40.0], [Some([8.0, 48.0, 120.0, 40.0]), None, None]);
    assert!(r.refresh_geometry().unwrap().is_some());
    let answer = g(&r);
    assert_eq!(answer[..2], [Value::Bool(true), Value::Number(1.0)]);
    assert_eq!(answer[2], rect(&[120.0, 40.0]));
    assert_eq!(answer[3], rect(&[8.0, 48.0, 120.0, 40.0]));
    assert_eq!(answer[4..], [Value::NONE, Value::NONE, Value::NONE]);
    assert!(r.refresh_geometry().unwrap().is_none());
    drawn([300.0, 40.0], [Some([8.0, 48.0, 300.0, 40.0]), None, None]);
    assert!(r.refresh_geometry().unwrap().is_some());
    assert_eq!(g(&r)[1], Value::Number(3.0));
    assert_eq!(g(&r)[3], rect(&[8.0, 48.0, 300.0, 40.0]));
}

#[test]
fn a_space_the_page_cannot_place_it_in_has_no_frame() {
    // In a node's border box the page cannot place it (a transformed node, a gone one): its
    // size without a frame; and the page's box goes when the element does
    drawn([120.0, 40.0], [Some([8.0, 48.0, 120.0, 40.0]), None, None]);
    let mut r = page(", \"node\", \"gone\"");
    ASKED.with(|a| a.borrow_mut().clear());
    assert!(r.refresh_geometry().unwrap().is_some());
    let placed = [
        Value::Bool(true),
        Value::Number(1.0),
        rect(&[120.0, 40.0]),
        Value::NONE,
    ];
    assert_eq!(g(&r)[..4], placed);
    assert_eq!(ASKED.with(|a| a.borrow()[0]), PageSpace::Node(None));
    DRAWN.with(|d| *d.borrow_mut() = None);
    assert!(r.refresh_geometry().unwrap().is_some());
    let gone = [
        Value::Bool(false),
        Value::Number(2.0),
        Value::NONE,
        Value::NONE,
    ];
    assert_eq!(g(&r)[..4], gone);
}

#[test]
fn a_fact_the_page_moves_after_each_answer_holds_at_the_bound() {
    // Each commit the facts make moves `hero` again: the chain stops after eight, its answer
    // held, and the page's next layout after a commit with another cause answers it again
    drawn([120.0, 40.0], [Some([0.0, 0.0, 120.0, 40.0]), None, None]);
    let mut r = page("");
    for width in 121..=128 {
        drawn(
            [width as f64, 40.0],
            [Some([0.0, 0.0, width as f64, 40.0]), None, None],
        );
        assert!(r.refresh_geometry().unwrap().is_some(), "at {width}");
    }
    drawn([129.0, 40.0], [Some([0.0, 0.0, 129.0, 40.0]), None, None]);
    assert!(r.refresh_geometry().unwrap().is_none(), "the ninth is held");
    assert_eq!(g(&r)[2], rect(&[128.0, 40.0]));
    r.act("grow", vec![]).unwrap();
    assert!(
        r.refresh_geometry().unwrap().is_some(),
        "another cause releases the hold"
    );
    assert_eq!(g(&r)[2], rect(&[129.0, 40.0]));
}
