//! A list's `gap` and row estimate, read again as they change (LLP 1070, as
//! amended for #226). A row list's `column-gap` is CSS's space between its
//! items: the index counts it after each row, so a row's extent is its size
//! and the gap after it, and the rows end one gap before their extents'
//! sum. A vertical list is a block, where CSS lays out no gap.
use super::*;

/// What a list's options say of its sizes: its row estimate and its gap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sizing {
    pub(crate) estimate: f64,
    pub(crate) gap: f64,
}

impl Sizing {
    /// From the values bound to the list's estimate and to its main-axis
    /// gap, as the list is created: an estimate that is not a positive
    /// number refuses the list; a gap that is not a nonnegative number is
    /// none.
    pub(crate) fn read(
        axis: ListAxis,
        estimate: Option<Value>,
        gap: Option<Value>,
    ) -> Result<Self, InstanceError> {
        let estimate = match estimate {
            None => ESTIMATED_HEIGHT,
            Some(Value::Number(n)) if n.is_finite() && n > 0.0 => n,
            Some(Value::Number(_)) => {
                return Err(invalid("estimated item height must be positive and finite"))
            }
            Some(_) => return Err(invalid("estimated item height must be a number")),
        };
        Ok(Self {
            estimate,
            gap: Self::gap(axis, gap.as_ref()),
        })
    }

    fn gap(axis: ListAxis, value: Option<&Value>) -> f64 {
        match value {
            Some(Value::Number(n)) if axis == ListAxis::Horizontal && n.is_finite() && *n > 0.0 => {
                *n
            }
            _ => 0.0,
        }
    }

    /// A row's extent before it is measured: the estimate and the gap after it.
    pub(crate) fn extent(self) -> f64 {
        self.estimate + self.gap
    }
}

impl Collection {
    /// The gap after a row in its extent: the list's, but none after a
    /// mounted row whose root is not displayed, which CSS lays no gap around.
    pub(super) fn gap_after(&self, row: Option<&Mounted>) -> f64 {
        if row.is_some_and(|r| r.undisplayed) {
            0.0
        } else {
            self.sizing.gap
        }
    }

    /// A mounted row's wrapper follows its root's `display: none` in a list
    /// with a gap (CSS Flexbox 1 §4: an item that is not displayed is no
    /// flex item, and no gap is laid out around it). The root's value is the
    /// one its binding last emitted.
    pub(super) fn sync_undisplayed(&mut self, u: &mut Update<'_>) -> Result<(), InstanceError> {
        let gapped = self.sizing.gap > 0.0;
        for row in &mut self.mounted {
            let undisplayed = gapped
                && matches!(&row.row.roots[..], [Child::Node(root)]
                    if root.bound_style(u.env.plan, u.sites, exact_kernel::StyleId::Display).as_ref().and_then(Value::as_str) == Some("none"));
            if row.undisplayed != undisplayed {
                row.undisplayed = undisplayed;
                let display = if undisplayed { "none" } else { "flex" };
                views::style(u, row.wrapper, &[("display", Value::str(display))])?;
            }
        }
        Ok(())
    }

    /// The estimate or the gap changed (a computed one): `estimate`, the
    /// value bound to the estimate now, and `gap`, to the main-axis gap. A
    /// new estimate is what every row the list has not mounted takes, its
    /// measurement taken under the old one dropped, as an eager list lays
    /// every row out again; a mounted row keeps its size until the host
    /// measures it again. A new gap moves every row after the first by it.
    /// The list keeps what shows. An estimate that is not a positive number
    /// changes nothing.
    pub(super) fn resize(
        &mut self,
        u: &mut Update<'_>,
        frames: &[Frame],
        estimate: Option<&Value>,
        gap: Option<&Value>,
    ) -> Result<(), InstanceError> {
        let estimate = match estimate {
            Some(&Value::Number(n)) if n.is_finite() && n > 0.0 => n,
            _ => self.sizing.estimate,
        };
        let next = Sizing {
            estimate,
            gap: Sizing::gap(self.axis, gap),
        };
        if next == self.sizing {
            return Ok(());
        }
        self.end_preview(u)?;
        let anchor = self.anchor()?;
        let kept: Vec<usize> = if next.estimate == self.sizing.estimate {
            (0..self.index.len()).collect()
        } else {
            self.mounted.iter().map(|m| m.position).collect()
        };
        self.index
            .resize(next.extent(), next.gap, &kept)
            .map_err(index_error)?;
        self.sizing = next;
        self.estimated_height = next.extent();
        self.zero_heights.retain(|key| {
            self.index
                .position(key)
                .is_some_and(|p| kept.binary_search(&p).is_ok())
        });
        self.restore_unless_unknown(anchor)?;
        self.realize_window(u, frames, false, CollectionFill::default())?;
        advance(&mut self.revision)?;
        Ok(())
    }
}
