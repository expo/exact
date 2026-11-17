use super::*;

const SOURCE: &str = include_str!("../../../../../contract/corpus/list-fixed.contract");

#[test]
fn fixed_children_match_the_eager_list_and_a_growing_trail_stays_followed() {
    let mut eager = boot_source(
        &SOURCE
            .replace("virtualized=true", "virtualized=false")
            .replace(" estimated-item-height=40", ""),
    );
    let mut virtualized = boot_source(SOURCE);
    settle(&mut eager);
    settle(&mut virtualized);
    for delta in [0., 80., 160., 100_000., -100_000.] {
        for p in [&mut eager, &mut virtualized] {
            let list = p
                .host
                .kernel()
                .node_by_key(p.host.kernel().find_by_test_id("list")[0])
                .unwrap()
                .id;
            p.wheel(list, 0., delta).unwrap();
            settle(p);
        }
        let kernel = virtualized.host.kernel();
        for id in ["lead-a", "lead-b", "trail-a", "trail-b", "tail"]
            .into_iter()
            .map(str::to_owned)
            .chain((0..10).map(|i| format!("row-{i}")))
        {
            for key in kernel.find_by_test_id(&id) {
                let actual = kernel.node_by_key(key).unwrap().frame;
                let oracle = eager
                    .host
                    .kernel()
                    .node_by_key(eager.host.kernel().find_by_test_id(&id)[0])
                    .unwrap()
                    .frame;
                assert_eq!(actual, oracle, "{id} after {delta}");
            }
        }
    }
    let list = virtualized.host.runner().collections()[0].view;
    virtualized.wheel(list, 0., 100_000.).unwrap();
    settle(&mut virtualized);
    let before = virtualized.scroll_of(list).1;
    let grow = named(&virtualized, "grow");
    assert!(virtualized
        .host
        .dispatch_at(grow, Event::Press, virtualized.host.now())
        .is_none());
    assert!(virtualized.after_commit().is_none());
    settle(&mut virtualized);
    assert_eq!(virtualized.scroll_of(list).1, before + 200.);
}
