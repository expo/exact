//! @ref LLP 1047 D2 — what a plan uses is read from its rows, never declared.

use exact_runner::{uses, Capability, Uses};

fn used(source: &str) -> Uses {
    uses(&contract::compile(source).unwrap_or_else(|e| panic!("{e}")))
}

#[test]
fn markdown_is_a_markup_prop_that_can_be_markdown() {
    let plain = used("component A\n  view\n    text \"**b**\"\n");
    assert_eq!(plain, Uses::NONE);
    let constant = used("component A\n  view\n    text \"**b**\" markup=\"markdown\"\n");
    assert!(constant.has(Capability::Markdown));
    assert_eq!(constant.to_string(), "markdown");
    // Another constant selects nothing; a computed one might be Markdown.
    let other = used("component A\n  view\n    text \"**b**\" markup=\"none\"\n");
    assert_eq!(other, Uses::NONE);
    let computed = used(
        "component A\n  state rich = true\n  view\n    text \"**b**\" markup=(rich ? \"markdown\" : \"none\")\n",
    );
    assert!(computed.has(Capability::Markdown));
}

#[test]
fn motion_is_a_spring_or_a_gesture_that_holds_a_value() {
    let css =
        used("component A\n  view\n    text \"a\" opacity=0.5 transition=\"opacity 200ms ease\"\n");
    // The browser plays an easing transition; a native engine plays it as
    // `transitions` (LLP 1047.001), never as motion.
    assert_eq!(css.to_string(), "transitions");
    assert!(!css.has(Capability::Motion));
    // Every value of the row parses by the transitions grammar, `none` too
    // (LLP 1047.001 §9.9), but `none` plays nothing as motion.
    let none = used("component A\n  view\n    text \"a\" opacity=0.5 transition=\"none\"\n");
    assert_eq!(none.to_string(), "transitions");
    assert!(!none.has(Capability::Motion));
    let spring = used(
        "component A\n  view\n    text \"a\" scale=1.5 transition=\"scale -exact-spring(180, 12, 1)\"\n",
    );
    assert!(spring.has(Capability::Motion));
    let swipe = used(
        "component A\n  state n = 0\n  action swipe\n    n = n + 1\n  view\n    text \"a\" swipe=swipe swipe-follow=\"right\"\n",
    );
    assert!(swipe.has(Capability::Motion));
    assert!(
        !swipe.has(Capability::Drag),
        "a swipe holds a value but tracks no handle"
    );
    let pan = used(
        "component A\n  state n = 0\n  action move(dx: number, dy: number)\n    n = n + dx\n  view\n    text \"a\" pan=move\n",
    );
    assert_eq!(pan, Uses::NONE, "a pan commits state; it holds nothing");
    let release = used(
        "component A\n  state n = 0\n  action move(dx: number, dy: number)\n    n = n + dx\n  view\n    text \"a\" pan=move panrelease=move\n",
    );
    assert!(
        release.has(Capability::Motion) && !release.has(Capability::Drag),
        "a pan's release velocity is the engine's tracker (LLP 1057 §10.6)"
    );
    let drag = used(
        "component A\n  view\n    column id=\"sheet\" height=100\n      column heightDragFor=\"sheet\"\n",
    );
    assert!(drag.has(Capability::Motion) && drag.has(Capability::Drag));
    assert_eq!(
        spring.with(Capability::Markdown).to_string(),
        "markdown, motion, transitions"
    );
}

#[test]
fn collections_are_lists_the_host_windows() {
    let list = |attrs: &str| {
        used(&format!(
            "component A\n  resource rows = rows() as shape string[]\n  view\n    list {attrs}\n      for x in rows key=x\n        column\n          text x\n"
        ))
    };
    assert_eq!(list("height=100"), Uses::NONE, "a plain list is the core's");
    assert!(list("virtualized=true height=100").has(Capability::Collections));
    assert_eq!(list("virtualized=false height=100"), Uses::NONE);
    assert!(
        list("virtualized=true estimated-item-height=20 height=100").has(Capability::Collections)
    );
}

