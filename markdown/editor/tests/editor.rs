//! The editing rules, driven the way a browser drives them: `beforeinput`
//! events, and plain typing the platform performs itself and reports back.

use exact_markdown_editor::{view::HIDDEN, Deco, Editor, Input, Outcome, Selected, SelectionMode};

fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[test]
fn a_drag_moves_source_through_cleanup_as_one_undo_step() {
    for (source, from, to, at, dropped, expected) in [
        ("alpha beta gamma", 6, 10, 16, "beta", "alpha  gammabeta"),
        ("alpha beta gamma", 6, 10, 0, "beta", "betaalpha  gamma"),
        (
            "alpha **beta** gamma",
            8,
            12,
            20,
            "beta",
            "alpha  gammabeta",
        ),
        (
            "[beta](url) **gamma**",
            1,
            5,
            19,
            "beta",
            "[](url) **gammabeta**",
        ),
        ("😀 beta\ngamma", 3, 7, 13, "beta", "😀 \ngammabeta"),
        ("beta beta beta", 0, 4, 14, "beta", " beta betabeta"),
    ] {
        let mut e = Editor::new();
        e.load(&u(source));
        e.set_words(&u("directions"));
        assert!(e.move_text(from, to, at, &u(dropped), 0.0).source);
        assert_eq!(e.source(), u(expected), "{source}");
        let selection = e.selection();
        e.command("undo", "", 1.0);
        assert_eq!(e.source(), u(source));
        assert_eq!(e.selection(), (from, to));
        assert_eq!(e.words_back(), Some(u("directions").as_slice()));
        e.command("redo", "", 2.0);
        assert_eq!(e.source(), u(expected));
        assert_eq!(e.selection(), selection);
        e.command("undo", "", 3.0);
        assert!(!e.command("undo", "", 4.0).source);
    }
    let mut e = Editor::new();
    e.load(&u("alpha beta gamma"));
    assert!(!e.move_text(6, 10, 8, &u("beta"), 0.0).source);
    assert_eq!(e.source(), u("alpha beta gamma"));
    assert!(!e.command("undo", "", 1.0).source);
}

struct Page {
    e: Editor,
    now: f64,
}

impl Page {
    fn open(text: &str) -> Self {
        let mut e = Editor::new();
        e.load(&u(text));
        Self { e, now: 0.0 }
    }

    fn source(&self) -> String {
        String::from_utf16(self.e.source()).unwrap()
    }

    fn tick(&mut self) -> f64 {
        self.now += 40.0;
        self.now
    }

    /// A click or arrow key: the platform puts the caret at `p`.
    fn caret(&mut self, p: u32) {
        if self.e.select(p, p, None) == Selected::Place {
            // The host places the snapped caret; its echo is ignored.
            let (a, b) = self.e.selection();
            assert_eq!(self.e.select(a, b, Some((a, b))), Selected::Ignore);
        }
    }

    fn input(&mut self, input: Input, data: &str) -> Outcome {
        let now = self.tick();
        self.e.before_input(input, &u(data), None, now)
    }

    /// Typing, a key at a time; a key the editor leaves to the platform is
    /// inserted at the caret and read back, as a contentEditable does.
    fn type_(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                self.input(Input::Paragraph, "");
                continue;
            }
            let data = ch.to_string();
            if self.input(Input::Text, &data) == Outcome::Native {
                self.platform_types(&data);
            }
        }
    }

    fn platform_types(&mut self, data: &str) {
        let (a, b) = self.e.selection();
        let mut text = self.e.source().to_vec();
        let data = u(data);
        text.splice(a as usize..b as usize, data.iter().copied());
        let at = a + data.len() as u32;
        let now = self.tick();
        self.e.reconcile(&text, Some((at, at)), now);
    }

    fn backspace(&mut self) {
        assert!(matches!(
            self.input(Input::Backward, ""),
            Outcome::Handled(_)
        ));
    }

    fn tool(&mut self, name: &str) {
        let now = self.tick();
        self.e.command(name, "", now);
    }
}

#[test]
fn pending_bold_then_plain() {
    let mut p = Page::open("");
    p.type_("Hello ");
    p.tool("bold");
    p.type_("world");
    p.tool("bold");
    p.type_(" again");
    assert_eq!(p.source(), "Hello **world** again");
    assert_eq!(p.e.visible_text(), "Hello world again");
}

#[test]
fn bold_input_event_toggles_pending_bold() {
    let mut p = Page::open("");
    p.type_("a ");
    p.input(Input::Bold, "");
    p.type_("b");
    p.input(Input::Bold, "");
    p.type_(" c");
    assert_eq!(p.source(), "a **b** c");
}

