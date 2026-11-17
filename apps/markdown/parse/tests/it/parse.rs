//! What the parser must get right, checked against the corpus it exists to
//! read: this repository's own Markdown. @ref LLP 1033

use markdown_parse::{parse, plain, Kind};

fn here(source: &str) -> markdown_parse::Document {
    parse(source, &|target: &str| target.to_string())
}

#[test]
fn document_values_share_repeated_text_without_changing_keys_or_encoding() {
    use exact_plan::Value;
    fn fields(value: &Value) -> &[Value] {
        match value {
            Value::Record(values) | Value::List(values) => values,
            _ => panic!("expected fields or items"),
        }
    }
    // One allocation for shared text; equal bytes for inline text, which has none.
    fn shared(a: &Value, b: &Value) {
        assert!(a.is_str() && b.is_str(), "expected strings");
        assert!(Value::same_str(a, b));
    }
    let doc = here("alpha **β** [site](next.md)\n\nalpha **β** [site](next.md)");
    let value = markdown_parse::value::blocks(&doc);
    let blocks = fields(&value);
    let first = fields(&blocks[0]);
    let second = fields(&blocks[1]);
    assert_eq!(first[0].as_str(), Some("0"));
    assert_eq!(second[0].as_str(), Some("1"));
    for i in [1, 3, 4, 5] {
        shared(&first[i], &second[i]);
    }
    shared(&first[3], &first[4]);
    let a = fields(&fields(&first[7])[0]);
    let b = fields(&fields(&second[7])[0]);
    for i in [0, 1, 3, 5] {
        shared(&a[i], &b[i]);
    }
    shared(&first[0], &a[0]);
    let (Value::List(a), Value::List(b)) = (&first[8], &second[8]) else {
        panic!("expected empty cells");
    };
    assert!(a.is_empty() && exact_plan::Items::ptr_eq(a, b));
    assert_eq!(Value::from_bytes(&value.to_bytes()).unwrap(), value);
    // A separate conversion has independent ownership; closing a document
    // must not leave its content in an intern pool.
    let kept = first[1].to_shared_str().expect("a string");
    drop(value);
    assert_eq!(exact_plan::Str::strong_count(&kept), 1);
    assert_eq!(
        fields(&markdown_parse::value::block(usize::MAX, &doc.blocks[0]))[0].as_str(),
        Some(usize::MAX.to_string().as_str())
    );
    // With no table in it, the reader's owned conversion is the same value.
    let encoded = markdown_parse::value::blocks(&doc).to_bytes();
    assert_eq!(
        markdown_parse::value::into_blocks_with_tables(doc).to_bytes(),
        encoded
    );
}

#[test]
fn document_string_sharing_preserves_styles_links_and_indices() {
    use markdown_parse::{Block, Document, Run};
    let plain = Run::text("same");
    let doc = Document {
        blocks: [
            plain.clone(),
            Run {
                bold: true,
                ..plain.clone()
            },
            Run {
                italic: true,
                ..plain.clone()
            },
            Run {
                code: true,
                ..plain.clone()
            },
            Run {
                href: "next.md".into(),
                ..plain.clone()
            },
            plain,
        ]
        .into_iter()
        .map(|run| Block {
            runs: vec![run],
            ..Block::default()
        })
        .collect(),
        ..Document::default()
    };
    let expected = exact_plan::Value::list(
        doc.blocks
            .iter()
            .enumerate()
            .map(|(i, b)| markdown_parse::value::block(i, b))
            .collect(),
    );
    assert_eq!(
        markdown_parse::value::into_blocks_with_tables(doc),
        expected
    );
}

#[test]
fn headings_paragraphs_and_emphasis() {
    let doc = here("# Title\n\nSome **bold** and *slanted* and `code` text.\n");
    assert_eq!(doc.title, "Title");
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].kind, Kind::Heading);
    assert_eq!(doc.blocks[0].depth, 1);
    assert_eq!(doc.blocks[0].href, "title");
    let runs = &doc.blocks[1].runs;
    assert_eq!(plain(runs), "Some bold and slanted and code text.");
    assert!(runs.iter().any(|r| r.text == "bold" && r.bold));
    assert!(runs.iter().any(|r| r.text == "slanted" && r.italic));
    assert!(runs.iter().any(|r| r.text == "code" && r.code));
}

