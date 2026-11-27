//! Taffy uses two coordinate systems to refer to grid lines (the gaps/gutters between rows/columns):
use super::super::types::TrackCounts;
use crate::geometry::Line;
use core::cmp::{Ordering, max};
use core::ops::{Add, AddAssign, Sub};

/// The maximum number of tracks in each direction from the start of the explicit grid (line 0 in
/// OriginZero coordinates), and the maximum span of a single grid item. This limits the grid to
/// `2 * MAX_GRID_TRACKS` total tracks in each axis (including explicit tracks, which count against
/// the positive limit). Grids larger than this limit are clamped.
///
/// See: <https://www.w3.org/TR/css-grid-1/#overlarge-grids>
pub(crate) const MAX_GRID_TRACKS: u16 = 10_000;

/// The lowest valid grid line in OriginZero coordinates
pub(crate) const MIN_OZ_LINE: i16 = -(MAX_GRID_TRACKS as i16);

/// The highest valid grid line in OriginZero coordinates
pub(crate) const MAX_OZ_LINE: i16 = MAX_GRID_TRACKS as i16;

/// EXACT PATCH 70: the lines a grid places its items between, in OriginZero coordinates. They are
/// `[MIN_OZ_LINE, MAX_OZ_LINE]` unless the container raises its highest line
/// (`GridContainerStyle::grid_line_limit`); the lowest then rises as far as it must for the grid
/// to have at most `i16::MAX` tracks, which keeps every track index an `i16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineLimit {
    /// The lowest line
    pub min: i16,
    /// The highest line, which is also the largest span
    pub max: i16,
}

impl LineLimit {
    /// The limit of a grid whose container does not raise it
    pub(crate) const DEFAULT: Self = Self { min: MIN_OZ_LINE, max: MAX_OZ_LINE };

    /// The limit of a grid whose container raises its highest line to `max_line`, at most
    /// `i16::MAX`; a value of `MAX_GRID_TRACKS` or less keeps the default
    pub(crate) fn with_max_line(max_line: u16) -> Self {
        if max_line <= MAX_GRID_TRACKS {
            return Self::DEFAULT;
        }
        let max = max_line.min(i16::MAX as u16) as i16;
        Self { min: MIN_OZ_LINE.max(max - i16::MAX), max }
    }

    /// The largest number of tracks an item spans
    pub(crate) fn max_span(self) -> u16 {
        self.max as u16
    }
}

/// Represents a grid line position in "CSS Grid Line" coordinates
///
/// "CSS Grid Line" coordinates are those used in grid-row/grid-column in the CSS grid spec:
///   - The line at left hand (or top) edge of the explicit grid is line 1
///     (and counts up from there)
///   - The line at the right hand (or bottom) edge of the explicit grid is -1
///     (and counts down from there)
///   - 0 is not a valid index
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(transparent)]
pub struct GridLine(i16);

impl From<i16> for GridLine {
    fn from(value: i16) -> Self {
        Self(value)
    }
}

impl GridLine {
    /// Returns the underlying i16
    pub fn as_i16(self) -> i16 {
        self.0
    }

    /// Convert into OriginZero coordinates using the specified explicit track count.
    ///
    /// The result is clamped into the limited grid, `limit`'s lines
    pub(crate) fn into_origin_zero_line(self, explicit_track_count: u16, limit: LineLimit) -> OriginZeroLine {
        let explicit_line_count = explicit_track_count as i32 + 1;
        let oz_line = match self.0.cmp(&0) {
            Ordering::Greater => self.0 as i32 - 1,
            Ordering::Less => self.0 as i32 + explicit_line_count,
            Ordering::Equal => panic!("Grid line of zero is invalid"),
        };
        OriginZeroLine(oz_line.clamp(limit.min as i32, limit.max as i32) as i16)
    }
}

/// Represents a grid line position in "OriginZero" coordinates
///
/// "OriginZero" coordinates are a normalized form:
///   - The line at left hand (or top) edge of the explicit grid is line 0
///   - The next line to the right (or down) is 1, and so on
///   - The next line to the left (or up) is -1, and so on
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct OriginZeroLine(pub i16);