#[test]
fn a_space_at_the_end_of_bold_goes_outside() {
    let mut p = Page::open("");
    p.tool("bold");
    p.type_("bold");
    p.type_(" plain");
    assert_eq!(p.source(), "**bold** plain");
}

#[test]
fn typing_after_a_typed_closer_stays_outside() {
    let mut p = Page::open("");
    p.type_("**hi**");
    p.type_("x");
    assert_eq!(p.source(), "**hi**x");
}

#[test]
fn heading_and_list_shortcuts_continue_and_end_lists() {
    let mut p = Page::open("");
    p.type_("# Title\n- one\n");
    p.type_("two\n\n");
    p.type_("after");
    assert!(
        p.source().starts_with("# Title\n- one\n- two\n"),
        "{:?}",
        p.source()
    );
    assert!(p.source().ends_with("after"), "{:?}", p.source());
}

#[test]
fn backspace_deletes_letters_never_markers() {
    let mut p = Page::open("plain **bold** end");
    for _ in 0..4 {
        p.backspace();
    }
    assert_eq!(p.source(), "plain **bold**");
    p.backspace();
    assert_eq!(p.source(), "plain **bol**");
}

#[test]
fn return_in_formatted_items_preserves_text_formats_and_caret() {
    for (source, caret, after, visible, expected_caret) in [
        (
            "- [docs](https://example.com) here",
            3,
            "- \n- [docs](https://example.com) here",
            "\ndocs here",
            5,
        ),
        (
            "- **Note:** details",
            4,
            "- \n- **Note:** details",
            "\nNote: details",
            5,
        ),
        ("- **bold**", 6, "- **bo**\n- **ld**", "bo\nld", 13),
        ("- **a😀b**", 5, "- **a**\n- **😀b**", "a\n😀b", 12),
        (
            "# [link](https://example.com) title",
            5,
            "# [li](https://example.com)\n[nk](https://example.com) title",
            "li\nnk title",
            29,
        ),
    ] {
        let mut p = Page::open(source);
        p.caret(caret);
        let before_sel = p.e.selection();
        p.input(Input::Paragraph, "");
        assert_eq!(p.source(), after, "{source}");
        assert_eq!(p.e.visible_text(), visible, "{source}");
        assert_eq!(
            p.e.selection(),
            (expected_caret, expected_caret),
            "{source}"
        );
        p.tool("undo");
        assert_eq!(p.source(), source);
        assert_eq!(p.e.selection(), before_sel);
        p.tool("redo");
        assert_eq!(p.source(), after);
        assert_eq!(p.e.selection(), (expected_caret, expected_caret));
        p.type_("X");
        let mut typed = u(after);
        typed.insert(expected_caret as usize, b'X' as u16);
        assert_eq!(p.e.source(), typed, "{source}");
        p.tool("undo");
        assert_eq!(p.source(), after);
    }
}

#[test]
fn return_and_code_beside_a_backtick_pad_the_span_and_undo_in_one_step() {
    // Return after a literal backtick: the left half's padding is hidden
    // syntax, and the caret starts the right half's text.
    let source = "- ``a`b``";
    let after = "- `` a` ``\n- ``b``";
    let mut p = Page::open(source);
    p.caret(6);
    let before = p.e.selection();
    p.input(Input::Paragraph, "");
    assert_eq!(p.source(), after);
    assert_eq!(p.e.visible_text(), "a`\nb");
    assert_eq!(p.e.selection(), (15, 15));
    p.tool("undo");
    assert_eq!(p.source(), source);
    assert_eq!(p.e.selection(), before);
    p.tool("redo");
    assert_eq!(p.source(), after);
    assert_eq!(p.e.selection(), (15, 15));
    p.type_("X");
    assert_eq!(p.source(), "- `` a` ``\n- ``Xb``");

    // Code on text that begins with a backtick pads it; one undo takes it back.
    let mut p = Page::open("say `a");
    p.e.select(4, 6, None);
    p.tool("code");
    assert_eq!(p.source(), "say `` `a ``");
    assert_eq!(p.e.visible_text(), "say `a");
    assert_eq!(p.e.selection(), (4, 9));
    p.tool("undo");
    assert_eq!(p.source(), "say `a");
    assert_eq!(p.e.selection(), (4, 6));

    // A caret the app puts in the hidden padding is drawn at a visible edge.
    let mut p = Page::open("x ` a ` y");
    p.e.set_selection(3, 3);
    let (s, e) = p.e.selection();
    assert_eq!(s, e);
    assert!(s == 2 || s == 4, "{s}");
}

