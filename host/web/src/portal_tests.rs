//! @ref LLP 1013.002 D6 — a `portal` mover whose shown parent changes is held
//! before the batch's destroys and lists, and one shown nowhere stays held.
use exact_runner::Event;

const APP: &str = r#"component App
  state tab = "other"
  state count = 0
  action show
    tab = "card"
  action hide
    tab = "other"
  action add
    count = count + 1
  view
    column
      button "Show" press=show testId="show"
      button "Hide" press=hide testId="hide"
      button `${count}` press=add testId="add"
      if tab == "card"
        box id="card" testId="card" width=80 height=60
      column portal="card" testId="mover"
        video "assets/motion.mp4" autoplay=true muted=true
"#;

#[test]
fn a_movers_element_waits_in_the_holder_until_a_list_shows_it() {
    let plan = contract::compile(APP).unwrap();
    let (mut host, boot) = crate::Host::boot(
        &plan.encode(),
        caltrain_data::Caltrain,
        Default::default(),
        "/",
    )
    .unwrap();
    let kernel = host.runner().kernel();
    let id = |name| {
        kernel
            .node_by_key(kernel.find_by_test_id(name)[0])
            .unwrap()
            .id
    };
    let [show, hide, add, mover] = ["show", "hide", "add", "mover"].map(id);
    let at = |batch: &str, op: &str| batch.find(op).unwrap_or_else(|| panic!("{op} in {batch}"));
    // Hidden at boot: made, then held.
    let hold = format!("{{\"op\":\"hold\",\"movers\":[[{mover},null]]}}");
    assert!(at(&boot, &format!("{{\"op\":\"create\",\"id\":{mover},")) < at(&boot, &hold));
    // Shown: held to be taken by the card's list.
    let shown = host.dispatch(show, Event::Press);
    let card = {
        let kernel = host.runner().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id("card")[0])
            .unwrap()
            .id
    };
    let to_card = format!("{{\"op\":\"hold\",\"movers\":[[{mover},{card}]]}}");
    let list = format!("{{\"op\":\"children\",\"id\":{card},\"ids\":[{mover}]}}");
    assert!(at(&shown, &to_card) < at(&shown, &list), "{shown}");
    // A commit that moves nothing holds nothing.
    assert!(!host.dispatch(add, Event::Press).contains("\"op\":\"hold\""));
    // Hidden again: held before the card is destroyed.
    let hidden = host.dispatch(hide, Event::Press);
    let destroy = format!("{{\"op\":\"destroy\",\"id\":{card}}}");
    assert!(at(&hidden, &hold) < at(&hidden, &destroy), "{hidden}");
}
