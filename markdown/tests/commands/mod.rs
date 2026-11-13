//! Source edits and their resulting UTF-16 selections.

use exact_markdown::*;

#[test]
fn inline_formats_leave_block_prefixes_outside_the_markers() {
    for (command, marker) in [
        (Command::Bold, "**"),
        (Command::Italic, "*"),
        (Command::Code, "`"),
        (Command::Strike, "~~"),
    ] {
        for prefix in ["# ", "- ", "1. ", "- [ ] ", "> ", "  - ", "> - [x] "] {
            assert_eq!(
                run(&format!("|{prefix}one\n{prefix}two|"), command.clone()),
                format!("{prefix}{marker}|one{marker}\n{prefix}{marker}two|{marker}"),
                "{command:?}, {prefix:?}"
            );
        }
        assert_eq!(
            run("- o|ne\n- tw|o", command.clone()),
            format!("- o{marker}|ne{marker}\n- {marker}tw|{marker}o")
        );
        assert_eq!(
            run("|# Title\n\ntext|", command.clone()),
            format!("# {marker}|Title{marker}\n\n{marker}text|{marker}")
        );
        assert_eq!(run("|# |Title", command.clone()), "|# |Title");
        assert_eq!(
            run("😀 |é\n- 字|", command.clone()),
            format!("😀 {marker}|é{marker}\n- {marker}字|{marker}")
        );
        let source = "- one\n- ";
        let edit = edit(source, Range::new(0, 8), command);
        assert_eq!(edit.apply(source), format!("- {marker}one{marker}\n- "));
    }
}

#[test]
fn return_preserves_inline_constructs_across_block_breaks() {
    for (before, after) in [
        ("- **|Note:** details", "- \n- |**Note:** details"),
        (
            "- [|docs](https://example.com) here",
            "- \n- |[docs](https://example.com) here",
        ),
        ("- **bo|ld**", "- **bo**\n- **|ld**"),
        ("- **a|😀b**", "- **a**\n- **|😀b**"),
        (
            "# [li|nk](https://example.com) title",
            "# [li](https://example.com)\n[|nk](https://example.com) title",
        ),
        ("- ***bo|th***", "- ***bo***\n- ***|th***"),
        ("- [**bo|ld**](u)", "- [**bo**](u)\n- [**|ld**](u)"),
        ("- **bo|ld**\n- next", "- **bo**\n- **|ld**\n- next"),
        ("- **bold|**", "- **bold**\n- |"),
        ("- **https://x|**", "- **https://x**\n- |"),
        ("- **|https://x**", "- \n- |**https://x**"),
        ("- [docs|](u)", "- [docs](u)\n- |"),
        ("- **one |two**", "- **one** \n- **|two**"),
        ("- **one| two**", "- **one**\n- | **two**"),
        ("- **a|bc|d**", "- **a**\n- **|d**"),
        ("1. **a|b**\n2. next", "1. **a**\n2. **|b**\n3. next"),
        ("**bo|ld**", "**bo\n|ld**"),
    ] {
        assert_eq!(run(before, Command::Newline), after, "{before}");
    }
}

#[test]
fn return_before_code_whitespace_keeps_the_nesting_order() {
    assert_eq!(
        run("- **`a| b`**", Command::Newline),
        "- **`a`**\n- **`| b`**"
    );
}

#[test]
fn return_after_code_whitespace_keeps_the_nesting_order() {
    assert_eq!(
        run("- **`a |b`**", Command::Newline),
        "- **`a `**\n- **`|b`**"
    );
}

#[test]
fn return_splits_each_inline_format_in_lists_quotes_and_headings() {
    for (prefix, next) in [
        ("- ", "- "),
        ("9) ", "10) "),
        ("- [x] ", "- [ ] "),
        ("  + ", "  + "),
        ("> ", "> "),
        ("> - ", "> - "),
        ("# ", ""),
    ] {
        for (open, close) in [
            ("**", "**"),
            ("_", "_"),
            ("`", "`"),
            ("~~", "~~"),
            ("[", "](<https://example.com/a(b)> \"title\")"),
        ] {
            let before = format!("{prefix}{open}a|😀b{close}");
            let after = format!("{prefix}{open}a{close}\n{next}{open}|😀b{close}");
            assert_eq!(run(&before, Command::Newline), after, "{before}");
        }
    }
}

