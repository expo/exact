use super::{paired, Fixture, SOURCE};
use exact_js_bake::{bake, Baked, Producer, Tools};
use exact_kernel::Kernel;
use exact_runner::{
    exact_canvas::Context2d, Answer, DataSource, DrawRequest, Drawn, Runner, Store, Value,
};
use serde_json::Value as Json;

fn model_fixture() -> Fixture {
    let f = Fixture::new();
    let package =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/language-model");
    let installed = f.0.join("node_modules/@exact/language-model");
    for sub in ["", "android", "apple", "web"] {
        std::fs::create_dir_all(installed.join(sub)).unwrap();
        std::fs::copy(
            package.join(sub).join("exact.json"),
            installed.join(sub).join("exact.json"),
        )
        .unwrap();
    }
    std::fs::copy(package.join("package.json"), installed.join("package.json")).unwrap();
    for name in ["index.js", "schema.js"] {
        std::fs::copy(
            package.join("web").join(name),
            installed.join("web").join(name),
        )
        .unwrap();
    }
    f.write(
        "app.json",
        r#"{"name":"Model","app":{"id":"test.exact.logic","name":"Model"},"rust":false}"#,
    );
    f
}

const MODEL_SOURCE: &str = "export const appId = 'test.exact.logic'; export const grants = ''; export const overlay = undefined; export const answer = () => { throw new Error('unknown source'); };\n";

#[test]
fn literal_language_model_tools_require_compatible_exported_handlers() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    f.write(
        "app.contract",
        r#"use createLanguageModel, LanguageModelTool, LanguageModelInfo from "@exact/language-model"
shape Query
  query: string
  count: option<number>
  tags: string[]
component App
  mutation made as shape LanguageModelInfo
  action make
    send made = createLanguageModel(tools=[LanguageModelTool(name="findNotes", description="Find notes", inputSchema="Query")])
  view
    text "Ready"
"#,
    );
    let mut producer = Producer::new(Tools::default()).unwrap();
    for handler in [
        "",
        "export const tools = { another: () => '' };",
        "export const tools = { findNotes: 42 };",
        "export const tools = { findNotes: ({query}: {query: number}) => query };",
        "export const tools = { findNotes: ({count}: {count: number}) => count };",
        "export const tools = { findNotes: ({tags}: {tags: string}) => tags };",
        "export const tools = { findNotes: ([query]: [string]) => query };",
    ] {
        f.write("app.ts", &format!("{MODEL_SOURCE}{handler}\n"));
        for result in [bake(&f.0, &Tools::default()), producer.bake(&f.0, None)] {
            let error = result
                .err()
                .expect("a missing or incompatible handler must fail at bake");
            assert!(
                error.contains("language-model-tool-handler"),
                "{handler}: {error}"
            );
        }
    }
    f.write("handlers.ts", "import type {LanguageModelTools} from './app.contract.d.ts';\nexport const tools: LanguageModelTools = { findNotes: async ({query, count, tags}, _store, _storage) => ({query, count, tags}) };\n");
    f.write(
        "app.ts",
        &format!("{MODEL_SOURCE}export * from './handlers';\n"),
    );
    for result in [bake(&f.0, &Tools::default()), producer.bake(&f.0, None)] {
        let baked = result.unwrap_or_else(|e| panic!("{e}"));
        assert!(baked.declarations.contains("findNotes"));
    }
    super::web_build(&f.0).unwrap_or_else(|e| panic!("{e}"));
    f.write("handlers.ts", "export const tools: Record<string, (args: {query: string; count?: number; tags: string[]}) => unknown> = { findNotes: ({query}) => query };\n");
    producer.bake(&f.0, None).unwrap_or_else(|e| panic!("{e}"));
    f.write("app.ts", MODEL_SOURCE);
    let error = super::web_build(&f.0).expect_err("the web bake enforces the same handler rule");
    assert!(error.contains("language-model-tool-handler"), "{error}");
}

