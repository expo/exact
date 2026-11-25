use super::{apple_uses, Capability};
use crate::uses;
use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};

fn plan(view: &str) -> exact_plan::Plan {
    let source = format!(
        "component A\n  state wide = true\n  action go\n    wide = false\n  view\n    {view}\n"
    );
    contract::compile(&source).unwrap_or_else(|e| panic!("{source}: {e:?}"))
}

/// A plan with one text node, and what `add` puts in its tables.
fn built(add: impl FnOnce(&mut PlanBuilder)) -> exact_plan::Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    add(&mut b);
    b.node(
        exact_kernel::NodeType::Text as u8,
        None,
        None,
        0,
        &[],
        &[],
        None,
    );
    b.finish().unwrap()
}

/// D10: Apple's `grid` covers every plan that writes a grid row or whose
/// layout reaches the grid algorithm; elsewhere it stays the grammars a grid
/// row needs.
#[test]
fn apple_links_grid_layout_for_every_plan_that_lays_out_a_grid() {
    let column = |attrs: &str| format!("column {attrs}\n      text \"a\"");
    let table = |attrs: &str| format!("table{attrs}\n      tr\n        td\n          text \"a\"");
    let grid_rows = column("display=\"grid\" grid-template-columns=\"1fr 1fr\"");
    let flex_rows = column("display=\"flex\" justify-items=\"center\"");
    for (view, apple, everywhere) in [
        ("text \"a\"".to_string(), false, false),
        (column("display=\"flex\""), false, false),
        (column("display=\"none\""), false, false),
        // A computed value counts as a use of whatever it might select.
        (column("display=(wide ? \"block\" : \"flex\")"), true, false),
        (column("display=\"grid\""), true, false),
        ("text \"a\" display=\"grid\"".to_string(), true, false),
        (grid_rows, true, true),
        // A grid row on a box that is no grid parses by grid's grammar.
        (flex_rows, true, true),
        (table(""), true, false),
        // A table that is a block holds an anonymous table, a grid too.
        (table(" display=\"block\""), true, false),
    ] {
        let plan = plan(&view);
        assert_eq!(apple_uses(&plan).has(Capability::Grid), apple, "{view}");
        assert_eq!(uses(&plan).has(Capability::Grid), everywhere, "{view}");
    }
}

/// D11: the module seam serves a package's modules, its views and its
/// events; each table alone selects it, and no other host's set moves.
#[test]
fn modules_are_a_package_module_view_or_event() {
    let plain = plan("text \"a\"");
    assert!(!apple_uses(&plain).has(Capability::Modules));
    let module = built(|b| {
        let ty = b.primitive(TypeKind::Number);
        let source = b.source("battery", &[], ty);
        b.module(
            source,
            ("battery", "@fixture/battery"),
            "{}",
            false,
            false,
            "",
            "",
            "",
        );
    });
    let event = built(|b| {
        let body = b.code(Asm::new());
        let action = b.action("hear", &[], &[], body);
        b.module_event(("bell", "@fixture/bell"), "chime", action, "emit", 4, 0);
    });
    let mut view = plan("text \"a\"");
    view.add_view("swatch-box", ("swatch", "@fixture/swatch"), true, &[], "");
    for plan in [&module, &event, &view] {
        assert!(apple_uses(plan).has(Capability::Modules));
        assert!(!uses(plan).has(Capability::Modules));
    }
}

/// D11: the tool seam serves a plan with a `tool` line.
#[test]
fn tools_are_a_plan_with_a_tool() {
    assert!(!apple_uses(&plan("text \"a\"")).has(Capability::Tools));
    let source = "component A\n  state n = 0\n  tool bump writes \"Add one.\" = bump\n  action bump\n    n = n + 1\n  view\n    text \"a\"\n";
    let tooled = contract::compile(source).unwrap();
    assert!(apple_uses(&tooled).has(Capability::Tools));
    assert!(!uses(&tooled).has(Capability::Tools));
}

/// D10: the pickers' family presents `saveFile` and the File System Access
/// pickers, so they select `picker` on Apple as well as `documents`.
#[test]
fn a_document_picker_links_the_pickers_family() {
    for call in [
        "saveFile(\"f\", \"app:/data/a.json\", \"a.json\")",
        "showOpenFilePicker(\"f\")",
        "showDirectoryPicker(\"f\")",
        "showSaveFilePicker(\"f\", \"a.txt\")",
    ] {
        let source = format!(
            "component A\n  action pick\n    {call}\n  view\n    button \"Pick\" press=pick\n"
        );
        let plan = contract::compile(&source).unwrap();
        assert!(apple_uses(&plan).has(Capability::Picker), "{call}");
        assert!(!uses(&plan).has(Capability::Picker), "{call}");
    }
}

