use super::grouped_list::{boot_source, id};

#[test]
fn a_sections_host_state_predicates_stay_on_the_section() {
    let r = boot_source(
        r#"component App
  view
    list appearance="auto" testId="root" width=402 height=874
      section testId="section" tabindex=0 disabled=true
        :hover
          box-shadow="0 8px 0 #ff0000"
        :focus-visible
          box-shadow="0 12px 0 #0000ff"
        row testId="row"
          text "Profile"
"#,
    );
    let section = r.kernel().node(id(&r, "section")).unwrap();
    let card = r
        .kernel()
        .node(r.kernel().node(id(&r, "row")).unwrap().parent.unwrap())
        .unwrap();
    assert_eq!(
        section.looks().len(),
        2,
        "the focusable section owns both predicates"
    );
    assert!(
        card.looks().is_empty(),
        "the card projects its section's shadow"
    );
}

#[test]
fn a_sections_shadow_belongs_to_its_rounded_card() {
    for style in [
        "box-shadow=\"0 4px 14px #0008\"",
        "class=Raised",
        "class=Raised box-shadow=\"0 4px 14px #0008\"",
    ] {
        let source = format!(
            r#"style Raised
  box-shadow="0 4px 14px #0008"
component App
  view
    column testId="root" width=402 height=874
      list appearance="auto" testId="list" flex=1
        section testId="section" {style}
          header
            text "Account"
          row testId="row"
            text "Profile"
          footer
            text "A note"
"#
        );
        let r = boot_source(&source);
        let k = r.kernel();
        let section = k.node(id(&r, "section")).unwrap();
        let row = k.node(id(&r, "row")).unwrap();
        let card = k.node(row.parent.unwrap()).unwrap();
        assert_eq!(card.style.box_shadow.shadows().len(), 1, "{style}");
        assert!(section.style.box_shadow.shadows().is_empty(), "{style}");
        assert_eq!(
            k.grouped_list(id(&r, "list")).unwrap().sections[0].card_view,
            card.id
        );
        assert!(card.frame.width < section.frame.width);
        assert!(card.frame.height < section.frame.height);
    }
}

#[test]
fn a_cardless_section_refuses_a_shadow() {
    let source = r#"component App
  view
    list appearance="auto"
      section background-color="transparent" box-shadow="0 4px 14px #0008"
        row
          text "Profile"
"#;
    let error = contract::compile(source).unwrap_err();
    assert_eq!(error.id, "lower-grouped-list");
    assert!(error.message.contains("shadow belongs to the card"));
}

#[test]
fn a_sections_shadow_follows_disabled_and_checked_states() {
    for class in [false, true] {
        let parts = "  :checked\n    box-shadow=\"0 8px 0 #ff0000\"\n  :disabled\n    box-shadow=\"none\"\n";
        let source = format!(
            r#"{style}component App
  state off = false
  state selected = false
  action disable
    off = not off
  action select
    selected = not selected
  view
    column testId="root" width=402 height=874
      button press=disable testId="disable"
        text "Disable"
      button press=select testId="select"
        text "Select"
      list appearance="auto" flex=1
        section testId="section" disabled=off aria-selected=selected box-shadow="0 4px 0 #0008" {class_attr}
{inline}          row testId="row"
            text "Profile"
"#,
            style = if class {
                format!("style Raised\n{parts}")
            } else {
                String::new()
            },
            class_attr = if class { "class=Raised" } else { "" },
            inline = if class {
                String::new()
            } else {
                parts
                    .lines()
                    .map(|line| format!("        {line}\n"))
                    .collect::<String>()
            }
        );
        let mut r = boot_source(&source);
        let card = r.kernel().node(id(&r, "row")).unwrap().parent.unwrap();
        for (action, offset) in [
            (None, Some(4.)),
            (Some("disable"), None),
            (Some("select"), None),
            (Some("disable"), Some(8.)),
            (Some("select"), Some(4.)),
        ] {
            if let Some(action) = action {
                r.dispatch(id(&r, action), exact_runner::Event::Press)
                    .unwrap();
            }
            assert_eq!(
                r.kernel()
                    .node(card)
                    .unwrap()
                    .style
                    .box_shadow
                    .shadows()
                    .first()
                    .map(|shadow| shadow.offset.y),
                offset,
                "{action:?}, class={class}"
            );
            assert!(r
                .kernel()
                .node(id(&r, "section"))
                .unwrap()
                .style
                .box_shadow
                .shadows()
                .is_empty());
        }
    }
}

