//! The Canvas host's boot: the reader's window, the device and the
//! store, before the first frame.

use super::reader::monotonic_ns;
use super::{agent, colors, device, surface, CanvasHost, MOVES};
use crate::delivery::Opener;
use exact_runner::DataSource;
use std::path::Path;

impl<D: DataSource + Default> CanvasHost<D> {
    /// Boot `D`'s app in a window of `size` pixels at `scale` pixels per
    /// point, with the insets the reader told for it (`device.rs`). Assets
    /// use the Linux environment; the carrier directly selects its Canvas
    /// recorder and enables the reader's native motion lowering. A library
    /// whose bake has a store passes its opener and the reader's no-backup
    /// directory (LLP 1030.006 D3): the selected generation boots, and one
    /// at store `"0"` passes none.
    pub fn boot(
        plan: &'static [u8],
        compat: &'static str,
        size: (u32, u32),
        scale: f32,
        hatches: Option<crate::hatches::Install>,
        updates: Option<(Opener, &Path)>,
    ) -> Result<CanvasHost<D>, String> {
        let started = std::time::Instant::now();
        let mut launch = crate::launch_marks::LaunchMarks::new();
        launch.launch_end();
        let origin_ns = monotonic_ns();
        // Eleven viewports of decoded pictures: the reader's copy is a GPU
        // buffer (no heap copy, no upload), a picture decoded again costs
        // more than the memory it holds, and pictures are kept at the size
        // they show at (`image::png_decode::DecodePlan`). Twelve until they
        // were asked for at mount (below): heavy then ended 10 MB higher.
        if std::env::var_os("EXACT_IMAGE_VIEWPORTS").is_none() {
            std::env::set_var("EXACT_IMAGE_VIEWPORTS", "11");
        }
        // A picture is asked for when its row is built (`presenter::images`).
        if std::env::var_os("EXACT_PICTURES_AT_MOUNT").is_none() {
            std::env::set_var("EXACT_PICTURES_AT_MOUNT", "1");
        }
        std::env::set_var("EXACT_SCALE", scale.to_string());
        let told = device::windowed(size, scale);
        let whole = (size.0 as f32 / scale, size.1 as f32 / scale);
        let viewport = told.map_or(whole, |told| told.inside());
        // The first frame realizes only the rows that show; the window's lead
        // follows once it is out ([`CanvasHost::frame`]).
        exact_runner::set_lead_scale(0.0);
        // A new list builds the rows the view can show before its first
        // layout, not in a second commit after it.
        exact_runner::set_bootstrap_extent(size.1.max(size.0) as f64 / scale as f64);
        // A retiring row is rebound to the next item instead of a row built
        // for it (LLP 1078), unless `EXACT_ROW_REUSE=0`.
        crate::app::row_reuse_from_env(true);
        crate::surfaces::prepare_gpu(compat);
        // The device's colours, before anything is resolved by a role (D4).
        let palette = colors::palette();
        if let Some(palette) = &palette {
            palette.tell();
        }
        let mut config = crate::app::Config::from_env_static(plan, compat);
        config.preferences = device::configured().map(|d| exact_runner::Preferences {
            screen_reader: if config.agent {
                exact_runner::ScreenReader::Unknown
            } else {
                d.screen_reader
            },
            accessibility_text_size: d.accessibility_text_size(),
            ..Default::default()
        });
        if exact_runner::Delivery::default().with_compat(compat).store != '0' {
            let (open, base) = updates.ok_or(
                "the baked store level requires the delivery adapter; regenerate the app entry",
            )?;
            match open(compat, plan, &config.assets, Some(base)) {
                Ok(store) => config.use_updates(store, plan),
                Err(e) => eprintln!("exact update: {e}"),
            }
        }
        config.page = Some(exact_runner::time::PageFacts::new(reader_page));
        let painter = crate::presenter::PainterBoot::canvas(
            crate::presenter::PainterChoice::from_baked(compat),
        )?;
        // Motion lowering reads this after the requested painter has been checked.
        std::env::set_var("EXACT_PAINTER", "canvas");
        (config.scale, config.hatches, config.clock_started) = (scale, hatches, Some(started));
        super::present::await_present();
        let (mut p, error) =
            crate::app::boot_presenter_with_painter::<D>(&mut config, viewport, painter)?;
        if let Some(e) = error {
            eprintln!("exact: {e}");
        }
        // The reader opens sounds, videos and pages from the generation that
        // booted (LLP 1030.006 D1); entry zero's are the APK's.
        crate::delivery::set_reader_root(config.entry.as_ref().and(config.selected_dir.clone()));
        // The reader's frame commits are its presents (LLP 1003.001 D9).
        p.presents_pictures(true);
        agent::own_time(&mut p, started);
        // A frame recorded here may still crash the reader's Kotlin: the
        // boot is good once the reader drew it (LLP 1030.006 D5).
        p.set_reader_blesses();
        // The reader's `GestureDetector` recognizes the double tap (LLP 1057.001.000 §5).
        p.set_shell_taps();
        crate::surfaces::acquire_fd();
        // What the reader read of the device, before the first frame (D3.1).
        if let Some(e) = device::configured().and_then(|d| p.set_device(&d)) {
            eprintln!("exact: device: {e}");
        }
        // The reader takes what the presenter asks of the platform (D5),
        // and makes the top layer's windows (D10) and the bars (D11).
        p.serve_requests();
        p.serve_frame_partitions(super::frame_partitions::kept);
        surface::serve(&mut p);
        if let Some(palette) = &palette {
            p.set_palette(palette);
        }
        eprintln!(
            "exact: canvas {}x{} px, scale {scale}, boot {:.1} ms",
            size.0,
            size.1,
            started.elapsed().as_secs_f64() * 1000.0
        );
        let mut host = CanvasHost {
            p,
            started,
            scale,
            viewport,
            painted: None,
            moved: 0,
            moved_at: 0.0,
            scrolled_at: 0.0,
            waiting: false,
            leftover: false,
            led: false,
            settling: false,
            force: false,
            scroll_hold: super::scroll::ScrollHold::default(),
            prefetching: false,
            quiet: None,
            travel: crate::travel::Travel::default(),
            rows_before: Vec::new(),
            rows_after: Vec::new(),
            paint_by: None,
            move_tracks: std::env::var("EXACT_MOVE_TRACKS").is_ok_and(|v| v == "1"),
            sliced: false,
            moves: std::env::var("EXACT_MOVES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(MOVES),
            surfaces: false,
            borrowed: false,
            owed_surfaces: false,
            origin_ns,
            tracks: Default::default(),
            feed: None,
            told: None,
            presents: Default::default(),
            launch,
            lead: false,
            painted_once: false,
            gpu_waiting: false,
            responding: false,
            shell: Default::default(),
        };
        // The page's place in the window, once its root is known (D3.3).
        if let Some(told) = told {
            host.set_window(told);
        }
        Ok(host)
    }
}

/// What `exactPage()` says from the reader's first settlement: its share
/// sheet takes text, a link and `app:/` files, and no element's picture (LLP
/// 1094.000 D7); whether its WebView keeps partitions (LLP 1020.002 D1).
fn reader_page(plan: &exact_plan::Plan) -> exact_runner::Page {
    let page = exact_runner::Page {
        can_share: true,
        can_share_files: true,
        ..Default::default()
    };
    crate::presenter::with_partitions(page, plan, super::frame_partitions::kept)
}
