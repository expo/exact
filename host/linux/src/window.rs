//! A Wayland window presents Exact's painted frames through a compositor
//! surface. Winit waits for compositor input, keyboard repeats and presenter work.

mod accessibility;
mod caret;
mod drop;
mod input;

use crate::event_loop::LocalPlan;
use exact_painted::app::{boot_presenter, Config};
use exact_painted::desktop::WindowSettings;
use exact_painted::presenter::{wall_since, Presenter};
use exact_painted::window_title::WindowTitle;
use exact_runner::DataSource;
use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::{Theme, Window, WindowId};

pub(super) fn run<D: DataSource + Default>(config: &mut Config, started: Instant) -> i32 {
    let result = EventLoop::<Event>::with_user_event()
        .build()
        .map_err(|e| e.to_string())
        .and_then(|events| {
            let mut app = App::<D> {
                proxy: events.create_proxy(),
                local_plan: config.dev_plan.as_deref().map(LocalPlan::new),
                config,
                started,
                live: None,
                settings: WindowSettings::default(),
                error: None,
                closed: Rc::new(Cell::new(false)),
            };
            let result = app.run(events);
            if let Some(live) = &mut app.live {
                live.presenter.end_hatches();
                live.presenter.end_packages();
            }
            result
        });
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("exact: Wayland: {error}");
            1
        }
    }
}

enum Event {
    Wake,
    Settings(WindowSettings),
    Access(accesskit_winit::Event),
    Drag(drop::Heard),
}

struct App<'a, D: DataSource> {
    proxy: EventLoopProxy<Event>,
    config: &'a mut Config,
    local_plan: Option<LocalPlan>,
    started: Instant,
    live: Option<Live<D>>,
    settings: WindowSettings,
    error: Option<String>,
    closed: Rc<Cell<bool>>,
}

struct Live<D: DataSource> {
    access: accessibility::Access,
    output_surface: Option<Arc<dyn smithay_client_toolkit::reexports::client::backend::ObjectData>>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    window: Rc<Window>,
    presenter: Presenter<D>,
    drops: Option<drop::Drops>,
    input: input::Input,
    extent: (u32, u32, f32),
    presented: bool,
    app_name: String,
    title: WindowTitle,
    caret: caret::Blink,
}

impl<D: DataSource + Default> App<'_, D> {
    fn run(&mut self, events: EventLoop<Event>) -> Result<(), String> {
        let proxy = events.create_proxy();
        let settings = exact_painted::desktop::watch_window_settings(move |settings| {
            let _ = proxy.send_event(Event::Settings(settings));
        });
        self.settings = settings.as_ref().map(|s| s.initial).unwrap_or_default();
        let wake = events.create_proxy();
        exact_painted::set_event_waker(Some(Arc::new(move || {
            let _ = wake.send_event(Event::Wake);
        })));
        let result = events.run_app(self);
        exact_painted::set_event_waker(None);
        result.map_err(|error| error.to_string())?;
        self.error.take().map_or(Ok(()), Err)
    }

    fn open(&mut self, events: &ActiveEventLoop) -> Result<Live<D>, String> {
        let receipt: serde_json::Value =
            serde_json::from_str(&self.config.compat).unwrap_or_default();
        let id = receipt["inputs"]["app"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(exact_painted::frames::app_name);
        let app_name = receipt["embedded"]["displayName"]
            .as_str()
            .unwrap_or(&id)
            .to_owned();
        let dark = self.settings.dark;
        let window = Rc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_name(id, "")
                        .with_title(&app_name)
                        .with_theme(dark.map(theme))
                        .with_inner_size(LogicalSize::new(self.config.size.0, self.config.size.1)),
                )
                .map_err(|error| error.to_string())?,
        );
        let access = accessibility::Access::new(events, &window, self.proxy.clone());
        window.set_visible(true);
        let context =
            softbuffer::Context::new(window.clone()).map_err(|error| error.to_string())?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|error| error.to_string())?;
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        self.config.scale = scale;
        self.config.clock_started = Some(self.started);
        let defaults = self.config.preferences;
        let output_surface = crate::rotation::surface_data(&window);
        self.config
            .preferences
            .get_or_insert_with(Default::default)
            .orientation = crate::rotation::screen(&window, output_surface.as_deref());
        if let Some(dark) = dark {
            self.config
                .preferences
                .get_or_insert_with(Default::default)
                .dark = dark;
        }
        exact_runner::page::set_awaits_present(true);
        let boot = boot_presenter::<D>(
            self.config,
            (size.width as f32 / scale, size.height as f32 / scale),
        );
        self.config.preferences = defaults;
        let (mut presenter, warning) = boot?;
        warn(warning);
        presenter.presents_pictures(true);
        presenter.set_live_clock(Some(wall_since(self.started)));
        presenter.set_system_scheme(dark.unwrap_or_else(|| defaults.unwrap_or_default().dark));
        let closed = self.closed.clone();
        presenter.set_window_closer(move || closed.set(true));
        window.request_redraw();
        Ok(Live {
            access,
            output_surface,
            surface,
            drops: drop::Drops::new(&window, self.proxy.clone()),
            window,
            presenter,
            input: input::Input::default(),
            extent: (size.width, size.height, scale),
            presented: false,
            app_name,
            title: WindowTitle::default(),
            caret: caret::Blink::default(),
        })
    }
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme::Dark
    } else {
        Theme::Light
    }
}