#[test]
fn surfaces_are_a_canvas_with_one() {
    assert_eq!(used("component A\n  view\n    canvas\n"), Uses::NONE);
    assert!(used("component A\n  view\n    canvas surface=sky()\n").has(Capability::Surfaces));
    let record = used(
        "shape Hud\n  beacons: number\ncomponent App\n  resource hud = exactSurface(\"world\") as shape Hud\n  view\n    text `${hud.beacons}`\n",
    );
    assert!(
        record.has(Capability::Surfaces),
        "a surface's record is read without a canvas"
    );
}

#[test]
fn the_router_is_a_plan_with_routes() {
    assert!(!used("component A\n  view\n    text \"a\"\n").has(Capability::Router));
    let routed = used("routes nav\n  tab home \"/\"\ncomponent App\n  view\n    text \"a\"\n");
    assert!(routed.has(Capability::Router));
}

#[test]
fn format_is_a_call_of_format_date_or_format_number_anywhere() {
    // `formatTime` is the core's, as `formatClockTime` was (LLP 1054.000.003 D1).
    let time = used("component A\n  view\n    text formatTime(0, 0, \"short\")\n");
    assert_eq!(time, Uses::NONE);
    let date = used("component A\n  view\n    text formatDate(0, 0, \"medium\")\n");
    assert_eq!(date.to_string(), "format");
    let day = used("component A\n  view\n    text formatDay(0, 0, 0, \"list\")\n");
    assert_eq!(day.to_string(), "format");
    // In an action's body, a derive, a callback and an inlined `fn` too.
    let action = used(
        "component A\n  state s = \"\"\n  action set\n    s = formatNumber(1250, \"compact\")\n  view\n    button s press=set\n",
    );
    assert!(action.has(Capability::Format));
    let callback = used(
        "component A\n  resource xs = xs() as shape number[]\n  derive ys = xs.map(x => formatNumber(x, \"compact\"))\n  view\n    text ys.join(\",\")\n",
    );
    assert!(callback.has(Capability::Format));
    let wrapped = used(
        "fn count(n: number): string = formatNumber(n, \"compact\")\ncomponent A\n  view\n    text count(3)\n",
    );
    assert!(wrapped.has(Capability::Format));
}

/// LLP 1088 D2: `toLowerCase` and `toUpperCase` reach the case tables
/// `text-transform` links, so a call links them; `slice` and `replaceAll`
/// need nothing.
#[test]
fn to_lower_case_links_the_case_tables_and_slice_links_nothing() {
    let cut = used(
        "component A\n  state s = \"Ab\"\n  view\n    text s.slice(1).replaceAll(\"b\", \"c\")\n",
    );
    assert_eq!(cut, Uses::NONE);
    let lower = used("component A\n  state s = \"Ab\"\n  view\n    text s.toLowerCase()\n");
    assert_eq!(lower.to_string(), "text_transform");
    let upper = used("component A\n  state s = \"Ab\"\n  view\n    text s.toUpperCase()\n");
    assert_eq!(upper.to_string(), "text_transform");
}

#[test]
fn materials_and_backdrop_filter_are_linked_by_use() {
    // @ref LLP 1053.000 §2 — the web core carries neither unless a plan names one.
    let plain = used("component A\n  view\n    text \"a\" white-space=\"pre\"\n");
    assert!(!plain.has(Capability::Materials) && !plain.has(Capability::Backdrop));
    assert!(
        used("component A\n  view\n    box backgroundMaterial=\"thin\"\n")
            .has(Capability::Materials)
    );
    assert!(
        used("component A\n  view\n    box backdrop-filter=\"blur(4px)\"\n")
            .has(Capability::Backdrop)
    );
}

#[test]
fn drag_timelines_are_linked_by_use() {
    // @ref LLP 1057.003 — the web core carries their grammar only when a plan sets a row.
    assert!(!used("component A\n  view\n    text \"a\"\n").has(Capability::Timelines));
    for row in [
        "-exact-drag-timeline=\"--dismiss\"",
        "animation-timeline=\"--dismiss\"",
        "animation-range=\"0px 300px\"",
        "timeline-scope=\"--dismiss\"",
        "timeline-scope=\"all\"",
    ] {
        let set = used(&format!("component A\n  view\n    box {row}\n"));
        assert!(
            set.has(Capability::Timelines) && set.has(Capability::Motion),
            "{row}"
        );
    }
}