#[test]
fn a_return_caret_survives_orphan_cleanup() {
    let mut p = Page::open("**ab** cd");
    p.e.select(3, 8, None);
    p.input(Input::Paragraph, "");
    assert_eq!(p.source(), "a\nd");
    assert_eq!(p.e.selection(), (2, 2));
}

#[test]
fn backspace_after_return_removes_only_the_block_prefix() {
    for (source, caret, after, visible) in [
        ("- **bold**", 6, "- **bo**\n**ld**", "bo\nld"),
        ("- [docs](u)", 5, "- [do](u)\n[cs](u)", "do\ncs"),
        ("- **`a b`**", 7, "- **`a `**\n**`b`**", "a \nb"),
        ("- **a😀b**", 5, "- **a**\n**😀b**", "a\n😀b"),
        ("> - [x] **bold**", 12, "> - [x] **bo**\n**ld**", "bo\nld"),
    ] {
        let mut p = Page::open(source);
        p.caret(caret);
        p.input(Input::Paragraph, "");
        let split = p.source();
        p.backspace();
        assert_eq!(p.source(), after, "{source}");
        assert_eq!(p.e.visible_text(), visible, "{source}");
        p.tool("undo");
        assert_eq!(p.source(), split);
        p.tool("redo");
        assert_eq!(p.source(), after);
    }
}

#[test]
fn backspace_at_formatted_block_starts_keeps_inline_syntax() {
    for prefix in ["# ", "- ", "1. ", "> ", "> - [x] ", "  - "] {
        for content in ["**text**", "`code`", "[label](target)", "\\*literal"] {
            let source = format!("{prefix}{content}");
            let mut p = Page::open(&source);
            p.caret(prefix.len() as u32);
            p.backspace();
            assert_eq!(p.source(), content, "{source}");
        }
    }
}

#[test]
fn return_in_link_labels_keeps_destinations_and_selected_edges() {
    for (source, start, end, after, visible, caret) in [
        ("- [docs](u)", 5, 7, "- [do](u)\n- ", "do\n", 12),
        ("- [docs](u)", 3, 5, "- \n- [cs](u)", "\ncs", 6),
        ("- [docs](u)", 3, 7, "- \n- ", "\n", 5),
        ("- [**bold**](u)", 7, 9, "- [**bo**](u)\n- ", "bo\n", 16),
        (
            "- <https://example.com>",
            14,
            14,
            "- [https\\://exa](https://example.com)\n- [mple.com](https://example.com)",
            "https://exa\nmple.com",
            41,
        ),
    ] {
        let mut p = Page::open(source);
        p.e.set_selection(start, end);
        p.input(Input::Paragraph, "");
        assert_eq!(p.source(), after, "{source}");
        assert_eq!(p.e.visible_text(), visible, "{source}");
        assert_eq!(p.e.selection(), (caret, caret), "{source}");
        p.type_("X");
        let mut typed = u(after);
        typed.insert(caret as usize, b'X' as u16);
        assert_eq!(p.e.source(), typed, "{source}");
        p.tool("undo");
        p.tool("undo");
        assert_eq!(p.source(), source);
    }
}

#[test]
fn formatting_selected_lines_preserves_their_blocks() {
    for command in ["bold", "italic", "code", "strike"] {
        let mut p = Page::open("# Title\n\n- one\n- two");
        p.e.select_all();
        p.tool(command);
        assert_eq!(p.e.visible_text(), "Title\n\none\ntwo");
        assert_eq!(p.e.lines()[2].deco, Some(Deco::Bullet));
        assert_eq!(p.e.lines()[3].deco, Some(Deco::Bullet));
        p.tool("undo");
        assert_eq!(p.source(), "# Title\n\n- one\n- two");
    }
}

#[test]
fn emptying_a_bold_word_removes_its_markers() {
    let mut p = Page::open("a **b** c");
    for _ in 0..3 {
        p.backspace();
    }
    assert_eq!(p.source(), "a ");
}

