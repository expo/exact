//! Service scheduled work and choose the next window wake.
use super::*;

impl<D: DataSource + Default + 'static> App<D> {
    /// The loop's turn before it waits. An app that has started to end, by
    /// a close or the status menu's Exit earlier in the turn, is served no
    /// more: its timers and commands would run against ended packages.
    pub(super) fn serve(&mut self, events: &ActiveEventLoop) {
        if self.ended {
            return;
        }
        if let Some(p) = &mut self.presenter {
            let now = p.host().now();
            self.touches.drain_cancels(p, now);
        }
        self.file_dialog_answers();
        self.handed_launches();
        self.tray_input(events);
        if self.ended {
            return;
        }
        self.unshown_first_pixel();
        self.wait(events, |p| {
            (p.painter_retry_deadline(), p.painter_present_deadline())
        });
        self.queue_file_dialogs();
        self.tray_sync();
    }

    pub(super) fn wait(
        &mut self,
        events: &ActiveEventLoop,
        deadlines: impl FnOnce(&Presenter<D>) -> (Option<Instant>, Option<Instant>),
    ) {
        let causes = if counters::enabled() {
            std::mem::take(&mut self.service_causes)
        } else {
            0
        };
        counters::service(causes);
        let _span = crate::probe::Span::new("carrier service", causes);
        crate::probe::sample();
        let now = self.now();
        for fired in self.timers.take_due(now) {
            self.deliver_timer(fired, now);
        }
        if let Some(p) = self.presenter.as_mut() {
            p.windows_field_edits(self.started.elapsed().as_secs_f64() * 1000.);
            p.timer_visibility(self.minimized || self.occluded || self.hidden);
            if let Some(e) = p.follow_local_offset() {
                eprintln!("exact-windows: {e}");
            }
            let now = self.started.elapsed().as_secs_f64() * 1000.0;
            if p.wake_due_ms().is_some_and(|due| due <= now) {
                if let Some(e) = p.advance(now) {
                    eprintln!("exact-windows: {e}");
                }
            }
            // Deliver nonvisual effects even when this turn needs no presentation
            if let Some(e) = p.pump(self.started.elapsed().as_secs_f64() * 1000.0) {
                eprintln!("exact-windows: {e}");
            }
            p.run_commands(D::default);
            if let Some(window) = &self.window {
                self.tooltip.update(
                    p,
                    window,
                    (self.pointer_inside && self.focused && self.buttons == 0)
                        .then_some(self.pointer),
                    false,
                );
            }
            crate::orientation::apply(p);
            p.hatch_turn();
            p.poll_images();
            if let Some(e) = p.admit_idle_timer(self.started.elapsed().as_secs_f64() * 1000.0) {
                eprintln!("exact-windows: {e}");
            }
        }
        if self.appearance.take() {
            if let (Some(window), Some(p)) = (&self.window, &mut self.presenter) {
                p.refresh_windows_place();
                system_theme(p, window.theme().unwrap_or(Theme::Light), self.config.agent);
                if let Some(scale) = self.appearance.text_scale() {
                    if let Some(error) = p.set_text_scale(scale) {
                        eprintln!("exact-windows: text size: {error}");
                    }
                }
                window.request_redraw();
            }
        }
        if let Err(error) = self.blink(Instant::now()) {
            self.fail(events, error);
            return;
        }
        let Some(p) = &mut self.presenter else { return };
        let wake = self.presents.poll(p, self.started);
        let retirement = presents::sooner(p.collect_painter_retirement(Instant::now()), wake);
        let pads_active = self.focused && !self.minimized && p.has_gamepad_input();
        if self.pads_active != pads_active {
            self.pads_active = pads_active;
            self.pads.forget();
            p.forget_pads();
        }
        if pads_active {
            let at = self.started.elapsed().as_secs_f64() * 1000.;
            self.pads.poll(
                Instant::now(),
                crate::pads::read,
                |index, connected, buttons, axes| {
                    crate::pads::register(index, connected, buttons, axes);
                    p.hardware_pad(index, connected, buttons, axes, at)
                },
            );
        }
        if let Some(window) = &self.window {
            if apply_title(&mut self.title, p, window, &self.name) {
                if let Some(access) = &mut self.accessibility {
                    // A full accessibility refresh can render a canvas before the display frame.
                    access.set_title(self.title.caption(&self.name));
                }
            }
        }
        if let Err(e) = self.timers.update(
            p.native_timer_generation(),
            p.host().runner().native_timers(),
            self.started.elapsed().as_secs_f64() * 1000.0,
        ) {
            self.fail(events, e);
            return;
        }
        if let Some(due) = p.wake_due_ms() {
            let deadline = self.started + Duration::from_secs_f64((due / 1000.).max(0.));
            events.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            events.set_control_flow(ControlFlow::Wait);
        }
        let continuous =
            self.frame_limit.is_some() || p.wants_display_frames() || p.needs_animation_frame();
        let unpaced =
            self.frame_limit.is_some() && std::env::var_os("EXACT_AGENT_WINDOW_UNPACED").is_some();
        let (retry, present) = deadlines(p);
        let schedule = FrameSchedule {
            cadence: (continuous && !unpaced).then_some(self.next_frame),
            retry,
            present,
        };
        let now = Instant::now();
        let redraw = (!self.minimized && !self.occluded && !self.hidden)
            .then(|| schedule.redraw(now, p.dirty() || continuous || self.redraw_pending))
            .flatten();
        if let Some(due) = redraw {
            if now >= due {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            if !matches!(events.control_flow(), ControlFlow::WaitUntil(t) if t <= due) {
                events.set_control_flow(ControlFlow::WaitUntil(due));
            }
        }
        if let Some(due) = self.timers.next_delivery() {
            let deadline = self.started + Duration::from_secs_f64(due / 1000.0);
            if !matches!(events.control_flow(), ControlFlow::WaitUntil(t) if t <= deadline) {
                events.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        }
        if let Some(due) = retirement {
            if !matches!(events.control_flow(), ControlFlow::WaitUntil(t) if t <= due) {
                events.set_control_flow(ControlFlow::WaitUntil(due));
            }
        }
        // Only an app with an input canvas needs controller discovery wakes.
        if pads_active {
            let soon = self.pads.next_poll(Instant::now());
            if !matches!(events.control_flow(), ControlFlow::WaitUntil(t) if t <= soon) {
                events.set_control_flow(ControlFlow::WaitUntil(soon));
            }
        }
        if self.focused
            && !self.minimized
            && !self.occluded
            && !self.hidden
            && !p.dirty()
            && !p.wants_display_frames()
            && !p.needs_animation_frame()
        {
            if let Some(soon) = self.caret.deadline() {
                if !matches!(events.control_flow(), ControlFlow::WaitUntil(t) if t <= soon) {
                    events.set_control_flow(ControlFlow::WaitUntil(soon));
                }
            }
        }
        self.modal.wait_until(match events.control_flow() {
            ControlFlow::WaitUntil(deadline) => Some(deadline),
            _ => None,
        });
    }
}
