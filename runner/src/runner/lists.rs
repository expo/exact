//! Logical text and key lookups over virtualized lists. LLP 1010 §6.
use super::*;

impl<D: DataSource> Runner<D> {
    /// Nearest unmounted row snap area in a collection, along its axis, using
    /// its measured sizes or current estimates. At most 64 local rows are
    /// evaluated; no row is mounted. `geometry`: the snapport start's
    /// projected rest, the snapport's length, the least and greatest rests,
    /// and the distance of the nearest mounted target (infinity when there is
    /// none), which bounds the search.
    pub fn collection_snap(&self, view: ViewId, geometry: [f64; 5]) -> Option<f64> {
        let [proposed, port, minimum, maximum, distance] = geometry;
        if self.poisoned
            || ![proposed, port, minimum, maximum]
                .into_iter()
                .all(f64::is_finite)
            || port <= 0.0
            || maximum < minimum
            || distance.is_nan()
            || distance <= 0.0
        {
            return None;
        }
        let mut ids = Ids::default();
        let mut u = Update::new(self.env(&[], &[]), &self.sites, &mut ids);
        u.fonts = Some(self.fonts.clone());
        self.tree
            .as_ref()?
            .collection_snap(&mut u, &self.kernel, view, geometry)
            .ok()
            .flatten()
    }

    /// The positions each unmounted row root with a snap alignment along a
    /// collection's axis rests at, first to last, in
    /// [`Runner::collection_snap`]'s coordinates: of the rows `window` covers,
    /// at most `cap` up to and including the one at `around` and `cap` after
    /// it. `snapport`: its length and the least and greatest positions of its
    /// start. No row is mounted.
    pub fn collection_snap_areas(
        &self,
        view: ViewId,
        (around, window, cap): (f64, [f64; 2], usize),
        snapport: [f64; 3],
    ) -> Vec<(f64, f64, bool)> {
        let [port, minimum, maximum] = snapport;
        if self.poisoned
            || ![around, window[0], window[1], port, minimum, maximum]
                .into_iter()
                .all(f64::is_finite)
            || port <= 0.0
            || maximum < minimum
        {
            return Vec::new();
        }
        let mut ids = Ids::default();
        let mut u = Update::new(self.env(&[], &[]), &self.sites, &mut ids);
        u.fonts = Some(self.fonts.clone());
        self.tree
            .as_ref()
            .and_then(|tree| {
                tree.collection_snap_areas(
                    &mut u,
                    &self.kernel,
                    view,
                    (around, window, cap),
                    snapport,
                )
                .ok()
            })
            .unwrap_or_default()
    }

    /// The first position past `release` toward `rest` an unmounted row
    /// root with `scroll-snap-stop: always` holds a snap at, in
    /// [`Runner::collection_snap`]'s coordinates; `snapport`: its length and
    /// the least and greatest positions of its start. At most 256 rows past
    /// the release are read; no row is mounted.
    pub fn collection_snap_stop(
        &self,
        view: ViewId,
        [release, rest]: [f64; 2],
        snapport: [f64; 3],
    ) -> Option<f64> {
        let [port, minimum, maximum] = snapport;
        if self.poisoned
            || ![release, rest, port, minimum, maximum]
                .into_iter()
                .all(f64::is_finite)
            || port <= 0.0
            || maximum < minimum
        {
            return None;
        }
        let mut ids = Ids::default();
        let mut u = Update::new(self.env(&[], &[]), &self.sites, &mut ids);
        u.fonts = Some(self.fonts.clone());
        self.tree
            .as_ref()?
            .collection_snap_stop(
                &mut u,
                &self.kernel,
                view,
                [release, rest, port, minimum, maximum],
            )
            .ok()
            .flatten()
    }

    /// Resolve an opaque row key without materializing that row.
    pub fn list_index(&self, view: ViewId, key: &str) -> Option<usize> {
        self.tree.as_ref()?.list_index(view, key)
    }

    /// Copy all logical text, or the range between two stable endpoints.
    /// At most one unmounted row is realized at a time; its operations are
    /// never committed. No data query, clock advance, or native view is created.
    pub fn list_text(
        &self,
        view: ViewId,
        range: Option<(ListTextPosition<'_>, ListTextPosition<'_>)>,
        policy: ListTextPolicy,
    ) -> Result<String, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let mut ids = Ids::default();
        let mut u = Update::new(self.env(&[], &[]), &self.sites, &mut ids);
        u.fonts = Some(self.fonts.clone());
        Ok(self
            .tree
            .as_ref()
            .expect("booted")
            .list_text(&mut u, view, range, policy)?)
    }
}
