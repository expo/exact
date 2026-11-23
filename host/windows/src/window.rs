use crate::keyboard::key_names;
use exact_ink::counters;
use exact_painted::{
    app::{self, Config},
    presenter::{PainterBoot, PointerKind},
    window_title::WindowTitle,
    Presenter,
};
use exact_runner::DataSource;
use std::{
    num::NonZeroU32,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    platform::windows::{EventLoopBuilderExtWindows, WindowAttributesExtWindows},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{CursorIcon, Fullscreen, Theme, Window, WindowId},
};

mod edit_menu;
mod file_dialog;
mod file_drop;
mod frames;
mod lifecycle;
mod presents;
mod service;
mod tray;
use frames::{next_frame_deadline, presentation_resumes, FrameSchedule};

struct App<D: DataSource> {
    service_causes: u8,
    proxy: EventLoopProxy<crate::accessibility::Event>,
    accessibility: Option<crate::accessibility::Access>,
    name: String,
    title: WindowTitle,
    min_size: (f32, f32),
    config: Config,
    painter: PainterBoot,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    preparing: Option<exact_ink::device::d3d11::Startup>,
    jump_list_started: bool,
    direct: bool,
    presenter: Option<Presenter<D>>,
    timers: crate::timers::Timers,
    started: Instant,
    pointer: (f32, f32),
    pointer_inside: bool,
    focused: bool,
    cursor: CursorIcon,
    buttons: u32,
    /// A button or a finger was down when the window lost the focus, and its
    /// release may go to another window.
    unheard: bool,
    clicks: crate::clicks::Clicks,
    touches: crate::touch::Touches,
    scale: f64,
    refit_after_scale: bool,
    next_frame: Instant,
    redraw_pending: bool,
    failed: bool,
    minimized: bool,
    occluded: bool,
    frames: u64,
    measured_first_screen: bool,
    measure_first_composition: bool,
    paint_ms: f64,
    present_ms: f64,
    previous_frame: Option<Instant>,
    samples: Vec<(f64, f64, f64)>,
    frame_logs: Vec<String>,
    frame_limit: Option<u64>,
    first_tap: Option<String>,
    pads: crate::pad_poll::Pads,
    pads_active: bool,
    caret: crate::caret::Caret,
    tooltip: crate::tooltip::Tooltip,
    edit_menu: Option<exact_painted::presenter::edit_menu::EditMenu>,
    ime: crate::ime::InputMethod,
    appearance: crate::appearance::Changes,
    modal: crate::modal::Schedule,
    dialogs: crate::file_dialog::Dialogs,
    desktop: Rc<crate::desktop::Desktop>,
    /// The app called `close()`: the window closes, never hides.
    closing: bool,
    /// Closing the window hides it (`keepRunning`).
    keep_running: bool,
    /// The window is not shown: hidden by a close, or since a hidden launch.
    hidden: bool,
    /// DWM is composing this window for its first frame after a show request.
    pending_show: Option<lifecycle::PendingShow>,
    /// Whether a canvas's picture was still loading as the last show was revealed.
    #[cfg(test)]
    reveal_awaited: Option<bool>,
    /// The app's one instance, which later launches reach.
    instance: Option<crate::instance::Instance>,
    cold_documents: Vec<String>,
    tray: Option<tray::Tray>,
    presents: presents::Presents,
}

impl<D: DataSource> Drop for App<D> {
    fn drop(&mut self) {
        crate::probe::save();
        if std::env::var("EXACT_AGENT_WINDOW_PROBE").as_deref() == Ok("on") {
            eprintln!("exact-windows: successful frames {}", self.frames);
        }
        self.timers.clear();
        self.accessibility.take();
        exact_ink::device::d3d11::fields::clear();
        // The module's GPU swapchain must release its HWND before the window.
        self.presenter.take();
        self.caret.clear();
        self.tooltip = Default::default();
    }
}

fn apply_title<D: DataSource>(
    title: &mut WindowTitle,
    p: &Presenter<D>,
    window: &Window,
    name: &str,
) -> bool {
    if !title.update(p.host().runner()) {
        return false;
    }
    window.set_title(title.caption(name));
    true
}

fn system_theme<D: DataSource>(p: &mut Presenter<D>, theme: Theme, agent: bool) {
    let dark = theme == Theme::Dark;
    p.set_system_scheme(dark);
    let contrast = crate::appearance::contrast();
    p.set_contrast_palette(contrast);
    let mut preferences = p.host().runner().viewport().preferences;
    let screen_reader = if agent {
        preferences.screen_reader
    } else {
        crate::appearance::screen_reader()
    };
    let custom = if contrast.is_some() {
        exact_runner::Contrast::Custom
    } else {
        exact_runner::Contrast::NoPreference
    };
    if preferences.dark != dark
        || preferences.contrast != custom
        || preferences.screen_reader != screen_reader
    {
        preferences.screen_reader = screen_reader;
        preferences.contrast = custom;
        preferences.dark = dark;
        if let Some(error) = p.set_preferences(preferences) {
            eprintln!("exact-windows: appearance: {error}");
        }
    }
}

pub(crate) fn boot_phase(name: &str) {
    if std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some() {
        if let Ok(ms) = exact_ink::device::process_ms() {
            eprintln!("exact-windows: boot {name} at {ms:.3} ms (process)");
        }
    }
}