#[test]
fn data_code_literal_tools_are_checked_and_computed_declarations_keep_the_runtime_path() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    f.write(
        "app.contract",
        "component App\n  view\n    text \"Ready\"\n",
    );
    let calls = r#"import {createLanguageModel as create} from '@exact/language-model';
import * as model from '@exact/language-model';
const FIND = {name:'findNotes', description:'Find notes', inputSchema:{type:'object', $defs:{Flag:{type:'boolean'}}, properties:{query:{type:'string'}, tone:{type:'string', enum:['plain','warm']}, flags:{type:'array',items:{$ref:'#/$defs/Flag'}}}, required:['query','flags']}};
export const make = () => create({tools:[FIND]});
export const other = () => model.createLanguageModel({tools:[{name:'another', description:'Other', inputSchema:{type:'object',properties:{id:{type:'number'}},required:['id']}}]});
"#;
    let mut producer = Producer::new(Tools::default()).unwrap();
    for handlers in [
        "export const tools = {findNotes: ({query}: {query:number}) => query, another: () => ''};",
        "export const tools = {findNotes: () => ''};",
        "export const tools = {findNotes: ({tone}: {tone:'plain'}) => tone, another: () => ''};",
        "export const tools = {findNotes: ({flags}: {flags:string[]}) => flags, another: () => ''};",
    ] {
        f.write("app.ts", &format!("{MODEL_SOURCE}{calls}{handlers}\n"));
        for result in [bake(&f.0, &Tools::default()), producer.bake(&f.0, None)] {
            let error = result
                .err()
                .expect("data code's literal tool needs a compatible handler");
            assert!(error.contains("language-model-tool-handler"), "{error}");
        }
    }
    let valid = "export const tools = {findNotes: ({query, tone, flags}: {query:string; tone?: 'plain'|'warm'; flags:boolean[]}) => ({query,tone,flags}), another: ({id}: {id:number}) => id};";
    f.write("app.ts", &format!("{MODEL_SOURCE}{calls}{valid}\n"));
    producer.bake(&f.0, None).unwrap_or_else(|e| panic!("{e}"));
    f.bake();
    super::web_build(&f.0).unwrap_or_else(|e| panic!("{e}"));
    f.write("app.ts", &format!("{MODEL_SOURCE}{calls}\n"));
    let error = super::web_build(&f.0).expect_err("web data code's tools are checked");
    assert!(error.contains("language-model-tool-handler"), "{error}");

    f.write("app.ts", &format!(r#"{MODEL_SOURCE}
import {{createLanguageModel}} from '@exact/language-model';
export const computed = (tools: NonNullable<Parameters<typeof createLanguageModel>[0]>['tools']) => createLanguageModel({{tools}});
export const named = (name: string) => createLanguageModel({{tools:[{{name, description:'Runtime', inputSchema:{{type:'object'}}}}]}});
export const shadowed = (createLanguageModel: (args: unknown) => unknown) => createLanguageModel({{tools:[{{name:'notAModelTool'}}]}});
"#));
    producer.bake(&f.0, None).unwrap_or_else(|e| panic!("{e}"));
    f.bake();
}

#[test]
fn mutated_language_model_tool_declarations_keep_the_runtime_path() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    f.write(
        "app.contract",
        "component App\n  view\n    text \"Ready\"\n",
    );
    let mut producer = Producer::new(Tools::default()).unwrap();
    for usage in [
        "definition.name = 'live';",
        "const alias = definition;\nalias.name = 'live';",
        "const options = {tools:[definition]};\n\
         const rename = (options: {tools: {name:string}[]}) => {\n\
           options.tools[0].name = 'live';\n\
         };\n\
         rename(options);",
        "export {definition};",
    ] {
        f.write(
            "app.ts",
            &format!(
                "{MODEL_SOURCE}\n\
                 import {{createLanguageModel}} from '@exact/language-model';\n\
                 const definition = {{name:'draft', description:'Tool', inputSchema:{{type:'object'}}}};\n\
                 {usage}\n\
                 export const make = () => createLanguageModel({{tools:[definition]}});\n\
                 export const tools = {{live: () => ''}};\n"
            ),
        );
        producer
            .bake(&f.0, None)
            .unwrap_or_else(|e| panic!("{usage}: {e}"));
    }
    f.bake();
    super::web_build(&f.0).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn unstructured_language_model_schemas_still_check_the_handler_argument() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    f.write(
        "app.contract",
        "component App\n  view\n    text \"Ready\"\n",
    );
    let mut producer = Producer::new(Tools::default()).unwrap();
    for kind in ["object", "array"] {
        let calls = format!(
            "import {{createLanguageModel}} from '@exact/language-model';\n\
             export const make = () => createLanguageModel({{tools:[{{\n\
               name:'inspect', description:'Inspect', inputSchema:{{type:'{kind}'}}\n\
             }}]}});\n"
        );
        f.write(
            "app.ts",
            &format!(
                "{MODEL_SOURCE}{calls}export const tools = {{inspect: (args: number) => args.toFixed()}};\n"
            ),
        );
        let error = producer
            .bake(&f.0, None)
            .err()
            .expect("a structured input cannot be passed to a number handler");
        assert!(
            error.contains("language-model-tool-handler"),
            "{kind}: {error}"
        );
        let input = if kind == "object" {
            "Record<string, unknown>"
        } else {
            "unknown[]"
        };
        f.write(
            "app.ts",
            &format!(
                "{MODEL_SOURCE}{calls}export const tools = {{inspect: (args: {input}) => args}};\n"
            ),
        );
        producer.bake(&f.0, None).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn resolved_language_model_schemas_are_checked_independently() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    let mut producer = Producer::new(Tools::default()).unwrap();
    let literal = "export const make = () => createLanguageModel({tools:[{name:'find', description:'Find', inputSchema:{type:'object',properties:{query:{type:'string'}},required:['query']}}]});\n";
    let contract = r#"use createLanguageModel, LanguageModelTool, LanguageModelInfo from "@exact/language-model"
shape Query
  query: string
component App
  mutation made as shape LanguageModelInfo
  action make
    send made = createLanguageModel(tools=[LanguageModelTool(name="find", description="Find", inputSchema="Query")])
  view
    text "Ready"
"#;
    for (declaration, data) in [
        ("component App\n  view\n    text \"Ready\"\n", literal),
        (contract, ""),
    ] {
        f.write("app.contract", declaration);
        let calls = format!(
            "{MODEL_SOURCE}import {{createLanguageModel}} from '@exact/language-model';\n\
             {data}\
             export const computed = (inputSchema: Record<string, unknown>) => createLanguageModel({{tools:[{{name:'find', description:'Find', inputSchema}}]}});\n"
        );
        f.write(
            "app.ts",
            &format!("{calls}export const tools = {{find: (args: number) => args.toFixed()}};\n"),
        );
        for result in [bake(&f.0, &Tools::default()), producer.bake(&f.0, None)] {
            let error = result
                .err()
                .expect("a computed schema must not weaken a literal schema");
            assert!(error.contains("language-model-tool-handler"), "{error}");
        }
        let error = super::web_build(&f.0).expect_err("web checks every resolved schema");
        assert!(error.contains("language-model-tool-handler"), "{error}");
        f.write(
            "app.ts",
            &format!("{calls}export const tools = {{find: (args: unknown) => args}};\n"),
        );
        producer.bake(&f.0, None).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn language_model_tools_in_type_only_dependencies_need_no_handler() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    f.write(
        "app.contract",
        "component App\n  view\n    text \"Ready\"\n",
    );
    f.write("helper.ts", "import {createLanguageModel} from '@exact/language-model';\n\
        export type Helper = {value: string};\n\
        export const make = () => createLanguageModel({tools:[{name:'find', description:'Find', inputSchema:{type:'object'}}]});\n");
    f.write(
        "bridge.ts",
        "export type {Helper} from './helper';\nexport const label = 'ready';\n",
    );
    let mut producer = Producer::new(Tools::default()).unwrap();
    for import in [
        "import type {Helper} from './helper'; export const value: Helper = {value:'ready'};",
        "import {Helper} from './helper'; export const value: Helper = {value:'ready'};",
        "export type {Helper} from './helper';",
        "export {label} from './bridge';",
    ] {
        f.write("app.ts", &format!("{MODEL_SOURCE}{import}\n"));
        for result in [bake(&f.0, &Tools::default()), producer.bake(&f.0, None)] {
            result.unwrap_or_else(|e| panic!("{import}: {e}"));
        }
    }
    super::web_build(&f.0).unwrap_or_else(|e| panic!("{e}"));
    f.write(
        "app.ts",
        &format!("{MODEL_SOURCE}export {{make}} from './helper';\n"),
    );
    let error = producer
        .bake(&f.0, None)
        .err()
        .expect("a runtime dependency's declaration needs a handler");
    assert!(error.contains("language-model-tool-handler"), "{error}");
    f.write("app.ts", &format!("{MODEL_SOURCE}export {{make}} from './helper';\nexport const tools = {{find: () => ''}};\n"));
    producer.bake(&f.0, None).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_bake_receipt_records_the_selected_backend_even_when_grants_are_identical() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = Fixture::new();
    let source = SOURCE.replace(
        "export const grants = '';",
        "const origin = 'http://127.0.0.1:4400';\nexport const grants = '';",
    );
    f.write("app.ts", &source);
    let first: Json = serde_json::from_str(&f.bake().receipt).unwrap();
    assert_eq!(first["backendOrigin"], "http://127.0.0.1:4400");
    std::fs::create_dir(f.0.join(".exact")).unwrap();
    f.write(".exact/backend-origin", "http://127.0.0.1:4412");
    let second: Json = serde_json::from_str(&f.bake().receipt).unwrap();
    assert_eq!(second["backendOrigin"], "http://127.0.0.1:4412");
    assert_eq!(first["grants"], second["grants"]);
    assert_eq!(std::fs::read_to_string(f.0.join("app.ts")).unwrap(), source);
}

const OPTIONAL: &str = r#"
import type { Draw } from './app.contract.d.ts';
export const surfaces = { spark: 0 };
export const draw: Draw = (_surface, _args, ctx) => {
  const path = new Path2D();
  path.rect(0, 0, 1, 1);
  ctx.fill(path);
  ctx.putImageData(new ImageData(1, 1), 0, 0);
  return true;
};
"#;

#[test]
fn the_baked_first_frame_can_use_native_bindings_after_reading_the_abi() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = Fixture::new();
    f.write(
        "app.ts",
        &SOURCE.replace("prefix + count", "new Headers({ x: 'ok' }).get('x') ?? ''"),
    );
    let baked = f.bake();
    let candidate = paired(&baked);
    let mut live = Runner::boot(
        candidate.plan,
        candidate.module,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(live.resource("message"), Some(&Value::str("ok")));
    assert!(
        !live.data().is_loaded(),
        "the answer is baked into the plan"
    );
}

fn assert_no_recorder(baked: &Baked) {
    let script = String::from_utf8_lossy(&baked.script);
    for code in ["canvasSeam", "ImageDataShim", "PathMoveTo", "aliceblue"] {
        assert!(
            !script.contains(code),
            "a module without drawing carries {code}"
        );
    }
    let mut candidate = paired(baked);
    candidate.module.bind(&candidate.plan);
    candidate.module.load().unwrap();
    let answer = candidate
        .module
        .answer(&mut Store::new("", []), "message", &[Value::Number(0.0)])
        .unwrap();
    let Answer::Now(answer) = answer else {
        panic!("the global probe answers synchronously");
    };
    assert_eq!(answer, Value::str("undefined,undefined"));
}

fn without_drawing() -> String {
    SOURCE.replace(
        "prefix + count",
        "[typeof Path2D, typeof ImageData].join(',')",
    )
}

#[test]
fn modules_without_drawing_do_not_carry_the_recorder() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let mut failures = Vec::new();
    let mut producer = Producer::new(Tools::default()).unwrap();
    for declaration in [
        "export * from './helper.cjs';",
        "export type { draw, surfaces } from './optional';",
        "// export const draw = () => false;\n// export const surfaces = {};",
        "/* export function draw() {}\nexport const surfaces = {}; */",
        "const example = 'export const draw = () => false; export const surfaces = {};';",
    ] {
        let f = Fixture::new();
        f.write("helper.cjs", "exports.helper = 1;");
        f.write("optional.ts", OPTIONAL);
        f.write("app.ts", &format!("{}\n{declaration}\n", without_drawing()));
        match bake(&f.0, &Tools::default()) {
            Ok(baked) => {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&baked.receipt).unwrap()["abi"],
                    1
                );
                assert_no_recorder(&baked);
                assert_no_recorder(&producer.bake(&f.0, None).unwrap());
            }
            Err(error) => failures.push(format!("{declaration}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn assert_drawing(baked: &Baked) {
    let receipt: serde_json::Value = serde_json::from_str(&baked.receipt).unwrap();
    assert_eq!(receipt["abi"], 2);
    assert_eq!(receipt["surfaces"], serde_json::json!({ "spark": 0 }));
    let mut candidate = paired(baked);
    candidate.module.bind(&candidate.plan);
    candidate.module.load().unwrap();
    let request = DrawRequest {
        canvas: 1,
        generation: 0,
        seq: 0,
        surface: "spark",
        args: &[],
        names: &[],
        frame: Default::default(),
        current_color: None,
        rtl: false,
        p3: false,
    };
    let Drawn::Now(reply) = candidate.module.draw(&request, &Context2d::new()) else {
        panic!("the canvas draws synchronously");
    };
    assert_eq!(reply.error, None);
    assert!(reply.wants_frame);
    assert!(
        !reply.lists.is_empty(),
        "the draw recorded its path and pixels"
    );
}

#[test]
fn both_producers_preserve_drawing_exports_and_canvas_abi() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let mut producer = Producer::new(Tools::default()).unwrap();
    let f = Fixture::new();
    f.write("optional.ts", OPTIONAL);
    f.write(
        "drawing.cjs",
        "exports.surfaces = { spark: 0 }; exports.draw = (_surface, _args, ctx) => { ctx.fillRect(0, 0, 1, 1); return true; };",
    );
    for declaration in [
        "export { draw } from './drawing.cjs'; export * from './drawing.cjs';",
        "export * from './optional';",
        "export { draw, surfaces } from './optional';",
        "import { draw, surfaces } from './optional'; export { draw, surfaces };",
        "import * as made from './optional'; export var draw = made.draw, surfaces = made.surfaces;",
        "import * as made from './optional'; export const { draw, surfaces } = made;",
        "export\n{ draw, surfaces } from './optional';",
    ] {
        f.write("app.ts", &format!("{SOURCE}\n{declaration}\n"));
        let standalone = f.bake();
        assert_drawing(&standalone);
        let resident = producer.bake(&f.0, None).unwrap();
        assert_drawing(&resident);
        assert_eq!(standalone.bytecode, resident.bytecode);
    }
    f.write("optional.ts", &OPTIONAL.replace("{ spark: 0 }", "{}"));
    let empty = f.bake();
    let receipt: serde_json::Value = serde_json::from_str(&empty.receipt).unwrap();
    assert_eq!(receipt["abi"], 2, "an empty roster still has a drawing ABI");
    assert_eq!(receipt["surfaces"], serde_json::json!({}));
    f.write("app.ts", &without_drawing());
    let removed = producer.bake(&f.0, None).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&removed.receipt).unwrap()["abi"],
        1
    );
    assert_no_recorder(&removed);
}

