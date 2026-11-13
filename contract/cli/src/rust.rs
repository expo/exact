//! Rust's side of the data seam, derived from the plan's types as
//! `typescript` derives TypeScript's (LLP 1054.000 R4): a data crate builds
//! its records from these structs, so a field's position is the compiler's,
//! not a comment's. Two same-typed fields swapped in a hand-written
//! `Value::record` pass the runner's shape check; a named field cannot swap.

use exact_plan::{Plan, TypeKind, TypesId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "box", "break", "const", "continue", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "static", "struct", "trait", "true", "try", "type", "unsafe", "use",
    "where", "while", "yield", "abstract", "become", "do", "final", "macro", "override", "priv",
    "typeof", "unsized", "virtual", "union",
];

/// `camelCase` or `kebab-case` as a snake_case field name, escaped where it
/// is a Rust keyword.
pub(crate) fn field_name(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    match out.as_str() {
        "self" | "super" | "crate" | "Self" => format!("{out}_"),
        k if KEYWORDS.contains(&k) => format!("r#{out}"),
        _ if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) || out == "_" => {
            format!("_{out}_")
        }
        _ => out,
    }
}

pub(crate) fn type_name(name: &str) -> String {
    // The loader scopes shapes with `__<file>`; module calls use the same
    // separator for their options and platform records.
    if name.contains("__") {
        let name: String = name
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|part| !part.is_empty())
            .map(|part| part[..1].to_ascii_uppercase() + &part[1..])
            .collect();
        return type_name(&name);
    }
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) || out == "_" {
        out = format!("_{out}_");
    }
    match out.as_str() {
        "self" | "Self" | "super" | "crate" => format!("{out}_"),
        k if KEYWORDS.contains(&k) => format!("r#{out}"),
        _ => out,
    }
}

pub(crate) fn check_fields<'a>(
    shape: &str,
    names: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let mut mapped = BTreeMap::new();
    for name in names {
        let rust = field_name(name);
        if let Some(previous) = mapped.insert(rust.clone(), name) {
            return Err(format!(
                "shape `{shape}`: fields `{previous}` and `{name}` both become Rust `{rust}`"
            ));
        }
    }
    Ok(())
}

/// Compile the data seam's plan and its generated call-record names. The names
/// stay beside the plan: its module signatures do not distinguish authored
/// option types from generated platform-word records with the same spelling.
/// Like `contract rust`, this does not check GPU surface arguments.
pub fn compile_path_for_rust(
    path: &Path,
) -> Result<(Plan, BTreeSet<String>), Vec<crate::CompileError>> {
    let src = crate::read_source(path)?;
    let app_root = crate::app_root(path)?;
    let (file, sources) = crate::sources::load(path, &src, &app_root)?;
    let call_records = file
        .modules
        .iter()
        .flat_map(crate::modules::records)
        .map(|shape| shape.name)
        .collect();
    let (plan, _) = crate::compile_loaded(
        path,
        &app_root,
        file,
        sources,
        false,
        false,
        contract_lower::Profile::Web,
        None,
    )?;
    Ok((plan, call_records))
}

