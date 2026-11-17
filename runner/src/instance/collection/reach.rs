//! A list's edges: when `reachstart` and `reachend` fire, and how far ahead
//! of them `reach-margin` and the window's travel lead say (@ref LLP 1145 D4,
//! LLP 1027.004 D3).
use super::*;

/// One side of a `reach-margin`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Length {
    Px(f64),
    /// Of the port's extent along the list's axis.
    Percent(f64),
}

/// How far past the port, before and after it, an edge row may be when its
/// edge fires. The default is one port each side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Margin([Length; 2]);

impl Default for Margin {
    fn default() -> Self {
        Self([Length::Percent(100.0); 2])
    }
}

impl Margin {
    /// One or two nonnegative px or % lengths (the compiler's
    /// `lower-reach-margin`); one value is both sides.
    pub(super) fn parse(text: &str) -> Option<Self> {
        let length = |part: &str| {
            let part = part.to_ascii_lowercase();
            let (number, unit): (&str, fn(f64) -> Length) = match part.strip_suffix("px") {
                Some(n) => (n, Length::Px),
                None => (part.strip_suffix('%')?, Length::Percent),
            };
            let n = number.parse::<f64>().ok()?;
            (n.is_finite() && n >= 0.0).then(|| unit(n))
        };
        let parts: Vec<_> = text.split_ascii_whitespace().map(length).collect();
        match parts[..] {
            [Some(both)] => Some(Self([both; 2])),
            [Some(before), Some(after)] => Some(Self([before, after])),
            _ => None,
        }
    }
    /// The margin in px for a port of `port` along the list's axis.
    pub(super) fn resolve(self, port: f64) -> [f64; 2] {
        self.0.map(|side| match side {
            Length::Px(n) => n,
            Length::Percent(n) => port * n / 100.0,
        })
    }
    /// The edge band before and after a port of `port` moving at
    /// `velocity`: the margin, and toward the side it travels the lead the
    /// window takes past its overscan ([`travel_lead`]). The default band
    /// is then the window's own lead, so a row the window mounts for a fling
    /// is within it; a set margin keeps its distance from the rest band.
    fn band(self, port: f64, velocity: f64) -> [f64; 2] {
        let mut band = self.resolve(port);
        if velocity != 0.0 {
            band[usize::from(velocity > 0.0)] += travel_lead(port, velocity);
        }
        band
    }
}

/// Whether an update put a page at a list's start, which re-arms the start
/// edge as rows put past the last row re-arm the end (@ref LLP 1168.000
/// D1.1; LLP 1010, the 2026-09-29 ruling's mirror): a reader still at the
/// start when an older page lands is offered the next one. With `old` the
/// keys before the update, `len` the rows now, and `position` and `key`
/// reading them, a page landed when
///
/// - the old first row is kept with every row now before it new (a page
///   put before it), or
/// - the old first row stays first, a leading row, and the old second row is
///   kept further down with every row between them new (a page put after the
///   leading row), or the update gave rows to a leading row that was alone
///   (the first page).
///
/// An arrival at the end, a reorder, an empty or identical answer, a
/// replaced first row and a window that slid past it are none.
pub(super) fn page_at_start<'a>(
    old: &[Rc<str>],
    len: usize,
    position: impl Fn(&str) -> Option<usize>,
    key: impl Fn(usize) -> Option<&'a str>,
) -> bool {
    let Some(first) = old.first().and_then(|k| position(k)) else {
        return false;
    };
    // The rows in `range` are new: none is among the old keys. The set holds
    // only the range; each old key is looked up in it once.
    let new_between = |range: std::ops::Range<usize>| {
        let Some(keys) = range
            .map(&key)
            .collect::<Option<std::collections::HashSet<&str>>>()
        else {
            return false;
        };
        !old.iter().any(|k| keys.contains(&**k))
    };
    match (first, old.get(1)) {
        (0, None) => len > 1,
        (0, Some(second)) => position(second).is_some_and(|q| q > 1 && new_between(1..q)),
        (p, _) => new_between(0..p),
    }
}

