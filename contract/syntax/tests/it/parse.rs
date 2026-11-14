//! The parser on the constructs the v1 app uses, and its rejections by id.

use contract_syntax::{
    parse, BinOp, Expr, Lexer, MutationMode, Node, Span, Stmt, TaskKind, TemplatePart, TypeExpr,
};

const APP: &str = r#"
// A slice of the Caltrain app.
shape Departure
  id: string
  train: number
  at: number

component App
  state stationId = none
  state query = ""
  derive selected = match stationId { case some(id) => id, case none => "mv" }
  resource board = departures(selected) as shape Departure[]
  derive count = board.length

  action selectStation(id)
    stationId = some(id)
    query = ""
  action setDark
    setScheme("dark")

  task ticker mount
    every(1000, tick)

  view
    column gap=16 testId="main"
      text `${count} trains` font-size=24 font-weight=700
      if query == "" and count > 0
        for d in board key=d.id
          Row(dep=d, press=selectStation)
      else
        text "searching"
      match stationId
        case some(id)
          text `at ${id}`
        case none
          text "nearest" color=colors.text

component Row
  props
    dep: Departure
    press: string
  view
    button press=press(dep.id) aria-label=`Train ${dep.train}`
      text countdownText(dep.at, nowMs) font-size=(1 + 2) * 3
"#;

#[test]
fn resource_context_appends_expressions_after_the_call_arguments() {
    let src = r#"component App
  resource plain = source(car, minute) as shape Status
  resource single = source(car) with minute as shape Status
  resource empty = source() with minute as shape Status
  resource several = source(car, account) with max(minute, 1), (ready ? 2 : 3), some(clock) as shape Status else preview("cached")
  view
    text "ready"
"#;
    let file = parse(src).unwrap();
    let resources = &file.components[0].resources;
    assert_eq!(resources[0].identity, None);
    assert_eq!(resources[0].args.len(), 2);
    assert_eq!(resources[1].identity, Some(1));
    assert_eq!(resources[1].args.len(), 2);
    assert_eq!(resources[2].identity, Some(0));
    assert_eq!(resources[2].args.len(), 1);
    let r = &resources[3];
    assert_eq!(r.identity, Some(2));
    assert_eq!(r.args.len(), 5);
    assert!(matches!(&r.args[0], Expr::Ident(name, _) if name == "car"));
    assert!(matches!(&r.args[1], Expr::Ident(name, _) if name == "account"));
    assert!(matches!(&r.args[2], Expr::Call(name, args, _) if name == "max" && args.len() == 2));
    assert!(matches!(&r.args[3], Expr::Ternary(..)));
    assert!(matches!(&r.args[4], Expr::Some(..)));
    let placeholder = r.placeholder.as_ref().unwrap();
    assert_eq!(placeholder.source, "preview");
    assert_eq!(placeholder.args.len(), 1);
}

#[test]
fn resource_context_requires_an_expression_after_with_and_each_comma() {
    for context in ["", ", minute", "minute,", "minute,, clock"] {
        let src = format!(
            "component App\n  resource value = source() with {context} as shape number\n  view\n    text value\n"
        );
        assert!(parse(&src).is_err(), "accepted {context:?}");
    }
}

#[test]
fn the_app_slice_parses_to_the_expected_tree() {
    let file = parse(APP).unwrap();
    assert_eq!(file.shapes.len(), 1);
    assert_eq!(file.shapes[0].fields.len(), 3);
    assert_eq!(file.components.len(), 2);
    let app = &file.components[0];
    assert_eq!(app.name, "App");
    assert_eq!(app.states.len(), 2);
    assert_eq!(app.derives.len(), 2);
    assert_eq!(app.resources.len(), 1);
    assert!(matches!(app.resources[0].shape, TypeExpr::List(..)));
    assert_eq!(app.actions.len(), 2);
    assert_eq!(app.actions[0].effects().len(), 2);
    assert!(
        matches!(app.actions[0].body[0], Stmt::Assign { ref target, .. } if target == "stationId")
    );
    assert!(
        matches!(app.actions[1].body[0], Stmt::Command { ref name, .. } if name == "setScheme")
    );
    assert_eq!(app.tasks[0].kind, TaskKind::Every);
    assert_eq!(app.tasks[0].timer.1, "tick");
    assert!(matches!(app.derives[0].expr, Expr::Match { .. }));

    let Node::Element {
        tag,
        attrs,
        children,
        ..
    } = &app.view[0]
    else {
        panic!()
    };
    assert_eq!(tag, "column");
    assert_eq!(
        attrs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["gap", "testId"]
    );
    assert_eq!(children.len(), 3);
    let Node::Element { positional, .. } = &children[0] else {
        panic!()
    };
    let Expr::Template(parts, _) = &positional[0] else {
        panic!()
    };
    assert!(matches!(&parts[0], TemplatePart::Expr(Expr::Ident(n, _)) if n == "count"));
    assert!(matches!(&parts[1], TemplatePart::Text(t) if t == " trains"));
    let Node::When {
        cond,
        then,
        otherwise,
        ..
    } = &children[1]
    else {
        panic!()
    };
    assert!(matches!(cond, Expr::Binary(BinOp::And, ..)));
    assert!(matches!(&then[0], Node::Each { var, .. } if var == "d"));
    let Node::Each { body, .. } = &then[0] else {
        panic!()
    };
    assert!(matches!(&body[0], Node::Use { name, args, .. } if name == "Row" && args.len() == 2));
    assert_eq!(otherwise.len(), 1);
    let Node::Match { some, none, .. } = &children[2] else {
        panic!()
    };
    assert_eq!(some.0, "id");
    assert_eq!(none.len(), 1);

    let row = &file.components[1];
    assert_eq!(row.props.len(), 2);
    let Node::Element {
        attrs, children, ..
    } = &row.view[0]
    else {
        panic!()
    };
    assert!(matches!(&attrs[0].value, Expr::Call(n, args, _) if n == "press" && args.len() == 1));
    let Node::Element { attrs, .. } = &children[0] else {
        panic!()
    };
    // (1 + 2) * 3 parses with the parenthesized sum on the left.
    assert!(
        matches!(&attrs[0].value, Expr::Binary(BinOp::Mul, l, _, _) if matches!(**l, Expr::Binary(BinOp::Add, ..)))
    );
}