/// CommonMark 0.31.2 §4.2: the optional closing sequence of `#` is removed
/// only when a space or tab precedes it and only spaces or tabs follow it.
#[test]
fn an_atx_heading_keeps_hashes_that_are_not_a_closing_sequence() {
    for (source, text) in [
        ("# C#\n", "C#"),
        // Examples 72 and 73: a closing sequence, of any length, then spaces.
        ("# foo ##################################\n", "foo"),
        ("### foo ###     \n", "foo"),
        // Example 74: a closing sequence followed by text is text.
        ("### foo ### b\n", "foo ### b"),
        // Example 75: no space before it.
        ("# foo#\n", "foo#"),
        // Example 76: an escaped `#` is not one.
        ("### foo \\###\n", "foo ###"),
        // Example 79: a heading of only a closing sequence is empty.
        ("### ###\n", ""),
    ] {
        let doc = here(source);
        assert_eq!(doc.blocks[0].kind, Kind::Heading, "{source:?}");
        assert_eq!(plain(&doc.blocks[0].runs), text, "{source:?}");
    }
}

fn assert_inline_runs(source: &str, expected: &[(&str, bool, bool, bool)]) {
    let doc = here(source);
    assert_eq!(doc.blocks.len(), 1, "{source:?}");
    let actual: Vec<_> = doc.blocks[0]
        .runs
        .iter()
        .map(|r| (r.text.as_str(), r.bold, r.italic, r.code))
        .collect();
    assert_eq!(actual, expected, "{source:?}");
}

#[test]
fn code_spans_take_precedence_over_emphasis_closers() {
    // CommonMark 0.31.2 examples 341, 478 and 479.
    assert_inline_runs(
        "*foo`*`",
        &[("*foo", false, false, false), ("*", false, false, true)],
    );
    for marker in ["*", "_", "**", "__"] {
        let bold = marker.len() == 2;
        assert_inline_runs(
            &format!("{marker}a `{marker}`{marker}"),
            &[("a ", bold, !bold, false), (marker, bold, !bold, true)],
        );
    }
    assert_inline_runs(
        "**Pass `**kwargs` through**",
        &[
            ("Pass ", true, false, false),
            ("**kwargs", true, false, true),
            (" through", true, false, false),
        ],
    );
    assert_inline_runs(
        "*violates `break-*: avoid`*",
        &[
            ("violates ", false, true, false),
            ("break-*: avoid", false, true, true),
        ],
    );
    assert_inline_runs(
        "*a ``*`x``*",
        &[("a ", false, true, false), ("*`x", false, true, true)],
    );
    assert_inline_runs(
        r"*a `\*`*",
        &[("a ", false, true, false), (r"\*", false, true, true)],
    );
    assert_inline_runs("*a `*", &[("a `", false, true, false)]);
    assert_inline_runs(
        r"*a \`*`",
        &[("a `", false, true, false), ("`", false, false, false)],
    );
    assert_inline_runs(r"*a \**", &[("a *", false, true, false)]);
}

#[test]
fn intraword_underscores_do_not_close_emphasis() {
    // CommonMark 0.31.2 examples 374 and 376.
    assert_inline_runs("_foo_bar", &[("_foo_bar", false, false, false)]);
    assert_inline_runs("_foo_bar_baz_", &[("foo_bar_baz", false, true, false)]);
    assert_inline_runs(
        "_see snake_case names_",
        &[("see snake_case names", false, true, false)],
    );
    assert_inline_runs("_foo_123_", &[("foo_123", false, true, false)]);
    assert_inline_runs("__foo__bar__", &[("foo__bar", true, false, false)]);
    assert_inline_runs(
        "_foo_, bar",
        &[("foo", false, true, false), (", bar", false, false, false)],
    );
    assert_inline_runs(
        "*foo*bar",
        &[("foo", false, true, false), ("bar", false, false, false)],
    );
}

