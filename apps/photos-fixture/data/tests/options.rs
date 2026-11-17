//! The Options field's text reaches `editImage` unchanged: anything but an
//! object (or nothing) is refused before a storage request, as malformed
//! JSON is, so the image-codec drive can go on.

use exact_plan::Value;
use exact_runner::{Answer, DataError, DataSource, Store};
use photos_fixture_data::Fixture;

fn edit(options: &str) -> Result<Answer, DataError> {
    let args = [
        Value::str("app:/tmp/input.jpg"),
        Value::str("transform"),
        Value::str(options),
    ];
    Fixture::default().answer(&mut Store::new("", []), "editImage", &args)
}

#[test]
fn options_that_are_not_an_object_are_refused_before_any_request() {
    for options in ["", "{}", "null", r#"{"resizeWidth": 32}"#] {
        assert!(matches!(edit(options), Ok(Answer::Later(_))), "{options}");
    }
    for options in ["[]", "123", r#""text""#, "true", "invalid"] {
        assert!(
            matches!(edit(options), Err(DataError::Unavailable(_))),
            "{options}"
        );
    }
}
