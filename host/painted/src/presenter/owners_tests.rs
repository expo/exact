//! @ref LLP 1123 D2, D17 — the painted hosts' owner updates, as the shell lays the routes out.
use super::super::bars::tests::{barred, tap, Nothing, APP};
use super::super::stage::{sheet_span, SHEET_HANDLE};
use super::super::*;
use exact_kernel::{KeyboardReport, OwnerUpdate};

fn update(p: &Presenter<Nothing>, test_id: &str) -> OwnerUpdate {
    let kernel = p.host().kernel();
    let view = kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .id;
    let found = p.owner_updates().into_iter().find(|(v, _)| *v == view);
    found.unwrap_or_else(|| panic!("no update for {test_id}")).1
}

/// A barred route's band is the room its bars take of it, the local insets its content reads now, with nothing the
/// painted host has no native form of; its map is a translation to where the page shows it; and the kernel takes it.
#[test]
fn a_routes_update_is_the_room_its_bars_take() {
    let (mut p, _) = barred(APP);
    let home = update(&p, "route-home");
    let kernel = p.host().kernel();
    let route = kernel
        .node_by_key(kernel.find_by_test_id("route-home")[0])
        .unwrap()
        .id;
    let [top, _, bottom, _] = home.band.unwrap();
    assert!(top > 0.0 && bottom > 0.0, "{:?}", home.band);
    assert_eq!(
        home.band,
        kernel.local_insets(route),
        "the room the route is inset by"
    );
    assert!(home.contributions.is_empty());
    assert_eq!(home.targets, Default::default(), "no native targets");
    assert_eq!(
        (&home.margins, &home.segments, &home.chrome, &home.regions),
        (&None, &None, &None, &None)
    );
    assert_eq!(home.keyboard, KeyboardReport::default());
    let page = p.page_origin();
    assert_eq!(
        home.window_map.unwrap().0,
        [1.0, 0.0, 0.0, 1.0, page.0, page.1],
        "at the page's origin"
    );
    let kernel = p.host.runner_mut().kernel_mut();
    assert_eq!(
        kernel.update_owner(route, home),
        Ok(false),
        "taken, as the local insets already hold it"
    );
    // Every route of every stack has one: the other tab's, once it has a route, barred too
    tap(&mut p, "tab-other").unwrap();
    assert!(update(&p, "route-other").band.unwrap()[0] > 0.0);
}

/// A shell with no bars gives each route the window's insets, which a desktop shell has none of: a band of 0, not an
/// unavailable one.
#[test]
fn a_shell_without_bars_reports_bands_of_zero() {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        Nothing,
        (412., 840.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let updates = p.owner_updates();
    assert!(!updates.is_empty());
    for (_, u) in &updates {
        assert_eq!(u.band, Some([0.0; 4]));
        assert_eq!(u.window_map.unwrap().0, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }
}

/// A route in a sheet reports the sheet's band, its handle at the top, and lies where the stage shows the sheet; once
/// the sheet has gone its route has no update.
#[test]
fn a_sheets_route_reports_its_sheet_and_goes_with_it() {
    let (mut p, _) = barred(include_str!("../../../../apps/shelf/app.contract"));
    let sheeted = |p: &Presenter<Nothing>| {
        let updates = p.owner_updates();
        updates
            .into_iter()
            .filter(|(_, u)| u.band.unwrap()[0] == SHEET_HANDLE)
            .collect::<Vec<_>>()
    };
    assert!(sheeted(&p).is_empty());
    tap(&mut p, "add-book").unwrap();
    let open = p.host().now() + 1000.;
    p.tick(open);
    let _ = p.frame();
    let found = sheeted(&p);
    assert_eq!(found.len(), 1, "the sheet's route");
    let [_, _, _, _, x, y] = found[0].1.window_map.unwrap().0;
    let (page, key) = (
        p.page_origin(),
        p.host().kernel().node(found[0].0).unwrap().key,
    );
    assert_eq!(
        x,
        page.0 + sheet_span(p.viewport.0).0,
        "at the sheet's left edge"
    );
    assert_eq!(
        y,
        page.1 + p.sheet_origin(key).unwrap().1,
        "at the stage's top for the sheet"
    );
    assert!(y >= page.1 && y < page.1 + p.viewport.1, "in the page: {y}");
    assert!(p.system_back(open));
    let closed = open + 1000.;
    p.tick(closed);
    let _ = p.frame();
    assert!(sheeted(&p).is_empty(), "gone with its sheet");
}

/// A route's configuration holds while nothing changes, and changes with the window.
#[test]
fn a_routes_configuration_follows_the_window() {
    let (mut p, _) = barred(APP);
    let configuration = |p: &Presenter<Nothing>| update(p, "route-home").configuration.unwrap();
    let before = configuration(&p);
    assert_eq!(configuration(&p), before);
    let mut window = *p.window().unwrap();
    window.size = (window.size.1, window.size.0);
    assert_eq!(p.set_window(window), None);
    let _ = p.after_commit();
    assert_ne!(configuration(&p), before);
}

/// A desktop shell's (Linux's, Windows's): with no bars and no window insets, each route's band stays 0 through a
/// resize while its configuration follows the window, and a route that leaves its stack has no update.
#[test]
fn a_desktop_shells_routes_keep_bands_of_zero_and_retire_with_their_stack() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        Nothing,
        (900., 600.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let home = update(&p, "route-home");
    assert_eq!(home.band, Some([0.0; 4]), "0, not unavailable");
    p.resize(1200., 800.);
    let _ = p.after_commit();
    let resized = update(&p, "route-home");
    assert_eq!(resized.band, Some([0.0; 4]));
    assert_ne!(
        resized.configuration, home.configuration,
        "the page's size is its configuration's"
    );
    tap(&mut p, "open").unwrap();
    let _ = p.after_commit();
    let detail = update(&p, "route-detail");
    assert_eq!(detail.band, Some([0.0; 4]));
    let kernel = p.host.runner_mut().kernel_mut();
    let route = kernel
        .node_by_key(kernel.find_by_test_id("route-detail")[0])
        .unwrap()
        .id;
    assert!(
        kernel.update_owner(route, detail).is_ok(),
        "the kernel takes it"
    );
    tap(&mut p, "back").unwrap();
    let settled = p.host().now() + 2000.;
    p.tick(settled);
    let _ = p.after_commit();
    let kernel = p.host().kernel();
    let gone = kernel
        .find_by_test_id("route-detail")
        .first()
        .and_then(|k| kernel.node_by_key(*k))
        .map(|n| n.id);
    assert!(
        p.owner_updates().iter().all(|(v, _)| Some(*v) != gone),
        "no update once it has left its stack"
    );
}
