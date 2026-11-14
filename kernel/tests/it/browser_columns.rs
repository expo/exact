//! CSS multi-column layout (LLP 1093) against literal Chrome.
//!
//! Chrome 154, 2026-10-04, by the method of `browser_cases.rs` (the outer box
//! is the kernel root, everything `box-sizing: content-box` in a 16px/18px
//! monospace font, with `scrollbar-width: none`), each node measured with
//! `getClientRects`, so a box that straddles columns is one rect per
//! fragment. Text is `white-space: pre` with a pixel `line-height`, so
//! Chrome's line boxes are exact and the reference measurer's are the same.
//!
//! Each line is a case's name, the root's declarations, its nodes
//! (`id>parent>declarations`, then `>` and a text node's lines joined by
//! `|`), Chrome's rects (`id=x,y,w,h`, fragments joined by `/`), the
//! multi-column containers' scroll sizes (`id=w,h`) and each paragraph's
//! line starts (`id:x,y;x,y`). A case D10 declares (its name says which)
//! carries three more fields, the kernel's own rects, scroll sizes and line
//! starts (`-` where the kernel matches Chrome), and both comparisons use
//! those: each row holds the deviation to exactly what is declared.
use crate::browser_cases::{css_rows, props};
use crate::support::reader::kernel;
use exact_kernel::StyleId::{FontSize, LineHeight};
use exact_kernel::{Kernel, NodeType, Offer, Op, PropId};
use std::collections::BTreeMap;

struct Node<'a> {
    id: u32,
    parent: u32,
    css: &'a str,
    text: Option<String>,
}

fn nodes(field: &str) -> Vec<Node<'_>> {
    field
        .split(" & ")
        .map(|node| {
            let part: Vec<_> = node.splitn(4, '>').collect();
            Node {
                id: part[0].parse().unwrap(),
                parent: part[1].parse().unwrap(),
                css: part[2],
                text: part.get(3).map(|t| t.replace('|', "\n")),
            }
        })
        .collect()
}

fn lay_out(root: &str, nodes: &[Node<'_>]) -> Kernel {
    let mut rows = vec![
        (FontSize, crate::support::reader::number(16.0)),
        (LineHeight, crate::support::reader::text("18px")),
    ];
    rows.extend(css_rows(root));
    let mut ops = vec![
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 1,
            patch: Box::new(props(&rows)),
        },
    ];
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for n in nodes {
        ops.push(Op::CreateView {
            id: n.id,
            node_type: if n.text.is_some() {
                NodeType::Text
            } else {
                NodeType::View
            },
        });
        ops.push(Op::SetStyle {
            id: n.id,
            patch: Box::new(props(&css_rows(n.css))),
        });
        if let Some(text) = &n.text {
            ops.push(Op::SetProp {
                id: n.id,
                prop: PropId::Text,
                value: text.as_str().into(),
            });
        }
        children.entry(n.parent).or_default().push(n.id);
    }
    for (id, list) in children {
        ops.push(Op::SetChildren { id, children: list });
    }
    ops.push(Op::AttachRoot { id: 1 });
    let mut k = kernel();
    k.apply(0, 1, &ops).unwrap();
    k.compute_layout(1, Offer::definite(800.0, 600.0)).unwrap();
    k
}

fn f(v: f32) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    format!("{r}")
}

/// A node's rects as the TSV writes them: its fragments, else its frame.
fn rects(k: &Kernel, id: u32) -> String {
    let node = k.node(id).unwrap();
    let fr = node.frame;
    let multicol = k.columns(node.key).is_some();
    match k.fragments(node.key).filter(|_| !multicol) {
        Some(frags) => frags
            .iter()
            .map(|g| {
                format!(
                    "{},{},{},{}",
                    f(fr.x + g.x),
                    f(fr.y + g.y),
                    f(g.width),
                    f(g.height)
                )
            })
            .collect::<Vec<_>>()
            .join("/"),
        None => format!("{},{},{},{}", f(fr.x), f(fr.y), f(fr.width), f(fr.height)),
    }
}