pub(super) fn run<D: DataSource + Default + 'static, const DEVELOPMENT: bool>(
    name: &str,
    opening: crate::Window,
    plan: &[u8],
    compat: &str,
    composed: exact_painted::Composed,
    services: crate::Services,
) -> i32 {
    if let Some(result) = exact_ink::device::d3d11::startup_watchdog::helper() {
        return result;
    }
    exact_ink::device::d3d11::startup_watchdog::start();
    let started = Instant::now();
    app::prepare_environment(compat);
    counters::initialize();
    boot_phase("host-entered");
    if app::print_baked_receipt(compat) {
        return 0;
    }
    if let Err(why) = exact_runner::delivery::refuse_analysis(compat) {
        eprintln!("exact-windows: {why}");
        return 1;
    }
    if exact_runner::Delivery::default().with_compat(compat).store != '0' {
        eprintln!("exact-windows: update-store delivery is not implemented; bake store level 0");
        return 1;
    }
    let frame_limit = match std::env::var("EXACT_AGENT_WINDOW_FRAMES") {
        Ok(value) => match value.parse::<u64>() {
            Ok(count @ 1..=10_000) => Some(count),
            _ => {
                eprintln!("exact-windows: EXACT_AGENT_WINDOW_FRAMES requires 1..=10000");
                return 1;
            }
        },
        Err(_) => None,
    };
    let login = crate::desktop::login_launch(&std::env::args().collect::<Vec<_>>());
    let painter = PainterBoot::windows::<DEVELOPMENT>(compat);
    let launch = app::prepare_window_painter(login, frame_limit.is_some(), || {
        let Some(warp) = painter.direct_d3d11() else {
            return Ok(None);
        };
        if !Config::windowed_from_env()
            || exact_ink::device::d3d11::startup_watchdog::after_layout()
        {
            return Ok(None);
        }
        exact_ink::device::d3d11::Startup::start(warp).map(Some)
    });
    let (hidden, preparing) = match launch {
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("exact-windows: {error}");
            return 1;
        }
    };
    // A distributed app locates data beside its executable, never in its caller's cwd.
    if std::env::var_os("EXACT_ASSETS").is_none() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(root) = exe.parent() {
                std::env::set_var("EXACT_ASSETS", root);
            }
        }
    }
    if std::env::var_os("EXACT_GPU_RENDER").is_none() {
        std::env::set_var("EXACT_GPU_RENDER", "1");
    }
    // Windows has no toolkit under this painter, so it shows the web's
    // neutral look (LLP 1115.000 D17), in a window as in a headless run.
    app::show_neutral_look();
    if std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some()
        && std::env::var("EXACT_AGENT_INK_FAULT").as_deref() == Ok("slow-config")
    {
        std::thread::sleep(Duration::from_millis(200));
    }
    let mut config = Config::from_bake::<DEVELOPMENT>(plan, compat, services);
    config.preferences = Some(exact_runner::Preferences {
        screen_reader: if config.agent {
            exact_runner::ScreenReader::Unknown
        } else {
            crate::appearance::screen_reader()
        },
        ..Default::default()
    });
    // The presenter makes the hatches' value after its first frame is shown.
    config.hatches = composed.hatches;
    config.packages = composed.packages;
    let first_tap = std::env::var("EXACT_AGENT_WINDOW_TAP").ok();
    if first_tap.is_some()
        && frame_limit.is_none()
        && std::env::var_os("EXACT_AGENT_WINDOW_READY").is_none()
    {
        eprintln!("exact-windows: EXACT_AGENT_WINDOW_TAP requires a measured window run");
        return 1;
    }
    if DEVELOPMENT && !config.windowed() {
        crate::displays::install_headless();
        return crate::clipboard::run_headless::<D>(&mut config, started);
    }
    let args = crate::instance::arguments();
    let instance = match lifecycle::claim(frame_limit.is_some(), compat, name, &args) {
        Ok(instance) => instance,
        Err(code) => return code,
    };
    let cold_documents = lifecycle::cold_documents(&mut config, compat, &args);
    boot_phase("config-ready");
    if std::env::var_os("EXACT_SIZE").is_none() {
        config.size = (opening.width, opening.height);
    }
    let mut builder = EventLoop::<crate::accessibility::Event>::with_user_event();
    // `crate::on_desktop_stack`'s thread, not the process's first.
    builder.with_any_thread(true);
    let ime = crate::ime::InputMethod::default();
    let desktop = crate::desktop::Desktop::install(name);
    let ime_message = ime.message_hook();
    let desktop_message = desktop.clone();
    let field_message = exact_ink::device::d3d11::fields::message_hook();
    // Winit keeps one hook; replacing it would lose composition completion on clicks.
    builder.with_msg_hook(move |message| {
        if field_message(message) {
            return true;
        }
        ime_message(message);
        desktop_message.message(message);
        false
    });
    let event_loop = match builder.build() {
        Ok(value) => value,
        Err(e) => {
            eprintln!("exact-windows: {e}");
            return 1;
        }
    };
    boot_phase("event-loop-ready");
    let proxy = event_loop.create_proxy();
    let wake = proxy.clone();
    desktop.set_wake(move || {
        let _ = wake.send_event(crate::accessibility::Event::Wake);
    });
    if let Some(instance) = &instance {
        let wake = proxy.clone();
        instance.set_wake(move || drop(wake.send_event(crate::accessibility::Event::Wake)));
    }
    let wake = proxy.clone();
    exact_painted::set_event_waker(Some(Arc::new(move || {
        let _ = wake.send_event(crate::accessibility::Event::Wake);
    })));
    let timer_proxy = proxy.clone();
    let timers = crate::timers::Timers::new(move |fired| {
        let _ = timer_proxy.send_event(crate::accessibility::Event::Timer(fired));
    });
    let state = App::<D> {
        service_causes: 0,
        proxy,
        accessibility: None,
        name: name.to_owned(),
        title: WindowTitle::default(),
        min_size: (opening.min_width, opening.min_height),
        config,
        painter,
        window: None,
        surface: None,
        preparing,
        jump_list_started: false,
        direct: painter.direct_d3d11().is_some(),
        presenter: None,
        timers,
        started,
        pointer: (0., 0.),
        pointer_inside: false,
        focused: true,
        cursor: CursorIcon::Default,
        clicks: Default::default(),
        touches: Default::default(),
        buttons: 0,
        unheard: false,
        scale: 1.,
        next_frame: started,
        redraw_pending: false,
        failed: false,
        minimized: false,
        occluded: false,
        frames: 0,
        measured_first_screen: false,
        measure_first_composition: false,
        paint_ms: 0.,
        present_ms: 0.,
        previous_frame: None,
        samples: Vec::new(),
        frame_logs: Vec::new(),
        frame_limit,
        first_tap,
        pads: Default::default(),
        pads_active: false,
        caret: Default::default(),
        tooltip: Default::default(),
        edit_menu: None,
        ime,
        appearance: Default::default(),
        modal: Default::default(),
        dialogs: Default::default(),
        desktop,
        refit_after_scale: false,
        closing: false,
        keep_running: opening.keep_running,
        hidden,
        pending_show: None,
        #[cfg(test)]
        reveal_awaited: None,
        instance,
        cold_documents,
        tray: None,
        presents: Default::default(),
    };
    let state = Rc::new(std::cell::RefCell::new(state));
    let result = event_loop.run_app(&mut file_drop::Window::new(state.clone()));
    let mut state = state.borrow_mut();
    if let Err(e) = result {
        eprintln!("exact-windows: {e}");
        state.failed = true;
    }
    // A launch from here on is the app's next instance, not this ending one's.
    state.instance.take();
    exact_painted::set_event_waker(None);
    if let Some(p) = &mut state.presenter {
        for failure in p.drain_file_exports() {
            eprintln!("exact: {failure}");
        }
    }
    // However the loop ended (the window closed, a bounded run, a failure): the store's writes, made on a thread of
    // their own, are on disk before the process ends, and what failed is said.
    if !exact_painted::host::drain_store_writes(std::time::Duration::from_secs(5)) {
        eprintln!("exact: the store's writes were not all on disk at exit");
    }
    if let Some(p) = &state.presenter {
        for line in exact_painted::host::take_store_warnings(p.host().runner().data_ref().app_id())
        {
            eprintln!("exact: {line}");
        }
    }
    if state.frame_limit.is_some()
        && std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some()
        && !state.measured_first_screen
    {
        eprintln!("exact-windows: the first-screen readiness target was not painted");
        state.failed = true;
    }
    frames::report(
        state.frames,
        state.paint_ms,
        state.present_ms,
        &state.samples,
    );
    i32::from(state.failed)
}

