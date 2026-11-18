use crate::data::text::Float;
use std::fmt::Write;
#[derive(Default)]
#[doc(hidden)]
pub struct Ring {
    #[doc(hidden)]
    pub values: Vec<f64>,
    #[doc(hidden)]
    pub len: usize,
    #[doc(hidden)]
    pub next: usize,
    #[doc(hidden)]
    pub count: u64,
    #[doc(hidden)]
    pub sum: f64,
    #[doc(hidden)]
    pub max: f64,
}
impl Ring {
    #[doc(hidden)]
    pub fn arm(&mut self, reset: bool) {
        if reset {
            *self = Self::default();
        }
        self.values.resize(16384, 0.);
    }
    #[doc(hidden)]
    pub fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
        self.count = 0;
        self.sum = 0.0;
        self.max = 0.0;
    }
    #[doc(hidden)]
    pub fn push(&mut self, value: f64) {
        if !value.is_finite() || value < 0.0 {
            return;
        }
        self.count += 1;
        self.sum += value;
        self.max = self.max.max(value);
        if self.values.is_empty() {
            return;
        }
        self.values[self.next] = value;
        self.next = (self.next + 1) % self.values.len();
        self.len = (self.len + 1).min(self.values.len());
    }
    #[doc(hidden)]
    pub fn json(&self, out: &mut String) {
        if self.len == 0 {
            out.push_str(r#"{"p50":null,"p95":null,"p99":null"#);
        } else {
            let mut values = self.values[..self.len].to_vec();
            values.sort_unstable_by(f64::total_cmp);
            let p = |percent: usize| Float(values[(self.len * percent).div_ceil(100) - 1]);
            write!(out, r#"{{"p50":{},"p95":{},"p99":{}"#, p(50), p(95), p(99)).unwrap();
        }
        // Totals include pushes made before arming (LLP 1182.000 D2).
        write!(
            out,
            r#","max":{},"count":{},"mean":{}}}"#,
            Float(self.max),
            self.count,
            Float(self.sum / self.count.max(1) as f64)
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::Ring;

    fn json(ring: &Ring) -> String {
        let mut out = String::new();
        ring.json(&mut out);
        out
    }

    #[test]
    fn unarmed_ring_keeps_totals_without_percentiles() {
        let mut ring = Ring::default();
        for value in [2., 4., 6.] {
            ring.push(value);
        }
        assert_eq!(
            json(&ring),
            r#"{"p50":null,"p95":null,"p99":null,"max":6,"count":3,"mean":4}"#
        );
    }

    #[test]
    fn armed_empty_ring_has_no_percentiles() {
        let mut ring = Ring::default();
        ring.arm(false);
        assert_eq!(
            json(&ring),
            r#"{"p50":null,"p95":null,"p99":null,"max":0,"count":0,"mean":0}"#
        );
    }

    #[test]
    fn first_armed_sample_sets_all_percentiles() {
        let mut ring = Ring::default();
        ring.push(2.);
        ring.arm(false);
        ring.push(6.);
        assert_eq!(
            json(&ring),
            r#"{"p50":6,"p95":6,"p99":6,"max":6,"count":2,"mean":4}"#
        );
    }

    #[test]
    fn measured_zero_has_numeric_percentiles() {
        let mut ring = Ring::default();
        ring.arm(false);
        ring.push(0.);
        assert_eq!(
            json(&ring),
            r#"{"p50":0,"p95":0,"p99":0,"max":0,"count":1,"mean":0}"#
        );
    }

    #[test]
    fn reset_clears_percentiles_and_totals() {
        let mut ring = Ring::default();
        ring.arm(false);
        ring.push(6.);
        ring.arm(true);
        assert_eq!(
            json(&ring),
            r#"{"p50":null,"p95":null,"p99":null,"max":0,"count":0,"mean":0}"#
        );
    }
}
