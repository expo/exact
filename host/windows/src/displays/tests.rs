use super::*;
use exact_painted::package::{json, Entry, Json};

/// The package's implementation, as a Windows app's build composes it
/// (`packages/window-management/windows/exact.json`).
#[path = "../../../../packages/window-management/linux/lib.rs"]
mod window_management;

/// A monitor from `gfx::Rect`-style rectangles (x, y, width, height), as
/// Chromium's `ScreenWin` tests give them.
fn monitor([x, y, w, h]: [i32; 4], [wx, wy, ww, wh]: [i32; 4], scale: f32) -> Monitor {
    Monitor {
        rect: [x, y, x + w, y + h],
        work: [wx, wy, wx + ww, wy + wh],
        scale,
        primary: (x, y) == (0, 0),
        ..Default::default()
    }
}

fn whole(rect: [i32; 4], scale: f32) -> Monitor {
    monitor(rect, rect, scale)
}

#[test]
fn square_monitor_keeps_its_natural_orientation() {
    for (dmdo, clockwise) in [0, 3, 2, 1].into_iter().enumerate() {
        assert_eq!(
            screen_orientation(1920, 1920, dmdo as u32),
            exact_runner::ScreenOrientation::rotated(true, clockwise)
        );
    }
}

#[test]
fn capture_pairs_physical_rectangles_with_the_display_lists_geometry() {
    for monitors in [
        vec![
            whole([0, 0, 2560, 1440], 1.25),
            whole([2560, 0, 3440, 1440], 1.25),
        ],
        vec![
            whole([0, 0, 1920, 1080], 1.),
            whole([-2560, -360, 2560, 1440], 2.),
        ],
        vec![
            whole([0, -2560, 1440, 2560], 1.5),
            whole([0, 0, 1920, 1080], 1.),
        ],
    ] {
        for client in [
            None,
            Some(Client {
                origin: [-2560, -360],
                dpi: 120,
            }),
        ] {
            let captures = capture_rects(&monitors, client);
            assert_eq!(
                captures.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
                screens(&monitors, None, client).screens
            );
            for (screen, physical) in captures {
                let monitor = monitors.iter().find(|m| m.rect == physical).unwrap();
                assert_eq!(screen.scale, f64::from(monitor.scale));
                if let Some(client) = client {
                    let rect = screen.client.unwrap();
                    assert_eq!(
                        rect[2] * f64::from(client.dpi) / 96.,
                        f64::from(physical[2] - physical[0])
                    );
                    assert_eq!(rect[0], f64::from(physical[0] - client.origin[0]) / 1.25);
                } else {
                    assert!(screen.client.is_none());
                }
            }
        }
    }
}

/// The expectations of `ui/display/win/screen_win_unittest.cc`'s
/// `GetDisplays` tests, by fixture.
#[test]
fn monitors_are_laid_out_as_chromium_lays_them_out() {
    let primary = |scale| monitor([0, 0, 1920, 1200], [0, 0, 1920, 1100], scale);
    // SingleDisplay1_25x, SingleDisplay1_5x
    assert_eq!(
        layout(&[primary(1.25)]),
        [([0, 0, 1536, 960], [0, 0, 1536, 880])]
    );
    assert_eq!(
        layout(&[primary(1.5)]),
        [([0, 0, 1280, 800], [0, 0, 1280, 734])]
    );
    // TwoDisplays1x2x, TwoDisplays2x1x
    let rects = |monitors: &[Monitor]| {
        layout(monitors)
            .into_iter()
            .map(|p| p.0)
            .collect::<Vec<_>>()
    };
    let second = |scale| whole([1920, 0, 800, 600], scale);
    assert_eq!(
        rects(&[primary(1.), second(2.)]),
        [[0, 0, 1920, 1200], [1920, 0, 400, 300]]
    );
    assert_eq!(
        rects(&[primary(2.), second(1.)]),
        [[0, 0, 960, 600], [960, 0, 800, 600]]
    );
    // TwoDisplays1_5x1x: the second's bottom is aligned with the first's.
    let one_and_a_half = [
        monitor([0, 0, 800, 600], [0, 0, 800, 550], 1.5),
        whole([800, 120, 640, 480], 1.),
    ];
    assert_eq!(
        layout(&one_and_a_half),
        [
            ([0, 0, 534, 400], [0, 0, 534, 367]),
            ([534, -80, 640, 480], [534, -80, 640, 480])
        ]
    );
    // ManyDisplays2x: a chain, each placed against the one it touches.
    let many = [
        monitor([0, 0, 640, 480], [0, 0, 640, 380], 2.),
        whole([640, 0, 1024, 768], 2.),
        whole([0, 480, 640, 300], 2.),
        whole([1664, 768, 400, 400], 2.),
        whole([1864, 1168, 200, 200], 2.),
    ];
    assert_eq!(
        rects(&many),
        [
            [0, 0, 320, 240],
            [320, 0, 512, 384],
            [0, 240, 320, 150],
            [832, 384, 200, 200],
            [932, 584, 100, 100]
        ]
    );
    // TwoDisplays2x1xVirtualized: a monitor that touches none is only scaled.
    let apart = [
        monitor([0, 0, 3200, 1600], [0, 0, 3200, 1500], 2.),
        whole([6400, 0, 3840, 2400], 2.),
    ];
    assert_eq!(
        layout(&apart),
        [
            ([0, 0, 1600, 800], [0, 0, 1600, 750]),
            ([3200, 0, 1920, 1200], [3200, 0, 1920, 1200])
        ]
    );
}