#[test]
fn return_replaces_a_selection_through_an_inline_edge() {
    for (before, after) in [
        ("- [do|cs|](u)", "- [do](u)\n- |"),
        ("- [|do|cs](u)", "- \n- [|cs](u)"),
        ("- [|docs|](u)", "- \n- |"),
        ("- **bo|ld|**", "- **bo**\n- |"),
        ("- [**bo|ld|**](u)", "- [**bo**](u)\n- |"),
        ("- [do|cs](u) and [wo|rd](v)", "- [do](u)\n- [|rd](v)"),
        // A selection from the list marker removes it and still splits the link.
        (
            "|- [do|cs](https://example.com)",
            "\n[|cs](https://example.com)",
        ),
        ("|- **bo|ld**", "\n**|ld**"),
    ] {
        assert_eq!(run(before, Command::Newline), after, "{before}");
    }
}

#[test]
fn return_preserves_an_autolinks_destination() {
    for (before, after, visible, href) in [
        (
            "- <https://exa|mple.com>",
            "- [https\\://exa](https://example.com)\n- [|mple.com](https://example.com)",
            "• https://exa\n• mple.com",
            "https://example.com",
        ),
        (
            "- <a@b|.com>",
            "- [a@b](mailto:a@b.com)\n- [|.com](mailto:a@b.com)",
            "• a@b\n• .com",
            "mailto:a@b.com",
        ),
        (
            "- <https://x/|a_b>",
            "- [https\\://x/](https://x/a_b)\n- [|a\\_b](https://x/a_b)",
            "• https://x/\n• a_b",
            "https://x/a_b",
        ),
    ] {
        assert_eq!(run(before, Command::Newline), after, "{before}");
        let source = after.replace('|', "");
        assert_eq!(plain(&source), visible);
        let styled = style(&source, None);
        let links: Vec<_> = styled
            .spans
            .iter()
            .filter(|s| s.style & LINK != 0)
            .collect();
        assert!(links.len() >= 2);
        assert!(links.iter().all(|s| s.href == href));
    }
}

/// Runs a command where `|` marks the selection and returns the same notation.
pub(super) fn run(marked: &str, command: Command) -> String {
    let first = marked.find('|').expect("a caret");
    let rest = marked[first + 1..].find('|');
    let source: String = marked.replace('|', "");
    let start = marked[..first].encode_utf16().count() as u32;
    let end = rest.map_or(start, |r| {
        start + marked[first + 1..first + 1 + r].encode_utf16().count() as u32
    });
    let edit = edit(&source, Range::new(start, end), command);
    let after = edit.apply(&source);
    let units: Vec<u16> = after.encode_utf16().collect();
    let cut = |a: u32, b: u32| String::from_utf16(&units[a as usize..b as usize]).unwrap();
    let sel = edit.selection;
    if sel.start == sel.end {
        format!(
            "{}|{}",
            cut(0, sel.start),
            cut(sel.start, units.len() as u32)
        )
    } else {
        format!(
            "{}|{}|{}",
            cut(0, sel.start),
            cut(sel.start, sel.end),
            cut(sel.end, units.len() as u32)
        )
    }
}

#[test]
fn inline_commands_toggle() {
    assert_eq!(
        run("make |this| bold", Command::Bold),
        "make **|this|** bold"
    );
    assert_eq!(run("make **th|is** bold", Command::Bold), "make th|is bold");
    assert_eq!(
        run("make **|this|** bold", Command::Bold),
        "make |this| bold"
    );
    assert_eq!(run("caret |here", Command::Italic), "caret *|*here");
    assert_eq!(run("caret *|*here", Command::Italic), "caret |here");
    assert_eq!(run("pad | this |out", Command::Code), "pad  `|this|` out");
    assert_eq!(run("a |b\nc| d", Command::Strike), "a ~~|b~~\n~~c|~~ d");
    assert_eq!(run("***b|oth***", Command::Italic), "**b|oth**");
    assert_eq!(
        run("a |word| here", Command::Link(String::new())),
        "a [word](|) here"
    );
    assert_eq!(
        run("a |word| here", Command::Link("https://x.dev".into())),
        "a [|word|](https://x.dev) here"
    );
    assert_eq!(
        run(
            "a [wo|rd](https://x.dev) here",
            Command::Link(String::new())
        ),
        "a wo|rd here"
    );
}

