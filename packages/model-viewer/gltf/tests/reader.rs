mod support;
use exact_model_gltf::glb::{parse, parse_with_limit};
use serde_json::json;
use support::{glb, triangle};

#[test]
fn required_extensions_are_refused_by_name() {
    let (mut json, bin) = triangle();
    json["extensionsRequired"] = json!(["KHR_draco_mesh_compression"]);
    let error = parse(&glb(&json, &bin)).expect_err("required extension was ignored");
    assert!(error.contains("KHR_draco_mesh_compression"), "{error}");
}

#[test]
fn sparse_accessors_are_refused_even_with_a_base_view() {
    let (mut json, bin) = triangle();
    json["accessors"][0]["sparse"] = json!({"count":1});
    let error = parse(&glb(&json, &bin)).expect_err("sparse accessor was ignored");
    assert!(
        error.contains("accessor 0") && error.contains("sparse"),
        "{error}"
    );
}

fn optimized() -> (serde_json::Value, Vec<u8>) {
    let bytes = include_bytes!("../../test/fixture/assets/assembly-meshopt.glb");
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json = serde_json::from_slice(&bytes[20..20 + length]).unwrap();
    (json, bytes[28 + length..].to_vec())
}

#[test]
fn extensions_work_when_used_or_required_and_other_optional_extensions_are_ignored() {
    let (original, bin) = optimized();
    for field in ["extensionsUsed", "extensionsRequired"] {
        let mut json = original.clone();
        json.as_object_mut().unwrap().remove("extensionsUsed");
        json.as_object_mut().unwrap().remove("extensionsRequired");
        json[field] = json!(["EXT_meshopt_compression", "KHR_mesh_quantization"]);
        assert_eq!(parse(&glb(&json, &bin)).unwrap().triangles(), 108);
    }
    let (mut json, bin) = triangle();
    json["extensionsUsed"] = json!(["KHR_draco_mesh_compression"]);
    assert_eq!(parse(&glb(&json, &bin)).unwrap().triangles(), 1);
}

#[test]
fn accessor_and_meshopt_refusals_name_their_source() {
    let cases = [
        ("type", json!("VEC2"), "VEC3"),
        ("componentType", json!(5125), "componentType"),
        ("count", json!(u64::MAX), "buffer"),
        ("byteOffset", json!(u64::MAX), "byteOffset"),
        ("normalized", json!(true), "normalized"),
    ];
    for (key, value, message) in cases {
        let (mut json, bin) = triangle();
        json["accessors"][0][key] = value;
        let error = parse(&glb(&json, &bin)).unwrap_err();
        assert!(
            error.contains("accessor 0") && error.contains(message),
            "{error}"
        );
    }
    for (key, value) in [
        ("byteStride", json!(4)),
        ("byteLength", json!(35)),
        ("buffer", json!(1)),
    ] {
        let (mut json, bin) = triangle();
        json["bufferViews"][0][key] = value;
        let error = parse(&glb(&json, &bin)).unwrap_err();
        assert!(error.contains("accessor 0"), "{error}");
    }
    let (mut json, bin) = triangle();
    json["buffers"][0]["uri"] = json!("outside.bin");
    let error = parse(&glb(&json, &bin)).unwrap_err();
    assert!(
        error.contains("accessor 0") && error.contains("external"),
        "{error}"
    );
    for (key, value, message) in [
        ("count", json!(100000000), "256 MiB"),
        ("byteStride", json!(3), "byteLength/byteStride"),
        ("filter", json!("UNKNOWN"), "UNKNOWN"),
        ("mode", json!("UNKNOWN"), "UNKNOWN"),
        ("byteLength", json!(1), "truncated"),
        ("byteOffset", json!(u64::MAX), "overflow"),
    ] {
        let (mut json, bin) = optimized();
        // View 1 is an attribute stream, so filter validation reaches the decoder.
        json["bufferViews"][1]["extensions"]["EXT_meshopt_compression"][key] = value;
        let error = parse(&glb(&json, &bin)).unwrap_err();
        assert!(
            error.contains("bufferView 1")
                && error.contains("EXT_meshopt_compression")
                && error.contains(message),
            "{error}"
        );
    }
}

#[test]
fn file_and_device_limits_refuse_before_geometry_allocation() {
    let (json, bin) = triangle();
    let file = glb(&json, &bin);
    // Three 32-byte vertices and three u32 indices.
    assert_eq!(parse_with_limit(&file, 96).unwrap().triangles(), 1);
    assert!(parse_with_limit(&file, 95)
        .unwrap_err()
        .contains("max_buffer_size"));
    for length in [0, 4, 12, 19, file.len() - 1] {
        assert!(parse(&file[..length]).unwrap_err().contains("GLB:"));
    }
    for (at, value) in [(4, 1u32), (8, 0), (12, u32::MAX), (16, 0x004e4942)] {
        let mut bytes = file.clone();
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(parse(&bytes).unwrap_err().contains("GLB:"));
    }
    assert!(parse(&vec![0; 64 * 1024 * 1024 + 1])
        .unwrap_err()
        .contains("64 MiB"));
    let mut json = json.clone();
    // A valid zero-initialized accessor still obeys the device limit.
    json["accessors"][0]
        .as_object_mut()
        .unwrap()
        .remove("bufferView");
    json["accessors"][0]["count"] = json!(1_000_000_000);
    assert!(parse_with_limit(&glb(&json, &[]), 1024)
        .unwrap_err()
        .contains("max_buffer_size"));
}
