//! GTK keeps the insertion caret solid, fades it out, holds it hidden, then
//! fades it in over four equal parts of the desktop's configured cycle.
use exact_painted::{desktop::CursorSettings, paint::Rect4, Presenter};
use exact_runner::DataSource;
use winit::event::{ElementState, Ime, WindowEvent};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Next {
    Idle,
    At(f64),
    Frame,
}

#[derive(Default)]
pub(super) struct Blink {
    position: Option<Rect4>,
    activity: f64,
}

impl Blink {
    pub(super) fn restart(&mut self, now: f64) {
        self.activity = now;
    }

    pub(super) fn input(&mut self, event: &WindowEvent, now: f64) {
        let edited = match event {
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => event.state.is_pressed(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            }
            | WindowEvent::Focused(_)
            | WindowEvent::Ime(Ime::Commit(_)) => true,
            WindowEvent::Ime(Ime::Preedit(text, _)) => !text.is_empty(),
            _ => false,
        };
        if edited {
            self.restart(now);
        }
    }

    fn phase(&self, settings: CursorSettings, now: f64) -> (u8, Next) {
        let cycle = settings.cycle.as_secs_f64() * 1000.;
        let end = self.activity + settings.timeout.as_secs_f64() * 1000.;
        if self.position.is_none() || !settings.blink || cycle == 0. || now >= end {
            return (255, Next::Idle);
        }
        // Editing postpones the next cycle by half a period.
        let start = self.activity + cycle / 2.;
        if now < start {
            return (255, Next::At((start + cycle / 4.).min(end)));
        }
        let elapsed = (now - start) % cycle;
        let phase = elapsed / cycle;
        let at = |part| Next::At(f64::min(now - elapsed + cycle * part, end));
        if phase < 0.25 {
            (255, at(0.25))
        } else if phase < 0.5 {
            (((2. - 4. * phase) * 255.).round() as u8, Next::Frame)
        } else if phase < 0.75 {
            (0, at(0.75))
        } else {
            (((4. * phase - 3.) * 255.).round() as u8, Next::Frame)
        }
    }

    pub(super) fn update<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        active: bool,
        settings: CursorSettings,
        now: f64,
    ) -> Next {
        let position = active.then(|| presenter.caret_frame()).flatten();
        if self.position != position {
            self.position = position;
            self.restart(now);
        }
        let (opacity, next) = self.phase(settings, now);
        if presenter.set_caret_opacity(if active { opacity } else { 0 }) {
            presenter.repaint();
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn caret_uses_frame_callbacks_only_while_fading_and_stays_solid_after_idle() {
        let settings = CursorSettings::default();
        let mut blink = Blink::default();
        assert_eq!(blink.phase(settings, 0.), (255, Next::Idle));
        blink.position = Some((1., 2., 1., 20.));
        assert_eq!(blink.phase(settings, 0.), (255, Next::At(900.)));
        assert_eq!(blink.phase(settings, 1050.), (128, Next::Frame));
        assert_eq!(blink.phase(settings, 1200.), (0, Next::At(1500.)));
        assert_eq!(blink.phase(settings, 1650.), (128, Next::Frame));
        blink.restart(1650.);
        assert_eq!(blink.phase(settings, 1650.), (255, Next::At(2550.)));
        assert_eq!(blink.phase(settings, 11650.), (255, Next::Idle));
        let disabled = CursorSettings {
            blink: false,
            ..settings
        };
        assert_eq!(blink.phase(disabled, 2700.), (255, Next::Idle));
        let short = CursorSettings {
            timeout: Duration::from_millis(150),
            ..settings
        };
        assert_eq!(blink.phase(short, 1650.), (255, Next::At(1800.)));
    }
}
