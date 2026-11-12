use super::{paired, Fixture, SOURCE};
use exact_js_bake::{bake, Baked, Producer, Tools};
use exact_kernel::Kernel;
use exact_runner::{
    exact_canvas::Context2d, Answer, DataSource, DrawRequest, Drawn, Runner, Store, Value,
};
use serde_json::Value as Json;

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