/// `gfx::ScaleToEnclosingRect` multiplies by `1.0f / scale`, and where the
/// quotient is whole that product can exceed it: 1680 pixels at 175 % are
/// 961 in Chrome, not 960 (`ui/gfx/geometry/rect.h`, called from
/// `screen_win.cc`'s `CreateDisplayFromDisplayInfo`). The expectations are
/// what that arithmetic gives compiled as C++.
#[test]
fn a_whole_quotient_is_enclosed_as_chromium_encloses_it() {
    assert_eq!(
        layout(&[whole([0, 0, 1680, 1050], 1.75)]),
        [([0, 0, 961, 600], [0, 0, 961, 600])]
    );
    assert_eq!(
        layout(&[whole([0, 0, 1680, 1050], 3.5)])[0].0,
        [0, 0, 481, 300]
    );
    // Left of the primary, its left edge floors below -960 and it is placed
    // by its width.
    let left = [
        whole([0, 0, 1920, 1080], 1.),
        whole([-1680, 0, 1680, 1050], 1.75),
    ];
    assert_eq!(
        layout(&left),
        [
            ([0, 0, 1920, 1080], [0, 0, 1920, 1080]),
            ([-961, 0, 961, 600], [-961, 0, 961, 600])
        ]
    );
    // Apart from every other monitor it keeps its own enclosed rectangle.
    let apart = [
        whole([0, 0, 1920, 1080], 1.),
        whole([-3577, 2000, 1680, 1050], 1.75),
    ];
    assert_eq!(layout(&apart)[1].0, [-2045, 1142, 961, 601]);
}

/// Chromium's `GetScaleFactorsForDPI`: the Text size setting multiplies a
/// monitor's scale, so at 150 % a 1920-pixel monitor at 96 DPI is 1280 wide
/// at a `devicePixelRatio` of 1.5. The window's viewport is in the DPI's
/// pixels (D2), so the same monitor covers 1920 of them.
#[test]
fn text_size_scales_the_screens_and_not_the_viewport() {
    assert_eq!(
        (scale(96, 1.), scale(120, 1.), scale(96, 1.5)),
        (1., 1.25, 1.5)
    );
    assert_eq!(scale(144, 1.25), 1.875);
    let monitors = [monitor(
        [0, 0, 1920, 1080],
        [0, 0, 1920, 1032],
        scale(96, 1.5),
    )];
    let window = Some(Client {
        origin: [0, 0],
        dpi: 96,
    });
    let all = screens(&monitors, Some(0), window);
    assert_eq!(
        (
            all.screens[0].rect,
            all.screens[0].avail,
            all.screens[0].scale
        ),
        ([0., 0., 1280., 720.], [0., 0., 1280., 688.], 1.5)
    );
    assert_eq!(all.screens[0].client, Some([0., 0., 1920., 1080.]));
    let p = pointer(&monitors, Some(0), 0, [960, 540], window).unwrap();
    assert_eq!((p.at, p.client), ([640., 360.], Some([960., 540.])));
}

