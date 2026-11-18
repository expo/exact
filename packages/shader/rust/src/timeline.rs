#[derive(Default)]
pub(crate) struct Timeline {
    last_ms: Option<f64>,
    elapsed_ms: f64,
    running: bool,
}

#[cfg(test)]
mod tests {
    use super::Timeline;

    #[test]
    fn an_unstamped_resume_excludes_time_without_paused_draws() {
        let mut timeline = Timeline::default();
        timeline.play(true, None);
        assert_eq!(timeline.at(0.), 0.);
        assert_eq!(timeline.at(1000.), 1.);
        timeline.play(false, None);
        timeline.play(true, None);
        assert_eq!(timeline.at(61000.), 1.);
        assert_eq!(timeline.at(62000.), 2.);
        timeline.play(true, None);
        assert_eq!(timeline.at(63000.), 3.);
    }
}

impl Timeline {
    pub fn play(&mut self, running: bool, at_ms: Option<f64>) {
        if self.running == running {
            return;
        }
        if let Some(last) = self.last_ms {
            self.at(at_ms.unwrap_or(last));
        }
        if running && at_ms.is_none() {
            // A resting host need not draw between pause and resume.
            self.last_ms = None;
        }
        self.running = running;
    }

    pub fn at(&mut self, now_ms: f64) -> f64 {
        if let Some(last) = self.last_ms {
            if self.running {
                self.elapsed_ms = (self.elapsed_ms + now_ms - last).max(0.0);
            }
        }
        self.last_ms = Some(now_ms);
        self.elapsed_ms / 1000.0
    }
}
