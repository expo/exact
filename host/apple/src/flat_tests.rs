use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_runner::{DataError, Value};
use serde_json::Value as Json;

struct FixtureData;
impl DataSource for FixtureData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        if name == "rows" {
            return Ok(Value::list(
                (0..1000).map(|i| Value::Number(i as f64)).collect(),
            ));
        }
        Err(DataError::UnknownSource(name.into()))
    }
}

fn boot(source: &str, flat: bool) -> (Host<FixtureData>, Json) {
    boot_measured(source, flat, Box::new(MonospaceMeasurer::default()))
}

fn boot_measured(
    source: &str,
    flat: bool,
    measurer: Box<dyn exact_kernel::TextMeasurer>,
) -> (Host<FixtureData>, Json) {
    let plan = contract::compile(source).unwrap().encode();
    let (mut host, batch) = Host::boot_stored_after_decode(
        PlanBytes::Copied(&plan),
        FixtureData,
        measurer,
        LaunchLayout {
            flat_nodes: flat,
            ..LaunchLayout::NONE
        },
        exact_runner::Viewport::sized(400., 600.),
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        "/",
        false,
        None,
        crate::link::Links::ALL,
        None,
        |runner| {
            let font = exact_kernel::ControlFont {
                family: "UIKit body".into(),
                family_id: 42,
                size: 17.,
                weight: 400,
                style: exact_kernel::FontStyle::Normal,
            };
            let mut env = runner.kernel().env();
            env.control_text_styles = Some(exact_kernel::ControlTextStyles {
                field: font.clone(),
                textarea: font.clone(),
                button: font,
            });
            runner.kernel_mut().set_env(env).unwrap();
            Ok(())
        },
    )
    .unwrap();
    host.commit_boot();
    (host, serde_json::from_str(&batch).unwrap())
}

fn creates(batch: &Json) -> usize {
    batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| op["op"] == "create")
        .count()
}

fn geometry(batch: &Json) -> BTreeMap<ViewId, [f64; 4]> {
    let ops = batch["ops"].as_array().unwrap();
    let parents: BTreeMap<_, _> = ops
        .iter()
        .filter(|op| op["op"] == "children")
        .flat_map(|op| {
            op["ids"].as_array().unwrap().iter().map(move |c| {
                (
                    c.as_u64().unwrap() as u32,
                    op["id"].as_u64().unwrap() as u32,
                )
            })
        })
        .collect();
    let frames: BTreeMap<_, _> = ops
        .iter()
        .filter(|op| op["op"] == "frame")
        .map(|op| {
            (
                op["id"].as_u64().unwrap() as u32,
                ["x", "y", "w", "h"].map(|k| op[k].as_f64().unwrap()),
            )
        })
        .collect();
    frames
        .iter()
        .map(|(&id, &f)| {
            let mut f = f;
            let mut parent = parents.get(&id);
            while let Some(p) = parent {
                let pf = frames[p];
                f[0] += pf[0];
                f[1] += pf[1];
                parent = parents.get(p);
            }
            (id, f)
        })
        .collect()
}

const SHOP: &str = r#"component App
  state presses: number = 0
  resource rows = rows() as shape number[]
  view
    column height=600 width=400 testId="root"
      list virtualized=true appearance="none" testId="list" height=600 width=400 estimated-item-height=240
        for n in rows key=n
          column
            row gap=12 padding-left=16 padding-right=16 padding-bottom=20 align-items="flex-start"
              for p in [0, 1] key=p
                column flex=1 min-width=0
                  button press=go appearance="none" width="100%" display="flex" flex-direction="column" gap=8 color="blue"
                    image "app:/photo.png" width="100%" height=80
                    text "A product" font-size=14
                    row gap=6 align-items="baseline"
                      text "$19" font-weight=700
                      text "$29" color="gray"
  action go
    presses = presses + 1
"#;