#[test]
fn emphasis_ignores_backticks_in_link_targets_and_autolinks() {
    for (source, label) in [
        ("[link](url \"a ` title\") *emphasis* and `code`", "link"),
        ("[link](url`path) *emphasis* and `code`", "link"),
        (
            "<https://example.com/`> *emphasis* and `code`",
            "https://example.com/`",
        ),
        ("<a`@b> *emphasis* and `code`", "a`@b"),
    ] {
        assert_inline_runs(
            &format!("p {source}"),
            &[
                ("p ", false, false, false),
                (label, false, false, false),
                (" ", false, false, false),
                ("emphasis", false, true, false),
                (" and ", false, false, false),
                ("code", false, false, true),
            ],
        );
    }
    for (source, label) in [
        (
            "*before [link](url \"a ` title\") after* and `code`",
            "link",
        ),
        (
            "*before <https://example.com/`> after* and `code`",
            "https://example.com/`",
        ),
    ] {
        assert_inline_runs(
            source,
            &[
                ("before ", false, true, false),
                (label, false, true, false),
                (" after", false, true, false),
                (" and ", false, false, false),
                ("code", false, false, true),
            ],
        );
    }
}

#[test]
fn emphasis_wraps_bare_urls_without_changing_their_targets() {
    use markdown_parse::Run;
    for marker in ["*", "**", "_", "__"] {
        for prefix in ["", "see "] {
            for url in [
                "https://example.com",
                "https://example.com/foo",
                "https://example.com/a",
                "https://example.com/snake_case",
            ] {
                let source = format!("{marker}{prefix}{url}{marker}");
                let bold = marker.len() == 2;
                let style = Run {
                    bold,
                    italic: !bold,
                    ..Run::default()
                };
                let mut expected = Vec::new();
                if !prefix.is_empty() {
                    expected.push(Run {
                        text: prefix.into(),
                        ..style.clone()
                    });
                }
                expected.push(Run {
                    text: url.into(),
                    href: format!("resolved:{url}"),
                    ..style
                });
                let doc = parse(&source, &|target| format!("resolved:{target}"));
                assert_eq!(doc.blocks.len(), 1, "{source:?}");
                assert_eq!(doc.blocks[0].runs, expected, "{source:?}");
            }
        }
    }
}

#[test]
fn emphasis_url_recognition_uses_the_content_before_its_closer() {
    assert_inline_runs(r"*http://a\*b*", &[("http://a*b", false, true, false)]);
    assert_inline_runs(
        "*http://a`*`*",
        &[("http://a", false, true, false), ("*", false, true, true)],
    );
}

#[test]
fn nested_emphasis_ends_url_opacity_before_a_code_span() {
    use markdown_parse::Run;
    for (outer, inner) in [("*", "__"), ("_", "**"), ("**", "_"), ("__", "*")] {
        let url = "https://example.com";
        let doc = here(&format!("{outer}{inner}{url}{inner}`{outer}`{outer}"));
        assert_eq!(
            doc.blocks[0].runs,
            vec![
                Run {
                    text: url.into(),
                    href: url.into(),
                    bold: true,
                    italic: true,
                    ..Run::default()
                },
                Run {
                    text: outer.into(),
                    code: true,
                    bold: outer.len() == 2,
                    italic: outer.len() == 1,
                    ..Run::default()
                },
            ],
            "outer={outer:?}, inner={inner:?}",
        );
    }
}

#[test]
fn emphasis_after_a_bare_url_uses_the_remaining_text() {
    let url = "https://example.com/`";
    let doc = here(&format!("p *x {url} *a* ` end"));
    assert_eq!(
        doc.blocks[0].runs,
        vec![
            markdown_parse::Run::text("p "),
            markdown_parse::Run {
                text: "x ".into(),
                italic: true,
                ..Default::default()
            },
            markdown_parse::Run {
                text: url.into(),
                href: url.into(),
                italic: true,
                ..Default::default()
            },
            markdown_parse::Run {
                text: " *a".into(),
                italic: true,
                ..Default::default()
            },
            markdown_parse::Run::text(" ` end"),
        ]
    );
}

#[test]
fn emphasis_keeps_bare_url_backticks_literal() {
    use markdown_parse::Run;
    for marker in ["*", "**", "_", "__"] {
        let style = Run {
            bold: marker.len() == 2,
            italic: marker.len() == 1,
            ..Run::default()
        };
        let url = "https://example.com/`";
        let source = format!("{marker}a {url} b{marker} ` {marker}c{marker}");
        let doc = here(&source);
        assert_eq!(
            doc.blocks[0].runs,
            vec![
                Run {
                    text: "a ".into(),
                    ..style.clone()
                },
                Run {
                    text: url.into(),
                    href: url.into(),
                    ..style.clone()
                },
                Run {
                    text: " b".into(),
                    ..style.clone()
                },
                Run::text(" ` "),
                Run {
                    text: "c".into(),
                    ..style
                },
            ],
            "{source:?}",
        );
    }
}

