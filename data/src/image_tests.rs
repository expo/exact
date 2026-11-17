use super::*;

/// A deadline no test reaches.
fn later() -> Instant {
    Instant::now() + std::time::Duration::from_secs(60)
}

/// A1.1's two limits, with no quality.
pub(crate) fn aim(max_dimension: u32, max_bytes: u64) -> Limits {
    Limits {
        max_dimension,
        max_bytes: Some(max_bytes),
        quality: None,
    }
}

/// `limits` under a ceiling of `quality` (A1.12).
pub(crate) fn at(quality: f64, limits: Limits) -> Limits {
    Limits {
        quality: Some(quality),
        ..limits
    }
}

/// A trial's quality as the recorded sequences give it, in whole percents.
fn percent(q: f64) -> u32 {
    (q * 100.0).round() as u32
}

/// The size a recorded case's `model` gives a `w × h` trial at quality `q`
/// (`scripts/fixtures/picker/compress-trials.json`).
fn model(name: &str, w: u32, h: u32, q: u32) -> usize {
    let (w, h, q) = (u64::from(w), u64::from(h), u64::from(q));
    (match name {
        "q*1000" => q * 1000,
        "floor(w*h/4)+q*1000" => w * h / 4 + q * 1000,
        "w*h" => w * h,
        "floor(w*h*q/1000)" => w * h * q / 1000,
        "floor(w*h*q/250)" => w * h * q / 250,
        other => panic!("no model {other}"),
    }) as usize
}

