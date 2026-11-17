//! A tool call's arguments against the bound action's parameter types (LLP
//! 1111 D4 step 1): stricter than decoding, so that a caller's mistake is
//! named rather than guessed at. Unknown fields, missing fields and
//! non-finite numbers are refused; `null` is `none` for an `option<T>`,
//! which is still written. The first failing path is reported. And MCP's
//! tool list for a door, from the plan (LLP 1111.000 D2).

use exact_js_value::Shape;
use exact_plan::{Plan, TypeKind, TypesId};
use serde_json::{json, Map, Value as Json};

/// Whether `json` is a value of `shape`, else the first failing path and
/// why, as `page: a number, not "120"`.
pub fn check(json: &Json, shape: &Shape, path: &str) -> Result<(), String> {
    let at = |why: String| {
        Err(match path.is_empty() {
            true => why,
            false => format!("{path}: {why}"),
        })
    };
    match (shape, json) {
        (Shape::Option(_), Json::Null) => Ok(()),
        (Shape::Option(inner), _) => check(json, inner, path),
        (Shape::Number, Json::Number(n)) if n.as_f64().is_some_and(f64::is_finite) => Ok(()),
        (Shape::Bool, Json::Bool(_)) | (Shape::String, Json::String(_)) => Ok(()),
        (Shape::Unit, Json::Null) => Ok(()),
        (Shape::List(inner), Json::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                check(item, inner, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        (Shape::Record(fields), Json::Object(given)) => {
            if let Some(extra) = given.keys().find(|k| !fields.iter().any(|(f, _)| f == *k)) {
                return at(format!("no field `{extra}`"));
            }
            for (name, field) in fields {
                let inner = match path.is_empty() {
                    true => name.clone(),
                    false => format!("{path}.{name}"),
                };
                match given.get(name) {
                    Some(value) => check(value, field, &inner)?,
                    None => return Err(format!("{inner}: missing")),
                }
            }
            Ok(())
        }
        _ => at(format!("{}, not {json}", describe(shape))),
    }
}

/// `shape` as a caller would read it.
fn describe(shape: &Shape) -> &'static str {
    match shape {
        Shape::Number => "a finite number",
        Shape::Bool => "true or false",
        Shape::String => "a string",
        Shape::Unit => "null",
        Shape::Option(_) => "a value or null",
        Shape::List(_) => "a list",
        Shape::Record(_) => "an object",
    }
}

/// Whether tool `tool`'s reach sends (LLP 1111 D3): its action, with the
/// calls of other actions the plan holds inline (LLP 1089), has a `send`.
/// The rest of the reach, a mutation's `then`, is reached only through a
/// send. A `reads` tool that sends is not read-only, since what a source
/// does with a send is the module's.
pub fn sends(plan: &Plan, tool: usize) -> bool {
    let body = plan.code(plan.actions[plan.tools[tool].action.0 as usize].body);
    crate::vm::instructions(plan.read_code(body).0)
        .flatten()
        .any(|i| i.op == exact_plan::Opcode::Send)
}

/// MCP's `Tool` objects (MCP 2025-11-25, server/tools) for the tools whose
/// face names `door`, in the plan's order (LLP 1111.000 D2). The schemas
/// are those `host/web-js/calls.js` registers with WebMCP: `inputSchema`
/// the action's parameters with the face's words, `outputSchema` the call's
/// envelope. The annotations are LLP 1111 D3's table. Every faced tool is
/// listed; a running app leaves out one whose `available when` is false.
pub fn mcp_tools(plan: &Plan, door: &str) -> Vec<Json> {
    let mut listed = Vec::new();
    for (i, t) in plan.tools.iter().enumerate() {
        let face: Json = serde_json::from_str(plan.str(t.face)).unwrap_or_default();
        let named = face["doors"].as_array();
        if !named.is_some_and(|d| d.iter().any(|x| x == door)) {
            continue;
        }
        let action = plan.action(t.action);
        let mut properties = Map::new();
        let mut required = Vec::new();
        for p in action.params.iter() {
            let p = plan.param(p);
            let name = plan.str(p.name);
            let words = &face["parameters"][name];
            properties.insert(name.into(), schema(plan, p.ty, words));
            required.push(json!(name));
        }
        let input = json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        });
        let returns = t.returns.map(|d| plan.derives[d.0 as usize].ty);
        let consequence = plan.str(t.consequence);
        let read_only = consequence == "reads" && !sends(plan, i);
        let mut annotations = json!({ "readOnlyHint": read_only, "openWorldHint": !t.closed });
        if !read_only {
            let destroys = consequence == "destroys" || (consequence == "writes" && !t.additive);
            annotations["destructiveHint"] = json!(destroys);
            annotations["idempotentHint"] = json!(t.idempotent);
        }
        let name = plan.str(t.name);
        listed.push(json!({
            "name": name,
            "title": face["title"].as_str().unwrap_or(name),
            "description": plan.str(t.description),
            "inputSchema": input,
            "outputSchema": envelope(plan, returns),
            "annotations": annotations,
        }));
    }
    listed
}