#[test]
fn emphasis_keeps_bare_url_brackets_and_escapes_literal() {
    use markdown_parse::Run;
    for marker in ["*", "**", "_", "__"] {
        let style = Run {
            bold: marker.len() == 2,
            italic: marker.len() == 1,
            ..Run::default()
        };
        for (path, suffix) in [("a[b", "](c)"), ("a![b", "](c)"), ("a\\", " tail")] {
            let url = format!("https://example.com/{path}");
            let source = format!("{marker}see {url}{marker}{suffix}");
            let doc = parse(&source, &|target| format!("resolved:{target}"));
            assert_eq!(
                doc.blocks[0].runs,
                vec![
                    Run {
                        text: "see ".into(),
                        ..style.clone()
                    },
                    Run {
                        text: url.clone(),
                        href: format!("resolved:{url}"),
                        ..style.clone()
                    },
                    Run::text(suffix),
                ],
                "{source:?}",
            );
        }
    }
}

#[test]
fn emphasis_recognizes_code_after_each_bare_url_terminator() {
    for separator in [" ", "\t", "\u{2003}", "<", ")", "\""] {
        let url = "https://example.com/`";
        assert_inline_runs(
            &format!("*a {url}{separator}`*` b*"),
            &[
                ("a ", false, true, false),
                (url, false, true, false),
                (separator, false, true, false),
                ("*", false, true, true),
                (" b", false, true, false),
            ],
        );
    }
}

#[test]
fn emphasis_code_spans_are_bounded_by_the_link_label() {
    assert_inline_runs(
        "[*a `b*](u) `c`",
        &[
            ("a `b", false, true, false),
            (" ", false, false, false),
            ("c", false, false, true),
        ],
    );
    let doc = here("[*a `b*](u) `c`");
    assert_eq!(doc.blocks[0].runs[0].href, "u");
    assert_inline_runs(
        "*before* [*a `b*](u) `c`",
        &[
            ("before", false, true, false),
            (" ", false, false, false),
            ("a `b", false, true, false),
            (" ", false, false, false),
            ("c", false, false, true),
        ],
    );
}

#[test]
fn intraword_underscore_runs_stay_literal() {
    for text in ["__foo__bar", "foo__bar__baz"] {
        assert_inline_runs(text, &[(text, false, false, false)]);
    }
    assert_inline_runs("__foo__bar__", &[("foo__bar", true, false, false)]);
}

#[test]
fn intraword_underscores_do_not_open_strong() {
    assert_inline_runs("foo__bar__", &[("foo__bar__", false, false, false)]);
}

#[test]
fn intraword_underscore_boundaries_include_unicode() {
    for text in [
        "_пристаням_стремятся",
        "пристаням__стремятся__",
        "_foo_１２３",
    ] {
        assert_inline_runs(text, &[(text, false, false, false)]);
    }
    assert_inline_runs(
        "_пристаням_стремятся_",
        &[("пристаням_стремятся", false, true, false)],
    );
}

#[test]
fn a_link_carries_its_target_through_the_resolver() {
    let doc = parse("See [the rules](rules/RULES.md).", &|target: &str| {
        format!("/repo/{target}")
    });
    let link = doc.blocks[0]
        .runs
        .iter()
        .find(|r| r.text == "the rules")
        .expect("the label is a run");
    assert_eq!(link.href, "/repo/rules/RULES.md");
    // The text around it is not a link.
    assert!(doc.blocks[0]
        .runs
        .iter()
        .any(|r| r.text == "See " && r.href.is_empty()));
}

#[test]
fn fenced_code_keeps_its_text_and_never_parses_it() {
    let doc = here("```sh\ncargo build --workspace\n# not a heading\n```\n");
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].kind, Kind::Code);
    assert_eq!(doc.blocks[0].href, "sh");
    assert_eq!(
        doc.blocks[0].text,
        "cargo build --workspace\n# not a heading"
    );
}

