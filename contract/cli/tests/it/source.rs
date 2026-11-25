//! The suite's one source type. Rust compiles the whole runner again for each
//! source type a binary boots (`docs/testing.md`, "Structure and isolation"),
//! so a test passes `boxed(its_source)` to `Runner::boot` and
//! `contract::bake`, never the source itself, and reads it back with
//! [`Read::source`].

use exact_runner::{DataError, DataSource, Runner, Value};
use std::any::Any;

/// A test's data source, whatever its type.
pub type Source = Box<dyn Typed>;

/// A source that can be read back as its own type.
pub trait Typed: DataSource + Any {}
impl<T: DataSource + Any> Typed for T {}

/// `source`, boxed for a runner.
pub fn boxed(source: impl Typed) -> Source {
    Box::new(source)
}

/// A runner's source, as the type the test booted it with.
pub trait Read {
    fn source<T: Any>(&mut self) -> &mut T;
    fn source_ref<T: Any>(&self) -> &T;
}

impl Read for Runner<Source> {
    fn source<T: Any>(&mut self) -> &mut T {
        let source: &mut dyn Any = &mut **self.data();
        source
            .downcast_mut()
            .expect("the runner's source is another type")
    }

    fn source_ref<T: Any>(&self) -> &T {
        let source: &dyn Any = &**self.data_ref();
        source
            .downcast_ref()
            .expect("the runner's source is another type")
    }
}

/// No app data: every source is unknown.
pub struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// Measure every dialog as a host does once it is open (`Kernel::measure_dialog`): a dialog no host
/// measures or presents has no layout box, as the web's closed `dialog` has none.
pub fn measure_dialogs(r: &mut exact_runner::Runner<Source>) {
    let rows = r.kernel().rows(None).unwrap_or_default();
    for row in rows {
        r.kernel_mut().measure_dialog(row.id, true);
    }
}