#[test]
fn drawing_exports_keep_their_type_and_pairing_checks() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = Fixture::new();
    f.write("optional.ts", OPTIONAL);
    for (declaration, expected) in [
        (
            "export var draw = 42; export var surfaces = {};",
            "not assignable",
        ),
        (
            "export { draw } from './optional';",
            "exports `draw` and `surfaces` together",
        ),
        (
            "export { surfaces } from './optional';",
            "exports `draw` and `surfaces` together",
        ),
    ] {
        f.write("app.ts", &format!("{SOURCE}\n{declaration}\n"));
        let error = bake(&f.0, &Tools::default())
            .err()
            .expect("invalid drawing exports must refuse");
        assert!(error.contains(expected), "{declaration}: {error}");
    }
    f.write(
        "optional.ts",
        "export const draw = 42; export const surfaces = {};",
    );
    f.write(
        "app.ts",
        &format!("{SOURCE}\nexport * from './optional';\n"),
    );
    let error = bake(&f.0, &Tools::default())
        .err()
        .expect("re-exported drawing is type-checked");
    assert!(error.contains("not assignable"), "{error}");
}

#[test]
fn a_tool_handler_must_be_an_own_property_of_the_tools_export() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    f.write(
        "app.contract",
        "component App\n  view\n    text \"Ready\"\n",
    );
    let calls = "import {createLanguageModel} from '@exact/language-model';\n\
        export const make = () => createLanguageModel({tools:[{name:'find', description:'Find', inputSchema:{type:'object',properties:{query:{type:'string'}},required:['query']}}]});\n";
    let mut producer = Producer::new(Tools::default()).unwrap();
    for handlers in [
        "export const tools: Record<string, (args: {query: string}) => unknown> = {};",
        "class Handlers { find(args: {query: string}) { return args.query; } }\nexport const tools = new Handlers();",
        "class Base { get find() { return (args: {query: string}) => args.query; } }\n\
         export const tools: {find: (args: {query: string}) => unknown} = new (class extends Base {})();",
    ] {
        f.write("app.ts", &format!("{MODEL_SOURCE}{calls}{handlers}\n"));
        for result in [bake(&f.0, &Tools::default()), producer.bake(&f.0, None)] {
            let error = result
                .err()
                .expect("a tool the export does not own fails at bake");
            assert!(error.contains("language-model-tool-handler"), "{handlers}: {error}");
        }
        let error = super::web_build(&f.0).expect_err("the web bake takes only own handlers");
        assert!(error.contains("language-model-tool-handler"), "{error}");
    }
    for handlers in [
        "export const tools: Record<string, (args: {query: string}) => unknown> = {find: ({query}) => query};",
        "class Handlers { find = (args: {query: string}) => args.query; }\nexport const tools = new Handlers();",
        "const made = (): Record<string, (args: {query: string}) => unknown> => ({});\nexport const tools = made();",
    ] {
        f.write("app.ts", &format!("{MODEL_SOURCE}{calls}{handlers}\n"));
        producer
            .bake(&f.0, None)
            .unwrap_or_else(|e| panic!("{handlers}: {e}"));
    }
}

