//! The page's geometry: the document's insets and segments, and its layout
//! for the geometry facts (LLP 1039 D9, LLP 1123 D6).

use super::Bridge;
use exact_runner::DataSource;

impl<D: DataSource> Bridge<D> {
    /// After the page laid out a commit's batch: the length of the batch
    /// that answers the geometry facts it moved, 0 when it moved none.
    pub fn geometry_settled(&mut self) -> u32 {
        match self.host.as_mut().and_then(|h| h.geometry_settled()) {
            Some(out) => self.emit(out),
            None => 0,
        }
    }
}

/// The page's geometry exports, invoked inside [`crate::host!`].
#[macro_export]
macro_rules! geometry_exports {
    () => {
        /// The browser's document insets and segment rectangles in the input buffer.
        #[no_mangle]
        pub extern "C" fn exact_set_viewport_geometry(len: u32) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().viewport_geometry(len as usize))
        }

        /// The page laid out a commit's batch (LLP 1123 D6); returns the
        /// length of the batch answering the geometry facts it moved, 0 for
        /// none.
        #[no_mangle]
        pub extern "C" fn exact_geometry_settled() -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().geometry_settled())
        }
    };
}
