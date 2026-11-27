//! Authored test steps answering held device and package tickets (`#2011`).

use contract_syntax::{Step, TicketAnswer};

#[test]
fn held_ticket_steps_parse_serialize_format_and_reject_invalid_forms() {
    let source = concat!(
        "test \"held tickets\"\n",
        "  tap \"checkout\"\n",
        "  tap \"@browser\" dismissed\n",
        "  tap \"@browser\" \"opened\"\n",
        "  type \"@date\" \"2026-10-10\"\n",
        "  clock data\n",
    );
    let tests = contract::tests(source).unwrap();
    let steps = &tests[0].steps;
    assert!(matches!(
        &steps[1],
        Step::Ticket { target, answer: TicketAnswer::Choice(c), .. }
            if target == "browser" && c == "dismissed"
    ));
    assert!(matches!(
        &steps[2],
        Step::Ticket { target, answer: TicketAnswer::Choice(c), .. }
            if target == "browser" && c == "opened"
    ));
    assert!(matches!(
        &steps[3],
        Step::Ticket { target, answer: TicketAnswer::Text(t), .. }
            if target == "date" && t == "2026-10-10"
    ));

    let json = contract::tests_json(&tests);
    for want in [
        r#"{"op":"ticket","target":"browser","choice":"dismissed","line":3}"#,
        r#"{"op":"ticket","target":"browser","choice":"opened","line":4}"#,
        r#"{"op":"ticket","target":"date","text":"2026-10-10","line":5}"#,
    ] {
        assert!(json.contains(want), "{want} not in {json}");
    }

    let formatted = contract_syntax::fmt::format(source).unwrap();
    assert_eq!(
        contract::tests_json(&contract::tests(&formatted).unwrap()),
        json,
        "formatted:\n{formatted}"
    );

    // Unquoted `@` produces a clear hint to quote the held ticket target.
    let lex_err = contract::tests("test \"t\"\n  tap @browser dismissed\n").unwrap_err();
    assert!(
        lex_err.message.contains("quote") && lex_err.message.contains("@browser"),
        "{lex_err:?}"
    );

    // Invalid ticket targets and malformed arguments are refused with clear diagnostics.
    for (bad, needle) in [
        ("test \"t\"\n  tap \"@1\" dismissed\n", "numeric ticket"),
        ("test \"t\"\n  type \"@42\" \"hi\"\n", "numeric ticket"),
        (
            "test \"t\"\n  tap \"@\" dismissed\n",
            "non-empty held ticket",
        ),
        ("test \"t\"\n  tap \"@a b\" dismissed\n", "without spaces"),
        ("test \"t\"\n  tap \"@browser\"\n", "choice"),
        ("test \"t\"\n  tap \"@browser\" \"\"\n", "choice"),
        ("test \"t\"\n  tap \"@browser\" dismissed extra\n", "choice"),
        (
            "test \"t\"\n  type \"@date\" \"2026-10-10\" append\n",
            "no `append`",
        ),
        (
            "test \"t\"\n  type \"@date\" \"\"\n",
            "non-empty answer text",
        ),
        ("test \"t\"\n  type \"@date\" key \"Enter\"\n", "in quotes"),
    ] {
        let err = contract::tests(bad).unwrap_err();
        assert!(
            err.message.contains(needle),
            "{bad:?} -> expected {needle:?}, got {err:?}"
        );
    }
}

#[test]
fn package_choose_ticket_choices_are_checked_against_linked_packages() {
    let dir = crate::scratch::Scratch::new(
        std::path::absolute(format!("target/ticket-check-{}", std::process::id())).unwrap(),
    );
    let nm = dir.join("node_modules/@exact");
    std::fs::create_dir_all(&nm).unwrap();
    let pkg =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/in-app-browser");
    let contacts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/contacts");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&pkg, nm.join("in-app-browser")).unwrap();
        std::os::unix::fs::symlink(&contacts, nm.join("contacts")).unwrap();
    }

    let app = dir.join("app.contract");
    std::fs::write(
        &app,
        concat!(
            "use openBrowser, BrowserResult from \"@exact/in-app-browser\"\n",
            "use selectContacts, showContactAccessPicker, ContactSelection from \"@exact/contacts\"\n",
            "component App\n",
            "  mutation browser as shape BrowserResult\n",
            "  mutation picked as shape ContactSelection\n",
            "  mutation shared as shape ContactSelection\n",
            "  action go\n",
            "    send browser = openBrowser(url=\"https://example.com\")\n",
            "  action pick\n",
            "    send picked = selectContacts(properties=[\"name\"], multiple=true)\n",
            "  action shareMore\n",
            "    send shared = showContactAccessPicker()\n",
            "  view\n",
            "    column\n",
            "      button press=go testId=\"go\"\n",
            "        text \"Go\"\n",
            "      button press=pick testId=\"pick\"\n",
            "        text \"Pick\"\n",
            "      button press=shareMore testId=\"share-more\"\n",
            "        text \"Share\"\n",
        ),
    )
    .unwrap();

    let check = |test_src: &str| {
        let file = contract_syntax::parse(test_src).unwrap();
        contract::check_test_substitutes(&app, &file, None)
    };

    // Valid choices (declared + cancel), typed answers when choose.typed is omitted (defaults true),
    // and built-in host tickets (even when a package like @exact/contacts shares the name `share`) pass.
    check(concat!(
        "test \"valid\"\n",
        "  tap \"go\"\n",
        "  tap \"@browser\" dismissed\n",
        "  tap \"@browser\" opened\n",
        "  tap \"@browser\" cancel\n",
        "  tap \"@share\" shared\n",
        "  type \"@contacts\" \"c2,c5\"\n",
        "  type \"@date\" \"2026-10-10\"\n",
    ))
    .unwrap();

    // Unknown choice on a linked package choose ticket is refused before running.
    let bad_choice = check("test \"bad\"\n  tap \"@browser\" bogus\n").unwrap_err();
    assert_eq!(bad_choice[0].id, "test-ticket-choice");
    assert!(
        bad_choice[0].message.contains("dismissed"),
        "{bad_choice:?}"
    );
    assert!(bad_choice[0].message.contains("opened"), "{bad_choice:?}");

    // Typing text into a choice-only package ticket is refused (including inside `perf`).
    let bad_type =
        check("test \"bad\"\n  perf\n    type \"@browser\" \"dismissed\"\n").unwrap_err();
    assert_eq!(bad_type[0].id, "test-ticket-choice");
    assert!(
        bad_type[0].message.contains("not typed text"),
        "{bad_type:?}"
    );
}