/// CommonMark 0.31.2 §4.5: a closing fence "may be followed only by spaces or
/// tabs, which are ignored".
#[test]
fn a_closing_fence_may_end_in_spaces_or_tabs() {
    for source in ["```\ncode\n``` \nnext\n", "~~~~\ncode\n~~~~~\t \nnext\n"] {
        let doc = here(source);
        assert_eq!(doc.blocks.len(), 2, "{source:?}");
        assert_eq!(doc.blocks[0].kind, Kind::Code, "{source:?}");
        assert_eq!(doc.blocks[0].text, "code", "{source:?}");
        assert_eq!(plain(&doc.blocks[1].runs), "next", "{source:?}");
    }
    // Anything else after the marker leaves the line inside the block.
    let doc = here("```\ncode\n``` x\n```\n");
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].text, "code\n``` x");
}

/// A fence's indentation is spaces and tabs, removed from each line up to
/// its width; leading Unicode spaces are the code's own.
#[test]
fn an_indented_fence_keeps_leading_unicode_spaces_as_code() {
    for (source, code) in [
        (" ```\n\u{2003}em\n x\n ```\n", "\u{2003}em\nx"),
        (
            "  ```\n\u{2003}em\n \u{a0}nbsp\n    four\n  ```\n",
            "\u{2003}em\n\u{a0}nbsp\n  four",
        ),
        ("```\n\u{a0}nbsp\n```\n", "\u{a0}nbsp"),
    ] {
        let doc = here(source);
        assert_eq!(doc.blocks.len(), 1, "{source:?}");
        assert_eq!(doc.blocks[0].kind, Kind::Code);
        assert_eq!(doc.blocks[0].text, code, "{source:?}");
    }
}

#[test]
fn lists_carry_their_marker_and_their_depth() {
    let doc = here("- one\n- two\n  - nested\n\n1. first\n2. second\n");
    let items: Vec<_> = doc.blocks.iter().filter(|b| b.kind == Kind::Item).collect();
    assert_eq!(items.len(), 5);
    assert_eq!(items[0].marker, "•");
    assert_eq!(items[0].depth, 0);
    assert_eq!(items[2].depth, 1);
    assert_eq!(plain(&items[2].runs), "nested");
    assert_eq!(items[3].marker, "1.");
    assert_eq!(items[4].marker, "2.");
}

#[test]
fn a_wrapped_list_item_is_one_item() {
    let doc = here("- a long item that\n  continues on the next line\n- another\n");
    let items: Vec<_> = doc.blocks.iter().filter(|b| b.kind == Kind::Item).collect();
    assert_eq!(items.len(), 2);
    assert_eq!(
        plain(&items[0].runs),
        "a long item that continues on the next line"
    );
}

#[test]
fn quotes_rules_and_images() {
    let doc = here("> quoted\n\n---\n\n![a train](assets/caltrain.png)\n");
    assert_eq!(doc.blocks[0].kind, Kind::Quote);
    assert_eq!(plain(&doc.blocks[0].runs), "quoted");
    assert_eq!(doc.blocks[1].kind, Kind::Rule);
    assert_eq!(doc.blocks[2].kind, Kind::Image);
    assert_eq!(doc.blocks[2].text, "a train");
    assert_eq!(doc.blocks[2].href, "assets/caltrain.png");
}

#[test]
fn a_pipe_table_is_rows_of_cells_and_prose_with_pipes_is_not() {
    let doc = here("| Crate | What |\n| --- | --- |\n| `plan` | the format |\n");
    let rows: Vec<_> = doc
        .blocks
        .iter()
        .filter(|b| b.kind == Kind::TableRow)
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].header);
    assert_eq!(plain(&rows[0].cells[0]), "Crate");
    assert!(!rows[1].header);
    assert_eq!(plain(&rows[1].cells[0]), "plan");
    assert!(rows[1].cells[0][0].code);

    let prose = here("Run `a | b` in a shell.\n");
    assert!(prose.blocks.iter().all(|b| b.kind != Kind::TableRow));
}

