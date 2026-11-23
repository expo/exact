//! The Live Activity packaging fixture's data source: it answers nothing.

#![deny(missing_docs)]

use exact_plan::Value;
use exact_runner::{DataError, DataSource};

/// The app's data source.
#[derive(Default, Clone, Copy)]
pub struct ActivityFixture;

impl DataSource for ActivityFixture {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    // Android shows the departure as a notification (LLP 1126 D13).
    fn grants(&self) -> &str {
        "device.notifications purpose.notifications"
    }
}
