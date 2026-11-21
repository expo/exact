//! The first window after a plan replacement (LLP 1029.000.002 D5).
use super::{
    nest::{ReadingPoint, ReadingPosition},
    *,
};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::instance) struct ListReading {
    axis: ListAxis,
    anchor: index::Anchor,
    measured: Vec<(Rc<str>, f64)>,
    fixed: [f64; 2],
}

impl Collection {
    pub(super) fn capture_reading(&self) -> Option<ListReading> {
        let mut anchor = if let Some(g) = &self.geometry {
            self.index
                .capture_anchor(
                    self.anchor_offset(g.offset, g.port_main),
                    g.port_main,
                    self.follows(),
                )
                .ok()?
        } else if let Some(position) = &self.restored_at {
            match &position.point {
                ReadingPoint::Row(key) => self.index.anchor_at(key, position.within)?,
                ReadingPoint::Fixed(side) => self.index.fixed_anchor(*side, position.within),
            }
        } else {
            // A second replacement can arrive before the first port report.
            let port = self
                .declared_port
                .unwrap_or(self.estimated_height * self.bootstrap_rows as f64);
            self.index
                .capture_anchor(
                    self.anchor_offset(self.start_offset, port),
                    port,
                    self.follows(),
                )
                .ok()?
        };
        anchor.keep_window(self.mounted.iter().map(|row| row.position).collect());
        let measured = self
            .mounted
            .iter()
            .filter_map(|row| {
                let key = self.index.shared_key(row.position)?;
                if !self.index.is_measured(key) {
                    return None;
                }
                Some((key.clone(), self.index.height(row.position)?))
            })
            .collect();
        Some(ListReading {
            axis: self.axis,
            anchor,
            measured,
            fixed: [self.index.fixed_size(0), self.index.fixed_size(1)],
        })
    }

    pub(super) fn seed_reading(&mut self, seed: &ListReading) -> Result<(), InstanceError> {
        if self.axis == seed.axis {
            for (key, size) in &seed.measured {
                self.index.seed_height(key, *size).map_err(index_error)?;
            }
        }
        let sizes = if self.axis == seed.axis && self.fixed.is_some() {
            seed.fixed
        } else {
            [0.0; 2]
        };
        for (side, size) in sizes.into_iter().enumerate() {
            self.index.measure_fixed(side, size).map_err(index_error)?;
        }
        if SizeIndex::follows_end(&seed.anchor) {
            self.at_end = true;
            return Ok(());
        }
        let point = if let Some(row) = self.index.surviving_row(&seed.anchor) {
            self.index
                .shared_key(row)
                .cloned()
                .zip(self.index.prefix(row))
                .map(|(key, start)| (ReadingPoint::Row(key), start))
        } else {
            seed.anchor
                .fixed_position()
                .filter(|_| self.fixed.is_some())
                .map(|side| {
                    (
                        ReadingPoint::Fixed(side),
                        if side == 0 {
                            0.0
                        } else {
                            sizes[0] + self.index.data_extent()
                        },
                    )
                })
        };
        let (point, start, within) = point
            .map_or((ReadingPoint::Fixed(0), 0.0, 0.0), |(point, start)| {
                (point, start, seed.anchor.within())
            });
        self.start_reading(
            ReadingPosition {
                point,
                start,
                within,
                sizes,
            },
            start + within,
        );
        Ok(())
    }
}