#[test]
fn command_capabilities_use_the_opcode_and_resolved_name() {
    use Capability::*;

    for (name, common, apple) in [
        ("share", &[Share][..], &[Share][..]),
        ("saveFile", &[Documents], &[Documents, Picker]),
        ("requestPermission", &[Documents], &[Documents]),
        ("showPicker", &[Picker], &[Picker]),
        ("startTrace_EXPERIMENTAL", &[], &[Traces]),
        ("requestPictureInPicture", &[], &[Video]),
        ("playSounds", &[], &[Sound]),
        ("sendMIDI", &[], &[Midi]),
        ("Share", &[], &[]),
    ] {
        let plan = built(|b| {
            let mut literal = Asm::new();
            literal.str(b.str("showPicker"));
            let literal = b.code(literal);
            b.action("literal", &[], &[], literal);
            let mut command = Asm::new();
            command.command(b.str(name), 0);
            let command = b.code(command);
            b.action("command", &[], &[], command);
        });
        let common_uses = uses(&plan);
        let apple_uses = apple_uses(&plan);
        for capability in [Share, Documents, Picker, Traces, Video, Sound, Midi] {
            assert_eq!(
                common_uses.has(capability),
                common.contains(&capability),
                "{name}"
            );
            assert_eq!(
                apple_uses.has(capability),
                apple.contains(&capability),
                "{name}"
            );
        }
        assert_eq!(uses::runs_sounds(&plan), name == "playSounds");
        assert_eq!(uses::uses_midi(&plan), name == "sendMIDI");
        assert_eq!(
            uses::uses_picture_in_picture(&plan),
            name == "requestPictureInPicture"
        );
    }
}

/// D10: the wake lock's family is a `wakelock` that can be `screen`.
#[test]
fn a_wake_lock_is_a_wakelock_that_can_be_screen() {
    let held = |view: &str| apple_uses(&plan(view)).has(Capability::WakeLock);
    assert!(held("text \"a\" wakelock=\"screen\""));
    // A computed value can be either word.
    assert!(held("text \"a\" wakelock=(wide ? \"screen\" : \"none\")"));
    assert!(!held("text \"a\" wakelock=\"none\""));
    assert!(!held("text \"a\""));
    assert!(!uses(&plan("text \"a\" wakelock=\"screen\"")).has(Capability::WakeLock));
}

/// D10: iOS selects text only where `user-select` says `text` or `all`,
/// on a box, an inline run or through a class; a computed value can be
/// either, and `none` or `auto` select nothing.
#[test]
fn text_selection_is_a_user_select_that_can_be_text_or_all() {
    let selects = |source: &str| {
        let plan = contract::compile(source).unwrap_or_else(|e| panic!("{source}: {e:?}"));
        apple_uses(&plan).has(Capability::TextSelection)
    };
    let view = |view: &str| format!("component A\n  state wide = true\n  view\n    {view}\n");
    for (source, selected) in [
        (view("text \"a\""), false),
        (view("text \"a\" user-select=\"text\""), true),
        (view("column user-select=\"all\"\n      text \"a\""), true),
        (view("text\n      text \"Folder: \"\n      text \"/tmp\" user-select=\"all\""), true),
        (view("text \"a\" user-select=(wide ? \"none\" : \"text\")"), true),
        (view("text \"a\" user-select=\"none\""), false),
        (view("text \"a\" user-select=\"auto\""), false),
        // An article is the Mac's document text, not iOS's.
        (view("article\n      text \"a\""), false),
        (
            "style Prose\n  user-select=\"text\"\n\ncomponent A\n  view\n    text \"a\" class=Prose\n".to_string(),
            true,
        ),
    ] {
        assert_eq!(selects(&source), selected, "{source}");
    }
    let plan = plan("text \"a\" user-select=\"text\"");
    assert!(!uses(&plan).has(Capability::TextSelection));
}