/// The Markdown reader's form of a table: one block, `depth` its header's
/// cell count, and its cells row by row, each row padded or cut to that
/// count as GFM's tables extension says. Two tables apart stay two.
#[test]
fn the_readers_blocks_make_each_table_one_block_of_the_headers_columns() {
    use exact_plan::Value;
    fn fields(value: &Value) -> &[Value] {
        match value {
            Value::Record(values) | Value::List(values) => values,
            _ => panic!("expected fields or items"),
        }
    }
    let doc = here(
        "Before.\n\n| A | B | C |\n| - | - | - |\n| 1 |\n| 2 | 3 | 4 | 5 |\n\n| x | y | z |\n| - | - | - |\n| p | q | r |\n\nAfter.\n",
    );
    let groups: Vec<_> = markdown_parse::value::block_groups(&doc).collect();
    assert_eq!(
        groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
        [1, 3, 2, 1]
    );
    let paged = Value::list(
        groups
            .iter()
            .enumerate()
            .map(|(i, group)| markdown_parse::value::block_group(i, group))
            .collect(),
    );
    let value = markdown_parse::value::into_blocks_with_tables(doc);
    assert_eq!(paged, value, "paging preserves whole tables and block ids");
    let blocks = fields(&value);
    let kinds: Vec<_> = blocks
        .iter()
        .map(|b| fields(b)[1].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["paragraph", "table", "table", "paragraph"]);
    let ids: Vec<_> = blocks
        .iter()
        .map(|b| fields(b)[0].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["0", "1", "2", "3"], "an id is the block's place");
    let text = |table: &Value| -> (f64, Vec<String>) {
        let f = fields(table);
        let cells = fields(&f[8])
            .iter()
            .map(|cell| {
                fields(&fields(cell)[1])
                    .iter()
                    .map(|run| fields(run)[1].as_str().unwrap().to_owned())
                    .collect()
            })
            .collect();
        (f[2].as_number().unwrap(), cells)
    };
    assert_eq!(
        text(&blocks[1]),
        (
            3.,
            ["A", "B", "C", "1", "", "", "2", "3", "4"]
                .map(String::from)
                .to_vec()
        )
    );
    assert_eq!(
        text(&blocks[2]),
        (
            3.,
            ["x", "y", "z", "p", "q", "r"].map(String::from).to_vec()
        )
    );
    let cell_ids: Vec<_> = fields(&fields(&blocks[2])[8])
        .iter()
        .map(|cell| fields(cell)[0].as_str().unwrap())
        .collect();
    assert_eq!(cell_ids, ["0", "1", "2", "3", "4", "5"]);
}

#[test]
fn raw_html_stays_visible_and_inert() {
    let doc = here("<div class=\"note\">hello</div>\n");
    assert_eq!(doc.blocks[0].kind, Kind::Html);
    assert_eq!(doc.blocks[0].text, "<div class=\"note\">hello</div>");
}

#[test]
fn snake_case_and_arithmetic_are_not_emphasis() {
    let doc = here("The value of a_b_c is 2 * 3 * 4.\n");
    assert_eq!(
        plain(&doc.blocks[0].runs),
        "The value of a_b_c is 2 * 3 * 4."
    );
    assert!(doc.blocks[0].runs.iter().all(|r| !r.italic));
}

#[test]
fn a_bare_url_becomes_a_link_without_its_sentence_stop() {
    let doc = here("See https://example.com/a, then stop.\n");
    let link = doc.blocks[0]
        .runs
        .iter()
        .find(|r| !r.href.is_empty())
        .unwrap();
    assert_eq!(link.text, "https://example.com/a");
    assert_eq!(link.href, "https://example.com/a");
}

#[test]
fn unicode_survives_inline_tokens_and_bare_url_detection() {
    let source = "hé 中🙂 \\* **hé🙂** [中](../中.md) `é` https://example.com/中, h🙂";
    let doc = here(source);
    assert_eq!(
        plain(&doc.blocks[0].runs),
        "hé 中🙂 * hé🙂 中 é https://example.com/中, h🙂"
    );
    assert!(doc.blocks[0]
        .runs
        .iter()
        .any(|r| r.bold && r.text == "hé🙂"));
    assert!(doc.blocks[0].runs.iter().any(|r| r.href == "../中.md"));
    assert!(doc.blocks[0]
        .runs
        .iter()
        .any(|r| r.href == "https://example.com/中"));
}

#[test]
fn rules_require_three_matching_markers_and_only_spaces_between_them() {
    for source in ["---", "* * *", "_ _ _ _", "  - - -  "] {
        assert_eq!(here(source).blocks[0].kind, Kind::Rule, "{source:?}");
    }
    for source in ["--", "-*-", "___é", "---\t", "prose ---"] {
        assert!(
            here(source).blocks.iter().all(|b| b.kind != Kind::Rule),
            "{source:?}"
        );
    }
}

#[test]
fn the_outline_is_every_heading_in_order() {
    let doc = here("# One\n\ntext\n\n## Two\n\n### Three\n");
    let outline = doc.outline();
    assert_eq!(outline.len(), 3);
    assert_eq!(outline[0], (1, "One".to_string(), 0));
    assert_eq!(outline[2].0, 3);
    assert_eq!(outline[2].1, "Three");
}

