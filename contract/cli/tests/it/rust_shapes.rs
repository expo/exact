//! `contract rust` (LLP 1054.000 R4): a struct per shape, its fields in
//! declaration order, converting to and from the runner's positional record.

const CONTRACT: &str = r#"
shape Author
  handle: string
  displayName: string

shape Post
  text: string
  type: string
  likeCount: number
  liked: bool
  author: Author
  tags: string[]
  reply: option<Author>

component App
  resource posts = feed() as shape Post[]
  view
    text `${posts.length}`
"#;

#[test]
fn a_struct_per_shape_in_declaration_order() {
    let rust = contract::rust(&contract::compile(CONTRACT).unwrap(), &Default::default()).unwrap();
    let post = &rust[rust.find("pub struct Post {").unwrap()..];
    let post = &post[..post.find('}').unwrap()];
    let fields: Vec<&str> = post.lines().skip(1).map(str::trim).collect();
    assert_eq!(
        fields,
        [
            "pub text: ::std::string::String,",
            "pub r#type: ::std::string::String,",
            "pub like_count: ::core::primitive::f64,",
            "pub liked: ::core::primitive::bool,",
            "pub author: Author,",
            "pub tags: ::std::vec::Vec<::std::string::String>,",
            "pub reply: ::core::option::Option<Author>,",
        ]
    );
    assert!(rust.contains(
        "pub struct Author {\n    pub handle: ::std::string::String,\n    pub display_name: ::std::string::String,\n}"
    ));
    // The record is written in that order, and read back field by field.
    let to = &rust[rust.find("impl ContractValue for Post").unwrap()..];
    let order: Vec<usize> = [
        "self.text",
        "self.r#type",
        "self.like_count",
        "self.liked",
        "self.author",
        "self.tags",
        "self.reply",
    ]
    .iter()
    .map(|f| to.find(f).unwrap())
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    assert!(to.contains("like_count: ContractValue::from_value(&fields[2])?,"));
}

#[test]
fn a_shape_may_take_a_name_rust_predefines() {
    // Each struct shadows the prelude's name in the module it is included
    // into, so the generated code spells Rust's own by its path.
    let source = "shape Option\n  values: string[]\n  result: option<Result>\nshape Result\n  all: Vec[]\nshape Vec\n  first: option<String>\nshape String\n  count: number\nshape From\n  flag: bool\nshape str\n  x: number\ncomponent App\n  state row = Option(values=[], result=none)\n  view\n    text \"x\"\n";
    let rust = contract::rust(&contract::compile(source).unwrap(), &Default::default()).unwrap();
    for name in ["Option", "Result", "Vec", "String", "From", "str"] {
        assert!(rust.contains(&format!("pub struct {name} {{")), "{name}");
    }
    assert!(rust.contains(
        "pub struct Option {\n    pub values: ::std::vec::Vec<::std::string::String>,\n    pub result: ::core::option::Option<Result>,\n}"
    ));
    assert!(rust.contains("pub all: ::std::vec::Vec<Vec>,"));
    assert!(rust.contains("pub first: ::core::option::Option<String>,"));
    // No struct here is generic and none is a trait: a bare generic or
    // trait use of these names would mean the shape, and not compile.
    for bare in [
        "Option<",
        "Result<",
        "Vec<",
        "From<",
        "TryFrom<",
        "Sized",
        "&'static str",
    ] {
        let unqualified = rust
            .match_indices(bare)
            .filter(|(at, _)| !rust[..*at].ends_with(|c: char| c == ':' || c.is_alphanumeric()))
            .count();
        assert_eq!(unqualified, 0, "bare `{bare}` in:\n{rust}");
    }
    let source = "shape ContractValue\n  x: number\ncomponent App\n  state row = ContractValue(x=1)\n  view\n    text \"x\"\n";
    let error =
        contract::rust(&contract::compile(source).unwrap(), &Default::default()).unwrap_err();
    assert!(
        error.contains("ContractValue") && error.contains("rename"),
        "{error}"
    );
}