#[test]
fn text_transform_is_linked_by_use() {
    // @ref LLP 1064 D5 — the web core carries no case tables unless a plan binds the row.
    assert!(!used("component A\n  view\n    text \"a\"\n").has(Capability::TextTransform));
    assert!(
        used("component A\n  view\n    text \"a\" text-transform=\"uppercase\"\n")
            .has(Capability::TextTransform)
    );
    assert!(used(
        "style S\n  text-transform=\"capitalize\"\ncomponent A\n  view\n    text \"a\" class=S\n"
    )
    .has(Capability::TextTransform));
}

#[test]
fn filter_and_clip_path_are_linked_by_use() {
    // @ref LLP 1055.000 D10, D14 — the web core carries neither grammar unless a plan binds one.
    assert!(!used("component A\n  view\n    box opacity=0.5\n").has(Capability::Effects));
    assert!(used("component A\n  view\n    box filter=\"blur(2px)\"\n").has(Capability::Effects));
    assert!(used("component A\n  view\n    box clip-path=\"none\"\n").has(Capability::Effects));
}

#[test]
fn css_animations_are_linked_by_use() {
    // @ref LLP 1055 D5 — the web core carries neither grammar unless a plan animates.
    assert!(!used("component A\n  view\n    box opacity=0.5\n").has(Capability::Animations));
    // `none` parses too, so a binding alone links the grammar.
    assert!(used("component A\n  view\n    box animation=\"none\"\n").has(Capability::Animations));
    assert!(used(
        "keyframes fade\n  from opacity=0\ncomponent A\n  view\n    box animation=\"fade 1s\"\n"
    )
    .has(Capability::Animations));
    assert!(
        used("component A\n  view\n    box -exact-exit-animation=\"none\"\n")
            .has(Capability::Animations)
    );
}

#[test]
fn gradients_are_linked_by_use() {
    // @ref LLP 1066 — the web core carries no gradient grammar unless a plan binds the row.
    assert!(!used("component A\n  view\n    box opacity=0.5\n").has(Capability::Gradients));
    assert!(used(
        "component A\n  view\n    view background-image=\"linear-gradient(#000, #fff)\"\n"
    )
    .has(Capability::Gradients));
}

#[test]
fn visual_grammars_follow_literal_class_and_computed_rows() {
    assert_eq!(used("component A\n  view\n    text \"a\"\n"), Uses::NONE);
    for (row, value, capability) in [
        ("box-shadow", "0 2px 4px #000", "shadows"),
        ("text-shadow", "1px 2px #000", "shadows"),
        ("corner-shape", "squircle", "corners"),
        ("rotate", "y 30deg", "rotate"),
        ("-webkit-text-stroke", "1px #f00", "text_stroke"),
        ("-webkit-text-stroke-width", "1px", "text_stroke"),
        ("-webkit-text-stroke-color", "#f00", "text_stroke"),
        ("-exact-symbol-palette", "#f00 #00f", "symbol_palette"),
        (
            "background-image",
            "conic-gradient(#f00, #00f)",
            "gradients",
        ),
        ("mask-image", "linear-gradient(#000, #fff)", "gradients"),
    ] {
        for source in [
            format!("component A\n  view\n    text \"a\" {row}=\"{value}\"\n"),
            format!("style S\n  {row}=\"{value}\"\ncomponent A\n  view\n    text \"a\" class=S\n"),
            format!(
                "component A\n  state paint = \"{value}\"\n  view\n    text \"a\" {row}=paint\n"
            ),
        ] {
            assert_eq!(used(&source).to_string(), capability, "{source}");
        }
    }
}