/// The corpus this reader exists for. Not a golden file — a shape check that
/// would fail loudly if block detection regressed on real documents.
#[test]
fn this_repositorys_own_documents_parse_into_blocks() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs");
    let mut files = 0;
    let mut headings = 0;
    let mut tables = 0;
    let entries = std::fs::read_dir(&root)
        .expect("docs/")
        .chain(std::fs::read_dir(root.join("recipes")).expect("docs/recipes/"));
    for entry in entries {
        let path = entry.expect("entry").path();
        // The lookup card is one untitled screen by design.
        if path.extension().and_then(|e| e.to_str()) != Some("md") || path.ends_with("card.md") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read");
        let doc = here(&source);
        assert!(
            !doc.blocks.is_empty(),
            "{} parsed to nothing",
            path.display()
        );
        // Every document opens with its own `# ` title.
        assert!(!doc.title.is_empty(), "{} has no title", path.display());
        headings += doc
            .blocks
            .iter()
            .filter(|b| b.kind == Kind::Heading)
            .count();
        tables += doc
            .blocks
            .iter()
            .filter(|b| b.kind == Kind::TableRow)
            .count();
        files += 1;
    }
    assert!(files > 20, "expected the corpus, found {files} documents");
    assert!(headings > 200, "expected headings, found {headings}");
    assert!(tables > 20, "expected tables, found {tables}");
}

#[test]
fn grouped_table_cells_retain_separator_alignment() {
    use exact_plan::Value;
    let Value::List(blocks) = markdown_parse::value::into_blocks_with_tables(here(
        "| Left | Right | Center |\n| :--- | ---: | :---: |\n| a | b | c |\n",
    )) else {
        panic!("blocks")
    };
    let Value::Record(table) = &blocks[0] else {
        panic!("table")
    };
    let Value::List(cells) = &table[8] else {
        panic!("cells")
    };
    for (cell, align) in cells.iter().zip(["left", "right", "center"].iter().cycle()) {
        let Value::Record(fields) = cell else {
            panic!("cell")
        };
        assert_eq!(fields.get(2), Some(&Value::str(align)));
    }
}

#[test]
fn a_long_tables_sizers_refer_to_its_cells_by_index() {
    use exact_plan::Value;
    fn fields(value: &Value) -> &[Value] {
        match value {
            Value::Record(values) | Value::List(values) => values,
            _ => panic!("expected fields or items"),
        }
    }
    fn refs(line: &Value) -> Vec<usize> {
        fields(&fields(line)[1])
            .iter()
            .map(|at| at.as_number().unwrap() as usize)
            .collect()
    }
    // Past 8,192 rows a table has several grids, and so sizers.
    let mut source = String::from("| A | B |\n| - | - |\n");
    for row in 0..9_000 {
        source.push_str(&format!("| r{row} | **x** y |\n"));
    }
    source.push_str("| r0 | `z` |\n");
    let blocks = markdown_parse::value::into_blocks_with_tables(here(&source));
    let table = fields(&fields(&blocks)[0]);
    let sizers = fields(&table[14]);
    assert_eq!(sizers.len(), 2);

    // Single-run cells join by style; a repeated cell is referred to once.
    let a = fields(&fields(&sizers[0])[1]);
    assert_eq!(a.len(), 2);
    assert_eq!(refs(&a[0]), [0]);
    assert_eq!(fields(&a[0])[3], Value::Number(700.));
    assert_eq!(
        refs(&a[1]),
        (1..=9_000).map(|row| row * 2).collect::<Vec<_>>()
    );
    assert_eq!(fields(&a[1])[2], Value::Number(-1.));
    assert_eq!(fields(&a[1])[3], Value::Number(400.));

    // A cell of several runs is its own line, by `cell`.
    let b = fields(&fields(&sizers[1])[1]);
    assert_eq!(b.len(), 3);
    assert_eq!(refs(&b[0]), [1]);
    assert!(refs(&b[1]).is_empty());
    assert_eq!(fields(&b[1])[2], Value::Number(3.));
    assert_eq!(refs(&b[2]), [9_001 * 2 + 1]);
    assert_eq!(fields(&b[2])[5], Value::Bool(true));
}
