use super::*;
use contract_types::{Ref, Ty};

#[test]
fn view_names_have_the_plan_regions_depth_and_do_not_escape_an_arm() {
    let file = contract_syntax::parse("component App\n  state xs: number[] = [1]\n  state o: option<string> = none\n  view\n    for x, i in xs key=x\n      if true\n        match o\n          case some(x)\n            text x\n          case none\n            text `${x}`\n      text `${x}`\n    text \"outside\"\n").unwrap();
    let checked = contract_types::check(&file, |_| false).unwrap();
    let checking = Checking::new(&checked);
    let view = checking.authored(0);
    let found: Vec<_> = view
        .scoped()
        .filter(|(node, _)| node.element().is_some())
        .map(|(_, scope)| {
            (
                scope.lookup("x").map(|(r, t)| (r, t.clone())),
                scope.lookup("i").map(|(r, _)| r),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (Some((Ref::Bound(0), Ty::String)), Some(Ref::Index(2))),
            (Some((Ref::Item(2), Ty::Number)), Some(Ref::Index(2))),
            (Some((Ref::Item(0), Ty::Number)), Some(Ref::Index(0))),
            (None, None),
        ]
    );
}

#[test]
fn inherited_facts_and_declaration_identities_stay_in_their_subtree() {
    let file = contract_syntax::parse("component App\n  view\n    column\n      if true\n        Child()\n      else\n        text \"other\"\n    text \"sibling\"\ncomponent Child\n  view\n    text \"child\"\n").unwrap();
    let checked = contract_types::check(&file, |_| false).unwrap();
    let checking = Checking::new(&checked);
    let view = checking.authored(0);
    let found: Vec<_> = view
        .walk_with(0, |_, depth| depth + 1)
        .map(|(node, depth)| {
            (
                node.span().line,
                depth,
                node.component_use().map(|(id, name, _)| (id, name)),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (3, 0, None),
            (4, 1, None),
            (5, 2, Some((Some(1), "Child"))),
            (7, 2, None),
            (8, 0, None)
        ]
    );
    assert_eq!(view.walk().count(), found.len());
    assert!(checking
        .expanded()
        .walk()
        .all(|node| node.component_use().is_none()));
}

#[test]
fn events_keep_postorder_and_incoming_facts_while_reverse_keeps_arm_order() {
    let file = contract_syntax::parse("component App\n  view\n    column\n      if true\n        text \"yes\"\n      else\n        text \"no\"\n    text \"last\"\n").unwrap();
    let checked = contract_types::check(&file, |_| false).unwrap();
    let checking = Checking::new(&checked);
    let view = checking.authored(0);
    let events: Vec<_> = view
        .events(0, |node, depth| {
            (node.span().line != 4).then_some(depth + 1)
        })
        .map(|(node, depth, entering)| (node.span().line, depth, entering))
        .collect();
    assert_eq!(
        events,
        [
            (3, 0, true),
            (4, 1, true),
            (4, 1, false),
            (3, 0, false),
            (8, 0, true),
            (8, 0, false),
        ]
    );
    assert_eq!(
        view.walk_reverse()
            .map(|node| node.span().line)
            .collect::<Vec<_>>(),
        [8, 3, 4, 7, 5]
    );
}

#[test]
fn scoped_facts_shadow_only_the_binding_arm_and_keep_the_repetition_body_private() {
    let file = contract_syntax::parse("component App\n  state xs = [1]\n  state value: option<number> = some(1)\n  view\n    for x, i in xs key=i\n      match value\n        case some(x)\n          text `${x}`\n        case none\n          text `${x}`\n      text `${x}`\n    text \"outside\"\n").unwrap();
    let checked = contract_types::check(&file, |_| false).unwrap();
    let checking = Checking::new(&checked);
    let view = checking.authored(0);
    let facts: Vec<_> = view
        .scoped_with(Vec::<String>::new(), |_, _, names, binds| {
            names
                .iter()
                .cloned()
                .chain(binds.iter().map(str::to_owned))
                .collect()
        })
        .filter(|(node, _, _)| node.element().is_some())
        .map(|(node, _, names)| (node.span().line, names))
        .collect();
    assert_eq!(
        facts,
        [
            (8, vec!["x".into(), "i".into(), "x".into()]),
            (10, vec!["x".into(), "i".into()]),
            (11, vec!["x".into(), "i".into()]),
            (12, vec![]),
        ]
    );
    let repetition = view
        .walk()
        .find(|node| node.repetition().is_some())
        .unwrap();
    assert_eq!(repetition.repetition_body().count(), 4);
}

#[test]
fn structural_regions_keep_source_order_and_distinguish_copied_sites() {
    let mut file = contract_syntax::parse("component App\n  view\n    column\n      if true\n        text \"yes\"\n      else\n        text \"no\"\n      for x in [1] key=x\n        text `${x}`\n").unwrap();
    let copy = file.components[0].view[0].clone();
    file.components[0].view.push(copy);
    let checked = contract_types::check(&file, |_| false).unwrap();
    let checking = Checking::new(&checked);
    let view = checking.authored(0);
    let roots: Vec<_> = view.structure().roots().collect();
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].span(), roots[1].span());
    assert_eq!(roots[0].instance(), roots[1].instance());
    assert!(roots[0].site() != roots[1].site());
    assert!(roots[0].site() == roots[0].subtree().walk().next().unwrap().site());
    let children: Vec<_> = roots[0].children().collect();
    assert_eq!(children.len(), 2);
    assert!(children[0].is_choice());
    assert!(children[1].repetition().is_some());
    let [yes, no] = children[0].regions();
    assert_eq!(yes.roots().next().unwrap().span().line, 5);
    assert_eq!(no.roots().next().unwrap().span().line, 7);
    assert_eq!(children[1].regions()[1].walk().count(), 0);
    assert_eq!(roots[0].subtree().walk().count(), 6);
    let pruned: Vec<_> = roots[0]
        .subtree()
        .events((), |node, _| (!node.is_choice()).then_some(()))
        .filter(|(_, _, entering)| *entering)
        .map(|(node, _, _)| node.span().line)
        .collect();
    assert_eq!(pruned, [3, 4, 8, 9]);
}