impl<D: DataSource + Default + 'static> App<D> {
    fn fail(&mut self, events: &ActiveEventLoop, error: impl std::fmt::Display) {
        eprintln!("exact-windows: {error}");
        self.failed = true;
        events.exit();
    }
    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.
    }
    fn resized(&mut self) {
        self.caret.clear();
        let (Some(window), Some(p)) = (&self.window, &mut self.presenter) else {
            return;
        };
        let size = window.inner_size();
        self.minimized = window.is_minimized() == Some(true) || size.width == 0 || size.height == 0;
        p.surface_lifecycle(
            self.minimized || self.occluded || self.hidden,
            !self.focused,
        );
        self.scale = window.scale_factor();
        if !self.minimized {
            if let Some(error) = p.resize_scaled(
                size.width as f32 / self.scale as f32,
                size.height as f32 / self.scale as f32,
                self.scale as f32,
            ) {
                eprintln!("exact-windows: resize: {error}");
            }
            window.request_redraw();
        }
    }
    fn deliver_timer(&mut self, fired: Arc<crate::timers::Fired>, now: f64) {
        if let Some(p) = &mut self.presenter {
            if fired.live(p.native_timer_generation()) {
                let due = p
                    .host()
                    .runner()
                    .native_timers()
                    .into_iter()
                    .find(|row| {
                        row.index == fired.row.index && row.generation == fired.row.generation
                    })
                    .map(|row| row.due_ms);
                if let Some(due) = due.filter(|due| *due > now) {
                    self.timers.defer(due, fired);
                    return;
                }
                if let Some(e) = p.fire_native_timer(fired.epoch, &fired.row, now) {
                    eprintln!("exact-windows: {e}");
                }
            }
        }
        fired.complete();
    }
    fn draw(&mut self, events: &ActiveEventLoop) -> Result<(), String> {
        if std::mem::take(&mut self.refit_after_scale) {
            if let Some(window) = &self.window {
                // Winit has applied its DPI size by the next redraw. It preserves
                // logical size even when that puts content below the desktop.
                if crate::placement::fit(window, self.min_size) {
                    self.resized();
                }
            }
        }
        let frame_started = Instant::now();
        if self.frames == 0 {
            boot_phase("first-draw-entered");
        }
        // A hidden window takes no present (`lifecycle::unshown_first_pixel`).
        if self.unseen() {
            return Ok(());
        }
        // A frame task can wake the loop from its own commit. Those wakes
        // must not advance the animation again before the next display tick.
        let continuous = self.frame_limit.is_some()
            || self
                .presenter
                .as_ref()
                .is_some_and(|p| p.wants_display_frames() || p.needs_animation_frame());
        let unpaced =
            self.frame_limit.is_some() && std::env::var_os("EXACT_AGENT_WINDOW_UNPACED").is_some();
        let schedule = FrameSchedule {
            cadence: (continuous && !unpaced).then_some(self.next_frame),
            retry: None,
            present: self
                .presenter
                .as_ref()
                .and_then(|p| p.painter_present_deadline()),
        };
        // A refused present defers the next paint. Ordinary D3D11 redraws acquire
        // the latency waitable before painting; they need no second timer delay.
        if schedule.defer(frame_started, &mut self.redraw_pending) {
            return Ok(());
        }
        let now = self.now();
        let (Some(window), surface, Some(p)) =
            (&self.window, &mut self.surface, &mut self.presenter)
        else {
            return Ok(());
        };
        if self.measure_first_composition {
            // Read before submitting another frame; the compositor has had the
            // intervening display period. Omitted in the probe-free interval run.
            match p.displayed_process_ms() {
                Ok(Some(ms)) => eprintln!(
                    "exact-windows: process to displayed first composition {ms:.3} ms (DXGI)"
                ),
                other => {
                    eprintln!("exact-windows: first composition timestamp unavailable: {other:?}")
                }
            }
            self.measure_first_composition = false;
        }
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        // A restore or DPI transition can expose the new client extent before
        // its resize event reaches this callback. Never present the old extent.
        let scale = window.scale_factor();
        let viewport = (
            size.width as f32 / scale as f32,
            size.height as f32 / scale as f32,
        );
        if self.scale != scale || p.viewport() != viewport {
            self.caret.clear();
            self.scale = scale;
            if let Some(error) = p.resize_scaled(viewport.0, viewport.1, scale as f32) {
                return Err(error);
            }
        }
        counters::service(counters::DRAW);
        let service_span = crate::probe::Span::new("draw service", counters::DRAW);
        p.pump(now);
        // A presented frame: its tasks, then the hatches' ticks and due
        // `after`s (LLP 1075.003.000.001 §2.4).
        p.animation_frame(now);
        p.run_commands(D::default);
        crate::orientation::apply(p);
        // The hatches' turn: what they asked of elements runs, then the
        // moments this turn's commits and the window's new size caused; an
        // overlay published here is in the frame painted below.
        p.hatch_turn();
        let prepared_at = frame_started.elapsed().as_secs_f64() * 1000.;
        p.poll_images();
        drop(service_span);
        // The OS draws the desktop cursor; this presenter's arrow is for DRM.
        p.set_pointer(None);
        if self.frames > 0 {
            p.sync_surfaces();
        }
        let images_at = frame_started.elapsed().as_secs_f64() * 1000.;
        p.tick(now);
        let tick_at = frame_started.elapsed().as_secs_f64() * 1000.;
        self.caret.advance(frame_started);
        p.set_caret_visible(self.caret.visible());
        if self.frames == 0 {
            eprintln!(
                "exact-windows: {}x{} pixels, scale {}, monitor {:?} mHz",
                window.inner_size().width,
                window.inner_size().height,
                self.scale,
                window
                    .current_monitor()
                    .and_then(|m| m.refresh_rate_millihertz())
            );
        }
        // Bounded runs measure an unchanged redraw, as the C4 still protocol does.
        if self.frame_limit.is_some() {
            p.repaint();
        }
        // A window being shown waits, bounded, for the pictures its draws
        // asked for, so its first visible frame has them (LLP 1178 D3).
        if let Some(pending) = &self.pending_show {
            p.settle_pictures(lifecycle::REVEAL_PICTURES.saturating_sub(pending.began.elapsed()));
        }
        let painting = p.dirty() || self.frames == 0;
        let paint = Instant::now();
        if let Some(previous) = self.caret.take_previous_frame() {
            p.recycle_frame(previous);
        }
        let RawWindowHandle::Win32(handle) =
            window.window_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Err("not a Windows window".into());
        };
        let owned = exact_ink::device::d3d11::Window::from_owner(window.clone())
            .map_err(|e| e.to_string())?;
        let mut frame = p.window_frame(owned.clone())?;
        let caret_rect = (self.focused && p.last_frame_succeeded())
            .then(|| p.caret_frame())
            .flatten();
        if self.caret.presented(
            handle.hwnd.get(),
            caret_rect,
            self.scale as f32,
            frame.clone(),
            frame_started,
        ) {
            p.set_caret_visible(true);
            // A programmatic move while hidden starts a fresh visible phase.
            frame = p.window_frame(owned)?;
            self.caret.presented(
                handle.hwnd.get(),
                caret_rect,
                self.scale as f32,
                frame.clone(),
                frame_started,
            );
        }
        // Geometry and accessibility queries below can paint into a discarded
        // frame. Preserve work discovered after this frame's pixels were drawn.
        let repaint_owed = p.dirty();
        let direct_present_ms = if painting && frame.is_none() && p.painter.name == "ink" {
            p.last_frame_ms().map_or(0., |(_, present)| present)
        } else {
            0.
        };
        let paint_ms = (paint.elapsed().as_secs_f64() * 1000. - direct_present_ms).max(0.);
        self.paint_ms += paint_ms;
        let (handed, direct) = (frame.is_some() || direct_present_ms > 0., frame.is_none());
        let present = Instant::now();
        if let Some(frame) = frame {
            let surface = match &mut *surface {
                Some(existing) => existing,
                None if self.direct => {
                    return Err("direct painter returned an unpresented frame".into())
                }
                slot => {
                    let context =
                        softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
                    slot.insert(
                        softbuffer::Surface::new(&context, window.clone())
                            .map_err(|e| e.to_string())?,
                    )
                }
            };
            surface.resize(width, height).map_err(|e| e.to_string())?;
            let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
            if buffer.len() != frame.data().len() / 4 {
                return Err(format!(
                    "frame extent {}x{} disagrees with window {}x{}",
                    frame.width(),
                    frame.height(),
                    width,
                    height
                ));
            }
            for (out, pixel) in buffer.iter_mut().zip(frame.data().as_chunks::<4>().0) {
                *out =
                    (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2]);
            }
            buffer.present().map_err(|e| e.to_string())?;
            p.recycle_frame(frame);
        }
        if !p.last_frame_succeeded() {
            return Ok(());
        }
        if let Some(pending) = self.pending_show.take() {
            #[cfg(test)]
            {
                self.reveal_awaited = Some(p.host().runner().canvas_images_pending());
            }
            pending.reveal()?;
        }
        if painting && handed {
            self.presents.drew(p, direct, self.started);
            // Service work can make the next picture dirty before the idle lookup.
            self.tooltip.update(
                p,
                window,
                (self.pointer_inside && self.focused && self.buttons == 0).then_some(self.pointer),
                false,
            );
        }
        // A keyboard command can change the cursor while the mouse is still.
        // Re-resolve the painted hit after the frame, including unmounts and HUDs.
        let cursor = if self.pointer_inside && self.focused {
            // A CSS keyword is winit's cursor name (cursor-icon parses
            // CSS's); `auto` and `none` show the default arrow.
            p.cursor_at(self.pointer.0, self.pointer.1)
                .name()
                .parse::<CursorIcon>()
                .unwrap_or(CursorIcon::Default)
        } else {
            CursorIcon::Default
        };
        if self.cursor != cursor {
            window.set_cursor(cursor);
            self.cursor = cursor;
        }
        let present_ms = present.elapsed().as_secs_f64() * 1000. + direct_present_ms;
        let presented_at = frame_started.elapsed().as_secs_f64() * 1000.;
        let render_ms = p.last_frame_ms().map_or(0., |(render, _)| render);
        self.present_ms += present_ms;
        if let Some(previous) = self.previous_frame.replace(frame_started) {
            let sample = (
                (frame_started - previous).as_secs_f64() * 1000.,
                paint_ms,
                present_ms,
            );
            if self.samples.len() < 4096 {
                self.samples.push(sample);
            } else {
                self.samples[(self.frames as usize - 1) % 4096] = sample;
            }
        }
        if self.frames == 0 {
            eprintln!(
                "exact-windows: first present returned at {:.3} ms after host start",
                self.started.elapsed().as_secs_f64() * 1000.
            );
        }
        // Activation can leave the baked picture unchanged. A clean frame need
        // not present again just to acknowledge that its data source is ready.
        p.first_pixel();
        let activated_at = frame_started.elapsed().as_secs_f64() * 1000.;
        // A screen's ready marker can exclude background requests that do not
        // contribute to it. Shelf's library-count appears with its seeded books.
        let screen_ready = std::env::var("EXACT_AGENT_WINDOW_READY").map_or_else(
            |_| !p.pending(),
            |target| p.host().kernel().find_by_test_id(&target).len() == 1,
        );
        if (self.frame_limit.is_some()
            || std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some()
            || std::env::var("EXACT_AGENT_WINDOW_PROBE").as_deref() == Ok("on"))
            && !self.measured_first_screen
            && p.last_frame_succeeded()
            && !p.dirty()
            && !p.data_activating()
            && screen_ready
        {
            if std::env::var("EXACT_AGENT_WINDOW_PROBE").as_deref() == Ok("off") {
                eprintln!("exact-windows: complete first screen submitted (probe off)");
            } else {
                let probe = Instant::now();
                let synchronized = exact_ink::device::compositor_sync_process_ms();
                let deadline = Instant::now() + Duration::from_secs(1);
                let displayed = loop {
                    let displayed = p.displayed_process_ms();
                    if displayed.is_ok() || Instant::now() >= deadline {
                        break displayed;
                    }
                    // The composition statistics can lag a DWM sync. Do not accept
                    // an older present as the complete screen's displayed frame.
                    if exact_ink::device::compositor_sync_process_ms().is_err() {
                        break displayed;
                    }
                };
                match displayed {
                    Ok(Some(ms)) => {
                        eprintln!(
                            "exact-windows: process to displayed first screen {ms:.3} ms (DXGI)"
                        )
                    }
                    Err(error) => {
                        eprintln!("exact-windows: displayed-frame timestamp unavailable: {error}")
                    }
                    Ok(None) => {}
                }
                match synchronized {
                    Ok(ms) => eprintln!(
                    "exact-windows: process to first-screen compositor sync {ms:.3} ms (DWM proxy)"
                ),
                    Err(error) => eprintln!("exact-windows: compositor sync: {error}"),
                }
                eprintln!(
                    "exact-windows: synchronization probe {:.3} ms",
                    probe.elapsed().as_secs_f64() * 1000.
                );
            }
            eprintln!(
                "exact-windows: first-screen painter {} adapter {:?}",
                p.painter.name, p.painter.adapter
            );
            self.measured_first_screen = true;
        }
        self.frames += 1;
        self.measure_first_composition = self.frames == 1
            && surface.is_none()
            && self.frame_limit.is_some()
            && std::env::var("EXACT_AGENT_WINDOW_PROBE").as_deref() != Ok("off");
        if std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some() {
            let line = format!("exact-windows: frame {} at {:.3} ms, {}x{}, scale {}, painter {}, caret {}, paint {:.3} ms, present {:.3} ms",
                self.frames, now, size.width, size.height, self.scale, p.painter.name,
                self.caret.visible(), paint_ms, present_ms);
            if self.frame_limit.is_some() {
                self.frame_logs.push(line);
            } else {
                eprintln!("{line}");
            }
        }
        let logged_at = frame_started.elapsed().as_secs_f64() * 1000.;
        if !self.jump_list_started && p.last_frame_succeeded() {
            self.jump_list_started = true;
            crate::jump_list::start(self.config.compat.clone());
        }
        apply_title(&mut self.title, p, window, &self.name);
        if let Some(access) = &mut self.accessibility {
            access.sync(p, window, self.title.caption(&self.name));
        }
        let accessible_at = frame_started.elapsed().as_secs_f64() * 1000.;
        self.ime.sync(window, p);
        let ime_at = frame_started.elapsed().as_secs_f64() * 1000.;
        if let Some(target) = self.first_tap.take() {
            let keys = p.host().kernel().find_by_test_id(&target);
            if keys.len() != 1 {
                return Err(format!(
                    "window benchmark tap {target:?} must identify exactly one view"
                ));
            }
            let id = p.host().kernel().node_by_key(keys[0]).unwrap().id;
            p.tap(id)?;
        }
        p.sync_surfaces();
        if std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some() {
            let line = format!("exact-windows: phases {} prepare {prepared_at:.3}, images {images_at:.3}, tick {tick_at:.3}, presented {presented_at:.3}, logged {logged_at:.3}, activated {activated_at:.3}, accessibility {accessible_at:.3}, ime {ime_at:.3}, surfaces {:.3} ms from frame start; render {render_ms:.3} ms",
                self.frames, frame_started.elapsed().as_secs_f64() * 1000.);
            if self.frame_limit.is_some() {
                self.frame_logs.push(line);
            } else {
                eprintln!("{line}");
            }
        }
        if repaint_owed || p.dirty() {
            p.repaint();
        }
        if self.frame_limit.is_some_and(|limit| self.frames >= limit) {
            // Bounded measurements write their per-frame log after the timed run.
            // Synchronous file writes can stall even with the DWM probe disabled.
            for line in self.frame_logs.drain(..) {
                eprintln!("{line}");
            }
            events.exit();
        }
        let period = window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .map_or(Duration::from_micros(16_667), |rate| {
                Duration::from_secs_f64(1000. / rate.max(1) as f64)
            });
        self.next_frame = if self.frame_limit.is_some()
            && std::env::var_os("EXACT_AGENT_WINDOW_UNPACED").is_some()
        {
            frame_started
        } else {
            next_frame_deadline(self.next_frame, frame_started, period)
        };
        events.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        if self.modal.active() {
            self.about_to_wait(events);
        }
        Ok(())
    }

    fn blink(&mut self, now: Instant) -> Result<(), String> {
        if self.unseen() || !self.focused {
            return Ok(());
        }
        let (Some(window), Some(p)) = (&self.window, &mut self.presenter) else {
            return Ok(());
        };
        if p.dirty() || p.wants_display_frames() || p.needs_animation_frame() {
            return Ok(());
        }
        if !self.caret.advance(now) {
            return Ok(());
        }
        p.set_caret_visible(self.caret.visible());
        if self.surface.is_none() || !self.caret.can_patch() {
            p.repaint();
            window.request_redraw();
            return Ok(());
        }
        if self.caret.needs_picture() {
            let other = p.frame();
            if !p.last_frame_succeeded() || p.dirty() {
                window.request_redraw();
                return Ok(());
            }
            self.caret.compare(other);
        }
        let mut buffer = self
            .surface
            .as_mut()
            .expect("patch surface")
            .buffer_mut()
            .map_err(|e| e.to_string())?;
        if let Some(damage) = self.caret.paint(&mut buffer, window.inner_size().width) {
            buffer
                .present_with_damage(&[damage])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl<D: DataSource + Default + 'static> ApplicationHandler<crate::accessibility::Event> for App<D> {
    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) && counters::enabled() {
            self.service_causes |= counters::TIMER;
        }
        let _span = crate::probe::Span::new("run loop wake", self.service_causes);
    }
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        boot_phase("resumed");
        let window = match events.create_window(
            Window::default_attributes()
                .with_no_redirection_bitmap(self.direct)
                .with_visible(false)
                .with_drag_and_drop(false)
                .with_title(&self.name)
                .with_window_icon(crate::icon::small())
                .with_taskbar_icon(crate::icon::large())
                .with_fullscreen(
                    (self.frame_limit.is_some()
                        && std::env::var_os("EXACT_AGENT_WINDOW_FULLSCREEN").is_some())
                    .then_some(Fullscreen::Borderless(None)),
                )
                .with_inner_size(LogicalSize::new(self.config.size.0, self.config.size.1))
                .with_min_inner_size(LogicalSize::new(self.min_size.0, self.min_size.1)),
        ) {
            Ok(value) => Rc::new(value),
            Err(e) => {
                self.fail(events, e);
                return;
            }
        };
        boot_phase("window-created");
        crate::placement::fit(&window, self.min_size);
        self.accessibility = Some(crate::accessibility::Access::new(
            events,
            &window,
            self.proxy.clone(),
        ));
        self.focused = window.has_focus();
        self.scale = window.scale_factor();
        self.config.scale = self.scale as f32;
        let size = window.inner_size();
        let viewport = (
            size.width as f32 / self.scale as f32,
            size.height as f32 / self.scale as f32,
        );
        self.config.clock_started = Some(self.started);
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                self.appearance
                    .install(handle.hwnd.get(), self.proxy.clone());
                self.modal.install(handle.hwnd.get(), self.proxy.clone());
                self.dialogs
                    .install(handle.hwnd.get(), self.modal.clone(), self.proxy.clone());
                self.desktop.set_window(handle.hwnd.get());
                crate::displays::install(handle.hwnd.get(), self.min_size);
                if let Err(error) = self.clicks.install(handle.hwnd.get()) {
                    self.fail(events, error);
                    return;
                }
            }
        }
        boot_phase("native-hooks-ready");
        exact_runner::page::set_awaits_present(true);
        let booted = app::boot_window_presenter::<D>(
            &mut self.config,
            viewport,
            self.hidden,
            self.painter,
            self.preparing.take(),
            self.direct && exact_ink::device::d3d11::startup_watchdog::after_layout(),
        );
        boot_phase("presenter-ready");
        match booted {
            Ok((mut p, warning)) => {
                if !self.hidden {
                    let attached = exact_ink::device::d3d11::Window::from_owner(window.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|owned| {
                            p.attach_painter_window(owned, [size.width, size.height])
                        });
                    match attached {
                        Ok(attached) => self.direct = attached,
                        Err(e) => {
                            self.fail(events, e);
                            return;
                        }
                    }
                    boot_phase("composition-ready");
                }
                p.host_mut().runner_mut().set_row_reuse(true);
                if let Some(scale) = self.appearance.text_scale() {
                    if let Some(error) = p.set_text_scale(scale) {
                        eprintln!("exact-windows: text size: {error}");
                    }
                }
                if let Some(theme) = window.theme() {
                    system_theme(&mut p, theme, self.config.agent);
                }
                if let Ok(handle) = window.window_handle() {
                    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                        p.set_clipboard(Box::new(crate::clipboard::Clipboard::new(
                            handle.hwnd.get(),
                        )));
                        let hwnd = handle.hwnd.get();
                        p.set_url_opener(move |url| crate::url::open(hwnd, url));
                        let proxy = self.proxy.clone();
                        p.set_window_closer(move || {
                            let _ = proxy.send_event(crate::accessibility::Event::Close);
                        });
                        lifecycle::window_commands(&mut p, &self.proxy);
                    }
                }
                if self.dialogs.available() {
                    p.serve_file_dialogs();
                }
                p.set_window_active(self.focused);
                // The window's clock owns time (the timers RFC, D1)
                p.use_native_timers(true);
                p.presents_pictures(true);
                p.set_live_clock(Some(exact_painted::presenter::wall_since(self.started)));
                if let Some(warning) = warning {
                    eprintln!("exact-windows: {warning}");
                }
                apply_title(&mut self.title, &p, &window, &self.name);
                self.presenter = Some(p);
                self.tray_start();
                let hidden = self.hidden;
                let can_open_files = self.dialogs.available();
                self.tell_page(|page| {
                    page.hidden = hidden;
                    page.can_open_files = can_open_files;
                });
            }
            Err(e) => {
                self.fail(events, e);
                return;
            }
        }
        boot_phase("before-show");
        window.set_visible(!self.hidden);
        boot_phase("visible");
        window.request_redraw();
        self.window = Some(window);
        self.observe_monitor();
    }
    fn user_event(&mut self, events: &ActiveEventLoop, event: crate::accessibility::Event) {
        let _span = crate::probe::Span::new("user event", 0);
        if matches!(event, crate::accessibility::Event::Close) {
            self.closing = true;
            if let Some(window) = &self.window {
                self.window_event(events, window.id(), WindowEvent::CloseRequested);
            }
            return;
        }
        match event {
            crate::accessibility::Event::TrayAccess(request) => {
                return self.tray_access(events, request)
            }
            crate::accessibility::Event::Focus => self.bring_forward(),
            crate::accessibility::Event::Hide => self.hide_by_app(),
            _ => {}
        }
        if let crate::accessibility::Event::Timer(fired) = event {
            if counters::enabled() {
                self.service_causes |= counters::TIMER;
            }
            self.deliver_timer(fired, self.now());
            self.about_to_wait(events);
            return;
        }
        if matches!(event, crate::accessibility::Event::ModalWake) {
            if counters::enabled() {
                self.service_causes |= counters::TIMER;
            }
            // Requesting another WM_PAINT is insufficient inside a native
            // modal loop. Consume its deadline in this dispatched callback.
            let result = self.draw(events);
            if counters::enabled() {
                self.service_causes &= !counters::TIMER;
                self.service_causes |= counters::DRAW;
            }
            if let Err(error) = result {
                self.fail(events, error);
                return;
            }
            self.about_to_wait(events);
            return;
        }
        // Executor, image, tray, appearance and hatch wakes are serviced in
        // `about_to_wait`, which requests a redraw only when the presenter
        // becomes dirty or needs continuous frames.
        if matches!(event, crate::accessibility::Event::Wake) && counters::enabled() {
            self.service_causes |= counters::WORKER;
        }
        if let crate::accessibility::Event::Access(event) = event {
            if let (Some(access), Some(p), Some(window)) =
                (&mut self.accessibility, &mut self.presenter, &self.window)
            {
                if event.window_id != window.id() {
                    return;
                }
                p.advance(self.started.elapsed().as_secs_f64() * 1000.);
                access.event(
                    p,
                    window,
                    self.title.caption(&self.name),
                    event.window_event,
                );
            }
        }
        self.about_to_wait(events);
    }
    fn window_event(&mut self, events: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let _span = crate::probe::Span::new("window event", 0);
        if counters::enabled() {
            self.service_causes |= match event {
                WindowEvent::MouseWheel { .. } => counters::WHEEL,
                WindowEvent::RedrawRequested => counters::DRAW,
                _ => counters::WINDOW,
            };
        }
        if let (Some(access), Some(window)) = (&mut self.accessibility, &self.window) {
            access.process_event(window, &event);
        }
        if presentation_resumes(&event) {
            if let Some(p) = &mut self.presenter {
                p.reset_painter_present_deadline();
            }
        }
        if matches!(
            event,
            WindowEvent::Focused(false) | WindowEvent::KeyboardInput { .. }
        ) {
            self.tooltip.clear();
        }
        if matches!(event, WindowEvent::CloseRequested) {
            return self.close_requested(events);
        }
        if matches!(
            event,
            WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. }
        ) {
            self.observe_monitor();
        }
        if matches!(event, WindowEvent::ScaleFactorChanged { .. }) {
            // Winit applies the new pixel extent after this callback. Pairing
            // its new scale with the old extent can clamp a list's scroll
            // position against a viewport that will never be shown.
            self.refit_after_scale = true;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }
        if matches!(event, WindowEvent::Resized(_)) {
            self.resized();
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            if let Err(e) = self.draw(events) {
                self.fail(events, e);
            }
            self.show_edit_menu();
            return;
        }
        let button_position = match &event {
            WindowEvent::MouseInput { state, button, .. } => self.clicks.position(*state, *button),
            _ => None,
        };
        let Some(p) = &mut self.presenter else { return };
        p.advance(self.started.elapsed().as_secs_f64() * 1000.);
        // The input's time: where the advance left the clock, a live runner's
        // sample past the time it was asked for
        let now = p.host().now();
        let (x, y) = self.pointer;
        // A button or a finger down when the focus went: each event the
        // person makes here asks Windows whether it is still down.
        if self.unheard
            && matches!(
                event,
                WindowEvent::Focused(true)
                    | WindowEvent::CursorEntered { .. }
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::Touch(_)
            )
        {
            let up = self.buttons & !buttons_down();
            if up != 0 {
                self.buttons &= !up;
                p.buttons_released(up);
            }
            self.touches.prune(p, crate::touch::in_contact);
            self.unheard = self.buttons != 0 || self.touches.any_down();
        }
        match event {
            WindowEvent::ThemeChanged(theme) => system_theme(p, theme, self.config.agent),
            WindowEvent::Moved(_) => self.ime.reposition(),
            WindowEvent::ModifiersChanged(modifiers) => {
                crate::keyboard::sync(p, Some(modifiers.state()))
            }
            WindowEvent::Focused(focused) => {
                self.clicks.clear();
                if !focused {
                    self.caret.clear();
                }
                if focused {
                    p.refresh_scrollbar_preferences();
                    p.set_window_active(true);
                    crate::keyboard::sync(p, None);
                }
                self.focused = focused;
                p.surface_lifecycle(self.minimized || self.occluded || self.hidden, !focused);
                if !focused {
                    // The contact ends, its release perhaps going to another
                    // window; each button and finger down holds a replacement
                    // until its release, or until Windows says it is up
                    // (`unheard`, LLP 1029.000.002 D9).
                    self.touches.forget(p, now);
                    self.unheard = self.buttons != 0 || self.touches.any_down();
                    p.set_window_active(false);
                    // Unfocused, the pads are not read: what they were is no longer known.
                    p.forget_pads();
                    self.pads.forget();
                }
            }
            WindowEvent::Occluded(occluded) => {
                if occluded {
                    self.caret.clear();
                }
                self.occluded = occluded;
                p.surface_lifecycle(self.minimized || occluded || self.hidden, !self.focused);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.buttons & 1 != 0 {
                    self.clicks.moved((position.x, position.y));
                }
                self.pointer_inside = true;
                self.pointer = (
                    position.x as f32 / self.scale as f32,
                    position.y as f32 / self.scale as f32,
                );
                p.set_pointer_kind(PointerKind::Mouse);
                let _ = p.pointer_move(self.pointer.0, self.pointer.1, now);
                if let Some(window) = &self.window {
                    self.tooltip.update(
                        p,
                        window,
                        (self.focused && self.buttons == 0).then_some(self.pointer),
                        true,
                    );
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.tooltip.clear();
                self.pointer_inside = false;
                // Windows captures pressed buttons. Preserve the drag until its
                // captured mouse-up, even outside the window; blur cancels it.
                if self.buttons == 0 {
                    let _ = p.pointer_lost(now);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.tooltip.clear();
                let (x, y) = if let Some((px, py)) = button_position {
                    self.pointer = (px as f32 / self.scale as f32, py as f32 / self.scale as f32);
                    self.pointer_inside = self.window.as_ref().is_some_and(|window| {
                        let size = window.inner_size();
                        px >= 0 && py >= 0 && (px as u32) < size.width && (py as u32) < size.height
                    });
                    self.pointer
                } else {
                    (x, y)
                };
                let down = state == ElementState::Pressed;
                p.set_pointer_kind(PointerKind::Mouse);
                if down {
                    self.caret.activity(Instant::now());
                    if button != MouseButton::Left {
                        self.clicks.clear();
                    }
                }
                let bit = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 4,
                    MouseButton::Back => 8,
                    MouseButton::Forward => 16,
                    _ => 0,
                };
                let released = !down && self.buttons & bit != 0;
                if down {
                    self.buttons |= bit;
                } else {
                    self.buttons &= !bit;
                }
                match button {
                    MouseButton::Left => {
                        let _ = if down {
                            let clicks = self.clicks.down(
                                (f64::from(x) * self.scale, f64::from(y) * self.scale),
                                Instant::now(),
                            );
                            p.pointer_down_with_clicks(x, y, now, clicks)
                        } else {
                            p.pointer_up(x, y, now)
                        };
                    }
                    MouseButton::Right => {
                        p.pointer_aux(2, down, x, y, now);
                        if released && self.buttons == 0 {
                            self.edit_menu = p.edit_menu_at((x, y));
                        }
                    }
                    MouseButton::Middle => p.pointer_aux(4, down, x, y, now),
                    // XBUTTON1 and XBUTTON2: Back and Forward at the release,
                    // as Chrome's (LLP 1115.000 D5.5).
                    MouseButton::Back => p.pointer_aux(8, down, x, y, now),
                    MouseButton::Forward => p.pointer_aux(16, down, x, y, now),
                    _ => {}
                }
            }
            WindowEvent::Touch(touch) => {
                self.tooltip.clear();
                self.clicks.clear();
                if self.touches.event(p, touch, self.scale, now) {
                    self.caret.activity(Instant::now());
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.tooltip.clear();
                self.clicks.clear();
                crate::wheel::dispatch(p, (x, y), delta, self.scale);
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                self.clicks.clear();
                // A key already down as the window gains focus (winit's
                // synthetic press) is held, not pressed here: a browser fires
                // no keydown for it, and it is no keyboard input to WinUI's
                // focus visual. Its release still arrives as a key's.
                if is_synthetic && event.state == ElementState::Pressed {
                    let (code, _) = key_names(event.physical_key, &event.logical_key, None);
                    p.hold_modifier(&code, true);
                    return;
                }
                if event.state == ElementState::Pressed {
                    self.caret.activity(Instant::now());
                }
                // Unicode input (VK_PACKET) has text but no physical key.
                let (code, key) = key_names(
                    event.physical_key,
                    &event.logical_key,
                    event.text.as_deref(),
                );
                if self.ime.takes_keys() {
                    p.hold_modifier(&code, event.state == ElementState::Pressed);
                    return;
                }
                let (handled, menu) = if event.state == ElementState::Pressed {
                    p.edit_menu_key(&code, &key, event.repeat)
                } else {
                    (false, None)
                };
                if handled {
                    self.edit_menu = menu;
                } else {
                    let down = event.state == ElementState::Pressed;
                    let text = event.text.as_deref();
                    crate::keyboard::press(p, (&code, &key), text, down, event.repeat);
                }
            }
            WindowEvent::Ime(event) => {
                self.caret.activity(Instant::now());
                if !exact_ink::device::d3d11::fields::focused() {
                    self.ime.event(p, event);
                }
            }
            _ => return,
        }
        if let Some(window) = &self.window {
            self.ime.sync(window, p);
            if (!self.pointer_inside || !self.focused) && self.cursor != CursorIcon::Default {
                window.set_cursor(CursorIcon::Default);
                self.cursor = CursorIcon::Default;
            }
            window.request_redraw();
        }
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        self.file_dialog_answers();
        self.handed_launches();
        self.tray_input(events);
        self.unshown_first_pixel();
        self.wait(events, |p| {
            (p.painter_retry_deadline(), p.painter_present_deadline())
        });
        self.queue_file_dialogs();
        self.tray_sync();
    }
}

#[cfg(test)]
pub(crate) mod tests;

/// The mouse buttons down now, as `pointer_aux` numbers them (1 the primary).
/// [GetAsyncKeyState](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getasynckeystate)
/// reads the physical buttons, so a swapped primary is read as the right one.
#[allow(unsafe_code)]
fn buttons_down() -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let (primary, secondary) = if swapped {
        (VK_RBUTTON, VK_LBUTTON)
    } else {
        (VK_LBUTTON, VK_RBUTTON)
    };
    [
        (primary, 1),
        (secondary, 2),
        (VK_MBUTTON, 4),
        (VK_XBUTTON1, 8),
        (VK_XBUTTON2, 16),
    ]
    .into_iter()
    .filter(|(key, _)| unsafe { GetAsyncKeyState(i32::from(*key)) } < 0)
    .fold(0, |bits, (_, bit)| bits | bit)
}