/// Whether an update put rows after a leading run at the top of a window
/// that began at the list's start: the old rows before `bound` (the window's
/// last row) that kept their places, then rows all new, then the next old
/// row kept. Order alone also reads an edit inside the window so; only a
/// `reachstart` still unanswered makes it a page (@ref LLP 1168.000 D1.2).
pub(super) fn page_after_leading_run<'a>(
    old: &[Rc<str>],
    bound: usize,
    position: impl Fn(&str) -> Option<usize>,
    key: impl Fn(usize) -> Option<&'a str>,
) -> bool {
    let run = (old.iter().take(bound))
        .enumerate()
        .take_while(|(i, k)| position(k) == Some(*i))
        .count();
    let Some(q) = old.get(run).and_then(|k| position(k)) else {
        return false;
    };
    let Some(keys) = (run..q)
        .map(&key)
        .collect::<Option<std::collections::HashSet<&str>>>()
    else {
        return false;
    };
    q > run && !old.iter().any(|k| keys.contains(&**k))
}

/// Whether an update answered a `reachstart`: a key put in, taken out or
/// moved (by its identity), or a row told apart from the one it replaced as
/// the JS target's `eq` tells them (`host/web-js/shape.js`): signed zero
/// apart, every NaN equal to every NaN. An answer's other fields, a key's or
/// a row body's inputs and equal rows do not (@ref LLP 1168.000 D1.2).
pub(super) fn rows_answered(
    old: &[Rc<str>],
    new: &[Rc<str>],
    was: &[Value],
    now: &[Value],
) -> bool {
    fn eq(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => {
                a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
            }
            (Value::Option(Some(a)), Value::Option(Some(b))) => eq(a, b),
            (Value::List(a), Value::List(b)) | (Value::Record(a), Value::Record(b)) => all(a, b),
            _ => a == b,
        }
    }
    fn all(a: &[Value], b: &[Value]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| eq(a, b))
    }
    old != new || !all(was, now)
}