/// D10: each native roster entry is selected by the rows that reach its
/// family's code, a computed value by whatever it can be, and a plain text
/// and button plan by none; no other host's set moves.
#[test]
fn each_native_entry_is_selected_by_the_rows_that_reach_its_family() {
    use Capability::{
        Collections, Controls, Dialogs, Effects, Flights, Images, NativeViews, Navigation, Sticky,
        Svg, Traces,
    };
    let plain = "column\n      text \"a\"\n      button \"b\" press=go";
    for (view, capability, selected) in [
        (plain, Svg, false),
        (plain, Images, false),
        (plain, NativeViews, false),
        (plain, Controls, false),
        (plain, Traces, false),
        (plain, Collections, false),
        (
            "svg viewBox=\"0 0 10 10\"\n      circle cx=5 cy=5 r=4",
            Svg,
            true,
        ),
        // A box's filter runs on box effects' engine, which SVG's uses.
        ("text \"a\" filter=\"blur(2px)\"", Svg, false),
        ("text \"a\" filter=\"blur(2px)\"", Effects, true),
        ("text \"a\" filter=(wide ? \"none\" : \"blur(2px)\")", Effects, true),
        // `clip-path` is the core's mask on Apple.
        ("text \"a\" clip-path=\"inset(1px)\"", Effects, false),
        (plain, Effects, false),
        // The host tells an SVG by its bytes: a computed source can be one,
        // and so can a download; a bundled file is the bake's to read.
        (
            "image (wide ? \"assets/a.png\" : \"assets/b.png\")",
            Svg,
            true,
        ),
        ("image \"https://example.test/a.png\"", Svg, true),
        ("image \"assets/a.png\"", Svg, false),
        ("image \"assets/a.png\"", Images, true),
        ("image \"symbol:star\"", Images, false),
        // The loader fails an empty source, which an `error` handler hears.
        ("image \"\"", Images, true),
        ("image \"symbol:star\"", Svg, false),
        ("view hatch=\"avatar\"", NativeViews, true),
        ("swatch-box", NativeViews, true),
        ("column\n      action-button press=go", NativeViews, true),
        ("button \"b\" press=go appearance=\"none\"", Controls, false),
        ("input type=\"checkbox\"", Controls, true),
        ("input type=\"checkbox\" role=\"switch\"", Controls, true),
        ("input type=\"range\"", Controls, true),
        ("progress value=0.5", Controls, true),
        ("input type=\"number\"", Controls, false),
        ("column sharedElement=\"hero\"", Flights, true),
        ("column sharedElement=(wide ? \"a\" : \"b\")", Flights, true),
        ("column", Flights, false),
        ("column position=\"sticky\" top=0", Sticky, true),
        ("column position=(wide ? \"sticky\" : \"static\") top=0", Sticky, true),
        ("column position=\"relative\" top=0", Sticky, false),
        (plain, Navigation, false),
        ("column navigationKey=\"a\"", Navigation, true),
        ("column navigationKey=(wide ? \"a\" : \"b\")", Navigation, true),
        (
            "column navigationKey=\"a\" navigationPresentation=\"modal\"",
            Navigation,
            true,
        ),
        ("row role=\"tablist\"\n      button \"a\" role=\"tab\"", Navigation, false),
        // A dialog's header is its sheet's bar (LLP 1141.004 D6).
        (
            "dialog id=\"d\"\n      header\n        text \"T\" role=\"heading\"",
            Navigation,
            true,
        ),
        ("dialog id=\"d\"\n      text \"a\"", Navigation, false),
        (plain, Dialogs, false),
        ("dialog id=\"d\"\n      text \"a\"", Dialogs, true),
        // A confirmation by its rows too: the host tells its shape when it
        // opens, and any `dialog` its author paints is a sheet.
        (
            "dialog id=\"d\" role=\"alertdialog\" aria-label=\"Clear?\"\n      button \"Clear\" press=go commandfor=\"d\" command=\"close\"\n      button \"Cancel\" commandfor=\"d\" command=\"close\"",
            Dialogs,
            true,
        ),
        // ARIA's role alone is no HTML dialog (`lower-dialog-role`).
        ("column role=\"dialog\"", Dialogs, false),
        ("input type=\"text\"", Controls, false),
        (
            "row role=\"tablist\"\n      button \"a\" press=go",
            Controls,
            true,
        ),
        ("row role=(wide ? \"tablist\" : \"group\")", Controls, true),
        ("row role=\"group\"", Controls, false),
        (
            "list\n      row delete=go\n        text \"a\"",
            Collections,
            true,
        ),
        (
            "scroll swipeContent=\"r\" overflow-x=\"scroll\" height=60\n      row id=\"r\"\n        text \"a\"",
            Collections,
            true,
        ),
        // A list whose rows are cells; the author's own rows are not.
        ("list\n      row\n        text \"a\"", Collections, true),
        (
            "list appearance=\"none\"\n      row\n        text \"a\"",
            Collections,
            false,
        ),
    ] {
        let plan = plan(view);
        assert_eq!(
            apple_uses(&plan).has(capability),
            selected,
            "{view}: {capability:?}"
        );
        if !matches!(capability, Collections | Effects) {
            assert!(!uses(&plan).has(capability), "{view}: {capability:?}");
        }
    }
    let traced = contract::compile(
        "component A\n  action go\n    startTrace_EXPERIMENTAL()\n  view\n    button \"b\" press=go\n",
    )
    .unwrap();
    assert!(apple_uses(&traced).has(Traces));
}