#[test]
fn emptying_inline_text_removes_its_markers_without_changing_other_blocks() {
    let before = "intro😀\n\n";
    let after = "\n\n**next paragraph**\n\n---\n\n```rs\ncode\n```";
    for (open, close) in [
        ("**", "**"),
        ("__", "__"),
        ("***", "***"),
        ("~~", "~~"),
        ("``", "``"),
        ("*", "*"),
        ("_", "_"),
        ("`", "`"),
        ("~~**_`", "`_**~~"),
        ("**~~_`", "`_~~**"),
    ] {
        for (prefix, remaining) in [
            ("", ""),
            ("> ", ""),
            ("- ", "- "),
            ("# ", "# "),
            ("a ", "a "),
        ] {
            for input in [Input::Backward, Input::Forward, Input::OtherBackward] {
                let word = if input == Input::OtherBackward {
                    "word"
                } else {
                    "x"
                };
                let source = format!("{before}{prefix}{open}{word}{close}{after}");
                let mut p = Page::open(&source);
                let caret = u(&format!("{before}{prefix}")).len() as u32;
                let start = caret + open.len() as u32;
                let end = start + word.len() as u32;
                match input {
                    Input::Backward => p.caret(end),
                    Input::Forward => p.caret(start),
                    _ => {
                        p.e.set_selection(start, end);
                    }
                }
                let selection = p.e.selection();
                assert!(matches!(p.input(input, ""), Outcome::Handled(_)));
                let expected = format!("{before}{remaining}{after}");
                let caret = u(&format!("{before}{remaining}")).len() as u32;
                assert_eq!(p.source(), expected, "{input:?}: {source}");
                assert_eq!(p.e.selection(), (caret, caret));
                assert_ne!(p.e.lines()[2].deco, Some(Deco::Rule));
                assert_eq!(p.e.lines()[4].flags, 0);
                p.tool("undo");
                assert_eq!(p.source(), source);
                assert_eq!(p.e.selection(), selection);
                p.tool("redo");
                assert_eq!(p.source(), expected);
                assert_eq!(p.e.selection(), (caret, caret));
                p.type_("z");
                assert_eq!(p.source(), format!("{before}{remaining}z{after}"));
            }
        }
    }
}

#[test]
fn emptying_inline_text_preserves_following_formatting_on_the_same_line() {
    for (open, close) in [
        ("~~", "~~"),
        ("``", "``"),
        ("~~**_`", "`_**~~"),
        ("**~~_`", "`_~~**"),
    ] {
        for prefix in ["", " ", "   "] {
            for later in ["**later**", "[later](https://example.com)", "~~later~~"] {
                for input in [Input::Backward, Input::Forward, Input::OtherBackward] {
                    let source = format!("{prefix}{open}x{close} {later}");
                    let expected = format!("{prefix} {later}");
                    let mut p = Page::open(&source);
                    let start = (prefix.len() + open.len()) as u32;
                    match input {
                        Input::Backward => p.caret(start + 1),
                        Input::Forward => p.caret(start),
                        _ => {
                            p.e.set_selection(start, start + 1);
                        }
                    }
                    let selection = p.e.selection();
                    assert!(matches!(p.input(input, ""), Outcome::Handled(_)));
                    let caret = prefix.len() as u32;
                    assert_eq!(p.source(), expected, "{input:?}: {source}");
                    assert_eq!(p.e.selection(), (caret, caret));
                    p.tool("undo");
                    assert_eq!(p.source(), source);
                    assert_eq!(p.e.selection(), selection);
                    p.tool("redo");
                    assert_eq!(p.source(), expected);
                    p.type_("z");
                    assert_eq!(p.source(), format!("{prefix}z {later}"));
                }
            }
        }
    }
}

#[test]
fn emptying_a_line_preserves_syntax_on_the_next_line() {
    for (source, expected, visible) in [
        ("> x\n> keep", "\n> keep", "\nkeep"),
        ("> **x**\n> keep", "\n> keep", "\nkeep"),
        ("`x`\n``\nnext", "\n``\nnext", "\n``\nnext"),
        ("**`x`**\n``\nnext", "\n``\nnext", "\n``\nnext"),
    ] {
        for input in [Input::Backward, Input::Forward, Input::OtherBackward] {
            let mut p = Page::open(source);
            let start = source.find('x').unwrap() as u32;
            match input {
                Input::Backward => p.caret(start + 1),
                Input::Forward => p.caret(start),
                _ => {
                    p.e.set_selection(start, start + 1);
                }
            }
            let selection = p.e.selection();
            assert!(matches!(p.input(input, ""), Outcome::Handled(_)));
            assert_eq!(p.source(), expected, "{input:?}: {source}");
            assert_eq!(p.e.visible_text(), visible);
            assert_eq!(p.e.selection(), (0, 0));
            p.tool("undo");
            assert_eq!(p.source(), source);
            assert_eq!(p.e.selection(), selection);
            p.tool("redo");
            assert_eq!(p.source(), expected);
            p.type_("z");
            assert_eq!(p.source(), format!("z{expected}"));
        }
    }
}