#[test]
fn a_task_fires_once_with_after() {
    let file = parse(
        "component A\n  state launching = true\n  action arrived\n    launching = false\n  task launch mount\n    after(60, arrived)\n  view\n    text \"a\"\n",
    )
    .unwrap();
    let task = &file.components[0].tasks[0];
    assert_eq!(task.name, "launch");
    assert_eq!(task.kind, TaskKind::After);
    assert!(matches!(task.timer.0, Expr::Number(ms, _) if ms == 60.0));
    assert_eq!(task.timer.1, "arrived");
    // One entry per task, whichever word.
    let e = parse(
        "component A\n  task t mount\n    after(60, a)\n    every(1000, b)\n  view\n    text \"a\"\n",
    )
    .unwrap_err();
    assert_eq!(e.id, "syntax-duplicate-declaration");
}

// @ref LLP 1073 D1 — `frame` in `every`'s first place is a word, even
// beside a slot of that name; `after(frame, …)` is refused.
#[test]
fn a_task_fires_each_frame_with_every_frame() {
    let file = parse(
        "component A\n  state frame = 0\n  action step\n    frame = frame + 1\n  task ticker mount\n    every(frame, step)\n  view\n    text \"a\"\n",
    )
    .unwrap();
    let task = &file.components[0].tasks[0];
    assert_eq!(task.kind, TaskKind::Frame);
    assert_eq!(task.timer.1, "step");
    // An expression that starts with the word is still an interval.
    let file = parse(
        "component A\n  state frame = 16\n  action step\n    frame = 16\n  task t mount\n    every(frame + 1, step)\n  view\n    text \"a\"\n",
    )
    .unwrap();
    assert_eq!(file.components[0].tasks[0].kind, TaskKind::Every);
    let e = parse("component A\n  task t mount\n    after(frame, a)\n  view\n    text \"a\"\n")
        .unwrap_err();
    assert_eq!(e.id, "contract-task-body");
}

#[test]
fn font_declarations_parse_in_shorthand_and_block_forms() {
    let file = parse(
        "font \"Body\" = \"assets/Body.ttf\"\nfont \"Display\"\n  400 = \"assets/Display.ttf\"\n  700 = \"assets/Display-Bold.otf\"\n  400 italic = \"assets/Display-Italic.ttf\"\n  100 900 = \"assets/Display-Variable.ttf\"\ncomponent App\n  view\n    text \"hello\"\n",
    )
    .unwrap();
    assert_eq!(file.fonts.len(), 2);
    assert_eq!(file.fonts[0].faces[0].weight, None, "the file's own range");
    assert_eq!(file.fonts[1].faces[0].weight, Some([400, 400]));
    assert_eq!(file.fonts[1].faces[3].weight, Some([100, 900]));
    assert!(!file.fonts[0].faces[0].italic);
    assert_eq!(file.fonts[1].faces.len(), 4);
    assert!(file.fonts[1].faces[2].italic);

    let error = parse("font \"Bad\"\n  1001 = \"assets/Bad.ttf\"\n").unwrap_err();
    assert_eq!(error.id, "syntax-font-weight");
    let error = parse("font \"Bad\"\n  700 400 = \"assets/Bad.ttf\"\n").unwrap_err();
    assert_eq!(error.id, "syntax-font-weight");
}

#[test]
fn precedence_and_multiline_calls_hold() {
    let f = parse("component A\n  derive x = 1 + 2 * 3 == 7 and not false\n  derive y = f(\n    1,\n    2)\n  view\n    text \"a\"\n").unwrap();
    let Expr::Binary(BinOp::And, l, r, _) = &f.components[0].derives[0].expr else {
        panic!()
    };
    assert!(matches!(**l, Expr::Binary(BinOp::Eq, ..)));
    assert!(matches!(**r, Expr::Unary(..)));
    assert!(
        matches!(&f.components[0].derives[1].expr, Expr::Call(n, a, _) if n == "f" && a.len() == 2)
    );
}