impl<D: DataSource + Default> ApplicationHandler<Event> for App<'_, D> {
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let at = Instant::now();
        let now = at.duration_since(self.started).as_secs_f64() * 1000.0;
        let orientation = crate::rotation::screen(&live.window, live.output_surface.as_deref());
        let mut prefs = live.presenter.host().runner().viewport().preferences;
        if prefs.orientation != orientation {
            prefs.orientation = orientation;
            warn(live.presenter.set_preferences(prefs));
        }
        live.turn(now);
        if let Some(drops) = live.drops.as_mut() {
            drops.turn(&mut live.presenter);
        }
        if self.closed.get() {
            events.exit();
            return;
        }
        if let Some(plan) = &mut self.local_plan {
            plan.poll(&mut live.presenter, D::default);
        }
        live.input.update_ime(&mut live.presenter, &live.window);
        let caret = live.caret.update(
            &mut live.presenter,
            live.input.active(),
            self.settings.cursor,
            now,
        );
        if caret == caret::Next::Frame
            || live.presenter.dirty()
            || live.presenter.wants_frames()
            || live.presenter.needs_animation_frame()
        {
            live.window.request_redraw();
        }
        let mut wait = live.presenter.turn_due_ms(now);
        if let caret::Next::At(at) = caret {
            let delay = (at - now).max(0.);
            wait = Some(wait.unwrap_or(delay).min(delay));
        }
        if self.local_plan.is_some() {
            wait = Some(wait.unwrap_or(100.0).min(100.0));
        }
        let deadline =
            wait.and_then(|ms| at.checked_add(Duration::try_from_secs_f64(ms / 1000.0).ok()?));
        events.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: Event) {
        let settings = match event {
            Event::Access(event) => {
                if let Some(live) = self
                    .live
                    .as_mut()
                    .filter(|live| live.window.id() == event.window_id)
                {
                    live.access.event(
                        &mut live.presenter,
                        &live.window,
                        live.title.caption(&live.app_name),
                        event.window_event,
                    );
                }
                return;
            }
            Event::Drag(heard) => {
                if let Some(live) = self.live.as_mut() {
                    if let Some(drops) = live.drops.as_mut() {
                        drops.heard(&mut live.presenter, heard);
                    }
                    if live.presenter.dirty() {
                        live.window.request_redraw();
                    }
                }
                return;
            }
            Event::Settings(settings) => settings,
            Event::Wake => return,
        };
        let previous = std::mem::replace(&mut self.settings, settings);
        if let Some(live) = &mut self.live {
            if settings.cursor != previous.cursor {
                live.caret
                    .restart(self.started.elapsed().as_secs_f64() * 1000.);
            }
            let dark = settings.dark;
            if previous.dark == dark {
                return;
            }
            live.window.set_theme(dark.map(theme));
            let mut preferences = live.presenter.host().runner().viewport().preferences;
            preferences.dark =
                dark.unwrap_or_else(|| self.config.preferences.unwrap_or_default().dark);
            live.presenter.set_system_scheme(preferences.dark);
            warn(live.presenter.set_preferences(preferences));
            live.window.request_redraw();
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        // The drag thread reads the display, which the event loop disconnects as it ends
        if let Some(live) = self.live.as_mut() {
            live.drops = None;
        }
    }

    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.live.is_none() {
            match self.open(events) {
                Ok(live) => self.live = Some(live),
                Err(error) => {
                    self.error = Some(error);
                    events.exit();
                }
            }
        }
    }

    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(live) = self.live.as_mut().filter(|live| live.window.id() == id) else {
            return;
        };
        let now = self.started.elapsed().as_secs_f64() * 1000.0;
        live.access.adapter.process_event(&live.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                warn(live.presenter.advance(now));
                if live.presenter.before_unload(live.presenter.host().now()) {
                    events.exit();
                }
            }
            WindowEvent::Destroyed => events.exit(),
            WindowEvent::RedrawRequested => {
                live.input.finish_ime_events(&mut live.presenter);
                live.caret.update(
                    &mut live.presenter,
                    live.input.active(),
                    self.settings.cursor,
                    now,
                );
                if let Err(error) = live.draw(self.started) {
                    self.error = Some(error);
                    events.exit();
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                live.window.request_redraw();
            }
            event => {
                live.caret.input(&event, now);
                if let WindowEvent::Focused(focused) = &event {
                    live.presenter.update_foreground(*focused);
                }
                warn(live.presenter.advance(now));
                let now = live.presenter.host().now();
                if let Err(error) = live
                    .input
                    .handle(&mut live.presenter, &live.window, event, now)
                {
                    eprintln!("exact: {error}");
                }
            }
        }
    }
}

