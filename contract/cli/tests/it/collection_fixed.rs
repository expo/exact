//! Fixed content around a virtualized list.

#[test]
fn vertical_collections_accept_fixed_content_and_name_unsupported_shapes() {
    let source = "component App\n  view\n    list virtualized=true appearance=\"none\" height=200\n      text \"Lead\"\n      for x in [1, 2] key=x\n        text `${x}`\n      text \"Trail\"\n";
    contract::compile(source).unwrap();
    for (source, id, fix) in [
        (source.replace("      text \"Trail\"", "      for y in [3] key=y\n        text `${y}`"), "lower-collection-template", "element"),
        (source.replace("height=200", "height=200 display=\"flex\""), "lower-collection-fixed", "vertical"),
        (source.replace("appearance=\"none\" ", ""), "lower-collection-fixed", "appearance=\"none\""),
        (source.replace("appearance=\"none\"", "appearance=\"auto\""), "lower-grouped-list", "appearance=\"none\""),
        (source.replace("      text \"Trail\"", "      column\n        list virtualized=true height=40\n          for y in [3] key=y\n            text `${y}`"), "lower-collection-fixed", "row"),
    ] {
        let error = contract::compile(&source).unwrap_err();
        assert_eq!(error.id, id, "{error}");
        assert!(error.message.contains(fix), "{error}");
    }
}

use crate::source::{boxed, Source};
use exact_kernel::{Kernel, Offer};
use exact_runner::{CollectionFeedback, DataError, DataSource, RowMeasurement, Runner, Value};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

fn boot(rows: &str, follow: bool) -> Runner<Source> {
    boot_start(rows, follow, "start")
}