#[test]
fn the_search_makes_blueskys_trials() {
    let recorded: serde_json::Value = serde_json::from_str(include_str!(
        "../../scripts/fixtures/picker/compress-trials.json"
    ))
    .unwrap();
    for case in recorded["cases"].as_array().unwrap() {
        let n = |k: &str| case[k].as_u64().unwrap();
        let name = case["model"].as_str().unwrap();
        let mut trials = Vec::new();
        let found = search(
            n("width") as u32,
            n("height") as u32,
            aim(n("maxDimension") as u32, n("maxBytes")),
            |w, h, q| {
                trials.push(serde_json::json!([w, h, percent(q)]));
                Ok(vec![0; model(name, w, h, percent(q))])
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::Value::Array(trials),
            case["trials"],
            "{}",
            case["name"]
        );
        let found = found.map(
            |c| serde_json::json!({"width": c.width, "height": c.height, "size": c.bytes.len()}),
        );
        assert_eq!(
            found.unwrap_or(serde_json::Value::Null),
            case["result"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn contain_floors_never_upscales_and_keeps_a_pixel() {
    assert_eq!(contain(6000, 4000, 4000), (4000, 2666));
    assert_eq!(contain(3024, 4032, 2000), (1500, 2000));
    assert_eq!(contain(800, 601, 4000), (800, 601));
    assert_eq!(contain(4000, 4000, 3200), (3200, 3200));
    // Bluesky would floor to 0; a side is at least one pixel.
    assert_eq!(contain(9000, 2, 2048), (2048, 1));
}

/// A1.2's declared deviation: a side is never 0, so a one-pixel limit or a
/// sliver of a source still makes trials (Bluesky's would ask for 0).
#[test]
fn the_smallest_sizes_keep_a_pixel_per_side() {
    let mut sizes = Vec::new();
    let found = search(64, 48, aim(1, 1), |w, h, _| {
        sizes.push((w, h));
        Ok(vec![0; 2])
    })
    .unwrap();
    assert!(found.is_none());
    assert!(sizes.iter().all(|&(w, h)| (w, h) == (1, 1)), "{sizes:?}");
    assert_eq!(sizes.len(), 8);
    let mut sizes = Vec::new();
    search(8000, 1, aim(4000, 10), |w, h, _| {
        sizes.push((w, h));
        Ok(vec![0; 1])
    })
    .unwrap()
    .unwrap();
    assert!(sizes.iter().all(|&s| s == (4000, 1)), "{sizes:?}");
}

#[test]
fn an_encoder_error_ends_the_search_with_it() {
    let mut calls = 0;
    let err = search(100, 100, aim(100, 10), |_, _, _| {
        calls += 1;
        Err(failure("timeout", "late"))
    })
    .unwrap_err();
    assert_eq!((calls, err.as_str()), (1, "compressImage: timeout: late"));
}

/// A1.11: a PNG is tried at the fitted size, then at each 0.8 shrink, at most
/// four of them; nothing fitting is `None`.
#[test]
fn a_png_shrinks_until_it_fits() {
    let mut sizes = Vec::new();
    let found = search_lossless(1000, 800, aim(1000, 300_000), |w, h| {
        sizes.push((w, h));
        Ok(vec![0; (w * h) as usize / 2])
    })
    .unwrap()
    .unwrap();
    assert_eq!(sizes, [(1000, 800), (800, 640)]);
    assert_eq!((found.width, found.height), (800, 640));
    let mut sizes = Vec::new();
    let none = search_lossless(1000, 800, aim(1000, 1), |w, h| {
        sizes.push((w, h));
        Ok(vec![0; 2])
    })
    .unwrap();
    assert!(none.is_none());
    assert_eq!(
        sizes,
        [(1000, 800), (800, 640), (640, 512), (512, 409), (409, 327)]
    );
}

#[test]
fn the_type_option_takes_jpeg_and_png() {
    assert_eq!(Format::parse("image/jpeg"), Some(Format::Jpeg));
    assert_eq!(Format::parse("image/png"), Some(Format::Png));
    for other in ["image/webp", "png", "IMAGE/PNG", ""] {
        assert_eq!(Format::parse(other), None, "{other}");
    }
    assert_eq!(Format::default().media_type(), "image/jpeg");
    assert_eq!(Format::Png.media_type(), "image/png");
}

#[test]
fn limits_are_integers_in_range() {
    assert_eq!(
        check_limits(4000.0, Some(2e6), None),
        Ok(aim(4000, 2_000_000))
    );
    for (d, b) in [
        (0.0, 1.0),
        (8193.0, 1.0),
        (1.5, 1.0),
        (1.0, 0.0),
        (1.0, 67108865.0),
        (f64::NAN, 1.0),
    ] {
        assert!(
            check_limits(d, Some(b), None)
                .unwrap_err()
                .starts_with("compressImage: invalid: "),
            "{d} {b}"
        );
    }
}

/// A1.12: `quality` is a number from 0 to 1; with it `maxBytes` may be
/// unsaid, without it not; a bad `maxBytes` is named before `quality`.
#[test]
fn quality_is_checked_and_makes_max_bytes_optional() {
    assert_eq!(
        check_limits(100.0, None, Some(0.5)),
        Ok(Limits {
            max_dimension: 100,
            max_bytes: None,
            quality: Some(0.5),
        })
    );
    assert_eq!(
        check_limits(100.0, Some(1000.0), Some(0.0)),
        Ok(at(0.0, aim(100, 1000)))
    );
    assert_eq!(
        check_limits(100.0, Some(1000.0), Some(1.0)),
        Ok(at(1.0, aim(100, 1000)))
    );
    for q in [-0.01, 1.01, f64::NAN, f64::INFINITY] {
        assert_eq!(
            check_limits(100.0, Some(1000.0), Some(q)).unwrap_err(),
            "compressImage: invalid: quality must be a number from 0 to 1",
            "{q}"
        );
    }
    assert_eq!(
        check_limits(100.0, None, None).unwrap_err(),
        "compressImage: invalid: needs maxBytes, quality or both"
    );
    assert_eq!(
        check_limits(100.0, Some(0.0), Some(2.0)).unwrap_err(),
        "compressImage: invalid: maxBytes must be an integer from 1 to 67108864"
    );
}

/// A search's trials, each `(w, h, quality)`, and what it found, `(w, h, size)`.
type Trials = (Vec<(u32, u32, f64)>, Option<(u32, u32, usize)>);

/// The trials a ceiling search makes over a `width × height` source, where
/// a trial is `size(w, h, q)` bytes.
fn ceiling_trials(
    (width, height): (u32, u32),
    limits: Limits,
    size: impl Fn(u32, u32, f64) -> usize,
) -> Trials {
    let mut trials = Vec::new();
    let found = search(width, height, limits, |w, h, q| {
        trials.push((w, h, q));
        Ok(vec![0; size(w, h, q)])
    })
    .unwrap();
    (trials, found.map(|c| (c.width, c.height, c.bytes.len())))
}

/// A1.12: a ceiling that fits is the result, in one encode at exactly the
/// quality asked.
#[test]
fn a_ceiling_that_fits_is_one_encode() {
    let (trials, found) =
        ceiling_trials((4000, 3000), at(0.855, aim(2000, 1_000_000)), |_, _, q| {
            (q * 1000.0) as usize
        });
    assert_eq!(trials, [(2000, 1500, 0.855)]);
    assert_eq!(found, Some((2000, 1500, 855)));
}

/// A1.12: a ceiling that misses bisects the whole percents below it, then
/// gives up on the size as A1.2 does and tries the ceiling first at the next.
#[test]
fn a_ceiling_that_misses_bisects_below_it_then_shrinks() {
    // q * 1000 bytes at 2000, a tenth of that smaller: 0.8 misses 100 at
    // 2000, then 40 and 20 miss, so the size shrinks; at 1600, 0.8 fits.
    let size = |w: u32, _: u32, q: f64| (q * if w == 2000 { 1000.0 } else { 100.0 }) as usize;
    let (trials, found) = ceiling_trials((4000, 3000), at(0.8, aim(2000, 100)), size);
    assert_eq!(
        trials,
        [
            (2000, 1500, 0.8),
            (2000, 1500, 0.4),
            (2000, 1500, 0.2),
            (1600, 1200, 0.8),
        ]
    );
    assert_eq!(found, Some((1600, 1200, 80)));
    // 0.8 misses, the bisection climbs from 40 and never reaches 80.
    let (trials, found) = ceiling_trials((4000, 3000), at(0.8, aim(2000, 700)), |_, _, q| {
        (q * 1000.0) as usize
    });
    let qualities: Vec<u32> = trials.iter().map(|t| percent(t.2)).collect();
    assert_eq!(qualities, [80, 40, 60, 70, 75, 73, 72, 71]);
    assert!(trials[1..].iter().all(|t| t.2 < 0.8));
    assert_eq!(found, Some((2000, 1500, 700)));
    // Nothing fits: four sizes, each the ceiling then 40 and 20; `unfit`.
    let (trials, found) = ceiling_trials((4000, 3000), at(0.8, aim(2000, 1)), |_, _, _| 2);
    let sizes: Vec<u32> = trials.iter().map(|t| t.0).collect();
    assert_eq!(
        sizes,
        [2000, 2000, 2000, 1600, 1600, 1600, 1280, 1280, 1280, 1024, 1024, 1024]
    );
    assert_eq!(found, None);
}

/// A1.12: the bisection's top is the least whole percent at or over the
/// ceiling, though `quality * 100` rounds across it; a ceiling under 0.14
/// still shrinks rather than stopping.
#[test]
fn the_bisection_stays_under_any_ceiling() {
    assert_eq!(top(None), 101);
    for (quality, expected) in [
        (0.56, 56),
        (0.57, 57),
        (0.14, 14),
        (0.855, 86),
        (1.0, 100),
        (0.0, 0),
        (0.005, 1),
    ] {
        assert_eq!(top(Some(quality)), expected, "{quality}");
    }
    // 0.56 * 100 is 56.00000000000001: the bisection still stops below 56.
    let (trials, _) = ceiling_trials((100, 100), at(0.56, aim(100, 559)), |_, _, q| {
        (q * 1000.0) as usize
    });
    let qualities: Vec<u32> = trials.iter().map(|t| percent(t.2)).collect();
    assert_eq!(qualities, [56, 28, 42, 49, 53, 55]);
    for quality in [0.0, 0.1, 0.56, 0.29, 1.0] {
        let (trials, _) = ceiling_trials((4000, 3000), at(quality, aim(2000, 1)), |_, _, _| 2);
        assert!(
            trials.iter().all(|t| t.2 <= quality),
            "{quality} {trials:?}"
        );
        let ceilings = trials.iter().filter(|t| t.2 == quality).count();
        assert_eq!(ceilings, 4, "{quality}: the ceiling at each of four sizes");
    }
}

/// A1.12: without `maxBytes`, one encode at `quality` at the fitted size;
/// over 64 MiB is nothing.
#[test]
fn quality_alone_is_one_encode() {
    let alone = Limits {
        max_dimension: 1000,
        max_bytes: None,
        quality: Some(0.7),
    };
    let (trials, found) = ceiling_trials((4000, 3000), alone, |_, _, _| 5_000_000);
    assert_eq!(trials, [(1000, 750, 0.7)]);
    assert_eq!(found, Some((1000, 750, 5_000_000)));
    let (trials, found) = ceiling_trials((4000, 3000), alone, |_, _, _| MAX_BYTES as usize + 1);
    assert_eq!((trials.len(), found), (1, None));
}

/// A1.12: a PNG has no quality; with `maxBytes` it searches as A1.11, and
/// without it it is one encode at the fitted size.
#[test]
fn a_png_ignores_quality() {
    let mut sizes = Vec::new();
    search_lossless(1000, 800, at(0.3, aim(1000, 300_000)), |w, h| {
        sizes.push((w, h));
        Ok(vec![0; (w * h) as usize / 2])
    })
    .unwrap()
    .unwrap();
    assert_eq!(sizes, [(1000, 800), (800, 640)]);
    let alone = Limits {
        max_dimension: 500,
        max_bytes: None,
        quality: Some(0.3),
    };
    let mut sizes = Vec::new();
    let found = search_lossless(1000, 800, alone, |w, h| {
        sizes.push((w, h));
        Ok(vec![0; MAX_BYTES as usize + 1])
    })
    .unwrap();
    assert_eq!((sizes, found), (vec![(500, 400)], None));
}

#[cfg(not(target_vendor = "apple"))]
#[test]
fn without_a_codec_the_call_is_unsupported() {
    let fixture = include_bytes!("../../scripts/fixtures/picker/oriented-gps.jpg");
    for format in [Format::Jpeg, Format::Png] {
        assert_eq!(
            compress(fixture, format, aim(4000, 2_000_000), later()).unwrap_err(),
            "compressImage: unsupported: no image encoder on this host"
        );
    }
}

#[cfg(target_vendor = "apple")]
mod apple_codec {
    use super::super::apple::{first_pixel, first_rgba, inspect, png};
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../scripts/fixtures/picker/oriented-gps.jpg");

    #[test]
    fn wide_component_and_decoder_surfaces_refuse_before_pixel_decode() {
        // Valid 16x16 blue RGBA PNGs, encoded at 8 and 16 bits per component.
        const EIGHT: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 16, 0, 0, 0, 16,
            8, 6, 0, 0, 0, 31, 243, 255, 97, 0, 0, 0, 24, 73, 68, 65, 84, 120, 156, 99, 96, 96,
            248, 255, 159, 50, 60, 106, 192, 168, 1, 163, 6, 12, 19, 3, 0, 50, 166, 254, 16, 229,
            181, 228, 171, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        const SIXTEEN: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 16, 0, 0, 0, 16,
            16, 6, 0, 0, 0, 79, 99, 35, 34, 0, 0, 0, 36, 73, 68, 65, 84, 120, 156, 99, 96, 0, 130,
            255, 64, 48, 144, 244, 168, 3, 70, 29, 48, 234, 128, 81, 7, 140, 58, 96, 212, 1, 163,
            14, 24, 80, 7, 0, 0, 224, 136, 252, 46, 208, 93, 226, 31, 0, 0, 0, 0, 73, 69, 78, 68,
            174, 66, 96, 130,
        ];
        assert!(compress_controlled(
            EIGHT,
            Format::Jpeg,
            aim(16, 1000),
            later(),
            &|| Ok(()),
            MAX_SOURCE_PIXELS,
            11000
        )
        .is_ok());
        for (source, scratch) in [(SIXTEEN, 11000), (EIGHT, 8000)] {
            let checks = std::cell::Cell::new(0);
            let result = compress_controlled(
                source,
                Format::Jpeg,
                aim(16, 1000),
                later(),
                &|| {
                    checks.set(checks.get() + 1);
                    Ok(())
                },
                MAX_SOURCE_PIXELS,
                scratch,
            );
            assert_eq!(
                result.unwrap_err(),
                failure("too-large", "image exceeds its codec memory reservation")
            );
            assert_eq!(checks.get(), 2, "no decode or encode trial may start");
        }
    }

    #[test]
    fn the_fixture_is_turned_upright_and_loses_its_metadata() {
        let source = inspect(FIXTURE).unwrap();
        assert_eq!(
            (source.width, source.height, source.orientation),
            (64, 48, Some(6))
        );
        assert!(source.gps && source.exif_date);
        let out = compress(FIXTURE, Format::Jpeg, aim(4000, 2_000_000), later()).unwrap();
        let read = inspect(&out.bytes).unwrap();
        assert!(read.jpeg, "{read:?}");
        assert_eq!((out.width, out.height), (48, 64));
        assert_eq!((read.width, read.height), (48, 64));
        assert!(matches!(read.orientation, None | Some(1)), "{read:?}");
        assert!(!read.gps && !read.exif_date && !read.tiff, "{read:?}");
        // Stored red over blue, turned a quarter clockwise: blue on the left.
        let [r, g, b] = first_pixel(&out.bytes);
        assert!(b > 200 && r < 60 && g < 60, "top-left {r},{g},{b}");
    }

    #[test]
    fn a_smaller_dimension_scales_the_upright_image() {
        let out = compress(FIXTURE, Format::Jpeg, aim(32, 2_000_000), later()).unwrap();
        assert_eq!((out.width, out.height), (24, 32));
        let read = inspect(&out.bytes).unwrap();
        assert_eq!((read.width, read.height), (24, 32));
    }

    /// Noise, which JPEG cannot compress: deterministic bytes from an LCG.
    fn noise(w: u32, h: u32) -> Vec<u8> {
        let mut seed: u32 = 0x1234_5678;
        (0..w * h * 4)
            .map(|i| {
                if i % 4 == 3 {
                    return 255;
                }
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn noise_shrinks_until_it_fits() {
        let source = png(1000, 800, &noise(1000, 800));
        // 1000×800 noise is far over 120 kB at quality 26; a smaller size fits.
        let out = compress(&source, Format::Jpeg, aim(1000, 120_000), later()).unwrap();
        assert!(out.bytes.len() <= 120_000);
        assert!(out.width < 1000 && out.width >= 512, "{}", out.width);
        assert_eq!(out.height, (f64::from(out.width) * 0.8).floor() as u32);
        let read = inspect(&out.bytes).unwrap();
        assert_eq!((read.width, read.height), (out.width, out.height));
    }

    /// A1.12 on ImageIO: lower quality is smaller; a ceiling that fits is
    /// that very encode; one that misses lands under the budget at the same
    /// size; a PNG is the same file whatever the quality.
    #[test]
    fn an_explicit_quality_is_what_imageio_encodes() {
        let source = png(256, 256, &noise(256, 256));
        let alone = |quality| {
            let limits = Limits {
                max_dimension: 256,
                max_bytes: None,
                quality: Some(quality),
            };
            compress(&source, Format::Jpeg, limits, later()).unwrap()
        };
        let (low, high) = (alone(0.2), alone(0.9));
        assert!(low.bytes.len() < high.bytes.len());
        assert_eq!((high.width, high.height), (256, 256));
        let fits = at(0.9, aim(256, high.bytes.len() as u64));
        assert_eq!(
            compress(&source, Format::Jpeg, fits, later())
                .unwrap()
                .bytes,
            high.bytes
        );
        let budget = (low.bytes.len() + high.bytes.len()) as u64 / 2;
        let under = compress(&source, Format::Jpeg, at(0.9, aim(256, budget)), later()).unwrap();
        assert!(under.bytes.len() as u64 <= budget);
        assert_eq!((under.width, under.height), (256, 256));
        let png_at = |limits| {
            compress(&source, Format::Png, limits, later())
                .unwrap()
                .bytes
        };
        assert_eq!(png_at(at(0.1, aim(64, 1 << 20))), png_at(aim(64, 1 << 20)));
    }

    #[test]
    fn nothing_fitting_is_unfit() {
        let source = png(200, 200, &noise(200, 200));
        let err = compress(&source, Format::Jpeg, aim(200, 100), later()).unwrap_err();
        assert!(err.starts_with("compressImage: unfit: "), "{err}");
    }

    #[test]
    fn transparency_becomes_white() {
        let clear = vec![0u8; 8 * 8 * 4];
        let out = compress(&png(8, 8, &clear), Format::Jpeg, aim(8, 100_000), later()).unwrap();
        let [r, g, b] = first_pixel(&out.bytes);
        assert!(r > 245 && g > 245 && b > 245, "{r},{g},{b}");
    }

    /// A1.11: a PNG keeps the source's alpha, where a JPEG fills it white.
    #[test]
    fn a_png_keeps_transparency() {
        // Clear on the left half, opaque red on the right.
        let rgba: Vec<u8> = (0..16 * 16)
            .flat_map(|i| {
                if i % 16 < 8 {
                    [0, 0, 0, 0]
                } else {
                    [255, 0, 0, 255]
                }
            })
            .collect();
        let source = png(16, 16, &rgba);
        let out = compress(&source, Format::Png, aim(8, 100_000), later()).unwrap();
        assert!(out.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!((out.width, out.height), (8, 8));
        let read = inspect(&out.bytes).unwrap();
        assert!(
            !read.jpeg && (read.width, read.height) == (8, 8),
            "{read:?}"
        );
        assert_eq!(first_rgba(&out.bytes)[3], 0, "the clear corner stays clear");
        let jpeg = compress(&source, Format::Jpeg, aim(8, 100_000), later()).unwrap();
        assert_eq!(first_rgba(&jpeg.bytes), [255, 255, 255, 255]);
        let err = compress(
            &png(200, 200, &noise(200, 200)),
            Format::Png,
            aim(200, 100),
            later(),
        )
        .unwrap_err();
        assert!(err.starts_with("compressImage: unfit: no PNG"), "{err}");
    }

    #[test]
    fn bytes_that_are_no_image_are_undecodable() {
        for bytes in [&b"not an image at all"[..], &[], &FIXTURE[..200]] {
            let err = compress(bytes, Format::Jpeg, aim(100, 100_000), later()).unwrap_err();
            assert!(err.starts_with("compressImage: undecodable: "), "{err}");
        }
    }

    #[test]
    fn a_source_over_the_limits_is_too_large() {
        let err = compress(
            &vec![0; MAX_BYTES as usize + 1],
            Format::Jpeg,
            aim(100, 100),
            later(),
        )
        .unwrap_err();
        assert!(err.starts_with("compressImage: too-large: "), "{err}");
        // The fixture's frame header rewritten to claim 9000 × 9000 (JPEG
        // has no checksum): refused from the header, before decoding.
        let mut header = FIXTURE.to_vec();
        let sof = header.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        header[sof + 5..sof + 7].copy_from_slice(&9000u16.to_be_bytes());
        header[sof + 7..sof + 9].copy_from_slice(&9000u16.to_be_bytes());
        let err = compress(&header, Format::Jpeg, aim(100, 100), later()).unwrap_err();
        assert!(err.starts_with("compressImage: too-large: "), "{err}");
    }

    #[test]
    fn a_passed_deadline_starts_nothing() {
        let err =
            compress(FIXTURE, Format::Jpeg, aim(4000, 2_000_000), Instant::now()).unwrap_err();
        assert_eq!(err, "compressImage: timeout: the search ran past its time");
    }

    #[test]
    fn a_one_pixel_limit_makes_a_one_pixel_jpeg() {
        let out = compress(FIXTURE, Format::Jpeg, aim(1, 2_000_000), later()).unwrap();
        assert_eq!((out.width, out.height), (1, 1));
        let read = inspect(&out.bytes).unwrap();
        assert_eq!((read.width, read.height), (1, 1));
    }

    #[test]
    fn the_photo_fixtures_decode() {
        for bytes in [
            &include_bytes!("../../scripts/fixtures/picker/photo.heic")[..],
            &include_bytes!("../../scripts/fixtures/picker/photo.png")[..],
        ] {
            let out = compress(bytes, Format::Jpeg, aim(100, 2_000_000), later()).unwrap();
            assert!(out.width.max(out.height) <= 100);
            assert!(inspect(&out.bytes).unwrap().jpeg);
        }
    }
}

#[test]
fn codec_control_refuses_before_decoding_and_before_the_next_trial() {
    let refused = compress_controlled(
        b"not an image",
        Format::Jpeg,
        aim(64, 1000),
        later(),
        &|| Err(failure("timeout", "cancelled by its owner")),
        MAX_SOURCE_PIXELS,
        512 << 20,
    );
    assert_eq!(
        refused.unwrap_err(),
        "compressImage: timeout: cancelled by its owner"
    );
    #[cfg(target_vendor = "apple")]
    {
        let source = include_bytes!("../../scripts/fixtures/picker/oriented-gps.jpg");
        let checks = std::cell::Cell::new(0);
        let refused = compress_controlled(
            source,
            Format::Jpeg,
            aim(64, 1000),
            later(),
            &|| {
                checks.set(checks.get() + 1);
                if checks.get() >= 4 {
                    Err(failure("timeout", "cancelled between trials"))
                } else {
                    Ok(())
                }
            },
            MAX_SOURCE_PIXELS,
            512 << 20,
        );
        assert_eq!(
            refused.unwrap_err(),
            "compressImage: timeout: cancelled between trials"
        );
        let refused = compress_controlled(
            source,
            Format::Jpeg,
            aim(64, 1000),
            later(),
            &|| Ok(()),
            MAX_SOURCE_PIXELS,
            1,
        );
        assert!(refused.unwrap_err().contains("codec memory reservation"));
    }
}