/// The JSON Schema of plan type `ty`, with a face's `title`, `description`
/// and `type: integer` (`words`, null for none), as calls.js's `schema`.
fn schema(plan: &Plan, ty: TypesId, words: &Json) -> Json {
    let row = &plan.types[ty.0 as usize];
    let fields = || {
        let start = row.fields.start as usize;
        &plan.fields[start..start + row.fields.len as usize]
    };
    let elem = || schema(plan, row.elem.expect("an element type"), &Json::Null);
    let mut out = match row.kind {
        TypeKind::Number if words["type"] == "integer" => json!({ "type": "integer" }),
        TypeKind::Number => json!({ "type": "number" }),
        TypeKind::Bool => json!({ "type": "boolean" }),
        TypeKind::String if !row.fields.is_empty() => {
            let cases: Vec<&str> = fields().iter().map(|f| plan.str(f.name)).collect();
            json!({ "type": "string", "enum": cases })
        }
        TypeKind::String => json!({ "type": "string" }),
        TypeKind::Unit => json!({ "type": "null" }),
        TypeKind::Option => json!({ "anyOf": [elem(), { "type": "null" }] }),
        TypeKind::List => json!({ "type": "array", "items": elem() }),
        TypeKind::Record => {
            let properties: Map<String, Json> = (fields().iter())
                .map(|f| (plan.str(f.name).into(), schema(plan, f.ty, &Json::Null)))
                .collect();
            let required: Vec<&str> = fields().iter().map(|f| plan.str(f.name)).collect();
            json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
        }
    };
    for word in ["title", "description"] {
        if let Some(text) = words[word].as_str().filter(|t| !t.is_empty()) {
            out[word] = json!(text);
        }
    }
    out
}