#[test]
fn rust_names_escape_keywords_and_refuse_colliding_fields() {
    let source = "shape type\n  value: number\ncomponent App\n  state row = type(value=1)\n  view\n    text \"x\"\n";
    let rust = contract::rust(&contract::compile(source).unwrap(), &Default::default()).unwrap();
    assert!(rust.contains("pub struct r#type {"));
    assert!(rust.contains("impl ContractValue for r#type {"));
    let source = "shape Row\n  a-b: number\n  a_b: number\ncomponent App\n  state row = Row(a-b=1, a_b=2)\n  view\n    text \"x\"\n";
    let error =
        contract::rust(&contract::compile(source).unwrap(), &Default::default()).unwrap_err();
    assert!(
        error.contains("a-b") && error.contains("a_b") && error.contains("Row"),
        "{error}"
    );
}

#[test]
fn both_rust_printers_refuse_type_names_that_collide_in_one_scope() {
    use exact_plan::{builder::PlanBuilder, TypesId};
    let source = "shape A-B\n  x: number\nshape A_B\n  y: string\ncomponent App\n  state one = A-B(x=1)\n  state two = A_B(y=\"two\")\n  view\n    text \"x\"\n";
    let plan = contract::compile(source).unwrap();
    let check = |result: Result<String, String>| {
        let error = result.unwrap_err();
        assert!(error.contains("A-B") && error.contains("A_B"), "{error}");
    };
    check(contract::rust(&plan, &Default::default()));
    for separate in [false, true] {
        let mut b = PlanBuilder::from_plan(plan.clone());
        for (name, extension) in [
            ("A-B", "one"),
            ("A_B", if separate { "two" } else { "one" }),
        ] {
            let ty = TypesId(
                plan.types
                    .iter()
                    .position(|t| plan.str(t.name) == name)
                    .unwrap() as u32,
            );
            let source = b.source(name, &[], ty);
            b.module(
                source,
                (extension, extension),
                name,
                false,
                false,
                "never",
                "",
                "{}",
            );
        }
        let binding = contract::module_rust(&b.finish().unwrap());
        if separate {
            assert_eq!(binding.unwrap().matches("pub struct A_B {").count(), 2);
        } else {
            check(binding);
        }
    }
}

#[test]
fn both_rust_printers_use_camel_case_for_scoped_shape_names() {
    use exact_plan::{builder::PlanBuilder, TypesId};

    let source = "shape Point__geometry_file_2\n  x: number\nshape Rect__geometry_file_2\n  points: option<Point__geometry_file_2>[]\ncomponent App\n  resource rect = load() as shape Rect__geometry_file_2\n  view\n    text \"x\"\n";
    let plan = contract::compile(source).unwrap();
    let mut b = PlanBuilder::from_plan(plan.clone());
    let ty = TypesId(
        plan.types
            .iter()
            .position(|t| plan.str(t.name) == "Rect__geometry_file_2")
            .unwrap() as u32,
    );
    let source = b.source("bounds", &[], ty);
    b.module(
        source,
        ("geometry", "@fixture/geometry"),
        "bounds",
        false,
        false,
        "never",
        "",
        "{}",
    );
    for rust in [
        contract::rust(&plan, &Default::default()).unwrap(),
        contract::module_rust(&b.finish().unwrap()).unwrap(),
    ] {
        for expected in [
            "pub struct PointGeometryFile2 {",
            "pub struct RectGeometryFile2 {",
            "pub points: ::std::vec::Vec<::core::option::Option<PointGeometryFile2>>",
        ] {
            assert!(rust.contains(expected), "{expected}\n{rust}");
        }
    }
    let source = "shape Point__geometry\n  x: number\nshape PointGeometry\n  y: number\ncomponent App\n  view\n    text \"x\"\n";
    let error =
        contract::rust(&contract::compile(source).unwrap(), &Default::default()).unwrap_err();
    assert!(
        error.contains("Point__geometry") && error.contains("PointGeometry"),
        "{error}"
    );
}
