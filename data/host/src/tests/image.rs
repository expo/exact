use super::*;

/// A Rust source's `fs.compressImage` (LLP 1069.002 A1.1): the operation a
/// TypeScript source's `storage.fs.compressImage` runs, through ibex2's
/// executor and the platform codec. On Apple the fixture comes back upright
/// as a JPEG; elsewhere the call is refused as `unsupported`.
#[test]
fn compress_image_answers_a_rust_source_as_typescripts() {
    let paths = Paths::new();
    let mut host = Storage::new(Fixture::new());
    paths.configure(&mut host);
    host.activate().unwrap();
    run(
        &mut host,
        storage::request("fs.mkdir", json!({"path":"app:/data/in"})),
    )
    .unwrap();
    std::fs::write(
        paths.0.join("data/in/photo.jpg"),
        include_bytes!("../../../../scripts/fixtures/picker/oriented-gps.jpg"),
    )
    .unwrap();
    let compress = |to: &str, dimension: Json, bytes: Json| {
        storage::request(
            "fs.compressImage",
            json!({"path":"app:/data/in/photo.jpg","destination":to,"maxDimension":dimension,"maxBytes":bytes}),
        )
    };
    // A1.12: `quality`, with a `maxBytes` that may be null.
    let at = |to: &str, bytes: Json, quality: f64| {
        storage::request(
            "fs.compressImage",
            json!({"path":"app:/data/in/photo.jpg","destination":to,"maxDimension":32,"maxBytes":bytes,"quality":quality}),
        )
    };
    assert!(host.take_storage_changed());
    let result = run(
        &mut host,
        compress("app:/data/out.jpg", json!(32), json!(2_000_000)),
    );
    assert_eq!(host.take_storage_changed(), result.is_ok());
    if cfg!(target_vendor = "apple") {
        let size = std::fs::metadata(paths.0.join("data/out.jpg"))
            .unwrap()
            .len();
        assert_eq!(
            result.unwrap(),
            json!({"path":"app:/data/out.jpg","type":"image/jpeg","size":size,"width":24,"height":32})
        );
        let err = run(
            &mut host,
            compress("app:/data/unfit.jpg", json!(64), json!(1)),
        )
        .unwrap_err();
        assert!(err.starts_with("compressImage: unfit: "), "{err}");
        // A source over 64 MiB is the documented `too-large`, not the
        // generic result limit its bounded read hits first.
        let big = std::fs::File::create(paths.0.join("data/in/big.jpg")).unwrap();
        big.set_len(64 * 1024 * 1024 + 1).unwrap();
        let err = run(
            &mut host,
            storage::request(
                "fs.compressImage",
                json!({"path":"app:/data/in/big.jpg","destination":"app:/data/big-out.jpg","maxDimension":32,"maxBytes":2_000_000}),
            ),
        )
        .unwrap_err();
        assert!(err.starts_with("compressImage: too-large: "), "{err}");
        // `quality` alone is one encode at it; lower is smaller.
        let low = run(&mut host, at("app:/data/low.jpg", json!(null), 0.1)).unwrap();
        let high = run(&mut host, at("app:/data/high.jpg", json!(null), 1.0)).unwrap();
        assert_eq!((&low["width"], &low["height"]), (&json!(24), &json!(32)));
        assert!(low["size"].as_u64() < high["size"].as_u64(), "{low} {high}");
        assert!(host.take_storage_changed());
    } else {
        assert_eq!(
            result.unwrap_err(),
            "compressImage: unsupported: no image codec"
        );
    }
    for (request, expected) in [
        (
            compress("app:/cache/out.jpg", json!(32), json!(100)),
            "denied: fs.write",
        ),
        (
            compress("doc:/1/out.jpg", json!(32), json!(100)),
            "compressImage: needs app:/ paths",
        ),
        (
            compress("app:/data/x.jpg", json!(0), json!(100)),
            "compressImage: invalid: maxDimension",
        ),
        (
            compress("app:/data/x.jpg", json!(32), json!("100")),
            "storage: maxBytes must be a number",
        ),
        (
            compress("app:/data/x.jpg", json!(32), json!(null)),
            "compressImage: invalid: needs maxBytes, quality or both",
        ),
        (
            at("app:/data/x.jpg", json!(100), 1.5),
            "compressImage: invalid: quality must be a number from 0 to 1",
        ),
    ] {
        let err = run(&mut host, request).unwrap_err();
        assert!(err.starts_with(expected), "{err}");
    }
    assert!(!paths.0.join("data/x.jpg").exists());
    assert!(!host.take_storage_changed());
}