#[test]
fn linked_grammars_follow_literal_class_and_computed_rows() {
    // A native artifact parses these rows only through the grammar it links by
    // use, so the row selects it however its value is written (LLP 1047 D2).
    for (row, value, alternative, capability) in [
        ("animation", "none", "none 1s", "animations"),
        ("-exact-exit-animation", "none", "none 1s", "animations"),
        ("filter", "blur(2px)", "none", "effects"),
        ("clip-path", "inset(2px)", "none", "effects"),
        ("grid-template-columns", "1fr 2fr", "auto", "grid"),
        ("grid-template-rows", "auto 40px", "25%", "grid"),
        ("grid-column", "1 / span 2", "auto", "grid"),
        ("grid-row", "rail / 2", "span 2", "grid"),
        ("grid-auto-flow", "column dense", "row", "grid"),
        ("justify-items", "safe center", "stretch", "grid"),
        ("transition", "opacity 1s", "none", "transitions"),
        ("-exact-layout-transition", "1s", "none", "transitions"),
        (
            "background-image",
            "linear-gradient(#000, #fff)",
            "none",
            "gradients",
        ),
        (
            "mask-image",
            "linear-gradient(#000, #fff)",
            "none",
            "gradients",
        ),
    ] {
        for source in [
            format!("component A\n  view\n    box {row}=\"{value}\"\n"),
            format!("style S\n  {row}=\"{value}\"\ncomponent A\n  view\n    text \"a\" class=S\n"),
            format!(
                "component A\n  state paint = \"{value}\"\n  view\n    box {row}=paint\n"
            ),
            format!(
                "component A\n  state on = true\n  view\n    box {row}=(on ? \"{value}\" : \"{alternative}\")\n"
            ),
        ] {
            assert!(
                used(&source).iter().any(|c| c.name() == capability),
                "{source}: {}",
                used(&source)
            );
        }
    }
}

#[test]
fn every_row_a_linked_grammar_parses_names_its_capability() {
    // The kernel's table, row by row: a row whose value parses by a grammar
    // linked by use names the capability that links it (kernel/build/codec.rs
    // refuses to generate one that does not).
    use exact_kernel::{StyleCodec, StyleId};
    for style in StyleId::ALL {
        let wanted = match style.codec() {
            StyleCodec::Animations => Some("animations"),
            StyleCodec::Filter | StyleCodec::ClipPath => Some("effects"),
            StyleCodec::Tracks | StyleCodec::Placement => Some("grid"),
            StyleCodec::Transitions => Some("transitions"),
            StyleCodec::BackgroundImage | StyleCodec::MaskImage => Some("gradients"),
            _ => None,
        };
        if let Some(wanted) = wanted {
            assert_eq!(style.capability(), Some(wanted), "{style:?}");
        }
    }
}

#[test]
fn decoration_styles_link_their_visual_grammars() {
    let field = "component A\n  view\n    textarea decorations=TextDecorations(value=\"a\", spans=[TextDecorationSpan(name=\"glow\", start=0, end=1)])\n";
    assert_eq!(used(field), Uses::NONE);
    assert_eq!(
        used(&format!("decoration glow color=\"#000\"\n{field}")),
        Uses::NONE
    );
    let shadow = "decoration glow text-shadow=\"0 0 1px #000\"\n";
    assert_eq!(used(&format!("{shadow}{field}")).to_string(), "shadows");
    assert_eq!(
        used(&format!("{shadow}component A\n  view\n    text \"a\"\n")),
        Uses::NONE
    );
}

#[test]
fn a_decoration_table_that_is_not_a_literal_can_name_any_row() {
    // The compiler writes the table as a literal; a decoded plan whose table
    // is computed selects what any row can, on Apple a box's `filter` too.
    let source = "decoration glow color=\"#000\"\ncomponent A\n  state rows = \"{}\"\n  view\n    column\n      text rows\n      textarea decorations=TextDecorations(value=\"a\", spans=[])\n";
    let mut plan = contract::compile(source).unwrap();
    let is_prop = |b: &exact_plan::BindingsRow, prop| {
        b.kind == exact_plan::BindingKind::Prop
            && exact_kernel::PropId::from_wire(b.id) == Some(prop)
    };
    let computed = plan
        .bindings
        .iter()
        .find(|b| is_prop(b, exact_kernel::PropId::Text))
        .unwrap()
        .expr;
    let table = plan
        .bindings
        .to_mut()
        .iter_mut()
        .find(|b| is_prop(b, exact_kernel::PropId::DecorationStyles))
        .unwrap();
    table.expr = computed;
    for capability in [
        Capability::Animations,
        Capability::Effects,
        Capability::Shadows,
    ] {
        assert!(uses(&plan).has(capability), "{capability:?}");
    }
    assert!(exact_runner::uses::apple_uses(&plan).has(Capability::Effects));
}