/// A paragraph's line starts: each line box at its unfragmented place plus
/// its fragment's offset.
fn lines(k: &Kernel, id: u32, line_height: f32) -> String {
    let node = k.node(id).unwrap();
    let fr = node.frame;
    let frags = k.fragments(node.key).unwrap_or(&[]);
    let count = match frags.iter().map(|g| g.lines.1).max() {
        Some(n) if n > 0 => n,
        _ => (fr.height / line_height).round() as u32,
    };
    (0..count)
        .map(|i| {
            let (dx, dy) = frags
                .iter()
                .find(|g| g.lines.0 <= i && i < g.lines.1)
                .map_or((0.0, 0.0), |g| (g.dx, g.dy));
            format!(
                "{},{}",
                f(fr.x + dx + k.text_item_box(&node).map_or(0.0, |item| item.x)),
                f(fr.y + dy + i as f32 * line_height)
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn line_height(css: &str) -> f32 {
    css.split(';')
        .find_map(|d| d.strip_prefix("line-height:"))
        .and_then(|v| v.trim_end_matches("px").parse().ok())
        .unwrap_or(18.0)
}

#[test]
fn columns_land_where_chrome_puts_them() {
    let wrong: Vec<_> = include_str!("fixtures/browser_columns.tsv")
        .lines()
        .flat_map(mismatches)
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Where the kernel differs from a row of the fixture's format.
fn mismatches(line: &str) -> Vec<String> {
    let mut wrong = Vec::new();
    let field: Vec<_> = line.split('\t').collect();
    let name = field[0];
    let list = nodes(field[2]);
    let k = lay_out(field[1], &list);
    // A D10 case's kernel columns, where it says the kernel differs.
    let want = |i: usize| match field.get(i + 3) {
        Some(own) if *own != "-" => *own,
        _ => field[i],
    };
    for item in want(3).split(' ') {
        let (id, rect) = item.split_once('=').unwrap();
        let got = rects(&k, id.parse().unwrap());
        if !same(&got, rect) {
            wrong.push(format!("{name}: #{id} kernel {got}, expected {rect}"));
        }
    }
    if want(4) != "-" {
        for item in want(4).split(' ') {
            let (id, size) = item.split_once('=').unwrap();
            let c = k.node(id.parse().unwrap()).unwrap().content;
            let got = format!("{},{}", f(c.0), f(c.1));
            if !same(&got, size) {
                wrong.push(format!("{name}: #{id} scroll {got}, expected {size}"));
            }
        }
    }
    if want(5) != "-" {
        for item in want(5).split(' ') {
            let (id, starts) = item.split_once(':').unwrap();
            let id: u32 = id.parse().unwrap();
            let css = list.iter().find(|n| n.id == id).map_or("", |n| n.css);
            let got = lines(&k, id, line_height(css));
            if !same(&got, starts) {
                wrong.push(format!("{name}: #{id} lines {got}, expected {starts}"));
            }
        }
    }
    wrong
}

// Equal lists of numbers, each to 0.01.
fn same(a: &str, b: &str) -> bool {
    let nums = |s: &str| -> Vec<f32> {
        s.split([',', '/', ';'])
            .map(|v| v.parse().unwrap_or(f32::NAN))
            .collect()
    };
    let (a, b) = (nums(a), nums(b));
    a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= 0.01)
}

/// A percentage height bound caps the columns where its basis is definite,
/// as layout resolved it: an authored height or a stretched flex item gives
/// one (Chrome 154 puts the fourth 20-point block at the top of the second
/// column of a 60-point cap, the sixth of a 100-point one), unless a larger
/// minimum raises it.
#[test]
fn a_percentage_height_bound_caps_the_columns() {
    let blocks = |parent: u32, count: u32| {
        (100..100 + count)
            .map(|id| format!(" & {id}>{parent}>height:20px"))
            .collect::<String>()
    };
    let columns = "column-count:2;column-gap:0px;column-fill:auto";
    let field = format!("2>1>{columns};max-height:15%{}", blocks(2, 8));
    let k = lay_out("width:400px;height:400px", &nodes(&field));
    assert_eq!(
        rects(&k, 103),
        "200,0,200,20",
        "the fourth block opens the second column"
    );
    let field = format!(
        "2>1>width:400px & 3>2>{columns};max-height:50%{}",
        blocks(3, 15)
    );
    let k = lay_out("display:flex;width:400px;height:200px", &nodes(&field));
    assert_eq!(
        rects(&k, 105),
        "200,0,200,20",
        "the sixth block opens the second column"
    );
    // A larger minimum wins over the maximum (CSS 2 §10.7)
    let field = format!(
        "2>1>{columns};min-height:100px;max-height:50%{}",
        blocks(2, 4)
    );
    let k = lay_out("width:400px;height:100px", &nodes(&field));
    assert_eq!(
        rects(&k, 102),
        "0,40,200,20",
        "the third block stays in the first column"
    );
}

/// A paragraph that is its own multi-column container is cut by its height
/// bounds too, laid out as a leaf (Taffy patch 30): eight 20-point lines
/// under `max-height: 10%` of a 400-point root fill two lines a column
/// (Chrome, for a 40-point cap: four columns, the third line at the top of
/// the second).
#[test]
fn a_paragraph_s_height_bound_caps_its_columns() {
    let lines8 = (1..=8)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("|");
    let field = format!(
        "2>1>column-count:2;column-gap:0px;column-fill:auto;max-height:10%;width:400px;white-space:pre;line-height:20px>{lines8}"
    );
    // A flex item's final layout sizes its content, and the bound holds
    // there too (Chrome: the same four columns)
    for root in [
        "display:block;height:400px",
        "display:flex;flex-direction:row;align-items:flex-start;height:400px",
    ] {
        let k = lay_out(root, &nodes(&field));
        let starts = lines(&k, 2, 20.0);
        assert_eq!(starts.split(';').nth(2), Some("200,0"), "{root}: {starts}");
    }
}

/// A height bound that holds a percentage caps the columns where its basis
/// is definite, as layout resolved it: an authored height or a stretched
/// flex item gives one (LLP 1116 D5; Chrome 154 puts the fourth 20-point
/// block at the top of the second column of a 60-point cap, the sixth of a
/// 100-point one).
#[test]
fn a_calculated_height_bound_caps_the_columns() {
    use exact_kernel::{StyleId::*, StyleProps, StyleValue};
    let row = |rows: &[(exact_kernel::StyleId, &str)]| {
        let mut p = StyleProps::default();
        for (id, v) in rows {
            let value = v
                .parse()
                .map_or_else(|_| StyleValue::Text((*v).into()), StyleValue::Number);
            p.set_dynamic(*id, &value).unwrap();
        }
        Box::new(p)
    };
    let columns = |cap: &str| {
        row(&[
            (ColumnCount, "2"),
            (ColumnGap, "0"),
            (ColumnFill, "auto"),
            (MaxHeight, cap),
        ])
    };
    let lay_out = |tree: Vec<(u32, Box<StyleProps>, Vec<u32>)>, blocks: (u32, u32)| {
        let mut ops = Vec::new();
        let (holder, count) = blocks;
        for (id, style, _) in &tree {
            ops.push(Op::CreateView {
                id: *id,
                node_type: NodeType::View,
            });
            ops.push(Op::SetStyle {
                id: *id,
                patch: style.clone(),
            });
        }
        for i in 0..count {
            ops.push(Op::CreateView {
                id: 100 + i,
                node_type: NodeType::View,
            });
            ops.push(Op::SetStyle {
                id: 100 + i,
                patch: row(&[(Height, "20px")]),
            });
        }
        for (id, _, children) in &tree {
            ops.push(Op::SetChildren {
                id: *id,
                children: children.clone(),
            });
        }
        ops.push(Op::SetChildren {
            id: holder,
            children: (100..100 + count).collect(),
        });
        ops.push(Op::AttachRoot { id: 1 });
        let mut k = kernel();
        k.apply(0, 1, &ops).unwrap();
        k.compute_layout(1, Offer::definite(800.0, 600.0)).unwrap();
        k
    };
    let k = lay_out(
        vec![
            (1, row(&[(Width, "400px"), (Height, "400px")]), vec![2]),
            (2, columns("min(50%, 60px)"), vec![]),
        ],
        (2, 8),
    );
    assert_eq!(
        rects(&k, 103),
        "200,0,200,20",
        "the fourth block opens the second column"
    );
    let k = lay_out(
        vec![
            (
                1,
                row(&[(Display, "flex"), (Width, "400px"), (Height, "200px")]),
                vec![2],
            ),
            (2, row(&[(Width, "400px")]), vec![3]),
            (3, columns("min(50%, 100px)"), vec![]),
        ],
        (3, 15),
    );
    assert_eq!(
        rects(&k, 105),
        "200,0,200,20",
        "the sixth block opens the second column"
    );
}

/// `align-content` aligns a multi-column container's columns as one subject
/// (CSS Box Alignment 3 §5.1), with block layout's fallbacks. The free space
/// is the content box less the columns' height, which with
/// `column-fill: auto` is the content's while it fits one column. Chrome
/// 156.0.8078.4, by `columns_land_where_chrome_puts_them`'s method: each
/// child's fragments' corners.
#[test]
fn align_content_moves_the_columns_as_one() {
    let blocks = &["height:20px"; 2][..];
    let monolithic = |h: &[u32]| -> Vec<String> {
        h.iter()
            .map(|h| format!("height:{h}px;overflow:hidden"))
            .collect()
    };
    let cases: Vec<(&str, Vec<String>, &str)> = vec![
        ("height:100px;align-content:flex-end", own(blocks), "0,80 160,80"),
        ("height:100px;align-content:center", own(blocks), "0,40 160,40"),
        ("align-content:flex-end", own(blocks), "0,0 160,0"),
        ("min-height:100px;align-content:end", own(blocks), "0,80 160,80"),
        ("height:100px;align-content:space-between", own(blocks), "0,0 160,0"),
        ("height:100px;align-content:space-around", own(blocks), "0,40 160,40"),
        ("height:100px;align-content:stretch", own(blocks), "0,0 160,0"),
        (
            "height:100px;padding-top:10px;padding-bottom:5px;border-width:3px;border-style:solid;align-content:end",
            own(blocks),
            "3,93 163,93",
        ),
        (
            "height:100px;align-content:end",
            monolithic(&[20, 30, 10]),
            "0,60 160,60 160,90",
        ),
        ("height:30px;align-content:center", monolithic(&[40, 40]), "0,0 160,0"),
        (
            "height:100px;align-content:end;column-fill:auto",
            own(blocks),
            "0,60 0,80",
        ),
        (
            "min-height:100px;align-content:center;column-fill:auto",
            own(blocks),
            "0,30 0,50",
        ),
        (
            "height:100px;align-content:end;column-fill:auto",
            own(&["height:20px", "height:30px;break-before:column"]),
            "0,0 160,0",
        ),
    ];
    let mut wrong = Vec::new();
    for (css, children, want) in &cases {
        let k = aligned(css, children);
        let got = corners(&k, children.len());
        if got != *want {
            wrong.push(format!("{css} {children:?}: kernel {got}, Chrome {want}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));

    // The used height is no input to the flow thread, so a box that grows
    // lands where a fresh layout puts it.
    for css in ["align-content:center", "height:100px;align-content:end"] {
        let mut k = aligned(css, &own(blocks));
        let patch = Box::new(props(&css_rows("height:30px")));
        k.apply(0, 2, &[Op::SetStyle { id: 4, patch }]).unwrap();
        k.compute_layout(1, Offer::definite(800.0, 600.0)).unwrap();
        let mut fresh = k.rehydrate(Box::new(exact_kernel::MonospaceMeasurer::default()));
        fresh
            .compute_layout(1, Offer::definite(800.0, 600.0))
            .unwrap();
        assert_eq!(corners(&k, 2), corners(&fresh, 2), "{css}");
    }
}

/// An absolutely positioned box in a multi-column container that
/// `align-content` moves (`fixtures/browser_columns_aligned.tsv`, in
/// `columns_land_where_chrome_puts_them`'s format). Chrome 157.0.8103.78,
/// 2026-11-13, by that method; Chrome 156.0.8078.4 gave the first and the
/// 30px rows too.
/// Chrome adds the alignment to the box's flow-thread top before it picks
/// the column, so `align-content: end` takes a `top: 0` box four columns
/// on. A containing block inside an `overflow: hidden` box moves with that
/// box.
#[test]
fn align_content_moves_an_absolute_box_as_chrome_does() {
    let wrong: Vec<_> = include_str!("fixtures/browser_columns_aligned.tsv")
        .lines()
        .flat_map(mismatches)
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A popover is laid out in the viewport, out of the columns' flow, so the
/// columns place it nowhere: in an aligned multi-column container its frame
/// is the one it has without columns.
#[test]
fn a_popover_in_columns_keeps_its_viewport_frame() {
    for (name, cb) in [
        ("fragmentable", "position:relative"),
        ("decorated", "position:relative;background-color:#ff0000"),
    ] {
        let frame = |columns: &str| {
            let field = format!(
                "2>1>width:320px;height:100px;{columns}align-content:end \
                 & 3>2>height:20px \
                 & 4>2>{cb} \
                 & 5>4>height:20px \
                 & 6>4>width:10px;height:10px"
            );
            let mut k = lay_out("", &nodes(&field));
            let popover = Op::SetProp {
                id: 6,
                prop: PropId::Popover,
                value: "auto".into(),
            };
            k.apply(0, 2, &[popover]).unwrap();
            k.compute_layout(1, Offer::definite(800.0, 600.0)).unwrap();
            let f = k.node(6).unwrap().frame;
            (f.x, f.y, f.width, f.height)
        };
        let alone = frame("");
        assert_eq!(frame("column-count:2;column-gap:0px;"), alone, "{name}");
    }
}

fn own(css: &[&str]) -> Vec<String> {
    css.iter().map(|c| c.to_string()).collect()
}

// A two-column container with `css` holding blocks #3, #4, … with `children`.
fn aligned(css: &str, children: &[String]) -> Kernel {
    let mut field = format!("2>1>width:320px;column-count:2;column-gap:0px;{css}");
    for (i, child) in children.iter().enumerate() {
        field += &format!(" & {}>2>{child}", 3 + i);
    }
    lay_out("display:block", &nodes(&field))
}

// The top-left corner of each fragment of blocks #3, #4, ….
fn corners(k: &Kernel, count: usize) -> String {
    (3..3 + count as u32)
        .map(|id| {
            let rects = rects(k, id);
            let corners = rects.split('/').map(|r| {
                let r: Vec<_> = r.split(',').collect();
                format!("{},{}", r[0], r[1])
            });
            corners.collect::<Vec<_>>().join("/")
        })
        .collect::<Vec<_>>()
        .join(" ")
}