/// Generate Rust structs for authored Contract shapes, data source signatures,
/// and the records their fields reach. Each has `From<&T> for Value` and
/// `TryFrom<&Value> for T` in the declared field order.
/// A build-time artifact: a data crate's `build.rs`
/// writes it into `OUT_DIR` and `include!`s it; it is never committed.
/// Pass the call-record names from [`compile_path_for_rust`], or an empty set
/// for a plan from [`crate::compile`], which cannot import native modules.
pub fn rust(plan: &Plan, call_records: &BTreeSet<String>) -> Result<String, String> {
    plan.redecode()
        .map_err(|e| format!("invalid plan: {e:?}"))?;
    // One struct per shape name; the plan may hold a shape more than once.
    let mut named = BTreeMap::new();
    for (i, row) in plan.types.iter().enumerate() {
        let name = plan.str(row.name);
        if row.kind == TypeKind::Record && !name.is_empty() && !call_records.contains(name) {
            reach(plan, TypesId(i as u32), &mut named)?;
        }
    }
    for row in &plan.sources {
        let name = plan.str(row.name);
        if exact_plan::runner_owned_source(name)
            || plan.module_named(name).is_some()
            || plan.fetch_named(name).is_some()
        {
            continue;
        }
        reach(plan, row.ty, &mut named)?;
        for param in row.params.iter() {
            reach(plan, plan.source_params[param.0 as usize].ty, &mut named)?;
        }
    }
    if let Some(ty) = named.get("ContractValue") {
        let name = plan.str(plan.types[ty.0 as usize].name);
        return Err(format!(
            "shape `{name}` takes the name of the generated trait `ContractValue`; rename the shape"
        ));
    }
    let ty = |i: TypesId| -> Result<String, String> { rust_type(plan, i) };
    // A shape may be named `Option`, `String`, `From` or `f64`, and its
    // struct then shadows that type or trait name in the module it is
    // included into, so every type and trait the generated code names from
    // Rust is spelled by its path. Values (`Some`, `None`), macros and
    // methods stay bare: a struct with braces names no value or macro, and
    // a prelude trait's methods stay in scope when its name is shadowed.
    let mut out = String::from(
        "// @generated by `contract rust` from Contract shapes; do not edit.\n\
         // Declaration order is the record's field order.\n\n\
         /// A value the data seam carries, with its conversion both ways.\n\
         pub trait ContractValue: ::core::marker::Sized {\n    \
             fn to_value(&self) -> ::exact_runner::Value;\n    \
             fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self>;\n\
         }\n\
         impl ContractValue for ::core::primitive::f64 {\n    \
             fn to_value(&self) -> ::exact_runner::Value { ::exact_runner::Value::Number(*self) }\n    \
             fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self> { value.as_number() }\n\
         }\n\
         impl ContractValue for ::core::primitive::bool {\n    \
             fn to_value(&self) -> ::exact_runner::Value { ::exact_runner::Value::Bool(*self) }\n    \
             fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self> { value.as_bool() }\n\
         }\n\
         impl ContractValue for ::std::string::String {\n    \
             fn to_value(&self) -> ::exact_runner::Value { ::exact_runner::Value::str(self) }\n    \
             fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self> { value.as_str().map(::std::string::ToString::to_string) }\n\
         }\n\
         impl ContractValue for () {\n    \
             fn to_value(&self) -> ::exact_runner::Value { ::exact_runner::Value::Unit }\n    \
             fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self> { matches!(value, ::exact_runner::Value::Unit).then_some(()) }\n\
         }\n\
         impl<T: ContractValue> ContractValue for ::core::option::Option<T> {\n    \
             fn to_value(&self) -> ::exact_runner::Value {\n        \
                 self.as_ref().map_or(::exact_runner::Value::Option(None), |v| ::exact_runner::Value::some(v.to_value()))\n    \
             }\n    \
             fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self> {\n        \
                 match value {\n            \
                     ::exact_runner::Value::Option(None) => Some(None),\n            \
                     ::exact_runner::Value::Option(Some(v)) => T::from_value(v).map(Some),\n            \
                     _ => None,\n        \
                 }\n    \
             }\n\
         }\n\
         impl<T: ContractValue> ContractValue for ::std::vec::Vec<T> {\n    \
             fn to_value(&self) -> ::exact_runner::Value { ::exact_runner::Value::list(self.iter().map(T::to_value).collect()) }\n    \
             fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self> {\n        \
                 match value {\n            \
                     ::exact_runner::Value::List(items) => items.iter().map(T::from_value).collect(),\n            \
                     _ => None,\n        \
                 }\n    \
             }\n\
         }\n",
    );
    for (name, &i) in &named {
        let row = &plan.types[i.0 as usize];
        let shape = plan.str(row.name);
        let fields = &plan.fields
            [row.fields.start as usize..row.fields.start as usize + row.fields.len as usize];
        check_fields(shape, fields.iter().map(|f| plan.str(f.name)))?;
        writeln!(out, "\n/// Contract `shape {shape}`.").unwrap();
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq, Default)]\npub struct {name} {{"
        )
        .unwrap();
        for f in fields {
            writeln!(
                out,
                "    pub {}: {},",
                field_name(plan.str(f.name)),
                ty(f.ty)?
            )
            .unwrap();
        }
        out.push_str("}\n");
        writeln!(out, "impl ContractValue for {name} {{").unwrap();
        out.push_str("    fn to_value(&self) -> ::exact_runner::Value {\n        ::exact_runner::Value::Record([\n");
        for f in fields {
            writeln!(
                out,
                "            self.{}.to_value(),",
                field_name(plan.str(f.name))
            )
            .unwrap();
        }
        out.push_str("        ].into())\n    }\n");
        out.push_str(
            "    fn from_value(value: &::exact_runner::Value) -> ::core::option::Option<Self> {\n",
        );
        writeln!(
            out,
            "        let ::exact_runner::Value::Record(fields) = value else {{ return None }};\n        \
             if fields.len() != {} {{ return None; }}\n        Some(Self {{",
            fields.len()
        )
        .unwrap();
        for (k, f) in fields.iter().enumerate() {
            writeln!(
                out,
                "            {}: ContractValue::from_value(&fields[{k}])?,",
                field_name(plan.str(f.name))
            )
            .unwrap();
        }
        out.push_str("        })\n    }\n}\n");
        writeln!(
            out,
            "impl ::core::convert::From<&{name}> for ::exact_runner::Value {{\n    \
                 fn from(v: &{name}) -> Self {{ v.to_value() }}\n\
             }}\n\
             impl ::core::convert::TryFrom<&::exact_runner::Value> for {name} {{\n    \
                 type Error = &'static ::core::primitive::str;\n    \
                 fn try_from(v: &::exact_runner::Value) -> ::core::result::Result<Self, Self::Error> {{\n        \
                     Self::from_value(v).ok_or(\"not a {name}\")\n    \
                 }}\n\
             }}"
        )
        .unwrap();
    }
    Ok(out)
}

