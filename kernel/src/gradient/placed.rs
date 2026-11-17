//! Size-dependent stop fix-up and repetition (LLP 1066, 2026-10-12).
use super::{fix_up, Color, ColorValue, Geometry, Gradient, Lookup, Stop, StopUnit};

impl Gradient {
    /// Whether painting needs the box before the stops can be resolved.
    pub fn needs_box(&self) -> bool {
        self.repeating || self.stops.iter().any(|s| s.unit != StopUnit::Percent)
    }

    /// A nonrepeating, percentage-only ramp and its placement for a native
    /// painter. The authored row is unchanged, so resizing resolves anew.
    pub fn placed(&self, width: f32, height: f32, dark: bool) -> (Self, Geometry) {
        self.placed_with(width, height, dark, &crate::style::wide::wide)
    }

    /// [`placed`](Self::placed), each wide stop's colour read through
    /// `wide` ([`Gradient::resolved_with`]).
    pub fn placed_with(
        &self,
        width: f32,
        height: f32,
        dark: bool,
        wide: Lookup<'_>,
    ) -> (Self, Geometry) {
        let mut geometry = self.geometry(width, height);
        if !self.needs_box() {
            return (self.clone(), geometry);
        }
        let length = match geometry {
            Geometry::Linear { start, end } => (end.0 - start.0).hypot(end.1 - start.1),
            Geometry::Radial { radii, .. } => radii.0,
            Geometry::Conic { .. } => 1.0,
        }
        .max(1e-6);
        let mut ramp = self.fixed_up(length);
        if self.repeating {
            let (first, period) = match ramp.period(length, dark, wide) {
                Ok(cycle) => cycle,
                Err(solid) => return (solid, geometry),
            };
            let last = ramp.stops.last().unwrap().at;
            let begin = (-first / period).floor() as i32;
            let end = ((100.0 - last) / period).ceil() as i32;
            let stops = ramp.stops.clone();
            ramp.stops.clear();
            for cycle in begin..=end {
                let start = first + cycle as f32 * period;
                let end = first + (cycle + 1) as f32 * period;
                ramp.stops
                    .extend(stops.iter().enumerate().map(|(i, s)| Stop {
                        // Adjacent cycles share exactly one boundary, including
                        // after f32 rounding: native ramps require sorted stops.
                        at: if i == stops.len() - 1 {
                            end
                        } else {
                            (s.at + cycle as f32 * period).clamp(start, end)
                        },
                        ..*s
                    }));
            }
        }
        // Move the line's endpoints, not the colours: all native APIs take
        // stops in 0..1, including when authored lengths reach past the box.
        let low = ramp.stops[0].at.min(0.0);
        let high = ramp.stops.last().unwrap().at.max(100.0);
        let span = high - low;
        for stop in &mut ramp.stops {
            stop.at = (stop.at - low) / span * 100.0;
        }
        match &mut geometry {
            Geometry::Linear { start, end } => {
                let d = (end.0 - start.0, end.1 - start.1);
                *end = (start.0 + d.0 * high / 100.0, start.1 + d.1 * high / 100.0);
                *start = (start.0 + d.0 * low / 100.0, start.1 + d.1 * low / 100.0);
            }
            Geometry::Radial { radii, .. } => {
                radii.0 *= high / 100.0;
                radii.1 *= high / 100.0;
            }
            Geometry::Conic { .. } => {}
        }
        (ramp, geometry)
    }

    /// One period of a repeating linear gradient in a `width` × `height`
    /// box: its ramp over the period (stops 0–100%) and the line from the
    /// period's first stop to its last, which a native host rasterizes once
    /// and tiles (#661). Where there is no period to repeat, the solid colour
    /// [`placed`](Self::placed) paints, over one CSS pixel. `None` for any
    /// other gradient, or a box with no extent along the line.
    pub fn cycle(&self, width: f32, height: f32, dark: bool) -> Option<(Self, Geometry)> {
        self.cycle_with(width, height, dark, &crate::style::wide::wide)
    }

