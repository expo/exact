use exact_meshopt::decode;
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn decoded_buffers_equal_npm_meshoptimizer() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/buffers.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 22);
    for case in cases.as_array().unwrap() {
        let encoded = unhex(case["encoded"].as_str().unwrap());
        let expected = unhex(case["expected"].as_str().unwrap());
        let result = decode(
            case["count"].as_u64().unwrap() as usize,
            case["stride"].as_u64().unwrap() as usize,
            &encoded,
            case["mode"].as_str().unwrap(),
            case["filter"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(result.len(), expected.len());
        assert!(
            result == expected,
            "{}: first difference {:?}",
            case["name"],
            result.iter().zip(&expected).position(|(a, b)| a != b)
        );
    }
}
#[test]
fn malformed_streams_refuse_without_panicking() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/buffers.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let encoded = unhex(case["encoded"].as_str().unwrap());
        let count = case["count"].as_u64().unwrap() as usize;
        let stride = case["stride"].as_u64().unwrap() as usize;
        let mode = case["mode"].as_str().unwrap();
        let filter = case["filter"].as_str().unwrap();
        for length in [0, 1, encoded.len() / 2, encoded.len() - 1] {
            assert!(
                decode(count, stride, &encoded[..length], mode, filter).is_err(),
                "{} truncated at {length}",
                case["name"]
            );
        }
        let mut extra = encoded.clone();
        extra.push(0);
        assert!(decode(count, stride, &extra, mode, filter).is_err());
    }
    for (mode, stride, filter) in [
        ("ATTRIBUTES", 3, "NONE"),
        ("INDICES", 4, "OCTAHEDRAL"),
        ("TRIANGLES", 1, "NONE"),
        ("ATTRIBUTES", 4, "QUATERNION"),
        ("ATTRIBUTES", 4, "COLOR"),
        ("OTHER", 4, "NONE"),
    ] {
        assert!(decode(3, stride, &[], mode, filter).is_err());
    }
    assert!(decode(usize::MAX, 4, &[], "ATTRIBUTES", "NONE").is_err());
    assert_eq!(
        decode(256 * 1024 * 1024 / 4 + 1, 4, &[], "ATTRIBUTES", "NONE").unwrap_err(),
        "meshopt decoded buffer exceeds 256 MiB"
    );
}