/// A Rust source's `fs.transformImage` (LLP 1133.019 D17): the record is
/// D14's options beside the two paths, and the answer is D14's record; and
/// `fs.compressImage` takes the edit as TypeScript's call does (D15).
#[test]
fn transform_image_answers_a_rust_source_as_typescripts() {
    let paths = Paths::new();
    let mut host = Storage::new(Fixture::new());
    paths.configure(&mut host);
    host.activate().unwrap();
    run(
        &mut host,
        storage::request("fs.mkdir", json!({"path":"app:/data/in"})),
    )
    .unwrap();
    std::fs::write(
        paths.0.join("data/in/photo.jpg"),
        include_bytes!("../../../../scripts/fixtures/picker/oriented-gps.jpg"),
    )
    .unwrap();
    let transform = |to: &str, options: Json| {
        let mut args = json!({"path":"app:/data/in/photo.jpg","destination":to});
        args.as_object_mut()
            .unwrap()
            .extend(options.as_object().unwrap().clone());
        storage::request("fs.transformImage", args)
    };
    assert!(host.take_storage_changed());
    let result = run(
        &mut host,
        transform("app:/data/turned", json!({"rotate":90,"metadata":"none"})),
    );
    assert_eq!(host.take_storage_changed(), result.is_ok());
    let cropped = run(
        &mut host,
        storage::request(
            "fs.compressImage",
            json!({"path":"app:/data/in/photo.jpg","destination":"app:/data/crop.jpg","maxDimension":100,"maxBytes":2_000_000,
                "crop":{"x":0,"y":0,"width":24,"height":64}}),
        ),
    );
    assert_eq!(host.take_storage_changed(), cropped.is_ok());
    if cfg!(target_vendor = "apple") {
        let size = std::fs::metadata(paths.0.join("data/turned.jpg"))
            .unwrap()
            .len();
        assert_eq!(
            result.unwrap(),
            json!({"path":"app:/data/turned.jpg","type":"image/jpeg","size":size,"width":64,"height":48,
                "colorSpace":"srgb","hdr":false,"metadata":"none"})
        );
        assert_eq!(cropped.unwrap()["width"], 24);
        let err = run(
            &mut host,
            transform(
                "app:/data/x.jpg",
                json!({"crop":{"x":0,"y":100,"width":1,"height":1}}),
            ),
        )
        .unwrap_err();
        assert!(err.starts_with("transformImage: outside: "), "{err}");
    } else {
        assert_eq!(
            result.unwrap_err(),
            "transformImage: unsupported: no image codec"
        );
        assert!(cropped.is_err());
    }
    for (request, expected) in [
        (
            transform("app:/cache/out.jpg", json!({})),
            "denied: fs.write",
        ),
        (
            transform("doc:/1/out.jpg", json!({})),
            "transformImage: needs app:/ paths",
        ),
    ] {
        let err = run(&mut host, request).unwrap_err();
        assert!(err.starts_with(expected), "{err}");
    }
    if cfg!(target_vendor = "apple") {
        let err = run(
            &mut host,
            transform("app:/data/x.jpg", json!({"flip":"both"})),
        )
        .unwrap_err();
        assert!(err.starts_with("transformImage: invalid: flip"), "{err}");
    }
    assert!(!paths.0.join("data/x.jpg").exists());
}
