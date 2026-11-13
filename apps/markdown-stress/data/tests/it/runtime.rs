//! Real Contract resource, input, paging and reparse behavior.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{DataSource, Event, Runner};
use markdown_stress_data::MarkdownStress;

fn boot() -> Runner<MarkdownStress> {
    let plan = contract::compile(include_str!("../../../app.contract")).unwrap();
    let baked = contract::bake(plan, MarkdownStress::default()).unwrap();
    Runner::boot(
        baked,
        MarkdownStress::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn press(r: &mut Runner<MarkdownStress>, name: &str) {
    let key = r.kernel().find_by_test_id(name)[0];
    r.dispatch(r.kernel().node_by_key(key).unwrap().id, Event::Press)
        .unwrap();
}

#[test]
fn paging_and_eager_mode_use_the_same_parsed_blocks() {
    let mut r = boot();
    press(&mut r, "profile-blocks");
    assert_eq!(r.kernel().find_by_test_id("block-0").len(), 1);
    assert_eq!(r.kernel().find_by_test_id("block-39").len(), 1);
    assert!(r.kernel().find_by_test_id("block-40").is_empty());
    press(&mut r, "next-page");
    assert!(r.kernel().find_by_test_id("block-0").is_empty());
    assert_eq!(r.kernel().find_by_test_id("block-40").len(), 1);
    press(&mut r, "toggle-eager");
    assert_eq!(r.kernel().find_by_test_id("block-0").len(), 1);
    assert_eq!(r.kernel().find_by_test_id("block-80").len(), 1);
    press(&mut r, "reset");
    assert_eq!(r.slot("eager"), Some(&Value::Bool(false)));
    assert_eq!(r.slot("size"), Some(&Value::Number(16_384.0)));
}

#[test]
fn typing_width_and_reparse_preserve_the_draft() {
    let mut r = boot();
    r.act("editDraft", vec![Value::str("Reading while reflowing 🦀")])
        .unwrap();
    press(&mut r, "width-wide");
    press(&mut r, "reparse");
    assert_eq!(
        r.slot("draft"),
        Some(&Value::str("Reading while reflowing 🦀"))
    );
    assert_eq!(r.slot("revision"), Some(&Value::Number(1.0)));
    press(&mut r, "start");
    for tick in 1..=22 {
        r.advance(tick as f64 * 1000.0).unwrap();
    }
    assert_eq!(r.slot("revision"), Some(&Value::Number(21.0)));
    assert_eq!(r.slot("running"), Some(&Value::Bool(false)));
    r.act("editDraft", vec![Value::str(&"x".repeat(513))])
        .unwrap();
    assert_eq!(r.slot("draftTooLong"), Some(&Value::Bool(true)));
    assert_eq!(
        r.slot("draft"),
        Some(&Value::str("Reading while reflowing 🦀"))
    );
}

#[test]
fn malformed_controls_are_refused_before_generation() {
    let mut source = MarkdownStress::default();
    for n in [f64::NAN, f64::INFINITY, -1.0, 16_384.5, 4_194_305.0] {
        assert!(source
            .query(
                "document",
                &[
                    Value::str("mixed"),
                    Value::Number(n),
                    Value::Number(0.0),
                    Value::Number(0.0),
                    Value::Bool(false),
                    Value::Bool(false)
                ]
            )
            .is_err());
    }
    assert!(source.query("document", &[]).is_err());
    assert!(source.query("unknown", &[]).is_err());
}

#[test]
fn a_giant_table_is_one_supplied_block_with_all_its_rows() {
    let mut source = MarkdownStress::default();
    let mut args = vec![
        Value::str("table"),
        Value::Number(16_384.),
        Value::Number(0.),
        Value::Number(0.),
        Value::Bool(false),
        Value::Bool(false),
    ];
    let manual = source.query("document", &args).unwrap();
    let Value::Record(fields) = &manual else {
        panic!("document record")
    };
    assert_eq!(fields[2], Value::Number(2.));
    assert_eq!(fields[3], Value::Number(2.));
    assert_eq!(fields[4], Value::Number(0.));
    assert_eq!(fields[5], Value::Number(2.));
    let Value::List(blocks) = &fields[9] else {
        panic!("supplied blocks")
    };
    assert_eq!(blocks.len(), 2);
    let Value::Record(table) = &blocks[1] else {
        panic!("table")
    };
    assert_eq!(table[1], Value::str("table"));
    assert_eq!(table[2], Value::Number(4.));
    let Value::List(cells) = &table[8] else {
        panic!("cells")
    };
    assert!(fields[8].as_number().unwrap() > 100.);
    assert_eq!(cells.len(), fields[8].as_number().unwrap() as usize * 4);
    assert!(fields[7].as_number().unwrap() > 10_000.);
    args[3] = Value::Number(99.);
    assert_eq!(source.query("document", &args).unwrap(), manual);
    args[4] = Value::Bool(true);
    assert_eq!(source.query("document", &args).unwrap(), manual);

    let mut runner = boot();
    press(&mut runner, "profile-table");
    assert_eq!(runner.kernel().find_by_test_id("table-1").len(), 1);
    assert!(runner.kernel().find_by_test_id("block-2").is_empty());
    press(&mut runner, "toggle-windowed");
    assert_eq!(runner.collections()[0].count, 2);
    assert_eq!(runner.kernel().find_by_test_id("table-1").len(), 1);
    let grouped = runner.resource("doc").unwrap().clone();
    press(&mut runner, "toggle-single");
    assert!(runner.collections().is_empty());
    assert!(runner.kernel().find_by_test_id("table-1").is_empty());
    assert!(runner.kernel().find_by_test_id("next-page").is_empty());
    assert_eq!(runner.kernel().find_by_test_id("markup-node").len(), 1);
    let Value::Record(single) = runner.resource("doc").unwrap() else {
        panic!("document")
    };
    assert_eq!(single[3], Value::Number(0.));
    assert_eq!(single[9], Value::list(vec![]));
    press(&mut runner, "toggle-single");
    assert_eq!(runner.resource("doc"), Some(&grouped));
    assert_eq!(runner.kernel().find_by_test_id("table-1").len(), 1);
    assert_eq!(runner.kernel().find_by_test_id("next-page").len(), 1);
}

#[test]
fn giant_table_rows_past_ten_thousand_keep_their_own_height() {
    use exact_kernel::{NodeType, Offer, ViewId};
    fn positions(k: &Kernel, id: ViewId, top: f32, out: &mut Vec<(String, f32, f32, f32)>) {
        let node = k.node(id).unwrap();
        let y = top + node.frame.y;
        if node.node_type == NodeType::Text && !node.is_inline_run() {
            let text: String = node.text_runs().iter().map(|r| r.text.as_ref()).collect();
            if matches!(
                text.as_str(),
                "000000"
                    | "008190"
                    | "008191"
                    | "008192"
                    | "009998"
                    | "009999"
                    | "010000"
                    | "011000"
            ) {
                out.push((text, y, node.frame.x, node.frame.width));
            }
        }
        for child in node.children() {
            positions(k, child, y, out);
        }
    }
    let mut runner = boot();
    press(&mut runner, "profile-table");
    press(&mut runner, "size-1048576");
    let root = runner.roots()[0];
    runner
        .kernel_mut()
        .compute_layout(root, Offer::definite(980., 820.))
        .unwrap();
    let mut rows = Vec::new();
    positions(runner.kernel(), root, 0., &mut rows);
    assert_eq!(rows.len(), 8);
    assert!(
        rows.iter()
            .all(|row| row.2 == rows[0].2 && row.3 == rows[0].3),
        "column widths differ: {rows:?}"
    );
    for pair in rows.windows(2) {
        assert!(pair[1].1 > pair[0].1, "table rows overlap: {pair:?}");
    }
}

#[test]
fn four_mib_table_groups_cover_all_cells_without_changing_paging_counts() {
    let mut source = MarkdownStress::default();
    let mut args = [
        Value::str("table"),
        Value::Number(4_194_304.),
        Value::Number(0.),
        Value::Number(0.),
        Value::Bool(false),
        Value::Bool(false),
    ];
    let Value::Record(doc) = source.query("document", &args).unwrap() else {
        panic!("document")
    };
    assert_eq!(doc[2], Value::Number(2.));
    assert_eq!(doc[3], Value::Number(2.));
    let Value::List(blocks) = &doc[9] else {
        panic!("blocks")
    };
    let Value::Record(table) = &blocks[1] else {
        panic!("table")
    };
    let Value::List(cells) = &table[8] else {
        panic!("cells")
    };
    let Value::List(groups) = &table[13] else {
        panic!("groups")
    };
    let Value::List(sizers) = &table[14] else {
        panic!("sizers")
    };
    assert!(doc[8].as_number().unwrap() > 32_767.);
    assert_eq!(cells.len(), doc[8].as_number().unwrap() as usize * 4);
    let mut end = 0.;
    for group in groups.iter() {
        let Value::Record(group) = group else {
            panic!("group")
        };
        assert_eq!(group[1].as_number(), Some(end));
        let next = group[2].as_number().unwrap();
        assert!((next - end) / 4. < 10_000.);
        end = next;
    }
    assert_eq!(end as usize, cells.len());
    assert_eq!(sizers.len(), 4);
    // A column of single-run cells joins them into lines by style, the
    // header's and the body's, each line referring to cells of its column.
    let rows = doc[8].as_number().unwrap() as usize;
    for (column, sizer) in sizers.iter().enumerate() {
        let Value::Record(sizer) = sizer else {
            panic!("sizer")
        };
        let Value::List(lines) = &sizer[1] else {
            panic!("lines")
        };
        assert!(lines.len() <= 1 + rows.div_ceil(16_384));
        for line in lines.iter() {
            let Value::Record(line) = line else {
                panic!("line")
            };
            assert_eq!(line[2], Value::Number(-1.));
            let Value::List(refs) = &line[1] else {
                panic!("refs")
            };
            assert!(refs
                .iter()
                .all(|at| at.as_number().unwrap() as usize % 4 == column));
        }
    }
    let bytes = Value::record(doc.to_vec()).to_bytes().len();
    assert!(
        bytes + 64 < exact_runner::MAX_HOST_WORK_BYTES,
        "the complete table must fit the Rust data module reply: {bytes} bytes"
    );
    // A table of distinct cells within Markdown's 4 MiB file limit: sizing
    // that copied their text would carry it twice.
    let mut distinct = String::from("| A | B | C | D |\n| - | - | - | - |\n");
    for row in 0..36_000 {
        let cells: Vec<_> = (0..4).map(|c| format!("cell{:021}", row * 4 + c)).collect();
        distinct.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    assert!(distinct.len() <= markdown_stress_data::fixture::MAX_BYTES);
    let blocks = markdown_parse::value::into_blocks_with_tables(markdown_parse::parse(
        &distinct,
        &str::to_owned,
    ));
    // The reader's other document fields are a few hundred bytes.
    let bytes = blocks.to_bytes().len();
    assert!(
        bytes + 4096 < exact_runner::MAX_HOST_WORK_BYTES,
        "a table of distinct cells must fit the Rust data module reply: {bytes} bytes"
    );

    let expected_source = markdown_stress_data::fixture::generate(
        markdown_stress_data::fixture::Profile::Table,
        4_194_304,
    )
    .unwrap();
    args[5] = Value::Bool(true);
    for eager in [false, true] {
        args[4] = Value::Bool(eager);
        let single = source.query("document", &args).unwrap();
        let bytes = single.to_bytes().len();
        assert!(
            bytes + 64 < exact_runner::MAX_HOST_WORK_BYTES,
            "the one-node table must fit the Rust data module reply: {bytes} bytes"
        );
        let Value::Record(single) = single else {
            panic!("document")
        };
        assert_eq!(single[3], Value::Number(0.));
        assert_eq!(single[4], Value::Number(0.));
        assert_eq!(single[5], Value::Number(0.));
        assert_eq!(single[9], Value::list(vec![]));
        assert_eq!(single[10], Value::str(&expected_source));
        for field in [0, 1, 2, 6, 7, 8] {
            assert_eq!(single[field], doc[field]);
        }
    }
    args[4] = Value::Bool(false);
    args[5] = Value::Bool(false);
    assert_eq!(source.query("document", &args).unwrap(), Value::Record(doc));
}