/// The `outputSchema` of a call's envelope (LLP 1111 D4), its `value` typed
/// from `returns`, as calls.js's `output`.
fn envelope(plan: &Plan, returns: Option<TypesId>) -> Json {
    let words = |description: &str| json!({ "type": "string", "description": description });
    let mut outcome = words("ok: it ran and its work settled. failed: the app reports that it failed. refused: it did not run, or it stopped because it needs a person. timeout: its work had not settled 10 s after it started.");
    outcome["enum"] = json!([
        "ok",
        "failed",
        "superseded",
        "timeout",
        "interrupted",
        "refused"
    ]);
    let mut certainty = words("none: the action did not run. applied: it ran and its work settled. uncertain: it ran, and its effect may or may not have landed; read before calling again.");
    certainty["enum"] = json!(["none", "applied", "uncertain"]);
    let value = match returns {
        Some(ty) => json!({
            "anyOf": [schema(plan, ty, &Json::Null), { "type": "null" }],
            "description": "What the tool returns; null when it was refused.",
        }),
        None => json!({ "type": "null" }),
    };
    let properties = json!({
        "outcome": outcome,
        "certainty": certainty,
        "reason": words("Why it was refused or failed, as one word (unavailable, arguments, busy, cancelled, …); empty when there is none."),
        "stale": { "type": "boolean", "description": "Something else changed what it reports while it ran." },
        "skipped": { "type": "array", "items": { "type": "string" }, "description": "The commands it did not run because a call has no person: focus, scrolling, haptics." },
        "value": value,
        "message": words("A sentence for the agent, empty when there is nothing to add."),
    });
    json!({
        "type": "object",
        "properties": properties,
        "required": ["outcome", "certainty", "reason", "stale", "skipped", "value", "message"],
        "additionalProperties": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_call_is_refused_at_its_first_bad_path() {
        let shape = Shape::Record(vec![
            ("id".into(), Shape::String),
            ("page".into(), Shape::Number),
            ("note".into(), Shape::Option(Box::new(Shape::String))),
        ]);
        assert!(check(&json!({"id": "a", "page": 3, "note": null}), &shape, "").is_ok());
        assert_eq!(
            check(&json!({"id": "a", "note": null}), &shape, ""),
            Err("page: missing".into())
        );
        assert_eq!(
            check(&json!({"id": "a", "page": 3}), &shape, ""),
            Err("note: missing".into()),
            "an option is still written"
        );
        assert_eq!(
            check(&json!({"id": "a", "page": "3", "note": null}), &shape, ""),
            Err("page: a finite number, not \"3\"".into())
        );
        assert_eq!(
            check(
                &json!({"id": "a", "page": 3, "note": null, "x": 1}),
                &shape,
                ""
            ),
            Err("no field `x`".into())
        );
        let list = Shape::List(Box::new(Shape::Bool));
        assert_eq!(
            check(&json!([true, 1]), &list, "flags"),
            Err("flags[1]: true or false, not 1".into())
        );
    }

    /// A plan compiled from `source` with `intents` as its app.json's.
    fn faced(name: &str, source: &str, intents: Json) -> Plan {
        let dir = std::env::temp_dir().join(format!("exact-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest =
            json!({ "app": { "id": "com.exact.mcp", "name": "Mcp" }, "intents": intents });
        std::fs::write(dir.join("app.json"), manifest.to_string()).unwrap();
        std::fs::write(dir.join("app.contract"), source).unwrap();
        let plan = contract::compile_path(&dir.join("app.contract"));
        std::fs::remove_dir_all(&dir).unwrap();
        plan.unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// The envelope's `outputSchema`, its value `value`, as
    /// `host/web-js/webmcp.test.mjs` reads calls.js's.
    fn envelope_with(value: Json) -> Json {
        let words = |d: &str| json!({ "type": "string", "description": d });
        let mut outcome = words("ok: it ran and its work settled. failed: the app reports that it failed. refused: it did not run, or it stopped because it needs a person. timeout: its work had not settled 10 s after it started.");
        outcome["enum"] = json!([
            "ok",
            "failed",
            "superseded",
            "timeout",
            "interrupted",
            "refused"
        ]);
        let mut certainty = words("none: the action did not run. applied: it ran and its work settled. uncertain: it ran, and its effect may or may not have landed; read before calling again.");
        certainty["enum"] = json!(["none", "applied", "uncertain"]);
        json!({
            "type": "object",
            "properties": {
                "outcome": outcome,
                "certainty": certainty,
                "reason": words("Why it was refused or failed, as one word (unavailable, arguments, busy, cancelled, …); empty when there is none."),
                "stale": { "type": "boolean", "description": "Something else changed what it reports while it ran." },
                "skipped": { "type": "array", "items": { "type": "string" }, "description": "The commands it did not run because a call has no person: focus, scrolling, haptics." },
                "value": value,
                "message": words("A sentence for the agent, empty when there is nothing to add."),
            },
            "required": ["outcome", "certainty", "reason", "stale", "skipped", "value", "message"],
            "additionalProperties": false,
        })
    }

    /// An mcp face keeps the plan's types (LLP 1111.000 D1): an enum is its
    /// cases, an option a value or null, a record its fields, each required
    /// and no others; the face's words and `integer` are the parameter's.
    #[test]
    fn the_mcp_list_types_every_parameter_and_the_envelope() {
        let source = r#"shape Item
  name: string
  aisle: enum<"produce", "dairy">

component App
  state items: Item[] = []
  state last = ""
  tool add writes additive closed "Add an item." = add
    returns items
  tool look reads "The last note." = look
    returns last
  tool clear destroys idempotent "Remove every item." = clear
  tool hidden writes "Not on this door." = hide
  action add(item: Item, note: option<string>, count: number, aisle: enum<"produce", "dairy">)
    items = items.concat([item])
  action look
    focus("n")
  action clear
    items = []
  action hide
    last = "x"
  view
    text testId="n" `${items.length}`
"#;
        let intents = json!({ "tools": {
            "add": { "title": "Add Item", "doors": ["mcp"], "parameters": {
                "count": { "title": "Count", "description": "How many.", "type": "integer" },
                "note": { "title": "Note" } } },
            "look": { "title": "Look", "doors": ["mcp", "apple"] },
            "clear": { "title": "Clear", "doors": ["mcp"] },
            "hidden": { "title": "Hidden", "doors": ["apple"] },
        } });
        let plan = faced("types", source, intents);
        let listed = mcp_tools(&plan, "mcp");
        let names: Vec<&str> = listed.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["add", "look", "clear"]);
        let enumerated = json!({ "type": "string", "enum": ["produce", "dairy"] });
        let item = json!({
            "type": "object",
            "properties": { "name": { "type": "string" }, "aisle": enumerated },
            "required": ["name", "aisle"],
            "additionalProperties": false,
        });
        assert_eq!(
            listed[0],
            json!({
                "name": "add",
                "title": "Add Item",
                "description": "Add an item.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "item": item,
                        "note": { "anyOf": [{ "type": "string" }, { "type": "null" }], "title": "Note" },
                        "count": { "type": "integer", "title": "Count", "description": "How many." },
                        "aisle": enumerated,
                    },
                    "required": ["item", "note", "count", "aisle"],
                    "additionalProperties": false,
                },
                "outputSchema": envelope_with(json!({
                    "anyOf": [{ "type": "array", "items": item }, { "type": "null" }],
                    "description": "What the tool returns; null when it was refused.",
                })),
                "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
            })
        );
        assert_eq!(listed[1]["inputSchema"]["required"], json!([]));
        assert_eq!(
            listed[1]["annotations"],
            json!({ "readOnlyHint": true, "openWorldHint": true }),
            "a reads with no send"
        );
        assert_eq!(
            listed[2]["outputSchema"],
            envelope_with(json!({ "type": "null" }))
        );
        assert_eq!(
            listed[2]["annotations"],
            json!({ "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": true })
        );
    }

    /// Shelf's tools, which its face offers on the default doors: the web
    /// door lists both, as WebMCP registers them, and the mcp door none.
    #[test]
    fn shelfs_tools_are_listed_for_the_doors_its_faces_name() {
        let shelf =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../apps/shelf/app.contract");
        let plan = contract::compile_path(&shelf).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(mcp_tools(&plan, "mcp"), Vec::<Json>::new());
        let web = mcp_tools(&plan, "web");
        let log = &web[0];
        assert_eq!(log["name"], "logPages");
        assert_eq!(log["title"], "Log Pages");
        assert_eq!(
            log["inputSchema"]["properties"],
            json!({
                "id": { "type": "string", "title": "Book" },
                "page": { "type": "integer", "title": "Page", "description": "The page you have read up to." },
            })
        );
        assert_eq!(
            log["outputSchema"],
            envelope_with(json!({ "type": "null" }))
        );
        assert_eq!(
            log["annotations"],
            json!({ "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false }),
            "a write that is not additive may overwrite"
        );
        assert_eq!(web[1]["name"], "addToShelf");
        assert_eq!(web[1]["annotations"]["destructiveHint"], false);
        assert_eq!(
            web[1]["inputSchema"]["required"],
            json!(["title", "author", "pages"])
        );
    }
}