#[test]
fn a_template_expression_balances_match_braces_and_string_braces() {
    let file = parse(
        "component A\n  state choice = some(\"yes\")\n  derive label = `value ${match choice { case some(value) => value, case none => \"}\" }}`\n  view\n    text label\n",
    )
    .unwrap();
    let Expr::Template(parts, _) = &file.components[0].derives[0].expr else {
        panic!()
    };
    assert!(matches!(&parts[1], TemplatePart::Expr(Expr::Match { .. })));
}

#[test]
fn a_template_literal_ends_on_its_own_line() {
    // A `${…}` fragment never holds a line break: the literal that would
    // carry one is refused before its expression is parsed.
    let error = parse(
        "component A\n  state count = 1\n  state extra = 2\n  view\n    text `total ${count\n+ extra}`\n",
    )
    .unwrap_err();
    assert_eq!(error.id, "syntax-unterminated-template", "{error}");
    assert_eq!(error.span.line, 5, "{error}");
}

#[test]
fn if_when_and_their_explicit_else_need_non_empty_blocks() {
    let cases = [
        (
            "component A\n  state n = 0\n  action go\n    if true\n    n = 1\n  view\n    text \"a\"\n",
            4,
        ),
        (
            "component A\n  state n = 0\n  action go\n    if true\n      n = 1\n    else\n    n = 2\n  view\n    text \"a\"\n",
            4,
        ),
        (
            "component A\n  view\n    if true\n    text \"always\"\n",
            3,
        ),
        (
            "component A\n  view\n    if true\n      text \"then\"\n    else\n    text \"always\"\n",
            3,
        ),
    ];
    for (src, line) in cases {
        let error = parse(src).unwrap_err();
        assert_eq!(error.id, "syntax-empty-block", "{src:?}: {error}");
        assert_eq!(error.span.line, line, "{src:?}: {error}");
    }
}

#[test]
fn rejections_carry_stable_ids_and_spans() {
    let cases = [
        ("use theme from \"./theme.ts\"\n", "contract-no-imports", 1),
        ("use theme from \"./theme\"\n", "contract-no-imports", 1),
        (
            "component A\n  state x = 1\n\tview\n",
            "syntax-tab-indent",
            3,
        ),
        (
            "component A\n  state x = \"open\n",
            "syntax-unterminated-string",
            2,
        ),
        (
            "component A\n  view\n    if x\n      text \"a\"\n   text \"b\"\n",
            "syntax-bad-dedent",
            5,
        ),
        (
            "component A\n  task t mount\n    later(1, x)\n",
            "contract-task-body",
            3,
        ),
        (
            "component A\n  view\n    match x\n      case some(y)\n        text \"a\"\n",
            "contract-match-arms",
            3,
        ),
        (
            "component A\n  derive x = (1 + \n",
            "syntax-expected-expression",
            3,
        ),
        (
            "component A\n  derive x = `a ${1 + }`\n",
            "syntax-expected-expression",
            2,
        ),
        (
            "shape S\n  a: string\nwhatever\n",
            "syntax-expected-declaration",
            3,
        ),
        // A list literal's items are separated by commas, and the web's
        // spread is refused with the `concat` it means (LLP 1088 §9.1).
        ("component A\n  derive xs = [1 2]\n", "syntax-expected", 2),
        (
            "component A\n  derive xs = [...ys, 1]\n",
            "syntax-refused-idiom",
            2,
        ),
    ];
    for (src, id, line) in cases {
        let err = parse(src).unwrap_err();
        assert_eq!(err.id, id, "{src:?} → {err}");
        assert_eq!(err.span.line, line, "{src:?} → {err}");
    }
}

#[test]
fn unknown_declarations_name_atom_among_expected_forms() {
    let err = parse("banana\ncomponent App\n  view\n    text \"a\"\n").unwrap_err();
    assert_eq!(err.id, "syntax-expected-declaration");
    assert_eq!((err.span.line, err.span.col), (1, 1));
    assert_eq!(
        err.message,
        concat!(
            "expected `routes`, `font`, `sound`, `shape`, `style`, `decoration`, `atom`, ",
            "`keyframes`, `timeline`, `color-profile`, `theme`, `fn`, `source`, `use`, ",
            "`component`, or `members`, found `banana`"
        )
    );
}

#[test]
fn a_leading_byte_order_mark_keeps_source_byte_columns() {
    for ending in ["\n", "\r\n"] {
        let src = format!("\u{feff}component App{ending}  view{ending}    text \"x\"{ending}");
        let file = contract_syntax::parse_source(&src, 7).unwrap();
        assert_eq!(file.components[0].name, "App");
        let tokens = Lexer::tokenize_at(
            &src,
            Span {
                source_id: 7,
                ..Span::point(1, 1)
            },
        )
        .unwrap();
        assert_eq!(
            tokens[0].span,
            Span {
                line: 1,
                col: 4,
                end_col: 13,
                source_id: 7
            }
        );
        for token in tokens.iter().filter(|t| t.span.col < t.span.end_col) {
            let line = src.lines().nth(token.span.line as usize - 1).unwrap();
            assert!(
                !line[token.span.col as usize - 1..token.span.end_col as usize - 1]
                    .contains('\u{feff}')
            );
        }
    }
    for (src, origin) in [
        ("\u{feff}component App", Span::point(2, 1)),
        ("\u{feff}component App", Span::point(1, 2)),
        ("\n\u{feff}component App", Span::point(1, 1)),
        (" \u{feff}component App", Span::point(1, 1)),
        ("\u{feff}\u{feff}component App", Span::point(1, 1)),
    ] {
        assert_eq!(
            Lexer::tokenize_at(src, origin).unwrap_err().id,
            "syntax-unexpected-char"
        );
    }
    let src = "component App\n  view\n    text \"\u{feff}\"\n";
    assert!(Lexer::tokenize(src, 1)
        .unwrap()
        .iter()
        .any(|t| matches!(&t.kind, contract_syntax::TokenKind::Str(s) if s == "\u{feff}")));
}