#[test]
fn emptying_a_line_typed_in_bold_leaves_a_plain_line() {
    let mut p = Page::open("");
    p.tool("bold");
    p.type_("hi");
    assert_eq!(p.source(), "**hi**");
    p.backspace();
    assert_eq!(p.source(), "**h**");
    p.backspace();
    assert_eq!(p.source(), "");
    assert_eq!(p.e.selection(), (0, 0));
    assert_eq!(p.e.lines()[0].deco, None);
    p.type_("x");
    assert_eq!(p.source(), "x");
}

#[test]
fn backspace_at_a_heading_start_drops_the_heading() {
    let mut p = Page::open("para\n# Title");
    p.caret(7);
    p.backspace();
    assert_eq!(p.source(), "para\nTitle");
}

#[test]
fn backspace_in_an_empty_item_drops_the_bullet() {
    let mut p = Page::open("- one\n- two");
    for _ in 0..3 {
        p.backspace();
    }
    assert_eq!(p.source(), "- one\n- ");
    p.backspace();
    assert_eq!(p.source(), "- one\n");
}

#[test]
fn backspace_after_a_link_deletes_its_text() {
    let mut p = Page::open("see [link](https://x.com)");
    p.backspace();
    assert_eq!(p.source(), "see [lin](https://x.com)");
}

#[test]
fn select_all_and_type_replaces_everything() {
    for source in [
        "plain **bold** and *it*",
        "# Heading\n\n[link](https://example.com)\n\n```js\ncode\n```",
        "- [x] done\n\n> quoted",
    ] {
        let mut p = Page::open(source);
        p.e.select_all();
        p.type_("Z");
        assert_eq!(p.source(), "Z");
        p.tool("undo");
        assert_eq!(p.source(), source);
        p.tool("redo");
        assert_eq!(p.source(), "Z");
    }
}

#[test]
fn replacing_a_selected_bold_word_keeps_it_bold() {
    let mut p = Page::open("plain **bold** end");
    p.e.select(8, 12, None);
    p.type_("X");
    assert_eq!(p.source(), "plain **X** end");
}

#[test]
fn a_click_after_a_bold_word_types_into_it() {
    let mut p = Page::open("plain **bold** end");
    p.caret(12);
    p.type_("!");
    assert_eq!(p.source(), "plain **bold!** end");
}

#[test]
fn a_click_before_a_bold_word_types_before_it() {
    let mut p = Page::open("plain **bold** end");
    p.caret(8);
    assert_eq!(p.e.selection(), (6, 6), "the caret snaps before the opener");
    p.type_("x");
    assert_eq!(p.source(), "plain x**bold** end");
}

#[test]
fn a_caret_past_hidden_markers_types_between_letters() {
    // Four right arrows from the start of `ab cd ef` land after `c`.
    let mut p = Page::open("ab **cd** ef");
    p.caret(6);
    p.type_("X");
    assert_eq!(p.source(), "ab **cXd** ef");
}

#[test]
fn a_task_box_toggles() {
    let mut p = Page::open("- [ ] task");
    let now = p.tick();
    p.e.toggle_task(0, now);
    assert_eq!(p.source(), "- [x] task");
}

#[test]
fn one_undo_removes_a_formatted_insertion() {
    let mut p = Page::open("");
    p.type_("Hello");
    p.now += 1600.0;
    p.tool("bold");
    p.type_("X");
    p.input(Input::Undo, "");
    assert_eq!(p.source(), "Hello");
    p.input(Input::Undo, "");
    assert_eq!(p.source(), "");
}

#[test]
fn undo_and_redo_a_list() {
    let mut p = Page::open("one");
    p.tool("bullet");
    assert_eq!(p.source(), "- one");
    p.tool("undo");
    assert_eq!(p.source(), "one");
    p.tool("redo");
    assert_eq!(p.source(), "- one");
}

#[test]
fn composition_commits_into_bold() {
    let mut p = Page::open("**ab**");
    p.caret(3);
    p.platform_types("日本");
    assert_eq!(p.source(), "**a日本b**");
    let mut p = Page::open("**ab** x");
    p.caret(4);
    p.platform_types("仮名");
    assert_eq!(p.source(), "**ab仮名** x");
}

#[test]
fn pasted_markdown_stays_markdown() {
    let mut p = Page::open("x ");
    p.type_("pasted **bold** text");
    assert_eq!(p.source(), "x pasted **bold** text");
    let mut p = Page::open("x ");
    p.input(Input::Paste, "pasted **bold**\r\ntext");
    assert_eq!(p.source(), "x pasted **bold**\ntext");
}