// Add and Sub with Self
impl Add<OriginZeroLine> for OriginZeroLine {
    type Output = Self;
    fn add(self, rhs: OriginZeroLine) -> Self::Output {
        OriginZeroLine(self.0 + rhs.0)
    }
}
impl Sub<OriginZeroLine> for OriginZeroLine {
    type Output = Self;
    fn sub(self, rhs: OriginZeroLine) -> Self::Output {
        OriginZeroLine(self.0 - rhs.0)
    }
}

// Add and Sub with u16. EXACT PATCH 70: saturating, as a line and a span each reach `i16::MAX`
// in a grid whose limit is raised; the sum is then clamped into the limited grid.
impl Add<u16> for OriginZeroLine {
    type Output = Self;
    fn add(self, rhs: u16) -> Self::Output {
        OriginZeroLine(self.0.saturating_add_unsigned(rhs))
    }
}
impl AddAssign<u16> for OriginZeroLine {
    fn add_assign(&mut self, rhs: u16) {
        self.0 = self.0.saturating_add_unsigned(rhs);
    }
}
impl Sub<u16> for OriginZeroLine {
    type Output = Self;
    fn sub(self, rhs: u16) -> Self::Output {
        OriginZeroLine(self.0.saturating_sub_unsigned(rhs))
    }
}

impl OriginZeroLine {
    /// Create an `OriginZeroLine`, clamping the line into the limited grid `[-MAX_GRID_TRACKS, MAX_GRID_TRACKS]`
    pub(crate) fn clamped(line: i32) -> Self {
        OriginZeroLine(line.clamp(MIN_OZ_LINE as i32, MAX_OZ_LINE as i32) as i16)
    }

    /// Convert into "CSS Grid Line" coordinates using the specified explicit track count.
    ///
    /// The inverse of [`GridLine::into_origin_zero_line`]
    pub(crate) fn into_grid_line(self, explicit_track_count: u16) -> GridLine {
        let explicit_line_count = explicit_track_count + 1;
        if self.0 >= 0 { GridLine(self.0 + 1) } else { GridLine(self.0 - explicit_line_count as i16) }
    }

    /// Converts a grid line in OriginZero coordinates into the index of that same grid line in the GridTrackVec.
    pub(crate) fn into_track_vec_index(self, track_counts: TrackCounts) -> usize {
        self.try_into_track_vec_index(track_counts).unwrap_or_else(|| {
            if self.0 > 0 {
                panic!("OriginZero grid line cannot be more than the number of positive grid lines");
            } else {
                panic!("OriginZero grid line cannot be less than the number of negative grid lines");
            }
        })
    }

    /// Converts a grid line in OriginZero coordinates into the index of that same grid line in the GridTrackVec.
    ///
    /// This fallible version is used for the placement of absolutely positioned grid items:
    ///
    ///    If a grid-placement property refers to a non-existent line either by explicitly specifying such a line or by
    ///    spanning outside of the existing implicit grid, it is instead treated as specifying auto (instead of creating
    ///    new implicit grid lines).
    ///
    /// The infallible version above if used when placing regular in-flow grid items.
    pub(crate) fn try_into_track_vec_index(self, track_counts: TrackCounts) -> Option<usize> {
        // OriginZero grid line cannot be less than the number of negative grid lines
        if self.0 < -(track_counts.negative_implicit as i16) {
            return None;
        };
        // OriginZero grid line cannot be more than the number of positive grid lines
        if self.0 > (track_counts.explicit + track_counts.positive_implicit) as i16 {
            return None;
        };

        Some(2 * ((self.0 + track_counts.negative_implicit as i16) as usize))
    }

    /// The minimum number of negative implicit track there must be if a grid item starts at this line.
    pub(crate) fn implied_negative_implicit_tracks(self) -> u16 {
        if self.0 < 0 { self.0.unsigned_abs() } else { 0 }
    }

    /// The minimum number of positive implicit track there must be if a grid item end at this line.
    pub(crate) fn implied_positive_implicit_tracks(self, explicit_track_count: u16) -> u16 {
        if self.0 > explicit_track_count as i16 { self.0 as u16 - explicit_track_count } else { 0 }
    }
}

impl Line<OriginZeroLine> {
    /// The number of tracks between the start and end lines
    #[inline(always)]
    pub(crate) fn span(self) -> u16 {
        max(self.end.0 - self.start.0, 0) as u16
    }
}

/// A trait for the different coordinates used to define grid lines.
pub trait GridCoordinate: Copy {}
impl GridCoordinate for GridLine {}
impl GridCoordinate for OriginZeroLine {}