#[test]
fn block_commands_toggle_lines() {
    assert_eq!(run("a tit|le", Command::Heading(2)), "## a tit|le");
    assert_eq!(run("## a tit|le", Command::Heading(2)), "a tit|le");
    assert_eq!(run("## a tit|le", Command::Heading(1)), "# a tit|le");
    assert_eq!(run("|one\n\ntwo|", Command::Bullet), "- |one\n\n- two|");
    assert_eq!(run("- |one\n- two|", Command::Ordered), "1. |one\n2. two|");
    assert_eq!(run("1. one|", Command::Task), "- [ ] one|");
    assert_eq!(run("- [ ] one|", Command::ToggleTask), "- [x] one|");
    assert_eq!(run("- [x] one|", Command::ToggleTask), "- [ ] one|");
    assert_eq!(run("|", Command::Bullet), "- |");
    assert_eq!(run("|a\nb|", Command::Quote), "> |a\n> b|");
    assert_eq!(run("> a|\n> b", Command::Quote), "a|\n> b");
    assert_eq!(run("- a|", Command::Indent), "  - a|");
    assert_eq!(run("  - a|\n\t- b", Command::Outdent), "- a|\n\t- b");
    // In a quote the indent goes after the `>` markers, where the item's depth is counted.
    assert_eq!(run("> - a\n> - b|", Command::Indent), "> - a\n>   - b|");
    assert_eq!(run("> - a\n>   - b|", Command::Outdent), "> - a\n> - b|");
    assert_eq!(run("> > - b|", Command::Indent), "> >   - b|");
    let source = "> - a\n>   - b";
    let depths: Vec<(u8, u8)> = style(source, None)
        .paragraphs
        .iter()
        .map(|p| (p.depth, p.quote))
        .collect();
    assert_eq!(depths, [(0, 1), (1, 1)]);
    assert_eq!(
        run("x\n|let a;\nlet b;|\ny", Command::CodeBlock),
        "x\n```\n|let a;\nlet b;|\n```\ny"
    );
    assert_eq!(
        run("x\n```\nlet |a;\n```\ny", Command::CodeBlock),
        "x\nlet |a;\ny"
    );
}

#[test]
fn return_continues_a_list_and_ends_it_on_an_empty_item() {
    assert_eq!(run("- one|", Command::Newline), "- one\n- |");
    assert_eq!(run("- one\n- |", Command::Newline), "- one\n|");
    assert_eq!(run("  * a|b", Command::Newline), "  * a\n  * |b");
    assert_eq!(run("- [x] done|", Command::Newline), "- [x] done\n- [ ] |");
    assert_eq!(run("> said|", Command::Newline), "> said\n> |");
    assert_eq!(run("> - item|", Command::Newline), "> - item\n> - |");
    assert_eq!(
        run("1. a|\n2. b\n   - x\n3. c\n\n1. other", Command::Newline),
        "1. a\n2. |\n3. b\n   - x\n4. c\n\n1. other"
    );
    // A lazy line continues the item above it (CommonMark 0.31.2 §5.2's
    // paragraph continuation text): the items after it still count on. A
    // fence or a rule at the item's indent starts a block, and ends the list.
    assert_eq!(
        run("1. hello|\nworld\n2. next", Command::Newline),
        "1. hello\n2. |\nworld\n3. next"
    );
    assert_eq!(
        run("1. a|\n2. b\nlazy\n3. c", Command::Newline),
        "1. a\n2. |\n3. b\nlazy\n4. c"
    );
    for end in ["```\n2. x\n```", "---\n2. x"] {
        let source = format!("1. a|\n2. b\n{end}");
        let after = format!("1. a\n2. |\n3. b\n{end}");
        assert_eq!(run(&source, Command::Newline), after, "{source}");
    }
    assert_eq!(run("9) nine|", Command::Newline), "9) nine\n10) |");
    assert_eq!(run("plain|", Command::Newline), "plain\n|");
    assert_eq!(
        run("```\n    deep|\n```", Command::Newline),
        "```\n    deep\n    |\n```"
    );
    assert_eq!(run("-| one", Command::Newline), "-\n| one");
}

#[test]
fn a_footnote_command_numbers_itself_and_opens_its_definition() {
    assert_eq!(
        run("A claim|.", Command::Footnote),
        "A claim[^1].\n\n[^1]: |"
    );
    assert_eq!(
        run("A[^1] claim|.\n\n[^1]: one\n", Command::Footnote),
        "A[^1] claim[^2].\n\n[^1]: one\n\n[^2]: |"
    );
    // At the end of the source the definition starts a line of its own.
    for (before, after) in [
        ("Text\n\n|", "Text\n\n[^1]\n\n[^1]: |"),
        ("Text\n|", "Text\n[^1]\n\n[^1]: |"),
        ("Text|", "Text[^1]\n\n[^1]: |"),
        ("|", "[^1]\n\n[^1]: |"),
    ] {
        assert_eq!(run(before, Command::Footnote), after, "{before:?}");
        let source = after.replace('|', "");
        let notes = style(&source, None).footnotes;
        assert_eq!(notes.len(), 1, "{source:?}");
        assert_eq!(notes[0].references.len(), 1, "{source:?}");
        assert!(notes[0].definition.is_some(), "{source:?}");
    }
}