#[test]
fn data_words_and_tabs_are_linked_by_use() {
    // @ref LLP 1075.003 §3.3, §3.7 — the web core carries neither the
    // `dataset` reading nor the tabs' walk unless a plan binds the row.
    let plain = used("component A\n  view\n    box testId=\"a\"\n");
    assert!(!plain.has(Capability::Dataset) && !plain.has(Capability::Tabs));
    let words = used("component A\n  view\n    box data-screen=\"home\"\n");
    assert_eq!(words.to_string(), "dataset");
    let tabs = used(
        "component A\n  view\n    column\n      row role=\"tablist\"\n        button role=\"tab\" aria-controls=\"one\"\n          text \"One\"\n      column role=\"tabpanel\" id=\"one\"\n",
    );
    assert_eq!(tabs.to_string(), "tabs");
}

#[test]
fn a_set_names_what_it_holds_beyond_another() {
    let markdown = Uses::NONE.with(Capability::Markdown);
    assert!(markdown.beyond(markdown).is_empty());
    assert_eq!(markdown.beyond(Uses::NONE), markdown);
    assert!(Uses::NONE.beyond(markdown).is_empty());
    assert_eq!(Uses::NONE.to_string(), "");
}

#[test]
fn share_is_a_plan_that_runs_the_command() {
    // @ref LLP 1069.003 — linked by use on the web (the web core's size).
    assert!(!used("component A\n  view\n    text \"a\"\n").has(Capability::Share));
    let shares = used(
        "component A\n  action send\n    share(text=\"hello\")\n  view\n    button \"Share\" press=send\n",
    );
    assert!(shares.has(Capability::Share));
}

#[test]
fn documents_are_a_plan_that_runs_save_file_or_a_file_picker() {
    // @ref LLP 1069.010 — linked by use on the web (the web core's size).
    assert!(!used("component A\n  view\n    text \"a\"\n").has(Capability::Documents));
    let picks = used(
        "component A\n  action open\n    showOpenFilePicker(\"opened\")\n  view\n    button \"Open\" press=open\n",
    );
    assert!(picks.has(Capability::Documents) && !picks.has(Capability::Share));
}

#[test]
fn a_drag_into_the_app_is_a_plan_that_hears_a_drop_or_its_enter_or_leave() {
    // @ref LLP 1094.001 D4 — linked by use on UIKit; the Mac's core and the web's documents glue take drops
    for event in ["drop", "dragenter", "dragleave"] {
        let heard = used(&format!(
            "component A\n  state n = 0\n  action took(e: DragEvent)\n    n = n + 1\n  view\n    column {event}=took\n      text \"zone\"\n"
        ));
        assert!(
            heard.has(Capability::DragIn) && heard.has(Capability::Documents),
            "{event}"
        );
    }
    let pasted = used(
        "component A\n  state n = 0\n  action pasted(e: ClipboardEvent)\n    n = n + 1\n  view\n    textarea paste=pasted\n",
    );
    assert!(pasted.has(Capability::Documents) && !pasted.has(Capability::DragIn));
    assert!(Uses::NATIVE.has(Capability::DragIn) && Uses::MAC_CORE.has(Capability::DragIn));
}

#[test]
fn the_picker_is_a_file_input_or_show_picker() {
    // @ref LLP 1069.002 — linked by use on the web (the web core's size).
    assert!(!used("component A\n  view\n    text \"a\"\n").has(Capability::Picker));
    let input = used("component A\n  view\n    input type=\"file\" accept=\"image/*\" id=\"f\"\n");
    assert!(input.has(Capability::Picker));
}