/// A literal source's file ships with the app, so the bake reads it; a
/// download's and an app file's bytes come at run time.
#[test]
fn a_bundled_image_source_is_the_bakes_to_read() {
    use super::ImageSource::{Computed, Literal};
    for (source, bundled) in [
        (Literal("assets/a.png"), Some("assets/a.png")),
        (
            Literal("data:image/png;base64,AAAA"),
            Some("data:image/png;base64,AAAA"),
        ),
        (Literal("https://example.test/a.svg"), None),
        (Literal("app:/photos/1"), None),
        (Computed, None),
    ] {
        assert_eq!(source.bundled(), bundled);
        assert_eq!(source.can_be_svg(), bundled.is_none());
    }
}

/// A Live Activity's sends and its resource select `activities`; a plan
/// without them does not.
#[test]
fn activities_are_a_plan_that_starts_or_reads_one() {
    let source = |body: &str| {
        format!("shape ID\n  id: string\nshape S\n  n: number\ncomponent A\n{body}  view\n    text \"a\"\n")
    };
    let sends = source("  mutation started as shape ID\n  action start\n    send started = startActivity(\"k\", S(n=1))\n");
    let reads = source("  resource live = exactActivities() as shape ID\n");
    for (text, selected) in [(sends, true), (reads, true), (source(""), false)] {
        let plan = contract::compile(&text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(
            apple_uses(&plan).has(Capability::Activities),
            selected,
            "{text}"
        );
        assert!(
            !uses(&plan).has(Capability::Activities),
            "the web's set names none"
        );
    }
}

/// An entry's plan and a Live Activity's sends select `widgets`; an app's
/// own plan does not.
#[test]
fn widgets_are_an_entry_plan_or_activities() {
    let widget = contract::Widget {
        kind: "card".into(),
        title: None,
        description: None,
        families: vec!["systemSmall".into()],
        watch_families: vec![],
        component: "Card".into(),
        props: "Props".into(),
        refresh: "refresh".into(),
        requires_secrets: false,
        preview: serde_json::json!({"title": "preview"}),
        configuration: None,
    };
    let card = "shape Props\n  title: string\ncomponent Card\n  props\n    title: string\n  view\n    text title\n";
    let entry = contract::snapshot::compile(card, &widget).unwrap();
    let app = contract::compile("component A\n  view\n    text \"a\"\n").unwrap();
    let activity = contract::compile(
        "shape ID\n  id: string\ncomponent A\n  resource live = exactActivities() as shape ID\n  view\n    text \"a\"\n",
    )
    .unwrap();
    for (plan, selected) in [(&entry, true), (&activity, true), (&app, false)] {
        assert_eq!(apple_uses(plan).has(Capability::Widgets), selected);
        assert!(
            !uses(plan).has(Capability::Widgets),
            "the web's set names none"
        );
    }
}

/// The families the system reaches through the app are selected by grants.
#[test]
fn notifications_host_events_and_wakes_are_their_grants() {
    use super::apple_grant_uses as grants;
    use Capability::{HostEvents, Notifications, Wakes};
    assert!(grants("device.notifications purpose.n\ndevice.push").has(Notifications));
    assert!(grants("device.cloudkit").has(HostEvents));
    for line in [
        "device.background-sync",
        "device.periodic-background-sync",
        "device.geolocation-background purpose.p",
    ] {
        assert!(grants(line).has(Wakes), "{line}");
    }
    assert!(grants("net.fetch https://a/\ndevice.camera purpose.c").is_empty());
}

/// A background transfer asks no device grant: its names select the
/// transfer carrier; another source does not.
#[test]
fn a_background_transfers_names_select_wakes() {
    for (name, selected) in [
        ("backgroundFetchIds", true),
        ("backgroundFetch", true),
        ("geofences", true),
        ("battery", false),
    ] {
        let plan = built(|b| {
            let ty = b.primitive(TypeKind::Number);
            b.source(name, &[], ty);
        });
        assert_eq!(apple_uses(&plan).has(Capability::Wakes), selected, "{name}");
        assert!(!uses(&plan).has(Capability::Wakes));
    }
}

/// D10: each of the media family's four products is linked by every plan
/// whose run makes its output, and no other host's set moves.
#[test]
fn media_is_four_products_each_by_its_own_use() {
    use Capability::{Midi, Sound, Video, WebViews};
    let action = |act: &str, view: &str| {
        contract::compile(&format!(
            "component A\n  action go\n    {act}\n  view\n    {view}\n"
        ))
        .unwrap()
    };
    let button = "button \"b\" press=go";
    for (plan, linked) in [
        (plan("text \"a\""), vec![]),
        (plan("video src=\"assets/a.mp4\""), vec![Video]),
        (plan("audio src=\"assets/a.mp3\""), vec![Video]),
        (
            action(
                "requestPictureInPicture(\"pic\")",
                "image \"assets/a.png\" testId=\"pic\"",
            ),
            vec![Video],
        ),
        (plan("iframe src=\"https://example.test\""), vec![WebViews]),
        (action("stopSounds()", button), vec![Sound]),
        (
            contract::compile("shape T\n  playing: bool\ncomponent A\n  resource t = exactTransport() as shape T\n  view\n    text \"a\"\n")
                .unwrap(),
            vec![Sound],
        ),
        (action("requestMIDIAccess()", button), vec![Midi]),
        (
            action("sendMIDI(\"out\", [144, 60, 100])", button),
            vec![Midi],
        ),
    ] {
        for capability in [Video, WebViews, Sound, Midi] {
            assert_eq!(
                apple_uses(&plan).has(capability),
                linked.contains(&capability),
                "{capability:?} in {linked:?}"
            );
            assert!(!uses(&plan).has(capability), "{capability:?}");
        }
    }
}

/// Only `@exact/search-index` writes the content index: its names select it.
#[test]
fn the_search_indexs_names_select_the_content_index() {
    for (name, selected) in [
        ("setContentIndex", true),
        ("contentIndexEntries", true),
        ("battery", false),
    ] {
        let plan = built(|b| {
            let ty = b.primitive(TypeKind::Number);
            b.source(name, &[], ty);
        });
        assert_eq!(
            apple_uses(&plan).has(Capability::ContentIndex),
            selected,
            "{name}"
        );
        assert!(!uses(&plan).has(Capability::ContentIndex));
    }
}

/// On Apple a reorder is the lists family's: a reorder grip selects
/// `Collections` and not `Drag`, which only a height or transform drag
/// selects. Elsewhere a grip still names `drag`.
#[test]
fn apple_links_drag_for_height_and_transform_drags_only() {
    use Capability::{Collections, Drag};
    let grip = plan("column id=\"rows\" height=200\n      column reorderFor=\"rows\" height=20");
    assert!(apple_uses(&grip).has(Collections));
    assert!(!apple_uses(&grip).has(Drag));
    assert!(uses(&grip).has(Drag));
    let sheet = plan(
        "column id=\"sheet\" box-sizing=\"border-box\" height=200\n      column heightDragFor=\"sheet\" height=20",
    );
    assert!(apple_uses(&sheet).has(Drag));
    let photo = plan(
        "column id=\"photo\" box-sizing=\"border-box\" width=100 height=100\n      column transformDragFor=\"photo\"",
    );
    assert!(apple_uses(&photo).has(Drag));
    // A list's `reorderdrop` is the reorder's too.
    let dropped = contract::compile(
        "component A\n  state rows = [\"a\", \"b\"]\n  action receive(item: string, before: option<string>)\n    rows = rows\n  view\n    list id=\"rows\" virtualized=true estimated-item-height=20 height=100 reorderdrop=receive\n      for x in rows key=x\n        column reorderFor=\"rows\" height=20\n          text x\n",
    )
    .unwrap();
    assert!(apple_uses(&dropped).has(Collections));
    assert!(!apple_uses(&dropped).has(Drag));
    assert!(uses(&dropped).has(Drag));
}

/// A link the host's own context menu serves links sharing, as a plan's
/// `share` does; a link that menu refuses does not, and the web's set moves
/// for neither.
#[test]
fn a_link_with_the_hosts_menu_links_sharing() {
    for (href, linked) in [
        ("https://example.com", true),
        ("HTTP://example.com", true),
        ("/about", true),
        ("about#top", true),
        ("mailto:a@example.com", false),
        ("tel:123", false),
        ("", false),
    ] {
        let plan = plan(&format!("link href=\"{href}\"\n      text \"a\""));
        assert_eq!(apple_uses(&plan).has(Capability::Share), linked, "{href:?}");
        assert!(!uses(&plan).has(Capability::Share), "{href:?}");
    }
    let computed = contract::compile(
        "component A\n  state u = \"x\"\n  view\n    link href=u\n      text \"a\"\n",
    )
    .unwrap();
    assert!(
        apple_uses(&computed).has(Capability::Share),
        "a computed href can be any"
    );
}