#[test]
fn toolbar_heading_and_quote() {
    let mut p = Page::open("hello");
    p.tool("heading1");
    assert_eq!(p.source(), "# hello");
    p.tool("quote");
    assert_eq!(p.source(), "> # hello");
    let mut p = Page::open("hello");
    let now = p.tick();
    p.e.command("heading", "2", now);
    assert_eq!(
        p.source(),
        "## hello",
        "the Contract form: format(id, \"heading\", \"2\")"
    );
}

#[test]
fn a_link_at_a_caret_is_labelled_by_its_host() {
    let mut p = Page::open("see ");
    let now = p.tick();
    p.e.command("link", "https://example.com/a/b", now);
    assert_eq!(p.source(), "see [example.com](https://example.com/a/b)");
    let now = p.tick();
    p.e.command("link", "", now);
    assert_eq!(
        p.source(),
        "see [example.com](https://example.com/a/b)",
        "an empty URL inserts nothing"
    );
}

#[test]
fn backspace_takes_the_platforms_grapheme() {
    // 👍🏽 is four code units; the platform's target range covers all four.
    let mut p = Page::open("a 👍🏽");
    let now = p.tick();
    p.e.before_input(Input::Backward, &[], Some((2, 6)), now);
    assert_eq!(p.source(), "a ");
    // A target that crosses hidden syntax is not trusted.
    let mut p = Page::open("**b** c");
    p.caret(6);
    let now = p.tick();
    p.e.before_input(Input::Backward, &[], Some((2, 6)), now);
    assert_eq!(p.source(), "**b**c");
}

#[test]
fn an_app_write_keeps_the_caret_and_resets_history() {
    let mut p = Page::open("hello world");
    p.caret(8);
    let change = p.e.set_value(&u("hi hello world"));
    assert!(change.source && change.place);
    assert_eq!(p.e.selection(), (11, 11));
    p.input(Input::Undo, "");
    assert_eq!(p.source(), "hi hello world");
    assert!(
        !p.e.set_value(&u("hi hello world")).source,
        "an echo changes nothing"
    );
    let mut p = Page::open("a\nb");
    assert!(
        !p.e.set_value(&u("a\r\nb")).source,
        "the same text with other line endings"
    );
}

#[test]
fn line_endings_and_lone_surrogates_are_normalized_once() {
    let mut e = Editor::new();
    let mut text = u("a\r\nb\rc");
    text.push(0xD800);
    e.load(&text);
    assert_eq!(String::from_utf16(e.source()).unwrap(), "a\nb\nc\u{FFFD}");
}

#[test]
fn pending_formats_show_in_the_toolbar_facts() {
    let mut p = Page::open("plain");
    p.tool("bold");
    assert_eq!(p.e.facts().formats, "bold");
    p.tool("bold");
    assert_eq!(p.e.facts().formats, "");
    let mut p = Page::open("**b**");
    p.caret(3);
    assert_eq!(p.e.facts().formats, "bold");
    p.tool("bold");
    assert_eq!(
        p.e.facts().formats,
        "",
        "turning bold off at a caret inside it"
    );
    p.type_("x");
    assert_eq!(p.source(), "**b**x");
}

#[test]
fn lines_hide_syntax_and_carry_paragraph_styles() {
    let e = {
        let mut e = Editor::new();
        e.load(&u("# Title\n- **one**\n1. two\n```\ncode\n```"));
        e
    };
    let lines = e.lines();
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[0].heading, 1);
    assert_eq!(lines[0].segments, vec![(0, 2, HIDDEN), (2, 7, 0)]);
    assert_eq!(lines[1].deco, Some(Deco::Bullet));
    let shown: Vec<_> = lines[1].segments.iter().filter(|s| s.2 != HIDDEN).collect();
    assert_eq!(shown, vec![&(12, 15, 1)]);
    assert!(matches!(lines[2].deco, Some(Deco::Number(18, 20))));
    use exact_markdown_editor::view::flags;
    assert_ne!(lines[3].flags & flags::COLLAPSED, 0);
    assert_eq!(
        lines[4].flags & (flags::CODE | flags::CODE_FIRST | flags::CODE_LAST),
        flags::CODE | flags::CODE_FIRST | flags::CODE_LAST
    );
    assert_ne!(lines[5].flags & flags::COLLAPSED, 0);
    assert_eq!(e.visible_text(), "Title\none\ntwo\ncode\n");
}