    /// [`cycle`](Self::cycle), each wide stop's colour read through `wide`.
    pub fn cycle_with(
        &self,
        width: f32,
        height: f32,
        dark: bool,
        wide: Lookup<'_>,
    ) -> Option<(Self, Geometry)> {
        let Geometry::Linear { start, end } = self.geometry(width, height) else {
            return None;
        };
        let length = (end.0 - start.0).hypot(end.1 - start.1);
        if !self.repeating || length < 1e-6 {
            return None;
        }
        let at = |percent: f32| {
            (
                start.0 + (end.0 - start.0) * percent / 100.0,
                start.1 + (end.1 - start.1) * percent / 100.0,
            )
        };
        let mut ramp = self.fixed_up(length);
        let (first, period) = match ramp.period(length, dark, wide) {
            Ok(cycle) => cycle,
            Err(solid) => {
                let line = Geometry::Linear {
                    start,
                    end: at(100.0 / length),
                };
                return Some((solid, line));
            }
        };
        for stop in &mut ramp.stops {
            stop.at = (stop.at - first) / period * 100.0;
        }
        let line = Geometry::Linear {
            start: at(first),
            end: at(first + period),
        };
        Some((ramp, line))
    }

    /// Length stops as percentages of a line `length` CSS px long, then
    /// CSS's fix-up: a ramp that no longer needs the box.
    fn fixed_up(&self, length: f32) -> Self {
        let mut ramp = self.clone();
        for stop in &mut ramp.stops {
            if stop.unit == StopUnit::Px {
                stop.at = stop.at / length * 100.0;
                stop.unit = StopUnit::Percent;
            }
        }
        fix_up(&mut ramp.stops);
        ramp.repeating = false;
        ramp
    }

    /// A fixed-up repeating ramp's first stop and period, in percent of a
    /// line `length` CSS px long; or, where it has none to repeat, the solid
    /// colour Chrome paints.
    fn period(&self, length: f32, dark: bool, wide: Lookup<'_>) -> Result<(f32, f32), Self> {
        let first = self.stops[0].at;
        let period = self.stops.last().unwrap().at - first;
        if period <= 0.0 {
            // Chrome paints the last stop for a zero-length repetition.
            let stop = *self.stops.last().unwrap();
            let mut solid = self.clone();
            solid.stops = [0.0, 100.0].map(|at| Stop { at, ..stop }).to_vec();
            return Err(solid);
        }
        // CSS Images 3 §3.3: unresolved repetitions are the average
        // colour. Bound the expanded native ramp for microscopic periods.
        if period * length / 100.0 < 0.01 || 100.0 / period > 8192.0 {
            return Err(self.clone().average(dark, wide));
        }
        Ok((first, period))
    }

    fn average(mut self, dark: bool, wide: Lookup<'_>) -> Self {
        let first = self.stops[0].at;
        let span = self.stops.last().unwrap().at - first;
        for s in &mut self.stops {
            s.at = (s.at - first) / span * 100.0;
        }
        let ramp = self.resolved_with(dark, wide);
        let mut sums = [0.0; 4];
        for pair in ramp.windows(2) {
            let weight = (pair[1].0 - pair[0].0) / 2.0;
            for (_, c) in pair {
                let a = c.a() as f32 / 255.0;
                for (sum, v) in
                    sums.iter_mut()
                        .zip([c.r() as f32 * a, c.g() as f32 * a, c.b() as f32 * a, a])
                {
                    *sum += weight * v;
                }
            }
        }
        let channel = |i: usize| {
            if sums[3] > 0.0 {
                (sums[i] / sums[3]).round() as u8
            } else {
                0
            }
        };
        let color = ColorValue::Fixed(Color::rgba(
            channel(0),
            channel(1),
            channel(2),
            (sums[3] * 255.0).round() as u8,
        ));
        self.interpolation = None;
        self.stops = [0.0, 100.0]
            .map(|at| Stop {
                at,
                color,
                unit: StopUnit::Percent,
            })
            .to_vec();
        self
    }
}