#[test]
fn a_card_keeps_only_its_shadow_transition_and_the_section_keeps_opacity() {
    use exact_motion::Property;
    for transition in [
        "box-shadow 1s linear, opacity 2s ease",
        "all 3s ease, box-shadow 1s linear, opacity 2s ease",
        "box-shadow 3s ease, 1s linear, opacity 2s ease",
    ] {
        let source = format!(
            r#"component App
  view
    column testId="root" width=402 height=874
      list appearance="auto" flex=1
        section testId="section" box-shadow="0 4px 14px #0008" transition="{transition}"
          row testId="row"
            text "Profile"
"#
        );
        let r = boot_source(&source);
        let k = r.kernel();
        let section = &k.node(id(&r, "section")).unwrap().style.transition;
        let card = &k
            .node(k.node(id(&r, "row")).unwrap().parent.unwrap())
            .unwrap()
            .style
            .transition;
        assert_eq!(
            card.matching(Property::BoxShadow)
                .expect("card transition")
                .duration,
            1.
        );
        assert!(card.matching(Property::Opacity).is_none());
        assert_eq!(section.matching(Property::Opacity).unwrap().duration, 2.);
        assert!(
            !section
                .0
                .iter()
                .any(|t| t.property
                    == exact_motion::TransitionProperty::Property(Property::BoxShadow))
        );
    }
}

#[test]
fn a_grouped_section_refuses_shadow_keyframes() {
    for animation in [
        "animation=\"glow 1s linear\"",
        "animation-name=\"glow\" animation-duration=\"1s\"",
        "animation-name=\"glow\" animation-duration=(on ? \"1s\" : \"2s\")",
        "animation-name=(on ? \"fade\" : \"glow\") animation-duration=\"1s\"",
        "animation=(on ? \"fade 1s\" : \"glow 1s\")",
        "animation=motion",
        "-exact-exit-animation=\"glow 1s\"",
        "class=Animated",
    ] {
        for frames in [
            "from box-shadow=\"0 8px 0 #f00\"\n  to box-shadow=\"0 16px 0 #00f\"",
            "to box-shadow=\"0 16px 0 #00f\"",
            "from box-shadow=\"0 8px 0 #f00\"",
        ] {
            let source = format!(
                r#"keyframes glow
  {frames}
keyframes fade
  to opacity=0.5
style Animated
  animation="glow 1s"
component App
  state on = false
  state motion = "glow 1s"
  view
    list appearance="auto"
      section {animation}
        row
          text "Profile"
"#
            );
            let error = contract::compile(&source).unwrap_err();
            assert_eq!(
                error.id, "lower-grouped-shadow-animation",
                "{animation}: {error:?}"
            );
            assert!(error.message.contains("shadow belongs to the card"));
            assert!(error.message.contains("not supported yet"));
            assert!(error.message.contains("transition"));
        }
    }
}

#[test]
fn a_grouped_section_keeps_animations_without_shadow_keyframes() {
    let source = r#"keyframes glow
  to box-shadow="0 16px 0 #00f"
keyframes fade
  to opacity=0.5
component App
  view
    column
      column animation="glow 1s"
      list appearance="auto"
        section animation="fade 1s" box-shadow="0 8px 0 #f00"
          row
            text "Profile"
"#;
    contract::compile(source).unwrap();
}

#[test]
fn a_grouped_sections_animation_names_do_not_depend_on_its_timing() {
    for animation in [
        "animation-name=\"fade\" animation-duration=(fast ? \"1s\" : \"2s\")",
        "animation=\"glow 1s\" animation-name=\"fade\" animation-duration=(fast ? \"1s\" : \"2s\")",
        "animation-name=(fast ? \"fade\" : \"none\") animation-duration=\"1s\"",
    ] {
        let source = format!(
            r#"keyframes glow
  to box-shadow="0 16px 0 #00f"
keyframes fade
  to opacity=0.5
component App
  state fast = false
  view
    list appearance="auto"
      section {animation}
        row
          text "Profile"
"#
        );
        contract::compile(&source).unwrap_or_else(|e| panic!("{animation}: {e:?}"));
    }
}

#[test]
fn a_grouped_sections_class_can_leave_animation_absent() {
    let source = r#"keyframes glow
  to box-shadow="0 16px 0 #00f"
keyframes fade
  to opacity=0.5
style Animated
  animation="fade 1s"
style Quiet
  opacity=1
component App
  state on = false
  view
    list appearance="auto"
      section class=(on ? Animated : Quiet)
        row
          text "Profile"
"#;
    contract::compile(source).unwrap();
}

#[test]
fn a_cardless_grouped_section_refuses_shadow_keyframes() {
    let source = r#"keyframes glow
  to box-shadow="0 16px 0 #00f"
component App
  view
    list appearance="auto"
      section background-color="transparent" animation="glow 1s"
        row
          text "Profile"
"#;
    let error = contract::compile(source).unwrap_err();
    assert_eq!(error.id, "lower-grouped-shadow-animation");
}