/// Every record `ty` holds, by its Rust name.
pub(crate) fn reach(
    plan: &Plan,
    ty: TypesId,
    out: &mut BTreeMap<String, TypesId>,
) -> Result<(), String> {
    let row = plan.types.get(ty.0 as usize).ok_or("a type out of range")?;
    match row.kind {
        TypeKind::Option | TypeKind::List => {
            reach(plan, row.elem.ok_or("a type with no element")?, out)
        }
        TypeKind::Record => {
            let name = plan.str(row.name);
            let rust = type_name(name);
            if let Some(previous) = out.get(&rust) {
                let previous = plan.str(plan.types[previous.0 as usize].name);
                return if previous == name {
                    Ok(())
                } else {
                    Err(format!(
                        "shapes `{previous}` and `{name}` both become Rust `{rust}`"
                    ))
                };
            }
            out.insert(rust, ty);
            for f in row.fields.iter().map(|id| &plan.fields[id.0 as usize]) {
                reach(plan, f.ty, out)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn rust_type(plan: &Plan, i: TypesId) -> Result<String, String> {
    let row = plan
        .types
        .get(i.0 as usize)
        .ok_or_else(|| format!("type {} out of range", i.0))?;
    let elem = || {
        row.elem
            .ok_or_else(|| format!("type {} has no element", i.0))
    };
    Ok(match row.kind {
        TypeKind::Number => "::core::primitive::f64".into(),
        TypeKind::Bool => "::core::primitive::bool".into(),
        TypeKind::String => "::std::string::String".into(),
        TypeKind::Unit => "()".into(),
        TypeKind::Option => format!("::core::option::Option<{}>", rust_type(plan, elem()?)?),
        TypeKind::List => format!("::std::vec::Vec<{}>", rust_type(plan, elem()?)?),
        TypeKind::Record => {
            let name = plan.str(row.name);
            if name.is_empty() {
                return Err(format!("type {} is a record with no shape name", i.0));
            }
            type_name(name)
        }
    })
}