#[test]
fn an_unexpected_character_is_named_whole_and_spans_its_bytes() {
    for (c, len) in [("@", 1), ("é", 2), ("—", 3), ("🙂", 4)] {
        let src = format!("component A\n  view\n    text {c} x\n");
        let errors = Lexer::tokenize_all(&src, Span::point(1, 1)).unwrap_err();
        assert_eq!(errors.len(), 1, "{c}: {errors:?}");
        let e = &errors[0];
        assert_eq!(e.id, "syntax-unexpected-char", "{c}: {e:?}");
        assert_eq!(e.message, format!("unexpected `{c}`"), "{c}: {e:?}");
        assert_eq!((e.span.line, e.span.col, e.span.end_col), (3, 10, 10 + len));
    }
}

#[test]
fn an_elements_attributes_continue_on_deeper_lines_that_begin_with_name_equals() {
    // LLP 1035.005 D1: `name=` on a deeper line continues the attribute
    // list; a child begins with a tag, so one-token lookahead decides.
    let src = "component A\n  state n = 0\n  action go\n    n = 1\n  view\n    button press=go\n      testId=\"go\" aria-label=\"Go\"\n      width=40\n      text \"a\"\n        font-size=12\n      text \"b\"\n";
    let file = parse(src).unwrap();
    let Node::Element {
        attrs, children, ..
    } = &file.components[0].view[0]
    else {
        panic!()
    };
    assert_eq!(
        attrs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["press", "testId", "aria-label", "width"]
    );
    assert_eq!(children.len(), 2);
    let Node::Element { tag, attrs, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(tag, "text");
    assert_eq!(attrs[0].name, "font-size");

    // Continued lines may sit deeper than the children that follow them.
    let file = parse("component A\n  view\n    main\n        display=\"flex\" gap=8\n        padding=4\n      text \"a\"\n        font-size=12\n      text \"b\"\n    text \"after\"\n").unwrap();
    let Node::Element {
        attrs, children, ..
    } = &file.components[0].view[0]
    else {
        panic!()
    };
    assert_eq!(
        attrs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["display", "gap", "padding"]
    );
    assert_eq!(children.len(), 2);
    assert_eq!(file.components[0].view.len(), 2);
    // A deeper block that is not attributes still has to dedent to a level.
    let e =
        parse("component A\n  view\n    main\n        text \"a\"\n      text \"b\"\n").unwrap_err();
    assert_eq!(e.id, "syntax-bad-dedent");

    // At the element's own depth (or shallower) `name=` is refused.
    let e = parse("component A\n  view\n    column gap=8\n      text \"a\"\n    width=40\n")
        .unwrap_err();
    assert_eq!(e.id, "syntax-continuation-indent");
    assert_eq!(e.span.line, 5);
    // A continued line holds attributes only.
    let e = parse("component A\n  view\n    text \"a\"\n      width=40 \"b\"\n").unwrap_err();
    assert_eq!(e.id, "syntax-expected-attr");
    // A duplicate across lines is the same refusal as on one line.
    let e = parse("component A\n  view\n    text \"a\" width=1\n      width=2\n").unwrap_err();
    assert_eq!(e.id, "syntax-duplicate-attr");
}

#[test]
fn a_component_uses_arguments_may_span_lines_inside_its_parentheses() {
    let src = "component A\n  view\n    Row(\n      a=1,\n      b=\"x\",\n    )\n      text \"filled\"\ncomponent Row\n  props\n    a: number\n    b: string\n  slot\n  view\n    children\n";
    let file = parse(src).unwrap();
    let Node::Use {
        name,
        args,
        children,
        ..
    } = &file.components[0].view[0]
    else {
        panic!()
    };
    assert_eq!(name, "Row");
    assert_eq!(
        args.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(children.len(), 1);
}

#[test]
fn source_identity_reaches_nested_ast_ranges_without_changing_syntax() {
    use contract_syntax::{parse_source, VisitSpans};
    let expected = parse(APP).unwrap();
    let mut tagged = parse_source(APP, 29).unwrap();
    let mut count = 0;
    tagged.visit_spans(&mut |span| {
        assert_eq!(span.source_id, 29, "{span:?}");
        assert!(span.end_col >= span.col, "{span:?}");
        span.source_id = 0;
        count += 1;
    });
    assert!(count > 60, "{count}");
    assert_eq!(tagged, expected);
}

#[test]
fn send_is_a_name_everywhere_but_where_the_send_statement_starts() {
    let src = "component A\n  props\n    send: action\n  state count = 0\n  mutation session as shape Session\n  action go\n    send()\n    send(count)\n    send session = login(send=count)\n    count = send\n  view\n    Row(send=go)\n";
    let file = parse(src).unwrap();
    let a = &file.components[0];
    assert_eq!(a.props[0].name, "send");
    assert!(matches!(a.props[0].ty, Some(TypeExpr::Named(ref t, _)) if t == "action"));
    let body = &a.actions[0].body;
    assert!(
        matches!(&body[0], Stmt::Command { name, args, .. } if name == "send" && args.is_empty())
    );
    assert!(
        matches!(&body[1], Stmt::Command { name, args, .. } if name == "send" && args.len() == 1)
    );
    assert!(matches!(
        &body[2],
        Stmt::Send { target, source, args, .. }
            if target == "session" && source == "login"
                && matches!(&args[0], Expr::NamedArg(n, _, _) if n == "send")
    ));
    assert!(matches!(&body[3], Stmt::Assign { target, expr, .. }
        if target == "count" && matches!(expr, Expr::Ident(n, _) if n == "send")));
    assert!(matches!(&a.view[0], Node::Use { args, .. } if args[0].name == "send"));

    let src = "component A\n  state send = 0\n  action go\n    send = send + 1\n  view\n    text `${send}`\n";
    let file = parse(src).unwrap();
    let a = &file.components[0];
    assert_eq!(a.states[0].name, "send");
    assert!(matches!(&a.actions[0].body[0], Stmt::Assign { target, .. } if target == "send"));
}

#[test]
fn let_starts_a_statement_only_before_a_name_and_records_build_with_named_arguments() {
    // LLP 1035.005.000 D2 and D3.
    let src = "component A\n  state let = 0\n  action go\n    let next = F(base, title=\"a\")\n    if next.pinned\n      let word = \"b\"\n    let = 1\n  view\n    text \"a\"\n";
    let file = parse(src).unwrap();
    let body = &file.components[0].actions[0].body;
    assert!(
        matches!(&body[0], Stmt::Let { name, expr: Expr::Call(shape, args, _), span }
        if name == "next" && shape == "F" && (span.line, span.col) == (4, 9)
            && matches!(&args[0], Expr::Ident(b, _) if b == "base")
            && matches!(&args[1], Expr::NamedArg(f, _, _) if f == "title"))
    );
    assert!(matches!(&body[1], Stmt::If { then, .. }
        if matches!(&then[0], Stmt::Let { name, .. } if name == "word")));
    assert!(matches!(&body[2], Stmt::Assign { target, .. } if target == "let"));
    let e =
        parse("component A\n  action go\n    let in = 1\n  view\n    text \"a\"\n").unwrap_err();
    assert_eq!(e.id, "syntax-expected-name", "{e:?}");
    let e = parse("component A\n  action go\n    let a 1\n  view\n    text \"a\"\n").unwrap_err();
    assert_eq!(e.id, "syntax-expected", "{e:?}");
}

#[test]
fn template_text_decodes_the_escapes_a_string_does_and_escaped_interpolation_is_literal() {
    fn parts(template: &str) -> Vec<TemplatePart> {
        let src = format!("component A\n  state x = 1\n  view\n    text {template}\n");
        let file = parse(&src).unwrap();
        let Node::Element { positional, .. } = &file.components[0].view[0] else {
            panic!()
        };
        let Expr::Template(parts, _) = &positional[0] else {
            panic!("{:?}", positional[0])
        };
        parts.clone()
    }
    fn text(template: &str) -> String {
        match parts(template).as_slice() {
            [TemplatePart::Text(t)] => t.clone(),
            other => panic!("{template}: {other:?}"),
        }
    }
    assert_eq!(text(r"`a\${x}`"), "a${x}");
    assert_eq!(text(r"`a\tb\nc`"), "a\tb\nc");
    assert_eq!(text(r#"`\`\\\"\$`"#), "`\\\"$");
    // A code point, as JavaScript and Swift write it (the euxy port's
    // non-breaking hyphen), in a template and a string alike.
    assert_eq!(
        text(r"`a\u{2011}b\u2011\u{1F600}`"),
        "a\u{2011}b\u{2011}\u{1F600}"
    );
    let file = parse("component A\n  view\n    text \"x\\u{2011}y\\u00e9\"\n").unwrap();
    let Node::Element { positional, .. } = &file.components[0].view[0] else {
        panic!()
    };
    assert!(
        matches!(&positional[0], Expr::Str(s, _) if s == "x\u{2011}y\u{e9}"),
        "{positional:?}"
    );
    for (bad, says) in [
        (r"\u{}", "`\\u` takes a code point in hex"),
        (r"\u{1234567}", "`\\u` takes a code point in hex"),
        (r"\u12", "`\\u` takes a code point in hex"),
        (r"\u{D800}", "`\\u{D800}` is no Unicode character"),
        (r"\u{110000}", "`\\u{110000}` is no Unicode character"),
    ] {
        let e = parse(&format!("component A\n  view\n    text \"{bad}\"\n")).unwrap_err();
        assert_eq!(e.id, "syntax-bad-escape", "{bad}: {e:?}");
        assert!(e.message.starts_with(says), "{bad}: {}", e.message);
    }
    // `\\` before `${` is an escaped backslash; the interpolation still runs.
    let p = parts(r"`a\\${x}\`b`");
    assert!(
        matches!(&p[0], TemplatePart::Text(t) if t == "a\\"),
        "{p:?}"
    );
    assert!(
        matches!(&p[1], TemplatePart::Expr(Expr::Ident(n, _)) if n == "x"),
        "{p:?}"
    );
    assert!(matches!(&p[2], TemplatePart::Text(t) if t == "`b"), "{p:?}");
    // The same escape table as a string, and the same refusal past it.
    let e = parse("component A\n  view\n    text `ok \\q`\n").unwrap_err();
    assert_eq!(e.id, "syntax-bad-escape", "{e:?}");
    assert_eq!((e.span.line, e.span.col), (3, 14), "{e:?}");
    let accepted = r#"a string accepts \n \t \" \\ \` \$ and \u{…}, a Unicode code point in hex (`\u{2011}`, `\u2011`)"#;
    assert_eq!(e.message, format!("unknown escape `\\q`; {accepted}"));
    // A string's refusal names the escapes it accepts too.
    let e = parse("component A\n  view\n    text \"a\\rb\"\n").unwrap_err();
    assert_eq!(e.id, "syntax-bad-escape", "{e:?}");
    assert_eq!(e.message, format!("unknown escape `\\r`; {accepted}"));
    // A `\` that ends the line (trailing spaces trimmed) escapes nothing.
    let e = parse("component A\n  view\n    text \"a\\  \n").unwrap_err();
    assert_eq!(e.id, "syntax-bad-escape", "{e:?}");
    assert_eq!(
        e.message,
        format!("a `\\` ends the line, escaping nothing; {accepted}")
    );
}

#[test]
fn a_mutations_clauses_may_continue_on_indented_lines() {
    // The codeedit diary: `then` on the next line was `syntax-expected-section`.
    let src = "component A\n  mutation edited as shape Jump refreshes page\n    then followEdit\n  mutation saved as shape Ok\n    queue\n    refreshes files, page\n    then reload\n  state n = 0\n  view\n    text \"a\"\n";
    let file = parse(src).unwrap();
    let a = &file.components[0];
    let [edited, saved] = &a.mutations[..] else {
        panic!("two mutations: {:?}", a.mutations.len())
    };
    assert_eq!(
        edited.refreshes.iter().map(|r| &*r.0).collect::<Vec<_>>(),
        ["page"]
    );
    assert_eq!(edited.then.as_ref().map(|t| &*t.0), Some("followEdit"));
    assert_eq!(saved.mode, MutationMode::Queue);
    assert_eq!(edited.mode, MutationMode::Newest);
    assert_eq!(saved.refreshes.len(), 2);
    let err = parse(
        "component A\n  mutation m as shape Ok refreshes board feed\n  view\n    text \"a\"\n",
    )
    .unwrap_err();
    assert_eq!(err.id, "syntax-expected-newline");
    assert!(err.message.contains("refreshes board, feed"), "{err:?}");
    assert_eq!(saved.then.as_ref().map(|t| &*t.0), Some("reload"));
    assert_eq!(a.states[0].name, "n");
    // Out of order, the continued line is named.
    let src = "component A\n  mutation m as shape Ok\n    then x\n    refreshes page\n  view\n    text \"a\"\n";
    let err = parse(src).unwrap_err();
    assert_eq!(err.id, "syntax-expected", "{}", err.message);
    assert!(err.message.contains("in that order"), "{}", err.message);
}

/// LLP 1161.000 D1: `parallel` takes `queue`'s place, and excludes it.
#[test]
fn a_mutation_is_parallel_where_queue_would_go_and_never_both() {
    let src = "component A\n  mutation renamed as shape Ok parallel refreshes page\n  mutation saved as shape Ok\n    parallel\n  view\n    text \"a\"\n";
    let file = parse(src).unwrap();
    let modes: Vec<_> = file.components[0]
        .mutations
        .iter()
        .map(|m| m.mode)
        .collect();
    assert_eq!(modes, [MutationMode::Parallel, MutationMode::Parallel]);
    for both in [
        "queue parallel",
        "parallel queue",
        "refreshes page parallel",
    ] {
        let src = format!("component A\n  mutation m as shape Ok {both}\n  view\n    text \"a\"\n");
        let err = parse(&src).unwrap_err();
        assert_eq!(err.id, "syntax-expected-newline", "{both}: {}", err.message);
        assert!(
            err.message.contains("is out of place"),
            "{both}: {}",
            err.message
        );
    }
}

/// Round 12 (LLP 1116.010 D3): a `-` against a number, straight after an
/// attribute's `=`, is the negative literal, the number it is as `40` is.
#[test]
fn a_negative_number_after_an_attributes_equals_is_a_number_literal() {
    fn value(attrs: &str, name: &str) -> Expr {
        let src = format!("component A\n  view\n    input {attrs}\n");
        let file = parse(&src).unwrap_or_else(|e| panic!("{attrs}: {e:?}"));
        let Node::Element { attrs, .. } = &file.components[0].view[0] else {
            panic!()
        };
        attrs.iter().find(|a| a.name == name).unwrap().value.clone()
    }
    for (written, want) in [
        ("min=-40", -40.0),
        ("step=-1", -1.0),
        ("margin-top=-8", -8.0),
        ("min=-0.5", -0.5),
        ("min=-1e300", -1e300),
        ("min=-2.5E-3", -2.5e-3),
        ("min=-999999999999999", -999_999_999_999_999.0),
    ] {
        let name = written.split('=').next().unwrap();
        assert!(
            matches!(value(written, name), Expr::Number(n, _) if n == want),
            "{written}: {:?}",
            value(written, name)
        );
    }
    // Its span covers the sign too, and a continued attribute line reads it alike.
    let Expr::Number(_, at) = value("min=-40", "min") else {
        panic!()
    };
    assert_eq!((at.col, at.end_col), (15, 18));
    let file =
        parse("component A\n  view\n    input type=\"number\"\n      min=-40 max=40\n").unwrap();
    let Node::Element { attrs, .. } = &file.components[0].view[0] else {
        panic!()
    };
    assert!(matches!(attrs[1].value, Expr::Number(n, _) if n == -40.0));
    assert!(matches!(attrs[2].value, Expr::Number(n, _) if n == 40.0));
    // The positive literal, its exponent forms among them, is unchanged.
    for (written, want) in [("min=40", 40.0), ("min=1e3", 1000.0), ("min=4e+2", 400.0)] {
        let name = written.split('=').next().unwrap();
        assert!(
            matches!(value(written, name), Expr::Number(n, _) if n == want),
            "{written}"
        );
    }
    // A unit written against it is still the unquoted length it was, negative or not.
    for written in ["margin-top=-8px", "margin-top=8px", "min=2em"] {
        let e = parse(&format!("component A\n  view\n    input {written}\n")).unwrap_err();
        assert_eq!(e.id, "syntax-unquoted-length", "{written}");
    }
    // A literal that overflows a double is no number, with an exponent or without.
    let e = parse("component A\n  view\n    input min=-1e999\n").unwrap_err();
    assert_eq!(e.id, "syntax-bad-number");
    let src = format!(
        "component A\n  state n = {}\n  view\n    text \"\"\n",
        "9".repeat(400)
    );
    let e = parse(&src).unwrap_err();
    assert_eq!(e.id, "syntax-bad-number");
    assert_eq!((e.span.line, e.span.col), (2, 13), "{e:?}");
    assert!(e.message.contains("1.8e308"), "{}", e.message);
}

/// `width=50%` is refused as the unquoted length it is, at the number and the
/// `%`, whatever follows on its line; a remainder with its operand is not.
#[test]
fn an_unquoted_percentage_is_refused_at_itself() {
    let message = "`width=50%` needs quotes: a value with a unit is a string, `width=\"50%\"` (a bare number is pixels)";
    for (src, at) in [
        (
            "component A\n  view\n    column width=50% testId=\"c\"\n      text \"a\"\n",
            (3, 18, 21),
        ),
        (
            "component A\n  view\n    column width=50%\n      text \"a\"\n",
            (3, 18, 21),
        ),
        (
            "component A\n  view\n    column width=50% apple.width=10\n",
            (3, 18, 21),
        ),
        (
            "component A\n  view\n    column\n      width=50%\n",
            (4, 13, 16),
        ),
        (
            "style Wide\n  width=50%\ncomponent A\n  view\n    text \"a\" class=\"Wide\"\n",
            (2, 9, 12),
        ),
        (
            "keyframes Grow\n  to width=50%\ncomponent A\n  view\n    text \"a\"\n",
            (2, 12, 15),
        ),
    ] {
        let e = parse(src).unwrap_err();
        assert_eq!(e.id, "syntax-unquoted-length", "{src}\n{e:?}");
        assert_eq!(e.message, message, "{src}");
        assert_eq!((e.span.line, e.span.col, e.span.end_col), at, "{src}");
    }
    let e = parse("component A\n  view\n    column margin-top=-5%\n").unwrap_err();
    assert!(e.message.contains("`margin-top=\"-5%\"`"), "{e:?}");
    // A remainder that has its operand stays one.
    let src = "component A\n  state a = 1\n  state b = 2\n  view\n    column width=a % b height=10 % b testId=\"c\"\n";
    parse(src).unwrap();
}

/// What the negative literal leaves alone: subtraction, a negation of
/// something that is not a lone number, and a vendor-prefixed name.
#[test]
fn the_negative_literal_leaves_subtraction_negation_and_exact_names_alone() {
    fn value(attrs: &str, name: &str) -> Expr {
        let src =
            format!("component A\n  state a = 5\n  state b = 3\n  view\n    column {attrs}\n");
        let file = parse(&src).unwrap_or_else(|e| panic!("{attrs}: {e:?}"));
        let Node::Element { attrs, .. } = &file.components[0].view[0] else {
            panic!()
        };
        attrs.iter().find(|a| a.name == name).unwrap().value.clone()
    }
    // `a - b`, spaced, is a subtraction, and a literal's sign is not read from the left.
    assert!(matches!(
        value("gap=a - b", "gap"),
        Expr::Binary(BinOp::Sub, ..)
    ));
    assert!(matches!(
        value("gap=a - 1", "gap"),
        Expr::Binary(BinOp::Sub, ..)
    ));
    assert!(matches!(
        value("gap=8 - 1", "gap"),
        Expr::Binary(BinOp::Sub, ..)
    ));
    assert!(matches!(
        value("gap=a -1", "gap"),
        Expr::Binary(BinOp::Sub, ..)
    ));
    // A negation of a name, of a group or inside a larger expression stays one.
    for written in ["gap=-a", "gap=-(1 + 2)", "gap=-8 + a", "gap=-8 * 2"] {
        let name = written.split('=').next().unwrap();
        assert!(
            !matches!(value(written, name), Expr::Number(..)),
            "{written}: {:?}",
            value(written, name)
        );
    }
    // The formatter's spelling of a negation, `- 8`, and a parenthesized `-(8)`, are the literal too.
    for written in ["gap=- 8", "gap=-(8)"] {
        assert!(
            matches!(value(written, "gap"), Expr::Number(n, _) if n == -8.0),
            "{written}"
        );
    }
    // A negative inside an expression is the unary it was, in a choice as anywhere.
    let Expr::Ternary(_, yes, no, _) = value("gap=(a > b ? -8 : 8)", "gap") else {
        panic!()
    };
    assert!(matches!(*yes, Expr::Unary(..)));
    assert!(matches!(*no, Expr::Number(n, _) if n == 8.0));
    // `-exact-…` is one name where it is a name, and a string where it is a string.
    let Node::Element { attrs, .. } = &parse(
        "component A\n  view\n    image \"symbol:x\" -exact-tint-color=\"-exact-label\" font=\"-exact-title1\" gap=-8\n",
    )
    .unwrap()
    .components[0]
        .view[0]
    else {
        panic!()
    };
    assert_eq!(attrs[0].name, "-exact-tint-color");
    assert!(matches!(&attrs[0].value, Expr::Str(s, _) if s == "-exact-label"));
    assert!(matches!(&attrs[1].value, Expr::Str(s, _) if s == "-exact-title1"));
    assert!(matches!(attrs[2].value, Expr::Number(n, _) if n == -8.0));
    // `em` after a number's `e` is a unit, not an exponent.
    assert!(parse("component A\n  view\n    image \"symbol:x\" width=\"2em\"\n").is_ok());
}

#[test]
fn update_operators_are_refused_with_alternatives() {
    for expression in [
        "--n",
        "++n",
        "n--",
        "n++",
        "10 - --n",
        "10--n",
        "-- n",
        "++ n",
        "n --",
        "n ++",
        "--3",
        "++3",
        "--(n)",
        "(n)++",
        "n.field--",
        "abs(--n)",
        "[n++]",
        "true ? --n : n",
        "`${--n}`",
        "`${n++}`",
    ] {
        for line in [
            format!("derive x = {expression}"),
            format!("action down()\n    n = {expression}"),
        ] {
            let src = format!("component A\n  state n = 5\n  {line}\n");
            let error = parse(&src).unwrap_err();
            assert_eq!(error.id, "syntax-refused-idiom", "{expression}");
            assert!(error.message.contains("n = n"), "{error}");
        }
    }
}

#[test]
fn dashed_profiles_and_literal_text_are_not_update_expressions() {
    let file = parse(
        r#"color-profile --brand-press src="assets/brand.icc"
theme
  ink = "color(--brand-press 0.1 0.2 0.3)"
  alias = "ink"
  -exact-secondary-label = "color(--brand-press 0.1 0.2 0.3)"
component A
  view
    text "--n ++n 010 01.02.03" color="var(--ink)"
    text `--n ++n 010 ${"--brand"}` animation-timeline="--scroll"
"#,
    )
    .unwrap();
    assert_eq!(file.color_profiles[0].name, "--brand-press");
}

#[test]
fn leading_zero_numbers_are_refused_with_alternatives() {
    for number in ["010", "08", "00", "01.5", "01e2"] {
        let src = format!("component A\n  derive x = {number}\n");
        let error = parse(&src).unwrap_err();
        assert_eq!(error.id, "syntax-bad-number", "{number}");
        assert!(error.message.contains("decimal"), "{error}");
    }
    for expression in [
        "- -n",
        "-(-n)",
        "n- -n",
        "10 - - -n",
        "0",
        "0.5",
        "0e2",
        "10",
    ] {
        parse(&format!(
            "component A\n  state n = 5\n  derive x = {expression}\n"
        ))
        .unwrap();
    }
}

#[test]
fn unsupported_numeric_spellings_do_not_compile_as_decimal_prefixes() {
    for number in [
        "0o10", "0O10", "0x10", "0X10", "0x", "0b10", "0B10", "0b", "1_000", "0_1", "1n", "0n",
        "0x10n",
    ] {
        assert!(
            parse(&format!("component A\n  derive x = {number}\n")).is_err(),
            "{number}"
        );
    }
}

#[test]
fn midi_bytes_keep_leading_zeroes() {
    parse(
        r#"test "MIDI"
  tap midi "keys" 90 00 08
  expect midi sent "synth" 90 00 08 at 0.5
"#,
    )
    .unwrap();
    for step in [
        "tap midi \"keys\" 90 00 08 at 010",
        "tap midi \"keys\" 90 00 08 clock 010",
        "expect midi sent \"synth\" 90 00 08 at 010",
    ] {
        assert_eq!(
            parse(&format!("test \"MIDI\"\n  {step}\n")).unwrap_err().id,
            "syntax-bad-number",
        );
    }
}