#[test]
fn native_range_readback_matches_whole_source_including_history() {
    for (source, from, to, replacement, caret) in [
        ("a\n\nb", 2, 2, "x", 3),
        ("first\n**hello**\nlast", 6, 15, "**héllo😀**", 13),
        ("before\ntext\nafter", 7, 11, "text*", 12),
        ("a\nb\nc", 2, 3, "b\nnew", 7),
    ] {
        let mut range = Editor::new();
        let mut whole = Editor::new();
        let original = u(source);
        range.load(&original);
        whole.load(&original);
        let mut next = original.clone();
        next.splice(from as usize..to as usize, u(replacement));
        let selection = Some((caret, caret));
        range.reconcile_range(from, to, &u(replacement), selection, 10.0);
        whole.reconcile(&next, selection, 10.0);
        assert_eq!(range.source(), whole.source());
        assert_eq!(range.selection(), whole.selection());
        assert_eq!(range.lines(), whole.lines());
        for command in ["undo", "redo"] {
            range.command(command, "", 20.0);
            whole.command(command, "", 20.0);
            assert_eq!(range.source(), whole.source());
            assert_eq!(range.selection(), whole.selection());
        }
    }
}

#[test]
fn a_range_replacement_places_the_selection_as_html_does() {
    // `setRangeText("XY", 2, 4, mode)` on "abcdef" with "bcde" selected.
    for (mode, sel) in [
        (SelectionMode::Preserve, (1, 5)),
        (SelectionMode::Select, (2, 4)),
        (SelectionMode::Start, (2, 2)),
        (SelectionMode::End, (4, 4)),
    ] {
        let mut p = Page::open("abcdef");
        p.e.set_selection(1, 5);
        p.e.replace(2, 4, &u("XY"), mode, 0.0);
        assert_eq!(
            (p.source().as_str(), p.e.selection()),
            ("abXYef", sel),
            "{mode:?}"
        );
    }
    // Preserve moves an end after the range by the change in length, and an
    // end inside it to the range's edge; CRLF arrives as LF.
    let mut p = Page::open("abcdef");
    p.e.set_selection(3, 6);
    p.e.replace(1, 4, &u("x\r\ny"), SelectionMode::Preserve, 0.0);
    assert_eq!(p.source(), "ax\nyef");
    assert_eq!(p.e.selection(), (1, 6));
}

#[test]
fn each_range_replacement_is_one_undo_step_that_restores_the_selection() {
    let mut p = Page::open("");
    p.type_("hello");
    p.e.set_selection(5, 5);
    // Two replacements at once: two steps, never merged with each other or
    // with the typing before them.
    p.e.replace(5, 5, &u("!"), SelectionMode::End, p.now);
    p.e.replace(6, 6, &u("?"), SelectionMode::End, p.now);
    assert_eq!((p.source().as_str(), p.e.selection()), ("hello!?", (7, 7)));
    p.tool("undo");
    assert_eq!((p.source().as_str(), p.e.selection()), ("hello!", (6, 6)));
    p.tool("undo");
    assert_eq!((p.source().as_str(), p.e.selection()), ("hello", (5, 5)));
    p.tool("redo");
    assert_eq!((p.source().as_str(), p.e.selection()), ("hello!", (6, 6)));
    p.tool("undo");
    p.tool("undo");
    assert_eq!(p.source(), "", "the typing's own step survives");
    // Typing right after a replacement is a step of its own.
    let mut p = Page::open("ab");
    p.e.replace(2, 2, &u("@x"), SelectionMode::End, 0.0);
    p.type_("y");
    p.tool("undo");
    assert_eq!(p.source(), "ab@x");
}

#[test]
fn an_app_caret_in_hidden_syntax_moves_to_its_visible_edge() {
    let mut p = Page::open("a **b** c");
    // Inside the opening `**`: the caret is drawn where the platform can
    // draw it, on the visible text beside it.
    p.e.set_selection(3, 3);
    let (s, e) = p.e.selection();
    assert_eq!(s, e);
    assert!(s == 2 || s == 4, "{s}");
    // A range is kept as the app gave it.
    p.e.set_selection(3, 6);
    assert_eq!(p.e.selection(), (3, 6));
    // Past the end clamps, as HTML's does.
    p.e.set_selection(99, 99);
    assert_eq!(p.e.selection(), (9, 9));
}

