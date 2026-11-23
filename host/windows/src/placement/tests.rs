use super::*;
use crate::displays::Client;

#[test]
fn scale_change_keeps_content_above_the_taskbar() {
    let work = [0, 0, 5120, 1344];
    assert_eq!(
        fit_rect([78, 0, 2304, 1592], work, (746, 1072)),
        [78, 0, 2304, 1344]
    );
    assert_eq!(
        fit_rect([80, 100, 1180, 900], work, (360, 500)),
        [80, 100, 1180, 900]
    );
    assert_eq!(
        fit_rect([-900, 100, 100, 1300], [-1920, 40, 0, 1080], (360, 500)),
        [-1000, 40, 0, 1080]
    );
    // An authored minimum can exceed the display; retain it and keep the title bar reachable.
    assert_eq!(
        fit_rect([20, 40, 1020, 1040], [0, 0, 800, 600], (900, 700)),
        [0, 0, 900, 700]
    );
}

#[test]
fn a_window_draws_at_its_own_dpi_on_screen_and_at_its_monitors_off_it() {
    assert_eq!(shown_dpi(true, 120, 168), 120);
    assert_eq!(shown_dpi(false, 120, 168), 168);
    assert_eq!(shown_dpi(false, 168, 120), 120);
    assert_eq!(shown_dpi(false, 120, 120), 120);
    // No monitor DPI could be read.
    assert_eq!(shown_dpi(false, 120, 0), 120);
}

/// A 900 × 600 window at (400, 300) on the NucBox's 5120 × 1440 display,
/// hidden at 125 %: the margins and places are what Windows 11 25H2 gave a
/// window whose `WM_DPICHANGED` was handled as winit handles it.
fn hidden_at_125() -> Frame {
    Frame {
        on_screen: false,
        state: State::Normal,
        dpi: 120,
        monitor_dpi: 168,
        outer: [400, 300, 1300, 900],
        client: [409, 338, 409 + 882, 338 + 553],
        margins: [12, 52, 12, 12],
        work: [0, 0, 5120, 1356],
    }
}

#[test]
fn a_hidden_window_is_described_where_it_will_show_after_a_scale_change() {
    let frame = hidden_at_125();
    assert_eq!(
        shown(&frame, (1., 1.)),
        Shown {
            outer: [400, 300, 1659, 1138],
            client: Client {
                origin: [412, 352],
                dpi: 168
            },
        }
    );
    // Back from 175 % to 125 %.
    let back = Frame {
        dpi: 168,
        monitor_dpi: 120,
        client: [412, 352, 412 + 876, 352 + 536],
        margins: [9, 38, 9, 9],
        work: [0, 0, 5120, 1380],
        ..frame
    };
    assert_eq!(
        shown(&back, (1., 1.)),
        Shown {
            outer: [400, 300, 1044, 730],
            client: Client {
                origin: [409, 338],
                dpi: 120
            },
        }
    );
    // On screen, or hidden with no change pending, it stays where it is.
    for unchanged in [
        Frame {
            on_screen: true,
            ..frame
        },
        Frame {
            monitor_dpi: 120,
            ..frame
        },
    ] {
        assert_eq!(
            shown(&unchanged, (1., 1.)),
            Shown {
                outer: frame.outer,
                client: Client {
                    origin: [409, 338],
                    dpi: 120
                },
            }
        );
    }
}

#[test]
fn a_hidden_window_that_will_outgrow_the_work_area_is_described_fitted() {
    // 2400 × 1200 of client at 125 % is 3360 × 1680 at 175 %: taller than
    // the work area, so it shows moved up to the top and cut to its height.
    let frame = Frame {
        outer: [100, 80, 2518, 1318],
        client: [109, 118, 2509, 1318],
        ..hidden_at_125()
    };
    let shown = shown(&frame, (360., 240.));
    assert_eq!(shown.outer, [100, 0, 100 + 3360 + 24, 1356]);
    assert_eq!(shown.client.origin, [112, 52]);
    assert_eq!(shown.client.dpi, 168);
}

#[test]
fn a_hidden_window_keeps_its_logical_size_as_winit_rounds_it() {
    // 1285 / 2.5 × 1.75 is 899.5 and rounds up; 1285 × 0.7 rounds down.
    let frame = Frame {
        dpi: 240,
        outer: [0, 0, 1309, 864],
        client: [12, 52, 12 + 1285, 52 + 800],
        ..hidden_at_125()
    };
    let shown = shown(&frame, (1., 1.));
    assert_eq!(shown.outer, [0, 0, 900 + 24, 560 + 64]);
}

#[test]
fn a_hidden_fullscreen_window_keeps_its_rectangle() {
    let frame = Frame {
        state: State::Fullscreen,
        outer: [0, 0, 5120, 1440],
        client: [0, 0, 5120, 1440],
        margins: [0, 0, 0, 0],
        ..hidden_at_125()
    };
    assert_eq!(
        shown(&frame, (1., 1.)),
        Shown {
            outer: [0, 0, 5120, 1440],
            client: Client {
                origin: [0, 0],
                dpi: 168
            },
        }
    );
}

#[test]
fn a_hidden_maximized_window_is_described_filling_the_work_area() {
    let frame = Frame {
        state: State::Maximized,
        outer: [-9, -9, 5129, 1389],
        client: [0, 29, 5120, 1380],
        ..hidden_at_125()
    };
    assert_eq!(
        shown(
            &Frame {
                state: State::Minimized,
                ..frame
            },
            (1., 1.)
        )
        .client,
        Client {
            origin: [0, 29],
            dpi: 120
        }
    );
    // Windows 11 25H2 maximized it at 175 % to (-12, -12)-(5132, 1368),
    // with its client area at (0, 40).
    assert_eq!(
        shown(&frame, (1., 1.)),
        Shown {
            outer: [-12, -12, 5132, 1368],
            client: Client {
                origin: [0, 40],
                dpi: 168
            },
        }
    );
}