impl<D: DataSource + Default> Live<D> {
    fn turn(&mut self, now: f64) {
        self.input.finish_ime_events(&mut self.presenter);
        let p = &mut self.presenter;
        warn(p.pump(now));
        p.poll_update();
        p.run_commands(D::default);
        warn(p.admit_idle_timer(now));
        p.hatch_turn();
        if p.wake_due_ms().is_some_and(|due| due <= now) {
            warn(p.follow_local_offset());
            warn(p.advance(now));
        }
        p.poll_images();
        p.poll_development(D::default);
        if self.title.update(p.host().runner()) {
            let title = self.title.caption(&self.app_name);
            self.window.set_title(title);
            self.access.sync(p, &self.window, title);
        }
    }

    fn draw(&mut self, started: Instant) -> Result<(), String> {
        let now = started.elapsed().as_secs_f64() * 1000.0;
        let size = self.window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let scale = self.window.scale_factor() as f32;
        let extent = (size.width, size.height, scale);
        let p = &mut self.presenter;
        if extent != self.extent {
            warn(p.resize_scaled(size.width as f32 / scale, size.height as f32 / scale, scale));
            self.extent = extent;
        }
        if p.wants_frames() {
            warn(p.follow_local_offset());
            warn(p.animation_frame(now));
        }
        if p.needs_animation_frame() {
            p.tick(now);
        }
        let frame = p.display_frame().ok_or("a frame is already pending")?;
        let pixels = frame.as_ref();
        if (pixels.width(), pixels.height()) != (size.width, size.height) {
            return Err(format!(
                "painted frame is {}x{} for a {}x{} window",
                pixels.width(),
                pixels.height(),
                width,
                height,
            ));
        }
        self.surface
            .resize(width, height)
            .map_err(|error| error.to_string())?;
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|error| error.to_string())?;
        for (out, pixel) in buffer.iter_mut().zip(pixels.data().as_chunks::<4>().0) {
            *out = (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2]);
        }
        self.window.pre_present_notify();
        buffer.present().map_err(|error| error.to_string())?;
        p.display_complete(&frame);
        warn(p.display_presented(&frame, started.elapsed().as_secs_f64() * 1000.0));
        p.follow_pointer();
        self.access
            .sync(p, &self.window, self.title.caption(&self.app_name));
        if !self.presented {
            self.presented = true;
            p.update_launched();
            println!("exact: Wayland window presented {width}x{height}");
        }
        Ok(())
    }
}

fn warn(warning: Option<String>) {
    if let Some(warning) = warning {
        eprintln!("exact: {warning}");
    }
}