impl Collection {
    /// Which edge rows are within the edge band ([`Margin::band`]) of a port
    /// moving at `velocity`. Pins never qualify an edge.
    ///
    /// An edge re-arms once outside the band of the report it is in, or,
    /// once its row is another than the one it fired at (a page landed past
    /// it), outside the band at rest: a page smaller than a fling's lead
    /// would otherwise leave the new edge inside the band, disarmed, until
    /// the reader left it. Rows that change elsewhere (a message appended to
    /// a chat whose reader is at its start) do not re-arm it. Either waits
    /// until the geometric window (one port each side, what a report at rest
    /// mounts) is measured: replacement estimates cannot manufacture a
    /// temporary edge exit. A band wider than that window reaches rows not
    /// mounted or not yet measured, so their estimates decide it.
    pub(super) fn geometric_edges(&mut self, velocity: f64) -> Result<[bool; 2], InstanceError> {
        let mut reached = [false; 2];
        if let Some(g) = &self.geometry {
            if self.index.len() > 0 {
                let window = self
                    .index
                    .window(g.offset, g.port_main, [None, None])
                    .map_err(index_error)?;
                let within = |band| -> Result<[bool; 2], InstanceError> {
                    let reach = self
                        .index
                        .window_led(g.offset, g.port_main, band, [None, None])
                        .map_err(index_error)?;
                    if self.fixed.is_some() && g.port_main > 0.0 {
                        let offset = g.offset.max(0.0);
                        return Ok([
                            offset - band[0] <= 0.0,
                            offset + g.port_main + band[1] >= self.index.total_height(),
                        ]);
                    }
                    Ok([0, self.index.len() - 1]
                        .map(|i| reach.segments.iter().any(|range| range.contains(&i))))
                };
                reached = within(self.reach.band(g.port_main, velocity))?;
                // With fixed content filling the port, no data row is in
                // the overscan, and the window's geometry is still known.
                let fixed_only =
                    self.fixed.is_some() && g.port_main > 0.0 && window.overscan.is_empty();
                let measured = self.index.range_measured(window.overscan.clone()) || fixed_only;
                if self.start_owed && measured {
                    self.edge_armed[0] = true;
                    self.start_owed = false;
                }
                if self.edge_armed != [true; 2] && measured {
                    let at_rest = within(self.reach.band(g.port_main, 0.0))?;
                    for i in 0..2 {
                        let row = self.index.key([0, self.index.len() - 1][i]);
                        let paged = self.edge_fired[i].as_deref() != row;
                        self.edge_armed[i] |= !reached[i] || (paged && !at_rest[i]);
                    }
                }
                // A start that re-armed asks again when next reached: what
                // it asked before is no longer outstanding.
                if self.edge_armed[0] {
                    self.start_asked = false;
                }
            }
        }
        Ok(reached)
    }
    /// Whether an update put a page at the list's start: the rows of `old`
    /// (the keys before it) against the rows now ([`page_at_start`]).
    pub(super) fn page_before_start(&self, old: &[Rc<str>]) -> bool {
        page_at_start(
            old,
            self.index.len(),
            |key| self.index.position(key),
            |i| self.index.key(i),
        )
    }
    /// An edge fired: disarmed until it re-arms ([`Collection::geometric_edges`]).
    pub(super) fn disarm_edge(&mut self, i: usize) {
        self.edge_armed[i] = false;
        // A start that fired has had its page; one owed before is spent.
        // Only a list that opens at its end keeps what it asked: no other
        // list's anchor reads it.
        if i == 0 {
            self.start_owed = false;
            self.start_asked = self.opens_at_end;
        }
        let row = [0, self.index.len().saturating_sub(1)][i];
        self.edge_fired[i] = self.index.shared_key(row).cloned();
    }
    /// The edge a report's geometry reached, if armed and handled.
    pub(super) fn edge_event(
        &mut self,
        fill: CollectionFill,
    ) -> Result<Option<CollectionEdges>, InstanceError> {
        let reached = self.geometric_edges(fill.velocity)?;
        let ready = [0, 1].map(|i| reached[i] && self.edge_armed[i] && self.edge_handlers[i]);
        let edge = (0..2).find(|&i| ready[i]);
        // A build-only report runs no action (LLP 1072 §5): the edge stays
        // armed and the report is pending, so the next report runs it.
        if fill.create_only && edge.is_some() {
            self.pending = true;
            return Ok(None);
        }
        Ok(edge.map(|i| {
            self.disarm_edge(i);
            CollectionEdges {
                first: [EventKind::Reachstart, EventKind::Reachend][i],
                // End remains armed until an action actually consumes it.
                end_after_noop: ready[0] && ready[1],
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LLP 1168.000 D1.1's cases, the keys before an update against after.
    #[test]
    fn a_page_at_the_start_is_new_rows_before_the_first_or_after_a_leading_row() {
        let lands = |old: &[&str], now: &[&str]| {
            let old: Vec<Rc<str>> = old.iter().map(|&k| Rc::from(k)).collect();
            page_at_start(
                &old,
                now.len(),
                |k| now.iter().position(|&n| n == k),
                |i| now.get(i).copied(),
            )
        };
        // A page put before the first row; one put after a leading row; the
        // first page after a leading row alone.
        assert!(lands(&["a", "b"], &["x", "y", "a", "b"]));
        assert!(lands(&["s", "a", "b"], &["s", "x", "y", "a", "b"]));
        assert!(lands(&["s"], &["s", "a", "b"]));
        // Accepted: one row that gains rows after it, and an unrelated row
        // put before a kept first row, read as pages.
        assert!(lands(&["a"], &["a", "b"]));
        assert!(lands(&["a", "b"], &["z", "a", "b"]));
        // None: an arrival at the end, a reorder, an identical or empty
        // answer, a replaced first row, a window that slid, rows of the old
        // list put before the first row.
        assert!(!lands(&["a", "b"], &["a", "b", "c"]));
        assert!(!lands(&["a", "b"], &["b", "a"]));
        assert!(!lands(&["a", "b"], &["a", "b"]));
        assert!(!lands(&["a", "b"], &[]));
        assert!(!lands(&[], &["a"]));
        assert!(!lands(&["a", "b"], &["c", "b"]));
        assert!(!lands(&["s", "a", "b"], &["s", "c", "d"]));
        assert!(!lands(&["a", "b", "c"], &["c", "a", "b"]));
    }

    /// LLP 1168.000 D1.2's leading run: rows after old rows that kept their
    /// places, before the window's last row (`bound`).
    #[test]
    fn rows_after_a_leading_run_before_the_bound() {
        let after = |old: &[&str], now: &[&str], bound: usize| {
            let old: Vec<Rc<str>> = old.iter().map(|&k| Rc::from(k)).collect();
            page_after_leading_run(
                &old,
                bound,
                |k| now.iter().position(|&n| n == k),
                |i| now.get(i).copied(),
            )
        };
        let old = ["l", "s", "a", "b", "c"];
        assert!(after(&old, &["l", "s", "x", "y", "a", "b", "c"], 4));
        assert!(after(&old, &["x", "l", "s", "a", "b", "c"], 4));
        assert!(after(&old, &["l", "s", "a", "x", "b", "c"], 4));
        // Rows after the window's last row, none, a replaced row, a reorder.
        assert!(!after(&old, &["l", "s", "a", "b", "x", "c"], 3));
        assert!(!after(&old, &old, 4));
        assert!(!after(&old, &["l", "s", "x", "b", "c"], 4));
        assert!(!after(&old, &["l", "s", "b", "a", "c"], 4));
    }

    /// What answers a `reachstart`: keys by identity, rows as the JS
    /// target's `eq`, NaN payloads included (LLP 1168.000 D1.2).
    #[test]
    fn rows_answer_by_key_or_by_value_as_eq() {
        let keys = |ks: &[&str]| -> Vec<Rc<str>> { ks.iter().map(|&k| Rc::from(k)).collect() };
        let row = |x: f64| Value::List(vec![Value::from("a"), Value::Number(x)].into());
        let (ab, ba) = (keys(&["a", "b"]), keys(&["b", "a"]));
        let answers = |was: f64, now: f64| rows_answered(&ab, &ab, &[row(was)], &[row(now)]);
        assert!(!answers(1.0, 1.0), "a fresh equal row");
        assert!(!answers(f64::NAN, f64::NAN), "NaN is itself");
        assert!(!answers(f64::NAN, -f64::NAN), "every NaN is equal");
        let payload = |bits: u64| f64::from_bits(0x7ff8_0000_0000_0000 | bits);
        assert!(!answers(payload(1), payload(2)), "whatever its payload");
        assert!(answers(0.0, -0.0), "signed zero apart");
        assert!(answers(1.0, 2.0));
        assert!(rows_answered(&ab, &ba, &[], &[]), "a key moved");
    }

    #[test]
    fn a_margin_is_one_or_two_nonnegative_lengths() {
        assert_eq!(Margin::parse("100%"), Some(Margin::default()));
        assert_eq!(Margin::parse("200%").unwrap().resolve(300.0), [600.0; 2]);
        assert_eq!(
            Margin::parse(" 0PX  50% ").unwrap().resolve(300.0),
            [0.0, 150.0]
        );
        assert_eq!(Margin::parse("1.5e2px").unwrap().resolve(10.0), [150.0; 2]);
        let margin = Margin::parse("10px 50%").unwrap();
        assert_eq!(margin.band(300.0, 0.0), [10.0, 150.0]);
        assert_eq!(margin.band(300.0, -400.0), [110.0, 150.0]);
        assert_eq!(margin.band(300.0, 4000.0), [10.0, 750.0], "two ports");
        for bad in [
            "",
            "10",
            "10em",
            "-1px",
            "1px 2px 3px",
            "auto",
            "NaN%",
            "infpx",
        ] {
            assert_eq!(Margin::parse(bad), None, "{bad}");
        }
    }
}