#[test]
fn region_contexts_share_sibling_updates_but_isolate_choice_arms() {
    use std::{cell::Cell, rc::Rc};
    let file = contract_syntax::parse("component App\n  state value: option<number> = some(1)\n  view\n    column\n      text \"before\"\n      match value\n        case some(x)\n          text `${x}`\n          text \"sibling\"\n        case none\n          text \"none\"\n      text \"after\"\n").unwrap();
    let checked = contract_types::check(&file, |_| false).unwrap();
    let checking = Checking::new(&checked);
    let mut found = Vec::new();
    for _ in
        checking
            .authored(0)
            .structure()
            .walk_regions_with(Rc::new(Cell::new(0)), |node, count| {
                if node.is_choice() {
                    assert_eq!(node.bindings()[0].iter().collect::<Vec<_>>(), ["x"]);
                    assert!(node.bindings()[1].is_empty());
                    return [
                        Some(Rc::new(Cell::new(count.get()))),
                        Some(Rc::new(Cell::new(count.get()))),
                    ];
                }
                if node.element().is_some_and(|(tag, _, _)| tag == "text") {
                    found.push((node.span().line, count.get()));
                    count.set(count.get() + 1);
                }
                [Some(count.clone()), None]
            })
    {}
    assert_eq!(found, [(5, 0), (8, 1), (9, 2), (11, 1), (12, 1)]);
    let pruned: Vec<_> = checking
        .authored(0)
        .structure()
        .walk_regions_with((), |node, _| {
            if node.is_choice() {
                [None, Some(())]
            } else {
                [Some(()), None]
            }
        })
        .map(|(node, ())| node.span().line)
        .collect();
    assert_eq!(pruned, [4, 5, 6, 11, 12]);
}
