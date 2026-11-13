//! The gallery's Rust capture source exercises the same export as TypeScript.
use exact_plan::Value;
pub use exact_runner::Placement;
use exact_runner::{Answer, DataError, DataSource, Outcome, Request, Store};

pub const RUST_SOURCES: &[&str] = &["captureRust"];
#[derive(Default)]
pub struct Capture;
pub type Data<J> = exact_data::Mixed<J, exact_data::Placed<Capture>>;
pub fn mixed<J: DataSource>(javascript: J, placement: Placement) -> Data<J> {
    exact_data::Mixed::new(
        javascript,
        exact_data::Placed::new(Capture, placement),
        &["capture"],
        RUST_SOURCES,
    )
    .expect("distinct sources of the gallery")
    .with_embedded_rust(|p| Ok(exact_data::Placed::new(Capture, p.given_placement())))
}
impl DataSource for Capture {
    fn app_id(&self) -> &str {
        "com.exact.canvasgallery"
    }
    fn grants(&self) -> &str {
        "fs.write app:/tmp"
    }
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, name: &str, args: &[Value]) -> Result<Answer, DataError> {
        if name != "captureRust" {
            return Err(DataError::UnknownSource(name.into()));
        }
        let [surface, path] = args else {
            return Err(DataError::BadArguments("surface and path".into()));
        };
        Ok(Answer::Later(Request::capture_canvas(
            surface.as_str().unwrap_or(""),
            path.as_str().unwrap_or(""),
        )))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let (record, path) = match exact_runner::canvas_capture::captured_image(&outcome) {
            Ok(c) => (
                format!("{} {}x{} @{}", c.path, c.width, c.height, c.scale),
                c.path,
            ),
            Err(e) => (e, String::new()),
        };
        Ok(Answer::Now(Value::record(vec![
            Value::str(&record),
            Value::str(&path),
        ])))
    }
}