/// A laptop's panel is built in whichever of the four technologies reports
/// it; a monitor on a cable is not.
#[test]
fn a_built_in_panel_is_internal_by_its_output_technology() {
    for (technology, name) in [
        (INTERNAL, "internal"),
        (DISPLAYPORT_EMBEDDED, "embedded DisplayPort"),
        (UDI_EMBEDDED, "embedded UDI"),
        (LVDS, "LVDS or DSI"),
    ] {
        assert!(built_in(technology), "{name}");
    }
    for (technology, name) in [
        (-1, "other"),
        (0, "VGA"),
        (4, "DVI"),
        (5, "HDMI"),
        (10, "external DisplayPort"),
        (12, "external UDI"),
        (15, "Miracast"),
    ] {
        assert!(!built_in(technology), "{name}");
    }
}

/// A module call of `body`, its answer.
fn call(p: &mut exact_painted::Presenter<()>, body: Json) -> Json {
    let (tx, rx) = std::sync::mpsc::channel();
    let reply = exact_runner::Reply::new(move |outcome| {
        if let exact_runner::Outcome::Response(r) = outcome {
            let _ = tx.send(serde_json::from_slice(&r.body).unwrap_or_default());
        }
    });
    let handler = p.host().runner().native_slot().handler().unwrap();
    handler(body.to_string().into_bytes(), reply);
    p.run_commands(|| ());
    rx.try_recv().unwrap_or_default()
}

/// LLP 1178 D6, expo/exact#726: a run with no window (an agent's session, a
/// screenshot) that binds the package reports the buffer it paints as its
/// one display, as the Linux host's does, and never the agent's fixture.
#[test]
fn a_run_with_no_window_reports_its_buffer_as_the_one_display() {
    install_headless();
    let plan = contract::compile("component App\n  view\n    text \"Hi\"\n")
        .unwrap()
        .encode();
    let (mut p, _) = exact_painted::Presenter::boot_with(
        &plan,
        (),
        (800., 600.),
        1.25,
        Default::default(),
        exact_painted::presenter::PainterChoice::Cpu,
    )
    .unwrap();
    p.serve_packages(vec![Entry::of::<window_management::WindowManagement>(
        "window-management",
    )]);
    let ask =
        |kind: &str, name: &str| json!({"extension": "window-management", kind: name, "args": {}});
    assert_eq!(
        call(&mut p, ask("source", "windowManagementStatus")),
        json!({"available": true, "state": "granted"})
    );
    let details = call(&mut p, ask("source", "getScreenDetails"));
    let screens = details["screens"].as_array().expect("screens");
    assert_eq!(screens.len(), 1, "{details}");
    let screen = &screens[0];
    assert_eq!(
        (
            &screen["width"],
            &screen["height"],
            &screen["devicePixelRatio"]
        ),
        (&json!(800.), &json!(600.), &json!(1.25)),
        "{details}"
    );
    assert_eq!(
        (&screen["label"], &screen["isPrimary"]),
        (&json!(""), &json!(true))
    );
    assert_eq!(details["currentScreen"], *screen);
    // No pointer is drawn until the drive places one.
    let at = call(&mut p, ask("send", "getPointerPosition"));
    assert_eq!(at["error"], "NotSupportedError", "{at}");
}

/// `ScreenWinTestTwoDisplays1_5x1x`'s `ScreenToDIPPoints`.
#[test]
fn the_pointer_is_in_the_holding_monitors_laid_out_space() {
    let monitors = [
        monitor([0, 0, 800, 600], [0, 0, 800, 550], 1.5),
        whole([800, 120, 640, 480], 1.),
    ];
    let at = |on, at| pointer(&monitors, None, on, at, None).unwrap();
    let first = at(0, [365, 452]);
    assert!((first.at[0] - 243.3333).abs() < 1e-3 && (first.at[1] - 301.3333).abs() < 1e-3);
    assert_eq!((first.on, first.client), (0, None));
    let second = at(1, [800, 120]);
    assert_eq!((second.at, second.on), ([534., -80.], 1));
    assert!(pointer(&monitors, None, 2, [0, 0], None).is_none());
}

