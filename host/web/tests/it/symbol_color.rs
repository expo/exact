//! Whether a symbol draws in its author's `color` (`data-symbol-color`)
//! reads the same from the kernel-free document as from the kernel: the
//! neutral sheet's starts that set `color` stop the walk in both.

use exact_web::Host;

#[test]
fn the_document_and_the_kernel_agree_on_a_symbols_said_colour() {
    let plan = contract::compile(
        r##"component App
  view
    column color="-exact-system-red"
      image "symbol:star" testId="page"
      dialog id="plain"
        image "symbol:star" testId="in-dialog"
      column id="surface" popover="auto"
        image "symbol:star" testId="in-popover"
      column id="painted" popover="auto" background-color="-exact-fill"
        image "symbol:star" testId="in-painted"
      column id="menu" popover="auto" role="menu"
        image "symbol:star" testId="in-menu"
"##,
    )
    .unwrap();
    let (mut host, _) = Host::boot(
        &plan.encode(),
        caltrain_data::Caltrain,
        Default::default(),
        "/",
    )
    .unwrap();
    let ids: Vec<(&str, u32, bool)> = ["page", "in-dialog", "in-popover", "in-painted", "in-menu"]
        .into_iter()
        .map(|t| {
            let k = host.runner().kernel();
            let node = k.node_by_key(k.find_by_test_id(t)[0]).unwrap();
            (t, node.id, node.symbol_color_said())
        })
        .collect();
    let tree = host.runner_mut().document_tree().unwrap();
    for (t, id, kernel) in ids {
        let document = tree.facts(id).unwrap().symbol_color;
        assert_eq!(document, kernel, "{t}");
    }
}