fn boot_start(rows: &str, follow: bool, start: &str) -> Runner<Source> {
    let source = format!(
        r#"component App
  state rows: number[] = {rows}
  state ends = 0
  state first = none
  state within = 0
  action moved(x: number, y: number, e: ScrollEvent)
    first = e.firstVisibleKey
    within = e.firstVisibleOffset
  action edge
    ends = ends + 1
  action prepend
    rows = [0].concat(rows)
  view
    list virtualized=true appearance="none" height=200 width=300 estimated-item-height=100 scrollFollowEnd={follow} scroll-start="{start}" reachend=edge reach-margin="0px" scroll=moved testId="feed"
      text "Lead" height=300 testId="lead"
      for x in rows key=x
        text `${{x}}` height=100
      text "Trail" height=800 testId="trail"
"#
    );
    Runner::boot(
        contract::compile(&source).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn report(r: &mut Runner<Source>, at: f64, fixed: [f64; 2]) -> f64 {
    report_velocity(r, at, fixed, 0.)
}

fn report_velocity(r: &mut Runner<Source>, at: f64, fixed: [f64; 2], velocity: f64) -> f64 {
    let c = r.collections().remove(0);
    let measurements = c
        .rows
        .iter()
        .map(|row| RowMeasurement {
            view: row.view,
            epoch: row.epoch,
            size: 100.0,
        })
        .chain(
            c.fixed
                .iter()
                .enumerate()
                .map(|(side, item)| RowMeasurement {
                    view: item.view,
                    epoch: 1,
                    size: fixed[side],
                }),
        )
        .collect();
    r.collection_feedback_filled(
        CollectionFeedback {
            view: c.view,
            revision: c.revision,
            scroll_sequence: c.scroll_sequence + 1,
            offset: at,
            port_main: 200.0,
            port_cross: 300.0,
            cross: 300.0,
            padding: [0.0; 2],
            measurements,
            focus_view: None,
            interaction_view: None,
        },
        exact_runner::CollectionFill {
            velocity,
            ..Default::default()
        },
    )
    .unwrap();
    r.collections()[0].correction.map_or(at, |c| c.offset)
}

#[test]
fn fixed_content_is_measured_without_becoming_rows_or_padding() {
    let mut r = boot("[1, 2, 3]", true);
    for _ in 0..3 {
        report(&mut r, 0.0, [300.0, 800.0]);
    }
    let c = r.collections().remove(0);
    assert_eq!((c.count, c.total_extent), (3, 1400.0));
    assert_eq!(c.rows[0].start, 300.0);
    assert_eq!(c.fixed.len(), 2);
    let kernel = r.kernel_mut();
    kernel
        .compute_layout(c.view, Offer::definite(300.0, 200.0))
        .unwrap();
    assert_eq!(
        kernel.collection_fixed_sizes(c.view, &c.fixed[0].roots, &c.fixed[1].roots),
        Some([300.0, 800.0])
    );
    for root in c.fixed.iter().flat_map(|f| &f.roots) {
        assert!(kernel
            .node(*root)
            .unwrap()
            .props
            .str(exact_kernel::PropId::ListItemKey)
            .is_none());
    }
    report(&mut r, 1200.0, [300.0, 800.0]);
    assert_eq!(report(&mut r, 1200.0, [300.0, 900.0]), 1300.0);
    // A host that ignores fixed items is refused before it can mis-window rows.
    let c = r.collections().remove(0);
    assert!(r
        .collection_feedback(CollectionFeedback {
            view: c.view,
            revision: c.revision,
            scroll_sequence: c.scroll_sequence + 1,
            offset: 0.0,
            port_main: 200.0,
            port_cross: 300.0,
            cross: 300.0,
            padding: [0.0; 2],
            measurements: Vec::new(),
            focus_view: None,
            interaction_view: None,
        })
        .is_err());
}

#[test]
fn an_empty_for_keeps_both_fixed_sides() {
    let mut r = boot("[1].slice(0, 0)", false);
    report(&mut r, 0.0, [300.0, 800.0]);
    let c = r.collections().remove(0);
    assert_eq!((c.count, c.total_extent), (0, 1100.0));
    assert!(c.fixed.iter().all(|f| !f.roots.is_empty()));
}

#[test]
fn the_first_visible_key_names_data_below_a_partly_visible_lead() {
    let mut r = boot("[1, 2, 3]", false);
    report(&mut r, 0.0, [300.0, 800.0]);
    let view = r.collections()[0].view;
    for (top, key, within) in [
        (0.0, Value::NONE, 0.0),
        (150.0, Value::some(Value::str("1")), -150.0),
        (850.0, Value::NONE, 0.0),
    ] {
        r.dispatch(
            view,
            exact_runner::Event::Scroll(exact_runner::ScrollEvent {
                left: 0.0,
                top,
                width: 300.0,
                height: 1400.0,
                client_width: 300.0,
                client_height: 200.0,
                zoom: 1.0,
            }),
        )
        .unwrap();
        assert_eq!(r.slot("first"), Some(&key), "at {top}");
        assert_eq!(r.slot("within"), Some(&Value::Number(within)), "at {top}");
    }
}

#[test]
fn a_reader_inside_the_trail_keeps_their_place_when_history_grows() {
    let mut r = boot("[1, 2, 3]", false);
    for _ in 0..3 {
        report(&mut r, 0.0, [300.0, 800.0]);
    }
    report(&mut r, 850.0, [300.0, 800.0]);
    r.act("prepend", Vec::new()).unwrap();
    assert_eq!(r.collections()[0].correction.unwrap().offset, 950.0);
    // A lead has its own identity too: changes below it do not move its reader.
    report(&mut r, 150.0, [300.0, 800.0]);
    r.act("prepend", Vec::new()).unwrap();
    assert!(r.collections()[0]
        .correction
        .is_none_or(|c| c.offset == 150.0));
}

#[test]
fn nested_fixed_reading_positions_survive_outer_row_retirement() {
    let source = r#"component App
  view
    list virtualized=true appearance="none" height=200 width=300 estimated-item-height=200
      for n in range(100) key=n
        view height=200
          list virtualized=true appearance="none" height=200 width=300 estimated-item-height=100 scroll-start="end" testId=`inner-${n}`
            view height=300
            for x in [1, 2, 3] key=x
              view height=100
            view height=800
"#;
    fn feedback(r: &mut Runner<Source>, view: u32, offset: f64) {
        let c = r
            .collections()
            .into_iter()
            .find(|c| c.view == view)
            .unwrap();
        let measurements = c
            .rows
            .iter()
            .map(|row| RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: if c.parent.is_some() { 100. } else { 200. },
            })
            .chain(c.fixed.iter().enumerate().map(|(side, f)| RowMeasurement {
                view: f.view,
                epoch: 1,
                size: [300., 800.][side],
            }))
            .collect();
        r.collection_feedback(CollectionFeedback {
            view,
            revision: c.revision,
            scroll_sequence: c.scroll_sequence + 1,
            offset,
            port_main: 200.,
            port_cross: 300.,
            cross: 300.,
            padding: [0.; 2],
            measurements,
            focus_view: None,
            interaction_view: None,
        })
        .unwrap();
    }
    for at in [150., 850.] {
        let mut r = Runner::boot(
            contract::compile(source).unwrap(),
            boxed(NoData),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let outer = r.collections()[0].view;
        feedback(&mut r, outer, 0.);
        let inner = r
            .kernel()
            .node_by_key(r.kernel().find_by_test_id("inner-0")[0])
            .unwrap()
            .id;
        feedback(&mut r, inner, 0.);
        feedback(&mut r, inner, 1200.);
        feedback(&mut r, inner, 1200.);
        feedback(&mut r, inner, at);
        feedback(&mut r, outer, 10000.);
        assert!(r.kernel().find_by_test_id("inner-0").is_empty());
        feedback(&mut r, outer, 0.);
        let inner = r
            .kernel()
            .node_by_key(r.kernel().find_by_test_id("inner-0")[0])
            .unwrap()
            .id;
        let c = r
            .collections()
            .into_iter()
            .find(|c| c.view == inner)
            .unwrap();
        assert_eq!(c.correction.unwrap().offset, at, "restored at {at}");
        feedback(&mut r, inner, at);
        let c = r
            .collections()
            .into_iter()
            .find(|c| c.view == inner)
            .unwrap();
        assert!(c.correction.is_none_or(|c| c.offset == at));
    }
}

#[test]
fn hidden_fixed_margins_have_eager_scroll_extents_and_corrections() {
    let source = r#"component App
  view
    list virtualized=true appearance="none" estimated-item-height=40 height=50 width=200 scroll-start="end" scrollFollowEnd=true testId="list"
      view display="none" margin-bottom=100
      for n in [1] key=n
        view height=40
"#;
    let mut eager = Runner::boot(
        contract::compile(
            &source
                .replace("virtualized=true", "virtualized=false")
                .replace(" estimated-item-height=40", "")
                .replace(" scroll-start=\"end\" scrollFollowEnd=true", ""),
        )
        .unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let list = r.collections()[0].view;
    let eager_list = eager
        .kernel()
        .node_by_key(eager.kernel().find_by_test_id("list")[0])
        .unwrap()
        .id;
    eager
        .kernel_mut()
        .compute_layout(eager_list, Offer::definite(200., 50.))
        .unwrap();
    for _ in 0..3 {
        r.kernel_mut()
            .compute_layout(list, Offer::definite(200., 50.))
            .unwrap();
        let c = r.collections().remove(0);
        let fixed = r
            .kernel()
            .collection_fixed_sizes(list, &c.fixed[0].roots, &c.fixed[1].roots)
            .unwrap();
        let measurements = c
            .rows
            .iter()
            .map(|row| RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: 40.,
            })
            .chain(c.fixed.iter().enumerate().map(|(side, f)| RowMeasurement {
                view: f.view,
                epoch: 1,
                size: fixed[side],
            }))
            .collect();
        r.collection_feedback(CollectionFeedback {
            view: list,
            revision: c.revision,
            scroll_sequence: c.scroll_sequence + 1,
            offset: 0.,
            port_main: 50.,
            port_cross: 200.,
            cross: 200.,
            padding: [0.; 2],
            measurements,
            focus_view: None,
            interaction_view: None,
        })
        .unwrap();
    }
    let c = r.collections().remove(0);
    let eager_extent = eager.kernel().node(eager_list).unwrap().content.1;
    assert_eq!(
        (c.total_extent - 50.).max(0.),
        (f64::from(eager_extent) - 50.).max(0.)
    );
    assert!(c.correction.is_none_or(|c| c.offset == 0.));
    assert_eq!(
        c.fixed.iter().map(|f| f.size).collect::<Vec<_>>(),
        vec![0., 0.]
    );
}

#[test]
fn fixed_content_admission_reads_literal_class_rows() {
    let source = "style Plain\n  appearance=\"none\"\n\ncomponent App\n  view\n    list virtualized=true class=Plain height=200\n      text \"Lead\"\n      for x in [1, 2] key=x\n        text `${x}`\n      text \"Trail\"\n";
    contract::compile(source).unwrap();
}

#[test]
fn fixed_content_refuses_a_horizontal_class_before_boot() {
    let source = "style Horizontal\n  display=\"flex\"\n\ncomponent App\n  view\n    list virtualized=true appearance=\"none\" class=Horizontal height=200\n      text \"Lead\"\n      for x in [1, 2] key=x\n        text `${x}`\n      text \"Trail\"\n";
    let error = contract::compile(source).unwrap_err();
    assert_eq!(error.id, "lower-collection-fixed");
    assert!(error.message.contains("vertical"));
}

#[test]
fn bounded_snapshots_charge_fixed_roots_without_mounted_data() {
    let lead = "      view height=1\n".repeat(300);
    let source = format!("component App\n  view\n    list virtualized=true appearance=\"none\" height=200\n{lead}      for x in range(0) key=x\n        text `${{x}}`\n");
    let r = Runner::boot(
        contract::compile(&source).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let ordinary = r.collections();
    assert!(ordinary[0].rows.is_empty());
    assert_eq!(ordinary[0].fixed[0].roots.len(), 300);
    assert_eq!(
        r.collections_bounded(1, 0, 4096, 1026).unwrap_err(),
        "collection wire capacity"
    );
    assert_eq!(r.collections_bounded(1, 0, 4096, 8192).unwrap(), ordinary);
}

#[test]
fn fixed_roots_refuse_positions_and_overlapping_flow() {
    for margin in [
        "10",
        "\"10px\"",
        "\"1lh\"",
        "\"2ch\"",
        "\"1rem\"",
        "\"1em\"",
        "\"10%\"",
        "\"1px 2px\"",
    ] {
        let source = format!("component App\n  view\n    list virtualized=true appearance=\"none\" height=200\n      view margin={margin}\n      for n in [1] key=n\n        view height=40\n");
        contract::compile(&source).unwrap();
    }
    for offset in ["0", "\"0\"", "\"0px\"", "\"auto\""] {
        for side in ["top", "bottom"] {
            let source = format!("component App\n  view\n    list virtualized=true appearance=\"none\" height=200\n      view position=\"relative\" {side}={offset}\n      for n in [1] key=n\n        view height=40\n");
            contract::compile(&source).unwrap();
        }
    }
    for inset in [
        "0",
        "\"0px\"",
        "\"auto\"",
        "\"0px -10px\"",
        "\"auto 10px 0px\"",
    ] {
        let source = format!("component App\n  view\n    list virtualized=true appearance=\"none\" height=200\n      view position=\"relative\" inset={inset}\n      for n in [1] key=n\n        view height=40\n");
        contract::compile(&source).unwrap();
    }
    for attrs in [
        "position=\"sticky\"",
        "position=\"absolute\"",
        "margin-bottom=-400",
        "margin-top=-10",
        "margin=-10",
        "margin-bottom=\"-1lh\"",
        "margin=\"1px -2px\"",
        "position=\"relative\" top=-10",
        "position=\"relative\" inset=\"-100px 0px 0px\"",
        "position=\"relative\" inset=\"0px 0px -100px\"",
        "position=\"relative\" inset=offset",
        "position=\"relative\" inset=`0px 0px ${offset}px`",
    ] {
        for root in [
            format!("view {attrs}"),
            format!("view display=\"contents\"\n        view {attrs}"),
        ] {
            let source = format!("component App\n  state offset = -100\n  view\n    list virtualized=true appearance=\"none\" height=200\n      {root}\n      for n in range(100) key=n\n        view height=40\n");
            let error = contract::compile(&source).expect_err(&source);
            assert_eq!(error.id, "lower-collection-fixed-flow", "{error}");
            assert!(error.message.contains("fixed"), "{error}");
        }
        if attrs.contains("offset") {
            continue;
        }
        let source = format!("style Fixed\n  {attrs}\n\ncomponent App\n  state offset = -100\n  view\n    list virtualized=true appearance=\"none\" height=200\n      if true\n        view class=Fixed\n      for n in [1] key=n\n        view height=40\n");
        assert_eq!(
            contract::compile(&source).unwrap_err().id,
            "lower-collection-fixed-flow"
        );
    }
}

#[test]
fn initial_follow_uses_fixed_geometry_and_explicit_end_still_wins() {
    for (rows, count) in [("[1]", 1), ("[1].slice(0, 0)", 0)] {
        for start in ["start", "end"] {
            let mut r = boot_start(rows, true, start);
            let offset = report(&mut r, 0., [600., 800.]);
            assert_eq!(
                offset,
                if start == "end" {
                    1200. + 100. * count as f64
                } else {
                    0.
                },
                "{rows} {start}"
            );
        }
    }
}

#[test]
#[ignore = "async lane: compares kernel collection measurement with Chrome's web glue"]
fn fixed_measurements_match_the_web_glue() {
    use exact_kernel::{NodeType, Op, StyleId, StyleProps, StyleValue};
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Each column styles the list, lead, trail, and their respective children.
    let cases = [
        ("plain", ["", "", "", "", ""]),
        ("point margins", ["", "margin-top:7px;margin-bottom:11px", "margin-top:13px;margin-bottom:17px", "", ""]),
        ("percentage margins", ["", "margin-top:10%;margin-bottom:20%", "margin-top:25%;margin-bottom:50%", "", ""]),
        ("hidden roots", ["", "display:none;margin-bottom:100px", "display:none;margin-bottom:50%", "", ""]),
        ("hidden descendants", ["", "", "", "display:none;margin-bottom:100px", "display:none;margin-bottom:50%"]),
        ("out of flow descendants", ["", "", "", "position:absolute;top:500px", "position:absolute;top:900px"]),
        ("list border and padding", ["padding-top:9px;padding-right:15px;padding-bottom:12px;padding-left:25px;border-top-width:3px;border-top-style:solid;border-bottom-width:5px;border-bottom-style:solid", "", "margin-bottom:50%", "", ""]),
        ("fixed border and padding", ["", "padding-top:7px;padding-bottom:11px;border-top-width:2px;border-top-style:solid", "padding-top:13px;padding-bottom:17px;border-bottom-width:3px;border-bottom-style:solid;margin-bottom:10%", "", ""]),
        ("relative roots", ["", "position:relative;left:12px", "position:relative;right:8px;margin-bottom:50%", "", ""]),
        ("relative descendants", ["", "", "", "position:relative;top:10px", "position:relative;top:-10px"]),
    ];
    let mut inputs = Vec::new();
    for (name, extra) in cases {
        let css = [
            format!("width:200px;height:100px;overflow-y:hidden;{}", extra[0]),
            extra[1].to_string(),
            "height:60px".to_string(),
            extra[2].to_string(),
            format!("height:30px;{}", extra[3]),
            format!("height:40px;{}", extra[4]),
        ];
        let mut kernel = Kernel::with_monospace();
        let mut ops = Vec::new();
        for (i, css) in css.iter().enumerate() {
            let id = i as u32 + 1;
            ops.push(Op::CreateView {
                id,
                node_type: NodeType::View,
            });
            let mut patch = StyleProps::default();
            for declaration in css.split(';').filter(|s| !s.is_empty()) {
                let (property, value) = declaration.split_once(':').unwrap();
                let field = match property {
                    "position" => "position_type".into(),
                    p if p.starts_with("border-") => {
                        let parts: Vec<_> = p.split('-').collect();
                        format!("border_{}_{}", parts[2], parts[1])
                    }
                    p => p.replace('-', "_"),
                };
                let id = StyleId::from_name(&field).unwrap_or_else(|| panic!("{property}"));
                let value = if property.ends_with("-width") {
                    StyleValue::Number(value.trim_end_matches("px").parse().unwrap())
                } else {
                    StyleValue::Text(value.into())
                };
                patch.set_dynamic(id, &value).unwrap();
            }
            ops.push(Op::SetStyle {
                id,
                patch: Box::new(patch),
            });
        }
        for (id, children) in [(1, vec![2, 3, 4]), (2, vec![5]), (4, vec![6])] {
            ops.push(Op::SetChildren { id, children });
        }
        ops.push(Op::AttachRoot { id: 1 });
        kernel.apply(0, 1, &ops).unwrap();
        kernel
            .compute_layout(1, Offer::definite(200., 100.))
            .unwrap();
        let sizes = kernel.collection_fixed_sizes(1, &[2], &[4]).unwrap();
        if name == "percentage margins" {
            assert_eq!(
                sizes[0] + 60.0 + sizes[1],
                f64::from(kernel.node(1).unwrap().content.1)
            );
        }
        inputs.push(serde_json::json!({"name":name,"css":css,"kernel":sizes}));
    }
    let script = r#"
import { readFileSync } from 'node:fs';
import { cdpBrowser } from './scripts/agent-launch.mjs';
const cases=JSON.parse(readFileSync(0,'utf8'));
const server=Bun.serve({port:0,hostname:'127.0.0.1',fetch(req){
  return new Response(new URL(req.url).pathname.endsWith('.js') ? readFileSync('host/web/collection-glue.js') : '<!doctype html><main></main>',
    {headers:{'content-type':new URL(req.url).pathname.endsWith('.js')?'text/javascript':'text/html'}});
}});
const suite=await cdpBrowser();
try {
  const {sessionId}=await suite.page(), call=(method,params)=>suite.cdp.send(method,params,sessionId);
  await call('Page.navigate',{url:`http://127.0.0.1:${server.port}`});
  const result=await call('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{
    const {collectionController}=await import('/collection-glue.js');
    return ${JSON.stringify(cases)}.map(({name,css,kernel})=>{
      const root=document.querySelector('main'),els=css.map(style=>{const el=document.createElement('div');el.style.cssText=style;return el;});
      const [list,lead,row,trail,a,b]=els;root.append(list);list.append(lead,row,trail);lead.append(a);trail.append(b);
      const frames=[],views=new Map(els.map((el,i)=>[i+1,el]));let measured;
      const controller=collectionController({root,views,report(bytes){const d=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),n=d.getUint32(96,true);measured=[d.getFloat64(112+(n-2)*20,true),d.getFloat64(112+(n-1)*20,true)];},requestFrame(f){frames.push(f);return frames.length;},cancelFrame(){}});
      try {
        controller.commit([{view:1,revision:'1',scrollSequence:'0',count:1,totalExtent:60,rows:[{view:3,root:3,index:0,start:0,size:60,epoch:'1'}],fixed:[{view:7,roots:[2],size:0,epoch:'1'},{view:8,roots:[4],size:0,epoch:'1'}]}]);
        for(const f of frames.splice(0))f();
        return {name,kernel,web:measured};
      } finally {controller.dispose();list.remove();}
    });
  })()`});
  if(result.exceptionDetails)throw Error(result.exceptionDetails.exception?.description??result.exceptionDetails.text);
  console.log(JSON.stringify(result.result.value));
} finally {await suite.close();server.stop(true);}
"#;
    let mut child = Command::new(std::env::var("BUN").unwrap_or_else(|_| "bun".into()))
        .args(["--eval", script])
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&inputs).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let differences: Vec<_> = results.iter().filter(|r| r["kernel"] != r["web"]).collect();
    assert!(differences.is_empty(), "{differences:#?}");
    assert_eq!(results.len(), cases.len());
}

#[test]
fn the_trail_waits_for_the_data_window_but_full_copy_keeps_document_order() {
    let mut r = boot("[1, 2, 3]", false);
    report(&mut r, 0.0, [300.0, 800.0]);
    let c = r.collections().remove(0);
    assert!(c.fixed[1].hidden);
    let doc = r.document_tree().unwrap();
    for root in &c.fixed[1].roots {
        assert_eq!(
            doc.node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::Inert),
            if c.fixed[1].hidden { Some(true) } else { None }
        );
        assert_eq!(
            doc.node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::AccessibilityElementsHidden),
            if c.fixed[1].hidden { Some(true) } else { None }
        );

        assert_eq!(
            r.kernel()
                .node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::Inert),
            if c.fixed[1].hidden { Some(true) } else { None }
        );
        assert_eq!(
            r.kernel()
                .node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::AccessibilityElementsHidden),
            Some(true)
        );
    }
    assert_eq!(
        r.list_text(c.view, None, exact_runner::ListTextPolicy::Web)
            .unwrap(),
        "Lead\n\n1\n\n2\n\n3\n\nTrail"
    );
    report(&mut r, 1200.0, [300.0, 800.0]);
    let c = r.collections().remove(0);
    assert!(!c.fixed[1].hidden);
    let doc = r.document_tree().unwrap();
    for root in &c.fixed[1].roots {
        assert_eq!(
            doc.node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::Inert),
            if c.fixed[1].hidden { Some(true) } else { None }
        );
        assert_eq!(
            doc.node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::AccessibilityElementsHidden),
            if c.fixed[1].hidden { Some(true) } else { None }
        );

        assert_eq!(
            r.kernel()
                .node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::Inert),
            if c.fixed[1].hidden { Some(true) } else { None }
        );
        assert_eq!(
            r.kernel()
                .node(*root)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::AccessibilityElementsHidden),
            None
        );
    }
}
#[test]
fn a_fling_does_not_expose_the_trail_before_the_geometric_window_reaches_it() {
    let mut r = boot("[1, 2, 3]", false);
    report(&mut r, 0., [300., 800.]);
    report_velocity(&mut r, 0., [300., 800.], 4000.);
    assert!(r.collections()[0].fixed[1].hidden);
    report(&mut r, 101., [300., 800.]);
    assert!(!r.collections()[0].fixed[1].hidden);
}

#[test]
fn the_trail_restores_authored_inertness_including_updates_while_gated() {
    let source = r#"component App
  state locked = true
  action toggle
    locked = !locked
  view
    list virtualized=true appearance="none" height=200 estimated-item-height=100
      button "Toggle" press=toggle testId="toggle"
      for n in range(20) key=n
        text `${n}` height=100
      column inert=locked testId="trail"
        button "End"
"#;
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let id = |r: &Runner<Source>, name| {
        r.kernel()
            .node_by_key(r.kernel().find_by_test_id(name)[0])
            .unwrap()
            .id
    };
    let trail = id(&r, "trail");
    let toggle = id(&r, "toggle");
    let inert = |r: &Runner<Source>| {
        r.kernel()
            .node(trail)
            .unwrap()
            .props
            .bool(exact_kernel::PropId::Inert)
    };
    for authored in [true, false, true] {
        report(&mut r, 0., [300., 800.]);
        assert_eq!(inert(&r), Some(true));
        assert_eq!(
            r.document_tree()
                .unwrap()
                .node(trail)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::Inert),
            Some(true)
        );
        report(&mut r, 2400., [300., 800.]);
        assert!(!r.collections()[0].fixed[1].hidden);
        assert_eq!(inert(&r), Some(authored));
        assert_eq!(
            r.document_tree()
                .unwrap()
                .node(trail)
                .unwrap()
                .props
                .bool(exact_kernel::PropId::Inert),
            Some(authored)
        );
        report(&mut r, 0., [300., 800.]);
        r.dispatch(toggle, exact_runner::Event::Press).unwrap();
        assert_eq!(inert(&r), Some(true));
    }
}

#[test]
fn events_in_fixed_content_reach_their_authored_scope() {
    let source = r#"component App
  state presses = 0
  action press
    presses = presses + 1
  view
    list virtualized=true appearance="none" height=200
      button "Lead" press=press testId="lead"
      for x in [1, 2] key=x
        text `${x}` height=40
      column
        for n in [1, 2] key=n
          button "Trail" press=press testId=`trail-${n}`
      text `${presses}` font-variant-numeric="tabular-nums" testId="presses"
"#;
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for id in ["lead", "trail-2"] {
        let view = r
            .kernel()
            .node_by_key(r.kernel().find_by_test_id(id)[0])
            .unwrap()
            .id;
        r.dispatch(view, exact_runner::Event::Press).unwrap();
    }
    let text = r
        .kernel()
        .node_by_key(r.kernel().find_by_test_id("presses")[0])
        .unwrap();
    assert_eq!(text.props.str(exact_kernel::PropId::Text), Some("2"));
}

#[test]
fn reachend_tracks_the_trail_end_and_rearms_while_reading_fixed_content() {
    let mut r = boot("[1, 2, 3]", false);
    for _ in 0..3 {
        report(&mut r, 0.0, [300.0, 800.0]);
    }
    assert_eq!(r.slot("ends").unwrap().as_number(), Some(0.0));
    report(&mut r, 1200.0, [300.0, 800.0]);
    assert_eq!(r.slot("ends").unwrap().as_number(), Some(1.0));
    report(&mut r, 850.0, [300.0, 800.0]);
    report(&mut r, 1200.0, [300.0, 800.0]);
    assert_eq!(r.slot("ends").unwrap().as_number(), Some(2.0));
}

#[test]
fn an_eager_for_inside_fixed_content_still_reports_the_long_list_notice() {
    let source = r#"component App
  state n = 1
  action grow
    n = 200
  view
    list virtualized=true appearance="none" height=200
      for x in [1, 2] key=x
        text `${x}`
      column
        for n in range(n) key=n
          text `${n}`
"#;
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.measure(true, false);
    r.act("grow", Vec::new()).unwrap();
    assert_eq!(
        r.journal()
            .filter(|line| line.contains("long-list:") && line.contains("repeats 200 rows"))
            .count(),
        1
    );
}

#[test]
fn logical_ranges_retain_fixed_endpoints_after_data_rows_retire() {
    use exact_runner::{ListTextPolicy, ListTextPosition};
    let mut r = boot("[1, 2, 3]", false);
    let view = r.collections()[0].view;
    let at = |fixed, key, offset| ListTextPosition {
        fixed,
        key,
        paragraph: 0,
        offset,
    };
    for offset in [0., 1200.] {
        report(&mut r, offset, [300., 800.]);
        for (a, b, expected) in [
            (
                at(Some(false), "", 2),
                at(Some(true), "", 4),
                "ad\n\n1\n\n2\n\n3\n\nTrai",
            ),
            (at(None, "n:2", 0), at(Some(true), "", 5), "2\n\n3\n\nTrail"),
            (at(Some(false), "", 0), at(None, "n:2", 1), "Lead\n\n1\n\n2"),
            (at(Some(true), "", 1), at(Some(true), "", 4), "rai"),
        ] {
            assert_eq!(
                r.list_text(view, Some((a, b)), ListTextPolicy::MacOS)
                    .unwrap(),
                expected
            );
            assert_eq!(
                r.list_text(view, Some((b, a)), ListTextPolicy::MacOS)
                    .unwrap(),
                expected
            );
        }
    }
}

#[test]
fn logical_copy_excludes_a_nested_collections_fixed_content() {
    let source = r#"component App
  view
    list virtualized=true appearance="none" height=200 estimated-item-height=80 testId="outer"
      text "Header"
      for n in range(3) key=n
        column
          text `Before ${n}`
          list virtualized=true appearance="none" height=40 estimated-item-height=40
            text "Inner header"
            for m in range(3) key=m
              text "Inner row" height=40
            text "Inner footer"
          text `After ${n}`
      text "Footer"
"#;
    let r = Runner::boot(
        contract::compile(source).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let outer = r
        .kernel()
        .node_by_key(r.kernel().find_by_test_id("outer")[0])
        .unwrap()
        .id;
    for policy in [
        exact_runner::ListTextPolicy::Web,
        exact_runner::ListTextPolicy::MacOS,
    ] {
        assert_eq!(
            r.list_text(outer, None, policy).unwrap(),
            "Header\n\nBefore 0\n\nAfter 0\n\nBefore 1\n\nAfter 1\n\nBefore 2\n\nAfter 2\n\nFooter"
        );
    }
}