/// expo/exact#506: three physical pixels each side of the seam between a
/// left display at 125 % and the primary, read as the browser reads them.
#[test]
fn a_point_beside_a_laid_out_origin_is_divided_once() {
    let monitors = [
        whole([-2560, 0, 2560, 1440], 1.25),
        whole([0, 0, 3440, 1440], 1.25),
    ];
    let at = |on, x| pointer(&monitors, None, on, [x, 400], None).unwrap().at;
    assert_eq!(at(0, -3), [-3. / 1.25, 320.]);
    assert_eq!(at(1, 3), [3. / 1.25, 320.]);
}

/// expo/exact#726's displays: 2560 × 1440 and 3440 × 1440 at 125 %, under a
/// window whose client area covers both.
#[test]
fn a_window_over_two_displays_reads_each_ones_place_in_its_viewport() {
    let monitors = [
        monitor([0, 0, 2560, 1440], [0, 0, 2560, 1392], 1.25),
        whole([2560, 0, 3440, 1440], 1.25),
    ];
    let window = Some(Client {
        origin: [0, 0],
        dpi: 120,
    });
    let all = screens(&monitors, Some(0), window);
    assert_eq!(all.current, 0);
    assert_eq!(
        (all.screens[0].rect, all.screens[0].avail),
        ([0., 0., 2048., 1152.], [0., 0., 2048., 1114.])
    );
    assert_eq!(all.screens[1].rect, [2048., 0., 2752., 1152.]);
    assert_eq!(all.screens[1].client, Some([2048., 0., 2752., 1152.]));
    assert_eq!(all.screens[1].scale, 1.25);
    let p = pointer(&monitors, Some(0), 1, [3000, 500], window).unwrap();
    assert_eq!(
        (p.at, p.client, p.on),
        ([2400., 400.], Some([2400., 400.]), 1)
    );
    assert_eq!(p.screens.screens[p.on].client, all.screens[1].client);
}

/// LLP 1178 D2: beside a display at another scale, a display's rectangle in
/// the viewport comes from physical pixels and the window's one scale, not
/// from the screens' own coordinates.
#[test]
fn a_client_rectangle_is_in_the_windows_scale_beside_another() {
    let monitors = [
        whole([0, 0, 1920, 1080], 1.),
        whole([1920, 0, 3840, 2160], 2.),
    ];
    // The window on the first display, 100 physical pixels in.
    let on_first = Some(Client {
        origin: [100, 50],
        dpi: 96,
    });
    let all = screens(&monitors, Some(0), on_first);
    assert_eq!(all.screens[1].rect, [1920., 0., 1920., 1080.]);
    assert_eq!(all.screens[1].client, Some([1820., -50., 3840., 2160.]));
    // The window on the second: the first covers half as many of its pixels.
    let on_second = Some(Client {
        origin: [1920, 0],
        dpi: 192,
    });
    let all = screens(&monitors, Some(1), on_second);
    assert_eq!(all.current, 1);
    assert_eq!(all.screens[0].client, Some([-960., 0., 960., 540.]));
    let p = pointer(&monitors, Some(1), 0, [960, 540], on_second).unwrap();
    assert_eq!((p.at, p.client), ([960., 540.], Some([-480., 270.])));
}

/// D1: the screens are in the specification's order, by `left` then `top`,
/// and with no window `currentScreen` is the primary wherever that puts it.
#[test]
fn screens_are_ordered_by_left_then_top_and_default_to_the_primary() {
    let monitors = [
        whole([0, 0, 1920, 1080], 1.),
        whole([-1920, 0, 1920, 1080], 1.),
        whole([-1920, -1080, 1920, 1080], 1.),
    ];
    let all = screens(&monitors, None, None);
    let lefts: Vec<_> = all.screens.iter().map(|s| (s.rect[0], s.rect[1])).collect();
    assert_eq!(lefts, [(-1920., -1080.), (-1920., 0.), (0., 0.)]);
    assert_eq!(all.current, 2);
    assert!(all.screens[2].primary && all.screens[2].client.is_none());
    assert_eq!(screens(&monitors, Some(1), None).current, 1);
    assert!(screens(&[], None, None).screens.is_empty());
}
