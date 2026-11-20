//! The back/forward cache's `pagehide` and `pageshow` with `persisted`
//! (HTML Standard, "Session history": PageTransitionEvent), as page facts
//! (LLP 1069.000 D2; LLP 1069.010 D8): `exactPage().persisted` while the
//! page is put away, and `exactPage().restores` counting its returns. A
//! `when` keyed on the count is the page's `pageshow` handler: it runs once
//! for each return, never at launch, and not for a page only hidden (a
//! minimized or covered window).

use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Page, Runner};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const DRAFTS: &str = r#"shape Page
  visibilityState: string
  persisted: bool
  restores: number

component App
  resource page = exactPage() as shape Page
  state drafts = 0
  state shelved = 0
  action newDraft()
    drafts = drafts + 1
  action shelve()
    shelved = shelved + 1
  task fresh when page.restores > 0 key=page.restores
    after(1, newDraft)
  task away when page.persisted
    after(1, shelve)
  view
    text `${page.visibilityState} ${page.persisted} ${page.restores}` testId="facts"
"#;

fn boot(src: &str) -> Runner<NoData> {
    let plan = contract::compile(src).unwrap_or_else(|e| panic!("{e}"));
    Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

#[test]
fn each_runner_keeps_its_host_through_page_changes_and_nested_boots() {
    use exact_runner::page::Platform;
    let src = r#"shape Page
  platform: string
  onLine: bool
component App
  resource page = exactPage() as shape Page
  view
    text page.platform
"#;
    let (mut browser, terminal) = Platform::Web.boot(|| {
        let terminal = Platform::Terminal.boot(|| boot(src));
        (boot(src), terminal)
    });
    browser
        .set_page(Page {
            on_line: false,
            ..Page::default()
        })
        .unwrap();
    let answer = |r: &Runner<NoData>| r.resource("page").cloned().unwrap();
    assert_eq!(
        answer(&browser),
        Value::record(vec![Value::str("web"), Value::Bool(false)])
    );
    assert_eq!(
        answer(&terminal),
        Value::record(vec![Value::str("terminal"), Value::Bool(true)])
    );
    assert_eq!(
        answer(&boot(src)),
        Value::record(vec![Value::str("headless"), Value::Bool(true)])
    );
}

/// Moves the runner's clock `ms` on (`advance` takes the time itself).
fn wait(r: &mut Runner<NoData>, clock: &mut f64, ms: f64) {
    *clock += ms;
    r.advance(*clock).unwrap();
}

fn count(r: &Runner<NoData>, slot: &str) -> Value {
    r.slot(slot).cloned().unwrap()
}

#[test]
fn a_return_from_being_put_away_runs_its_task_once_and_a_minimize_none() {
    let mut r = boot(DRAFTS);
    let mut clock = 0.0;
    assert_eq!(
        r.resource("page").cloned(),
        Some(Value::record(vec![
            Value::str("visible"),
            Value::Bool(false),
            Value::Number(0.0),
        ]))
    );
    wait(&mut r, &mut clock, 10.0);
    assert_eq!(
        count(&r, "drafts"),
        Value::Number(0.0),
        "a launch is no return"
    );

    // Minimized, then restored: hidden and visible again, never put away.
    let shown = r.page();
    r.set_page(Page {
        hidden: true,
        ..shown
    })
    .unwrap();
    wait(&mut r, &mut clock, 10.0);
    r.set_page(shown).unwrap();
    wait(&mut r, &mut clock, 10.0);
    assert_eq!(
        count(&r, "drafts"),
        Value::Number(0.0),
        "a minimize is no return"
    );
    assert_eq!(count(&r, "shelved"), Value::Number(0.0));

    // Closed (pagehide, persisted), then reopened (pageshow, persisted).
    let away = Page {
        hidden: true,
        persisted: true,
        ..shown
    };
    r.set_page(away).unwrap();
    wait(&mut r, &mut clock, 10.0);
    assert_eq!(
        count(&r, "shelved"),
        Value::Number(1.0),
        "pagehide with persisted"
    );
    let back = Page {
        restores: 1,
        ..shown
    };
    r.set_page(back).unwrap();
    assert_eq!(
        count(&r, "drafts"),
        Value::Number(0.0),
        "the task's timer, not the commit"
    );
    wait(&mut r, &mut clock, 1.0);
    assert_eq!(
        count(&r, "drafts"),
        Value::Number(1.0),
        "pageshow with persisted"
    );
    wait(&mut r, &mut clock, 10.0);
    assert_eq!(
        count(&r, "drafts"),
        Value::Number(1.0),
        "once for each return"
    );

    // A minimize after the return runs nothing; a second close and reopen runs it again.
    r.set_page(Page {
        hidden: true,
        ..back
    })
    .unwrap();
    r.set_page(back).unwrap();
    wait(&mut r, &mut clock, 10.0);
    assert_eq!(count(&r, "drafts"), Value::Number(1.0));
    r.set_page(Page {
        hidden: true,
        persisted: true,
        ..back
    })
    .unwrap();
    wait(&mut r, &mut clock, 10.0);
    r.set_page(Page {
        restores: 2,
        ..back
    })
    .unwrap();
    wait(&mut r, &mut clock, 1.0);
    assert_eq!(count(&r, "drafts"), Value::Number(2.0));
    assert_eq!(count(&r, "shelved"), Value::Number(2.0));
}

#[test]
fn frozen_bakes_false_and_unknown_page_fields_refuse() {
    let source = "shape Page\n  frozen: bool\ncomponent App\n  resource page = exactPage() as shape Page\n  view\n    text `${page.frozen}`\n";
    let plan = contract::bake(contract::compile(source).unwrap(), NoData).unwrap();
    assert_eq!(
        Value::from_bytes(plan.bytes(plan.resources[0].initial)).unwrap(),
        Value::record(vec![Value::Bool(false)])
    );
    let error = contract::bake(
        contract::compile(&source.replace("frozen", "freezing")).unwrap(),
        NoData,
    )
    .unwrap_err();
    assert!(error.to_string().contains("bake-page-field"), "{error}");
}

#[test]
fn a_geometry_fact_bakes_and_refuses_a_shape_an_answer_cannot_fill() {
    // LLP 1123 D6: `exactGeometry`'s fields are filled by name, nested ones too, and a quantity
    // it may not have is `none`, so a field it lacks, at any depth, and a family that is not an
    // `option` are refused, each named
    let bake = |shapes: &str, hero: &str| {
        let source = format!("{shapes}shape Hero\n{hero}component App\n  resource g = exactGeometry(\"hero\") as shape Hero\n  view\n    column id=\"hero\"\n");
        contract::bake(contract::compile(&source).unwrap(), NoData)
    };
    let rect = "shape Rect\n  x: number\n  y: number\n  width: number\n  height: number\n";
    let insets = "shape Insets\n  top: number\n  right: number\n  bottom: number\n  left: number\n";
    assert!(bake(rect, "  available: bool\n  frame: option<Rect>\n").is_ok());
    for (shapes, hero, named) in [
        ("", "  shown: bool\n", "`shown`"),
        (
            "shape Rect\n  x: number\n  w: number\n",
            "  frame: option<Rect>\n",
            "`frame.w`",
        ),
        (
            &format!("{insets}shape Observed\n  safeArea: Insets\n"),
            "  observations: option<Observed>\n",
            "`observations.safeArea`",
        ),
    ] {
        let error = bake(shapes, hero).unwrap_err().to_string();
        assert!(error.contains("bake-geometry-field"), "{error}");
        assert!(error.contains(named), "{named}: {error}");
    }
}