#[test]
fn a_javascript_dependency_with_declarations_is_still_checked() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = model_fixture();
    f.write(
        "app.contract",
        "component App\n  view\n    text \"Ready\"\n",
    );
    f.write("helper.js", "import {createLanguageModel} from '@exact/language-model';\n\
        export const make = () => createLanguageModel({tools:[{name:'find', description:'Find', inputSchema:{type:'object'}}]});\n");
    f.write("helper.d.ts", "export declare const make: () => unknown;\n");
    f.write(
        "app.ts",
        &format!("{MODEL_SOURCE}export {{make}} from './helper.js';\n"),
    );
    let mut producer = Producer::new(Tools::default()).unwrap();
    for result in [bake(&f.0, &Tools::default()), producer.bake(&f.0, None)] {
        let error = result
            .err()
            .expect("a declared JavaScript dependency's tool needs a handler");
        assert!(error.contains("language-model-tool-handler"), "{error}");
    }
    let error = super::web_build(&f.0).expect_err("the web bake reads the same dependency");
    assert!(error.contains("language-model-tool-handler"), "{error}");
    f.write(
        "app.ts",
        &format!("{MODEL_SOURCE}export {{make}} from './helper.js';\nexport const tools = {{find: () => ''}};\n"),
    );
    producer.bake(&f.0, None).unwrap_or_else(|e| panic!("{e}"));
}
