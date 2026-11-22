//! A Wayland window presents Exact's painted frames through a compositor
//! surface. Winit dispatches window events while Exact waits on both the
//! compositor and the presenter's worker descriptors.

mod input;

use crate::event_loop::{poll, work_timeout, LocalPlan};
use exact_painted::app::{boot_presenter, Config};
use exact_painted::presenter::{wall_since, Presenter};
use exact_painted::window_title::WindowTitle;
use exact_runner::DataSource;
use std::cell::Cell;
use std::num::NonZeroU32;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::{Window, WindowId};

pub(super) fn run<D: DataSource + Default>(config: &mut Config, started: Instant) -> i32 {
    let mut app = App::<D> {
        local_plan: config.dev_plan.as_deref().map(LocalPlan::new),
        config,
        started,
        live: None,
        error: None,
        closed: Rc::new(Cell::new(false)),
    };
    let result = app.run();
    if let Some(live) = &mut app.live {
        live.presenter.end_hatches();
        live.presenter.end_packages();
    }
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("exact: Wayland: {error}");
            1
        }
    }
}

struct App<'a, D: DataSource> {
    config: &'a mut Config,
    local_plan: Option<LocalPlan>,
    started: Instant,
    live: Option<Live<D>>,
    error: Option<String>,
    closed: Rc<Cell<bool>>,
}

struct Live<D: DataSource> {
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    window: Rc<Window>,
    presenter: Presenter<D>,
    input: input::Input,
    extent: (u32, u32, f32),
    presented: bool,
    app_name: String,
    title: WindowTitle,
}

impl<D: DataSource + Default> App<'_, D> {
    fn run(&mut self) -> Result<(), String> {
        let mut events = EventLoop::new().map_err(|error| error.to_string())?;
        let display = events.as_raw_fd();
        loop {
            // Drawing and lifecycle callbacks stay inside winit. Only the wait
            // is shared with the presenter's data, image and update workers.
            let status = events.pump_app_events(Some(Duration::ZERO), self);
            if let Some(error) = self.error.take() {
                return Err(error);
            }
            if self.closed.get() || matches!(status, PumpStatus::Exit(_)) {
                return Ok(());
            }
            let live = self.live.as_mut().ok_or("no window was created")?;
            let now = self.started.elapsed().as_secs_f64() * 1000.0;
            live.turn(now);
            if self.closed.get() {
                return Ok(());
            }
            if let Some(plan) = &mut self.local_plan {
                plan.poll(&mut live.presenter, D::default);
            }
            live.input.update_ime(&mut live.presenter, &live.window);
            if live.presenter.dirty()
                || live.presenter.wants_frames()
                || live.presenter.needs_animation_frame()
            {
                live.window.request_redraw();
            }
            let due = live.presenter.turn_due_ms(now).map(|wait| now + wait);
            let mut timeout = work_timeout(true, false, due, now);
            if self.local_plan.is_some() {
                timeout = if timeout < 0 { 100 } else { timeout.min(100) };
            }
            let p = &live.presenter;
            let mut fds = vec![display, p.executor_fd(), p.image_fd()];
            fds.extend(p.content_region_fd());
            fds.extend(p.update_fd());
            poll(&fds, display, timeout)?;
        }
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
        let window = Rc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_name(id, "")
                        .with_title(&app_name)
                        .with_inner_size(LogicalSize::new(self.config.size.0, self.config.size.1)),
                )
                .map_err(|error| error.to_string())?,
        );
        let context =
            softbuffer::Context::new(window.clone()).map_err(|error| error.to_string())?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|error| error.to_string())?;
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        self.config.scale = scale;
        self.config.clock_started = Some(self.started);
        exact_runner::page::set_awaits_present(true);
        let (mut presenter, warning) = boot_presenter::<D>(
            self.config,
            (size.width as f32 / scale, size.height as f32 / scale),
        )?;
        warn(warning);
        presenter.presents_pictures(true);
        presenter.set_live_clock(Some(wall_since(self.started)));
        let closed = self.closed.clone();
        presenter.set_window_closer(move || closed.set(true));
        window.request_redraw();
        Ok(Live {
            surface,
            window,
            presenter,
            input: input::Input::default(),
            extent: (size.width, size.height, scale),
            presented: false,
            app_name,
            title: WindowTitle::default(),
        })
    }
}

impl<D: DataSource + Default> ApplicationHandler for App<'_, D> {
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
        match event {
            WindowEvent::CloseRequested => {
                warn(live.presenter.advance(now));
                if live.presenter.before_unload(live.presenter.host().now()) {
                    events.exit();
                }
            }
            WindowEvent::Destroyed => events.exit(),
            WindowEvent::RedrawRequested => {
                if let Err(error) = live.draw(self.started) {
                    self.error = Some(error);
                    events.exit();
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                live.window.request_redraw();
            }
            event => {
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
            self.window.set_title(self.title.caption(&self.app_name));
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