/// LLP 1047.001.000 E: iOS's photo picker is a file input whose `accept` is
/// only images and videos (the compiler takes only a literal); a document
/// input links the picker without it.
#[test]
fn the_media_picker_is_a_file_input_that_accepts_only_media() {
    let input = |attrs: &str| {
        used(&format!("component A\n  view\n    column\n      input type=\"file\" id=\"f\"{attrs}\n      text \"a\"\n"))
    };
    for media in [" accept=\"image/*\"", " accept=\"image/png, video/mp4\""] {
        let uses = input(media);
        assert!(
            uses.has(Capability::Picker) && uses.has(Capability::MediaPicker),
            "{media}"
        );
    }
    for document in ["", " accept=\"image/*, */*\"", " accept=\"*/*\""] {
        let uses = input(document);
        assert!(
            uses.has(Capability::Picker) && !uses.has(Capability::MediaPicker),
            "{document}"
        );
    }
}

#[test]
fn svg_islands_are_a_mask_a_filter_element_or_a_filter_on_an_svg_element() {
    // @ref LLP 1055.000 D10, D14 — what makes a native host open its island module.
    let islands = |s: &str| {
        exact_runner::svg_islands(&contract::compile(s).unwrap_or_else(|e| panic!("{e}")))
    };
    assert!(!islands("component A\n  view\n    svg viewBox=\"0 0 10 10\"\n      rect width=10 height=10 fill=\"#f00\"\n"));
    assert!(
        !islands("component A\n  view\n    box filter=\"blur(2px)\"\n"),
        "a box's filter is not an island"
    );
    assert!(islands("component A\n  view\n    svg viewBox=\"0 0 10 10\"\n      rect width=10 height=10 filter=\"blur(1px)\"\n"));
    let mask = "component A\n  view\n    svg viewBox=\"0 0 10 10\"\n      defs\n        mask id=\"m\"\n          rect width=5 height=5 fill=\"#fff\"\n      rect width=10 height=10 mask=\"url(#m)\"\n";
    assert!(islands(mask));
    // A mask alone is an island and no filter (no GPU filter pipelines).
    let filters = |s: &str| {
        exact_runner::svg_filters(&contract::compile(s).unwrap_or_else(|e| panic!("{e}")))
    };
    assert!(!filters(mask));
    assert!(filters("component A\n  view\n    svg viewBox=\"0 0 10 10\"\n      g filter=\"blur(1px)\"\n        rect width=10 height=10\n"));
    assert!(!filters(
        "component A\n  view\n    box filter=\"blur(2px)\"\n"
    ));
}

/// LLP 1047.001 D2: a grouped list (`list appearance="auto"`, which Contract
/// marks with `listStyle`) is a native capability; a plain list is not.
#[test]
fn a_grouped_list_is_a_list_whose_appearance_is_auto() {
    let plain = used("component A\n  view\n    list\n      text \"a\"\n");
    assert!(!plain.has(Capability::GroupedLists));
    let grouped = used(
        "component A\n  view\n    list appearance=\"auto\"\n      section\n        text \"a\"\n",
    );
    assert!(grouped.has(Capability::GroupedLists));
    assert_eq!(grouped.to_string(), "grouped_lists, corners");
}

/// LLP 1115.001.000 D2: a date or time control, which on Android links
/// Compose Material 3 for its picker.
#[test]
fn dates_are_a_date_or_time_control() {
    for kind in ["date", "time", "datetime-local"] {
        let plan = format!("component A\n  view\n    input type=\"{kind}\" value=\"\"\n");
        assert!(used(&plan).has(Capability::Dates), "{kind}");
    }
    assert!(!used("component A\n  view\n    input value=\"\"\n").has(Capability::Dates));
    assert!(!used("component A\n  view\n    input type=\"file\"\n").has(Capability::Dates));
}

/// LLP 1035.006.000 D5: a tip links the tips capability (TipKit on Apple);
/// one whose title is always empty shows nothing and links nothing.
#[test]
fn tips_are_an_element_with_a_tip() {
    let button = |attrs: &str| {
        format!("component A\n  state t = \"\"\n  view\n    button id=\"add\"{attrs}\n      text \"Add\"\n")
    };
    let tipped = used(&button(" tip=\"Add a station\""));
    assert!(tipped.has(Capability::Tips));
    assert_eq!(tipped.to_string(), "tips");
    assert!(
        used(&button(" tip=t")).has(Capability::Tips),
        "a computed title may be one"
    );
    assert!(!used(&button(" tip=\"\"")).has(Capability::Tips));
    assert!(!used(&button("")).has(Capability::Tips));
}
