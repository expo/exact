//! A widget's entity parameters (LLP 1126 D2, LLP 1133.023 D2): admission,
//! the `option<string>` field, and the refresh's argument.
use super::tests::source;
use super::*;
use crate::Manifest;
use exact_plan::Value;
use serde_json::json;

fn manifest(parameters: serde_json::Value, entities: serde_json::Value) -> Manifest {
    Manifest {
        json: json!({"intents":{"entities":entities},
            "widgets":[{"kind":"card","families":["systemSmall","accessoryRectangular"],"component":"Card","props":"Props",
            "refresh":"refreshCard","preview":{"title":"Preview"},
            "configuration":{"title":"Card","parameters":parameters}}]}),
        id: "test.app".into(),
        name: "Test".into(),
        declared: true,
    }
}

#[test]
fn an_entity_parameter_names_a_declared_kind_and_takes_no_values() {
    let books = json!({"book":{"title":"Book"}});
    let book = json!({"name":"book","title":"Favourite","type":"entity","entity":"book"});
    let widget = manifest(json!([book]), books.clone()).widgets().unwrap();
    assert_eq!(
        widget[0].configuration.as_ref().unwrap().json()["parameters"][0],
        json!({"name":"book","type":"entity","title":"Favourite","entity":"book","entityTitle":"Book"})
    );
    let with = |key: &str, value: serde_json::Value| {
        let mut row = book.clone();
        row[key] = value;
        row
    };
    let without_kind = json!({"name":"book","title":"Favourite","type":"entity"});
    let fixed = json!({"name":"rows","title":"Rows","type":"enum","entity":"book","values":[{"value":"a","title":"A"}]});
    for (row, entities, edit) in [
        (
            with("values", json!([{"value":"a","title":"A"}])),
            books.clone(),
            "parameters[0].values belongs to `type: \"enum\"`; remove it",
        ),
        (
            with("default", json!("a")),
            books.clone(),
            "parameters[0].default belongs to `type: \"enum\"`; remove it",
        ),
        (
            without_kind,
            books.clone(),
            "parameters[0].entity is required: a kind that `intents.entities` declares",
        ),
        (
            with("entity", json!(7)),
            books.clone(),
            "parameters[0].entity is required",
        ),
        (
            with("entity", json!("author")),
            books.clone(),
            "names `author`, which `intents.entities` does not declare; add it there with its title, as \"author\": {\"title\": …}",
        ),
        (
            book.clone(),
            json!({"book":{"title":"Book\nShelf"}}),
            "intents.entities.book.title must be a non-empty string without control characters",
        ),
        (
            fixed,
            books.clone(),
            "parameters[0].entity belongs to `type: \"entity\"`; remove it",
        ),
    ] {
        let error = manifest(json!([row]), entities).widgets().unwrap_err();
        assert!(error.contains(edit), "{row}: {error}");
    }

    // The watch offers no entity choices, so they add no recommendations.
    let values = (0..8)
        .map(|i| json!({"value":i.to_string(),"title":i.to_string()}))
        .collect::<Vec<_>>();
    let fixed = |name: &str| json!({"name":name,"type":"enum","title":name,"values":values});
    let full = manifest(json!([fixed("rows"), fixed("detail"), book]), books);
    assert!(full.widgets().is_ok(), "64 fixed combinations still fit");
}

#[test]
fn an_entity_field_is_an_optional_id_that_the_refresh_receives() {
    use exact_runner::{DataError, DataSource};
    let rows = json!({"name":"rows","type":"enum","title":"Rows","values":[{"value":"two","title":"2"},{"value":"one","title":"1"}]});
    let book = json!({"name":"book","title":"Book","type":"entity","entity":"book"});
    let widget = manifest(json!([rows, book]), json!({"book":{"title":"Book"}}))
        .widgets()
        .unwrap()
        .remove(0);
    let inject = "  inject\n    snapshot_configuration: Configuration\n";
    let src = source(
        "    text (match snapshot_configuration.book { case some(id) => id, case none => title })",
        "shape Configuration\n  book: option<string>\n  rows: string",
        inject,
    );
    let plan = compile(&src, &widget).unwrap();
    struct Asked(Vec<Value>);
    impl DataSource for Asked {
        fn query(&mut self, _: &str, args: &[Value]) -> Result<Value, DataError> {
            self.0 = args.to_vec();
            Err(DataError::Unavailable("checked by declarations".into()))
        }
    }
    let declaration = Declaration::Widget(widget.clone());
    let entries =
        std::collections::BTreeMap::from([("card".to_owned(), Entry { declaration, plan })]);
    let mut asked = Asked(vec![]);
    check_refreshes(&entries, &mut asked).unwrap();
    let unchosen = Value::record(vec![Value::Option(None), Value::str("two")]);
    assert_eq!(
        asked.0,
        [Value::Number(0.0), unchosen],
        "the bake asks with nothing chosen"
    );
    let app = crate::compile(&src).unwrap();
    let declarations = crate::typescript(&source_plan(&entries, &app).unwrap()).unwrap();
    let written =
        r#"refreshCard: (args: [number, { book: string | null; rows: "two" | "one" }]) =>"#;
    assert!(declarations.contains(written), "{declarations}");

    for field in ["string", "option<number>", "enum<\"book\">"] {
        let shape = format!("shape Configuration\n  rows: string\n  book: {field}");
        let errors = compile(&source("    text title", &shape, inject), &widget).unwrap_err();
        let edit = "declare `Configuration.book` as `option<string>`: the id of the `book` entry the person chose";
        assert!(
            errors
                .iter()
                .any(|e| e.id == "snapshot-configuration" && e.message.contains(edit)),
            "{field}: {errors:?}"
        );
    }
}