/// `link_destinations` over `text`, each as its opener and its closer.
fn destinations(text: &str) -> Vec<(String, String)> {
    let units = u(text);
    let piece = |a: u32, b: u32| String::from_utf16(&units[a as usize..b as usize]).unwrap();
    let styled = exact_markdown::style(text, None);
    exact_markdown_editor::link_destinations(text, &styled)
        .into_iter()
        .map(|[o, oe, c, ce]| (piece(o, oe), piece(c, ce)))
        .collect()
}

#[test]
fn angle_link_destinations_hide_the_full_syntax() {
    for destination in ["<b)c>", "<b(c>", "<a b>", r"<a\<b\>c>", " <b)c> \"title\""] {
        for prefix in ["", "!"] {
            let source = format!("😀 {prefix}[日本]({destination}) [after](e)");
            assert_eq!(
                destinations(&source),
                [
                    (format!("{prefix}["), format!("]({destination})")),
                    ("[".into(), "](e)".into()),
                ],
                "{source}"
            );
        }
    }
    for source in [r"[a](<foo\>)", "![a](<foo<bar>)", "[a](<foo\nbar>)"] {
        assert!(destinations(source).is_empty(), "{source}");
    }
}

#[test]
fn link_destinations_are_each_links_own_syntax() {
    let pair = |o: &str, c: &str| (o.to_string(), c.to_string());
    // A long percent-encoded target, a title, an image and a file link.
    let cite = "t3-citation://v1?text=%E2%80%9Cquoted%E2%80%9D&comment=a%20b";
    let text =
        format!("Ask [Assistant quote]({cite}) and [x](y \"t\") ![i](p.png) [a.rs](./src/a.rs)");
    assert_eq!(
        destinations(&text),
        [
            pair("[", &format!("]({cite})")),
            pair("[", "](y \"t\")"),
            pair("![", "](p.png)"),
            pair("[", "](./src/a.rs)"),
        ]
    );
    // Exactly the destination, whatever hidden syntax is beside it.
    assert_eq!(
        destinations("[a](b)[c](d)"),
        [pair("[", "](b)"), pair("[", "](d)")]
    );
    assert_eq!(destinations("**[a](b)**"), [pair("[", "](b)")]);
    assert_eq!(destinations("[*a*](b)"), [pair("[", "](b)")]);
    assert_eq!(destinations("[`x`](y)"), [pair("[", "](y)")]);
    assert_eq!(
        destinations("[![i](p)](u)"),
        [pair("[", "](u)"), pair("![", "](p)")]
    );
    // One that spans a line break stays shown; code and plain brackets hide nothing.
    assert!(destinations("[a](\nb)").is_empty());
    assert!(destinations("`[a](b)` [c] (d) <https://e.f>").is_empty());
    // The editor's own caret rule over the same run: from its start to its
    // end, a caret sits after it.
    let close = |text: &str| {
        let styled = exact_markdown::style(text, None);
        let [_, _, a, b] = exact_markdown_editor::link_destinations(text, &styled)[0];
        (a, b)
    };
    let (a, b) = close(&text);
    let mut p = Page::open(&text);
    for at in [a, a + 1, b - 1, b] {
        p.caret(at);
        assert_eq!(p.e.selection(), (b, b), "{at}");
    }
}

/// LLP 1045 D11: each undo step keeps the host's words for the state it
/// returns to, however many keystrokes it holds; the app's new words forget
/// them all.
#[test]
fn undo_steps_keep_the_hosts_words() {
    let back = |p: &Page| p.e.words_back().map(|w| String::from_utf16(w).unwrap());
    let mut p = Page::open("A\nB\nC");
    p.e.set_words(&u("first"));
    p.e.select(1, 5, None);
    p.input(Input::Forward, "");
    assert_eq!(p.source(), "A");
    p.e.set_words(&u("second"));
    p.caret(1);
    p.type_(&"x".repeat(64));
    p.e.set_words(&u("third"));
    assert_eq!(back(&p), None, "an edit gives nothing back");
    p.input(Input::Undo, "");
    assert_eq!((p.source(), back(&p)), ("A".into(), Some("second".into())));
    p.input(Input::Undo, "");
    assert_eq!(
        (p.source(), back(&p)),
        ("A\nB\nC".into(), Some("first".into()))
    );
    p.input(Input::Redo, "");
    assert_eq!(back(&p), Some("second".into()));
    p.e.set_words(&u("again"));
    assert_eq!(back(&p), None, "the host's own words since");
    p.e.forget_words();
    p.input(Input::Undo, "");
    assert_eq!(
        (p.source(), back(&p)),
        ("A\nB\nC".into(), Some(String::new()))
    );
}