#[test]
fn a_shop_row_omits_five_boxes_and_keeps_window_frames() {
    let (full, before) = boot(SHOP, false);
    let (flat, after) = boot(SHOP, true);
    let list = full.runner.collections()[0].clone();
    let rows = list.rows.len();
    assert!(rows > 0);
    assert_eq!(creates(&before) - creates(&after), rows * 5);
    eprintln!(
        "shop rows={rows}: creates {} -> {}, per row 17 -> 12",
        creates(&before),
        creates(&after)
    );
    let before = geometry(&before);
    for (view, frame) in geometry(&after) {
        assert_eq!(frame, before[&view], "view {view}");
    }
    for row in &list.rows {
        assert!(flat.mirror.contains_key(&row.view));
        assert!(flat.mirror.contains_key(&row.root));
    }
    for view in &flat.flat.nodes {
        let node: Json =
            serde_json::from_str(&flat.agent(&format!(r#"{{"op":"node","id":{view}}}"#))).unwrap();
        assert!(node["layoutOnly"]["parent"].is_number());
    }
    assert_eq!(
        flat.runner.kernel().export(None).unwrap(),
        full.runner.kernel().export(None).unwrap()
    );
}

fn apply(host: &mut Host<FixtureData>, ops: &[exact_kernel::Op]) -> Json {
    let receipt = host.runner.kernel_mut().apply(0, 100, ops).unwrap();
    serde_json::from_str(&host.commit(&[Timed { at_ms: 0., receipt }], None)).unwrap()
}

fn wrapper(host: &Host<FixtureData>) -> ViewId {
    let row = &host.runner.collections()[0].rows[0];
    host.runner.kernel().node(row.root).unwrap().children()[0]
}

#[test]
fn presentation_promotes_in_one_batch_and_never_demotes() {
    use exact_kernel::{Op, StyleProps, StyleValue};
    for property in ["paint", "id", "testId", "look", "animation"] {
        let (mut host, _) = boot(SHOP, true);
        let wrapper = wrapper(&host);
        let old_flat = host.flat.nodes.clone();
        assert!(old_flat.contains(&wrapper));
        let mut paint = StyleProps::default();
        paint
            .set_dynamic(StyleId::BackgroundColor, &StyleValue::Text("red".into()))
            .unwrap();
        let target = if property == "animation" {
            // A change elsewhere in the row promotes every omitted wrapper.
            host.runner.kernel().node(wrapper).unwrap().children()[0]
        } else {
            wrapper
        };
        let op = match property {
            "id" | "testId" => Op::SetProp {
                id: target,
                prop: if property == "id" {
                    PropId::Id
                } else {
                    PropId::TestId
                },
                value: PropValue::Str("addressable".into()),
            },
            "look" => Op::SetLooks {
                id: target,
                looks: vec![(exact_kernel::LOOK_HOVER, paint.clone())],
            },
            "animation" => {
                let animated = format!(
                    "keyframes fade\n  from opacity=1\n  to opacity=0.5\n{}",
                    SHOP.replacen("row gap=12", "row animation=\"fade 1s linear\" gap=12", 1)
                );
                let (source, _) = boot(&animated, false);
                let mut style = source
                    .runner
                    .kernel()
                    .node(
                        source
                            .runner
                            .kernel()
                            .node(source.runner.collections()[0].rows[0].root)
                            .unwrap()
                            .children()[0],
                    )
                    .unwrap()
                    .style
                    .clone();
                style.mask = exact_kernel::StyleMask::of(StyleId::Animation);
                style.written = style.mask;
                Op::SetStyle {
                    id: target,
                    patch: Box::new(style),
                }
            }
            _ => Op::SetStyle {
                id: target,
                patch: Box::new(paint),
            },
        };
        let batch = apply(&mut host, &[op]);
        assert!(!host.flat.nodes.contains(&wrapper), "{property}");
        let ops = batch["ops"].as_array().unwrap();
        let create = ops
            .iter()
            .position(|op| op["op"] == "create" && op["id"] == wrapper)
            .unwrap();
        let attached = ops
            .iter()
            .position(|op| {
                op["op"] == "children"
                    && op["ids"].as_array().unwrap().contains(&Json::from(wrapper))
            })
            .unwrap();
        assert!(create < attached, "{property}: {batch}");
        for id in old_flat.difference(&host.flat.nodes) {
            let parent = host.presented_parent(*id).unwrap();
            assert!(host.mirror[&parent].children.contains(id));
            assert!(host.mirror.contains_key(id));
        }
        if matches!(property, "look" | "animation") {
            let root = host.runner.collections()[0].rows[0].root;
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                assert!(
                    !host.flat.nodes.contains(&id),
                    "{property}: row must be mirrored"
                );
                stack.extend(host.runner.kernel().node(id).unwrap().children());
            }
        }
        let clear = match property {
            "id" | "testId" => Op::ClearProp {
                id: target,
                prop: if property == "id" {
                    PropId::Id
                } else {
                    PropId::TestId
                },
            },
            "look" => Op::SetLooks {
                id: target,
                looks: vec![],
            },
            _ => Op::ClearStyle {
                id: target,
                mask: exact_kernel::StyleMask::of(if property == "animation" {
                    StyleId::Animation
                } else {
                    StyleId::BackgroundColor
                }),
                written: false,
            },
        };
        let batch = apply(&mut host, &[clear]);
        assert!(host.mirror.contains_key(&wrapper));
        assert!(!host.flat.nodes.contains(&wrapper));
        assert!(!batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["op"] == "destroy" && op["id"] == wrapper));
    }
}

#[test]
fn unsafe_content_in_an_inactive_arm_keeps_the_whole_row_mirrored() {
    for content in [
        "text \"moving\" transition=\"opacity 1s\"",
        "scroll height=20\n                  text \"scroll\"",
        "list virtualized=false\n                  text \"nested\"",
        "column -exact-layout-transition=\"200ms ease\"\n                  text \"move\"",
        "column\n                  :hover\n                    color=\"red\"\n                  text \"hover\"",
        "text \"still\" animation=\"none\"",
        "dialog id=\"inline-dialog\"\n                  text \"dialog\"",
        "column role=\"alertdialog\"\n                  text \"dialog\"",
        "input type=\"checkbox\" checked=false",
        "input value=\"draft\"",
        "row role=\"tablist\"\n                  button role=\"tab\"\n                    text \"tab\"",
        "column navigationKey=\"screen\"\n                  text \"route\"",
        "column overflow-x=\"hidden\"\n                  text \"computed scroller\"",
        "view column-count=2\n                  column\n                    text \"columns\"",
        "view column-width=100\n                  column\n                    text \"columns\"",
        "svg width=20 height=20\n                  path d=\"M0 0 L10 10\"",
    ] {
        let source = SHOP.replacen(
            "            row gap=12",
            &format!("            if n < 0\n              {content}\n            row gap=12"),
            1,
        );
        let (host, _) = boot(&source, true);
        assert!(host.flat.nodes.is_empty(), "{content}");
    }
    for content in [
        "progress aria-label=\"Loading\"",
        "text \"clipped title\" overflow=\"hidden\"",
    ] {
        let source = SHOP.replacen(
            "            row gap=12",
            &format!("            if n < 0\n              {content}\n            row gap=12"),
            1,
        );
        let (host, _) = boot(&source, true);
        assert!(host.flat.nodes.contains(&wrapper(&host)), "{content}");
    }
}

const CONTACT: &str = r#"component App
  state presses: number = 0
  action go
    presses = presses + 1
  view
    list virtualized=true appearance="none" height=600 estimated-item-height=120
      for n in [0] key=n
        column
          view testId="earlier" opacity=1 height=10
          column
            column
              button testId="held" press=go position="relative" appearance="none" height=40
                text "Hold"
"#;

#[test]
fn promotion_preserves_only_unchanged_logical_paths() {
    use exact_kernel::{Op, StyleProps, StyleValue};
    for reparent in [false, true] {
        let (mut host, _) = boot(CONTACT, true);
        let root = host.runner.collections()[0].rows[0].root;
        let children = host.runner.kernel().node(root).unwrap().children();
        let outer = children[1];
        let inner = host.runner.kernel().node(outer).unwrap().children()[0];
        let held = host.runner.kernel().node(inner).unwrap().children()[0];
        assert!(host.flat.nodes.contains(&outer));
        assert!(host.flat.nodes.contains(&inner));
        let mut patch = StyleProps::default();
        patch
            .set_dynamic(StyleId::Opacity, &StyleValue::Number(0.5))
            .unwrap();
        let mut ops = vec![Op::SetStyle {
            id: children[0],
            patch: Box::new(patch),
        }];
        if reparent {
            ops.push(Op::SetChildren {
                id: outer,
                children: vec![inner, held],
            });
        }
        let batch = apply(&mut host, &ops);
        assert!(!host.flat.nodes.contains(&outer), "{batch}");
        let promotions: Vec<_> = batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|op| op["op"] == "layoutPromote")
            .collect();
        assert_eq!(promotions[0]["id"], outer);
        let keeps = promotions
            .iter()
            .any(|op| op["keep"].as_array().unwrap().contains(&Json::from(held)));
        assert_eq!(keeps, !reparent, "{batch}");
        assert!(promotions.iter().any(|op| op["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pair| pair[0] == held)));
    }
}

const COLUMNS: &str = r#"component App
  view
    list virtualized=true appearance="none" height=600 estimated-item-height=120
      for n in [0] key=n
        view height=100 width=300 column-count=2 column-gap=20 column-fill="auto"
          view
            view height=60
            view height=60
            view height=60
"#;

#[test]
fn fragmenting_rows_keep_their_boxes_and_promote_before_fragment_publication() {
    use exact_kernel::{Op, StyleMask, StyleProps, StyleValue};
    let (full, before) = boot(COLUMNS, false);
    let (flat, after) = boot(COLUMNS, true);
    assert!(flat.flat.nodes.is_empty());
    assert_eq!(creates(&before), creates(&after));
    assert_eq!(geometry(&before), geometry(&after));
    assert_eq!(
        full.runner.kernel().fragmented().len(),
        flat.runner.kernel().fragmented().len()
    );
    let fragmented = wrapper(&flat);
    assert!(
        flat.runner
            .kernel()
            .fragments(flat.runner.kernel().node(fragmented).unwrap().key)
            .unwrap()
            .len()
            > 1
    );
    assert!(after["ops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|op| op["op"] == "fragments" && op["id"] == fragmented));

    let quiet = COLUMNS.replace(" column-count=2", "");
    let (mut host, _) = boot(&quiet, true);
    let wrapper = wrapper(&host);
    let root = host.runner.collections()[0].rows[0].root;
    assert!(host.flat.nodes.contains(&wrapper));
    let mut patch = StyleProps::default();
    patch
        .set_dynamic(StyleId::ColumnCount, &StyleValue::Number(2.))
        .unwrap();
    let batch = apply(
        &mut host,
        &[Op::SetStyle {
            id: root,
            patch: Box::new(patch),
        }],
    );
    assert!(host.flat.nodes.is_empty());
    let ops = batch["ops"].as_array().unwrap();
    let promotion = ops
        .iter()
        .position(|op| op["op"] == "layoutPromote" && op["id"] == wrapper)
        .unwrap();
    let fragments = ops
        .iter()
        .position(|op| op["op"] == "fragments" && op["id"] == wrapper)
        .unwrap();
    assert!(promotion < fragments);
    apply(
        &mut host,
        &[Op::ClearStyle {
            id: root,
            mask: StyleMask::of(StyleId::ColumnCount),
            written: false,
        }],
    );
    assert!(host.flat.nodes.is_empty(), "a promoted box stays mirrored");
}

#[test]
fn authored_nonlayout_rows_and_handlers_keep_their_box() {
    for attribute in [
        "color=\"blue\"",
        "color=\"inherit\"",
        "width=\"unset\"",
        "opacity=1",
        "overflow=\"hidden\"",
        "id=\"named\"",
        "testId=\"named\"",
        "aria-label=\"named\"",
        "press=go",
        "display=\"grid\"",
        "draggable=true dataTransfer=DataTransfer(text=\"a\")",
    ] {
        let source = SHOP.replacen("row gap=12", &format!("row {attribute} gap=12"), 1);
        let (host, _) = boot(&source, true);
        assert!(host.mirror.contains_key(&wrapper(&host)), "{attribute}");
    }
    // Colour inherited through a layout-only box is not its declaration.
    let (host, _) = boot(SHOP, true);
    assert!(host.flat.nodes.contains(&wrapper(&host)));
}

#[test]
fn only_appearance_none_collection_descendants_can_be_omitted() {
    let (host, _) = boot("component App\n  view\n    column testId=\"root\"\n      row\n        column\n          text \"hello\"\n", true);
    assert!(host.flat.nodes.is_empty());
    let (mut host, _) = boot(SHOP, true);
    let list = host.runner.collections()[0].view;
    let mut style = exact_kernel::StyleProps::default();
    style
        .set_dynamic(
            StyleId::Appearance,
            &exact_kernel::StyleValue::Text("auto".into()),
        )
        .unwrap();
    apply(
        &mut host,
        &[exact_kernel::Op::SetStyle {
            id: list,
            patch: Box::new(style),
        }],
    );
    assert!(host.flat.nodes.is_empty());

    let (mut host, _) = boot(SHOP, true);
    let list = host.runner.collections()[0].view;
    apply(
        &mut host,
        &[exact_kernel::Op::SetProp {
            id: list,
            prop: PropId::ListStyle,
            value: PropValue::Str("plain".into()),
        }],
    );
    assert!(host.flat.nodes.is_empty(), "grouped lists own their rows");

    for source in [
        SHOP.replace(
            "column height=600 width=400 testId=\"root\"",
            "column popover=\"auto\" height=600 width=400 testId=\"root\"",
        ),
        SHOP.replace(
            "column height=600 width=400 testId=\"root\"",
            "dialog id=\"dialog\" height=600 width=400 testId=\"root\"",
        ),
    ] {
        let (host, _) = boot(&source, true);
        assert!(host.flat.nodes.is_empty());
    }
}

#[test]
fn a_layout_change_keeps_the_wrapper_omitted_and_updates_its_children() {
    use exact_kernel::{Op, StyleProps, StyleValue};
    let (mut full, _) = boot(SHOP, false);
    let (mut flat, _) = boot(SHOP, true);
    let id = wrapper(&flat);
    let children = flat.flat_children(flat.runner.kernel().node(id).unwrap().children());
    let mut style = StyleProps::default();
    style
        .set_dynamic(StyleId::PaddingTop, &StyleValue::Number(17.25))
        .unwrap();
    let ops = [Op::SetStyle {
        id,
        patch: Box::new(style),
    }];
    apply(&mut full, &ops);
    let batch = apply(&mut flat, &ops);
    assert_eq!(creates(&batch), 0);
    assert!(flat.flat.nodes.contains(&id));
    for child in children {
        let frame = flat.mirror[&child].frame.unwrap();
        let node = flat.runner.kernel().node(child).unwrap();
        let parent = flat
            .runner
            .kernel()
            .node(flat.presented_parent(child).unwrap())
            .unwrap();
        assert_eq!(frame, relative(node.frame, Some(parent.frame)));
    }
    assert_eq!(
        flat.runner.kernel().export(None).unwrap(),
        full.runner.kernel().export(None).unwrap()
    );
}

const OVERLAP: &str = r#"component App
  view
    list virtualized=true appearance="none" width=200 height=200 estimated-item-height=100
      for n in [0] key=n
        row width=200 height=100
          column width=80 height=80
            image "app:/first.png" testId="first" width=80 height=80 opacity=1
          column width=80 height=80 margin-left=-40
            image "app:/last.png" testId="last" width=80 height=80
"#;

#[test]
fn a_ranked_descendant_keeps_the_later_sibling_above_it() {
    use exact_kernel::{Op, StyleProps, StyleValue};
    fn check(full: &Host<FixtureData>, flat: &Host<FixtureData>) {
        let ranks: BTreeMap<_, _> = flat.runner.kernel().paint_order().into_iter().collect();
        let root = flat.runner.collections()[0].rows[0].root;
        let siblings = flat.runner.kernel().node(root).unwrap().children();
        let first = flat.runner.kernel().node(siblings[0]).unwrap().children()[0];
        let last = flat.runner.kernel().node(siblings[1]).unwrap().children()[0];
        let top = |host: &Host<FixtureData>| {
            let mut children = host.flat_children(siblings.clone());
            children.sort_by_key(|id| ranks[id].rank);
            let mut id = *children.last().unwrap();
            while let Some(child) = host.runner.kernel().node(id).unwrap().children().last() {
                id = *child;
            }
            id
        };
        assert_eq!(top(full), last);
        assert_eq!(top(flat), last);
        if ranks[&first].rank != 0 {
            assert!(flat.flat.nodes.contains(&siblings[0]));
            assert!(ranks[&siblings[1]].isolated);
            assert_eq!(ranks[&siblings[1]].rank, ranks[&first].rank);
            assert!(flat.mirror.contains_key(&siblings[1]));
        }
    }
    for opacity in ["1", "0.4"] {
        let source = OVERLAP.replace("opacity=1", &format!("opacity={opacity}"));
        let (mut full, _) = boot(&source, false);
        let (mut flat, _) = boot(&source, true);
        check(&full, &flat);
        if opacity == "1" {
            let first = flat
                .runner
                .kernel()
                .node_by_key(flat.runner.kernel().find_first_by_test_id("first").unwrap())
                .unwrap()
                .id;
            let mut patch = StyleProps::default();
            patch
                .set_dynamic(StyleId::Opacity, &StyleValue::Number(0.4))
                .unwrap();
            let ops = [Op::SetStyle {
                id: first,
                patch: Box::new(patch),
            }];
            apply(&mut full, &ops);
            let batch = apply(&mut flat, &ops);
            check(&full, &flat);
            let siblings = flat
                .runner
                .kernel()
                .node(flat.runner.collections()[0].rows[0].root)
                .unwrap()
                .children();
            let ops = batch["ops"].as_array().unwrap();
            let promotion = ops
                .iter()
                .position(|op| op["op"] == "layoutPromote" && op["id"] == siblings[1])
                .unwrap();
            let restyle = ops
                .iter()
                .position(|op| {
                    op["op"] == "present" && op["property"] == "opacity" && op["id"] == first
                })
                .unwrap();
            assert!(promotion < restyle);
        }
    }
}

/// Field chrome that stays provisional while its flag is set.
struct Chrome(std::rc::Rc<std::cell::Cell<bool>>);
impl exact_kernel::TextMeasurer for Chrome {
    fn measure(
        &mut self,
        request: &exact_kernel::TextMeasureRequest<'_>,
    ) -> exact_kernel::TextMetrics {
        MonospaceMeasurer::default().measure(request)
    }
    fn field_chrome(&mut self, _: &exact_kernel::FieldChromeRequest) -> exact_kernel::FieldChrome {
        exact_kernel::FieldChrome {
            top: 6.,
            right: 8.,
            bottom: 6.,
            left: 8.,
            minimum_height: 34.,
            provisional: self.0.get(),
        }
    }
}

#[test]
fn a_destroyed_omitted_box_withdraws_its_hits_when_layout_fails() {
    // `earlier`: a layout already failed before the batch that destroys it.
    for earlier in [false, true] {
        withdraws_when_layout_fails(earlier);
    }
}

fn font_size(id: ViewId, size: f64) -> exact_kernel::Op {
    let mut patch = exact_kernel::StyleProps::default();
    patch
        .set_dynamic(StyleId::FontSize, &exact_kernel::StyleValue::Number(size))
        .unwrap();
    exact_kernel::Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

fn withdraws_when_layout_fails(earlier: bool) {
    use exact_kernel::Op;
    let source = SHOP.replace(
        "    column height=600 width=400 testId=\"root\"\n",
        "    column height=600 width=400 testId=\"root\"\n      input testId=\"field\" value=\"x\"\n",
    );
    let failing = std::rc::Rc::new(std::cell::Cell::new(false));
    let (mut host, _) = boot_measured(&source, true, Box::new(Chrome(failing.clone())));
    let root = host.runner.collections()[0].rows[0].root;
    let row = host.runner.kernel().node(root).unwrap().children()[0];
    let cards = host.runner.kernel().node(row).unwrap().children();
    assert!(host.flat.nodes.contains(&cards[1]));
    assert!(host.flat.hits[&root].0.iter().any(|hit| hit.2 == cards[1]));
    let kernel = host.runner.kernel();
    let field = kernel
        .node_by_key(kernel.find_by_test_id("field")[0])
        .unwrap()
        .id;
    failing.set(true);
    if earlier {
        let batch = apply(&mut host, &[font_size(field, 29.)]);
        assert!(batch["error"].is_string(), "{batch}");
    }
    let mut doomed = vec![cards[1]];
    let mut at = 0;
    while at < doomed.len() {
        doomed.extend(host.runner.kernel().node(doomed[at]).unwrap().children());
        at += 1;
    }
    let mut ops = vec![
        font_size(field, 31.),
        Op::SetChildren {
            id: row,
            children: vec![cards[0]],
        },
    ];
    ops.extend(doomed.iter().rev().map(|&id| Op::DestroyView { id }));
    let batch = apply(&mut host, &ops);
    assert!(
        batch["error"]
            .as_str()
            .is_some_and(|e| e.contains("provisional")),
        "{batch}"
    );
    let withdrawn = batch["ops"].as_array().unwrap().iter().any(|op| {
        op["op"] == "layoutHitsWithdrawn"
            && op["id"] == root
            && op["ids"] == Json::from(vec![cards[1]])
    });
    assert!(withdrawn, "earlier failure {earlier}: {batch}");
}
