use super::*;

fn encode(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, width, height);
        e.set_color(color);
        e.set_depth(depth);
        e.add_text_chunk("Comment".into(), "private metadata".into())
            .unwrap();
        let mut w = e.write_header().unwrap();
        w.write_image_data(data).unwrap();
        w.finish().unwrap();
    }
    out
}

#[test]
fn dib_header_stride_row_order_and_alpha_match_known_bytes() {
    let rgba = [
        255, 0, 0, 255, 200, 100, 50, 128, 90, 80, 70, 0, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255,
        255, 1,
    ];
    let dib = dib_v5(3, 2, &rgba).unwrap();
    assert_eq!(dib.len(), 124 + 24);
    assert_eq!(
        &dib[..24],
        &[124, 0, 0, 0, 3, 0, 0, 0, 2, 0, 0, 0, 1, 0, 32, 0, 3, 0, 0, 0, 24, 0, 0, 0,]
    );
    assert!(dib[24..40].iter().all(|b| *b == 0));
    assert_eq!(
        &dib[40..60],
        &[0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 66, 71, 82, 115,]
    );
    assert!(dib[60..108].iter().all(|b| *b == 0));
    assert_eq!(
        &dib[108..124],
        &[4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        &dib[124..],
        &[
            0, 255, 0, 255, 255, 0, 0, 255, 1, 1, 1, 1, 0, 0, 255, 255, 25, 50, 100, 128, 0, 0, 0,
            0,
        ]
    );
    assert!(dib_v5(3, 2, &rgba[..20]).is_err());
}

#[test]
fn sanitization_keeps_straight_alpha_in_png_and_removes_metadata() {
    let rgba = [200, 100, 50, 128, 90, 80, 70, 0];
    let input = encode(2, 1, png::ColorType::Rgba, png::BitDepth::Eight, &rgba);
    let image = Image::decode(&input).unwrap();
    assert_eq!(image.dimensions(), (2, 1));
    let mut reader = png::Decoder::new(Cursor::new(&image.png))
        .read_info()
        .unwrap();
    let mut pixels = [0; 8];
    reader.next_frame(&mut pixels).unwrap();
    reader.finish().unwrap();
    assert_eq!(pixels, rgba);
    assert!(reader.info().uncompressed_latin1_text.is_empty());
    assert!(reader.info().animation_control.is_none());
    assert_eq!(
        &image.dib.as_ref().unwrap()[124..],
        &[25, 50, 100, 128, 0, 0, 0, 0]
    );
}

#[test]
fn grayscale_rgb_and_sixteen_bit_pngs_are_expanded() {
    for (color, depth, data, bgra) in [
        (
            png::ColorType::Grayscale,
            png::BitDepth::One,
            vec![0b10000000],
            [255, 255, 255, 255],
        ),
        (
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            vec![200, 128],
            [100, 100, 100, 128],
        ),
        (
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            vec![7, 9, 11],
            [11, 9, 7, 255],
        ),
        (
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            vec![0xff, 0xff, 0, 0, 0, 0, 0xff, 0xff],
            [0, 0, 255, 255],
        ),
    ] {
        let image = Image::decode(&encode(1, 1, color, depth, &data)).unwrap();
        assert_eq!(&image.dib.as_ref().unwrap()[124..], &bgra);
    }
    let mut palette = Vec::new();
    {
        let mut e = png::Encoder::new(&mut palette, 1, 1);
        e.set_color(png::ColorType::Indexed);
        e.set_palette(vec![200, 100, 50]);
        e.set_trns(vec![128]);
        e.write_header().unwrap().write_image_data(&[0]).unwrap();
    }
    assert_eq!(
        &Image::decode(&palette).unwrap().dib.as_ref().unwrap()[124..],
        &[25, 50, 100, 128]
    );
}

#[test]
fn apng_uses_first_animation_frame_including_offset_and_separate_default() {
    for separate in [false, true] {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 2, 1);
            e.set_color(png::ColorType::Rgba);
            e.set_animated(2, 0).unwrap();
            e.set_sep_def_img(separate).unwrap();
            let mut w = e.write_header().unwrap();
            if separate {
                w.write_image_data(&[255, 0, 0, 255, 255, 0, 0, 255])
                    .unwrap();
                w.set_frame_dimension(1, 1).unwrap();
                w.set_frame_position(1, 0).unwrap();
                w.write_image_data(&[0, 255, 0, 255]).unwrap();
            } else {
                w.write_image_data(&[0, 255, 0, 255, 0, 255, 0, 255])
                    .unwrap();
            }
            w.reset_frame_position().unwrap();
            w.set_frame_dimension(2, 1).unwrap();
            w.write_image_data(&[0, 0, 255, 255, 0, 0, 255, 255])
                .unwrap();
            w.finish().unwrap();
        }
        let image = Image::decode(&out).unwrap();
        assert_eq!(&image.dib.as_ref().unwrap()[128..], &[0, 255, 0, 255]);
        assert_eq!(
            &image.dib.as_ref().unwrap()[124..128],
            if separate {
                &[0, 0, 0, 0]
            } else {
                &[0, 255, 0, 255]
            }
        );
    }
}

#[test]
fn malformed_png_and_dimensions_are_refused_before_allocating_pixels() {
    assert_eq!(pixel_bytes(8000, 8000).unwrap(), 256_000_000);
    for (w, h) in [
        (0, 1),
        (1, 0),
        (u32::MAX, u32::MAX),
        (9000, 9000),
        (i32::MAX as u32, 1),
    ] {
        assert!(pixel_bytes(w, h).is_err(), "{w}x{h}");
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend(w.to_be_bytes());
        header.extend(h.to_be_bytes());
        assert!(Image::decode(&header).is_err());
    }
    let over_limit = encode(
        9000,
        9000,
        png::ColorType::Grayscale,
        png::BitDepth::One,
        &vec![0; 1125 * 9000],
    );
    assert!(over_limit.len() < 64 * 1024);
    assert_eq!(
        Image::decode(&over_limit).unwrap_err(),
        "too large: 9000×9000 is over 64,000,000 pixels"
    );
    for bytes in [b"JPEG".as_slice(), b"\x89PNG\r\n\x1a\n"] {
        assert!(Image::decode(bytes).is_err());
    }
    let mut bytes = encode(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    bytes.truncate(bytes.len() / 2);
    assert!(Image::decode(&bytes).is_err());
    assert!(Image::decode(&vec![0; MOST_FILE + 1])
        .unwrap_err()
        .contains("64 MiB"));
}

#[test]
fn unicode_text_has_windows_line_endings_and_one_terminator() {
    assert_eq!(
        super::super::text_bytes("a\rb\nc\r\n🦀"),
        vec![
            97, 0, 13, 0, 10, 0, 98, 0, 13, 0, 10, 0, 99, 0, 13, 0, 10, 0, 62, 216, 128, 221, 0, 0,
        ]
    );
}

#[test]
fn narrow_interlaced_apng_keeps_both_rows_at_the_canvas_stride() {
    // 4×2 Adam7 canvas, separate red default, 2×2 green/blue frame at (1, 0).
    let source = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 2, 8, 6,
        0, 0, 1, 8, 175, 77, 245, 0, 0, 0, 8, 97, 99, 84, 76, 0, 0, 0, 1, 0, 0, 0, 0, 180, 45, 233,
        160, 0, 0, 0, 19, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 159, 1, 78, 160, 48,
        96, 24, 0, 21, 29, 15, 241, 219, 89, 16, 96, 0, 0, 0, 26, 102, 99, 84, 76, 0, 0, 0, 0, 0,
        0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 10, 0, 0, 41, 218, 3, 192, 0, 0, 0,
        23, 102, 100, 65, 84, 0, 0, 0, 1, 120, 156, 99, 96, 248, 15, 132, 16, 130, 225, 63, 136,
        245, 31, 0, 67, 207, 7, 249, 1, 160, 178, 75, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let image = Image::decode(&source).unwrap();
    let mut reader = png::Decoder::new(Cursor::new(&image.png))
        .read_info()
        .unwrap();
    let mut pixels = [0; 32];
    reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        pixels,
        [
            0, 0, 0, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0,
            0, 255, 255, 0, 0, 0, 0
        ]
    );
}

#[test]
fn color_information_survives_sanitization_without_a_mislabeled_dib() {
    for kind in ["gamma", "primaries", "icc"] {
        let mut info = png::Info::with_size(1, 1);
        info.color_type = png::ColorType::Rgba;
        info.bit_depth = png::BitDepth::Eight;
        match kind {
            "gamma" => info.source_gamma = Some(png::ScaledFloat::new(1.0)),
            "primaries" => {
                info.source_chromaticities = Some(png::SourceChromaticities::new(
                    (0.3127, 0.3290),
                    (0.68, 0.32),
                    (0.265, 0.69),
                    (0.15, 0.06),
                ))
            }
            _ => info.icc_profile = Some(std::borrow::Cow::Owned(vec![0; 128])),
        }
        let mut input = Vec::new();
        png::Encoder::with_info(&mut input, info.clone())
            .unwrap()
            .write_header()
            .unwrap()
            .write_image_data(&[128, 128, 128, 255])
            .unwrap();
        let image = Image::decode(&input).unwrap();
        let mut reader = png::Decoder::new(Cursor::new(&image.png))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().gama_chunk, info.source_gamma, "{kind}");
        assert_eq!(
            reader.info().chrm_chunk,
            info.source_chromaticities,
            "{kind}"
        );
        assert_eq!(reader.info().icc_profile, info.icc_profile, "{kind}");
        assert!(reader.info().srgb.is_none());
        let mut pixel = [0; 4];
        reader.next_frame(&mut pixel).unwrap();
        assert_eq!(pixel, [128, 128, 128, 255]);
        let item = super::super::Item {
            image: Some(image),
            text: None,
        };
        assert_eq!(
            item.description(true)["platformTypes"],
            serde_json::json!(["PNG"])
        );
    }
}

#[test]
fn icc_takes_precedence_over_srgb_without_losing_the_profile() {
    let profile = vec![0; 128];
    let mut info = png::Info::with_size(1, 1);
    info.color_type = png::ColorType::Rgba;
    info.bit_depth = png::BitDepth::Eight;
    info.icc_profile = Some(std::borrow::Cow::Borrowed(&profile));
    let mut input = Vec::new();
    {
        let mut writer = png::Encoder::with_info(&mut input, info)
            .unwrap()
            .write_header()
            .unwrap();
        // The encoder omits iCCP if Info has sRGB, so write the second chunk directly.
        writer.write_chunk(png::chunk::sRGB, &[0]).unwrap();
        writer.write_image_data(&[128, 64, 32, 255]).unwrap();
        writer.finish().unwrap();
    }
    let source = png::Decoder::new(Cursor::new(&input)).read_info().unwrap();
    assert!(source.info().srgb.is_some());
    assert_eq!(
        source.info().icc_profile.as_deref(),
        Some(profile.as_slice())
    );
    let image = Image::decode(&input).unwrap();
    assert!(image.dib.is_none());
    let mut reader = png::Decoder::new(Cursor::new(&image.png))
        .read_info()
        .unwrap();
    assert!(reader.info().srgb.is_none());
    assert_eq!(
        reader.info().icc_profile.as_deref(),
        Some(profile.as_slice())
    );
    let mut pixel = [0; 4];
    reader.next_frame(&mut pixel).unwrap();
    assert_eq!(pixel, [128, 64, 32, 255]);
}

#[test]
fn srgb_cicp_normalizes_to_srgb_png_and_bitmap() {
    for extra in ["none", "srgb", "icc", "gamma"] {
        let mut info = png::Info::with_size(1, 1);
        info.color_type = png::ColorType::Rgba;
        info.bit_depth = png::BitDepth::Eight;
        match extra {
            "srgb" => info.srgb = Some(png::SrgbRenderingIntent::Perceptual),
            "icc" => info.icc_profile = Some(std::borrow::Cow::Owned(vec![0; 128])),
            "gamma" => info.source_gamma = Some(png::ScaledFloat::new(1.0)),
            _ => {}
        }
        let mut input = Vec::new();
        {
            let mut writer = png::Encoder::with_info(&mut input, info)
                .unwrap()
                .write_header()
                .unwrap();
            writer
                .write_chunk(png::chunk::cICP, &[1, 13, 0, 1])
                .unwrap();
            writer.write_image_data(&[200, 100, 50, 128]).unwrap();
            writer.finish().unwrap();
        }
        let image = Image::decode(&input).unwrap_or_else(|error| panic!("{extra}: {error}"));
        let mut reader = png::Decoder::new(Cursor::new(&image.png))
            .read_info()
            .unwrap();
        assert!(reader.info().srgb.is_some(), "{extra}");
        assert!(reader.info().coding_independent_code_points.is_none());
        assert!(reader.info().icc_profile.is_none());
        let mut pixel = [0; 4];
        reader.next_frame(&mut pixel).unwrap();
        assert_eq!(pixel, [200, 100, 50, 128]);
        assert_eq!(&image.dib.as_ref().unwrap()[124..], &[25, 50, 100, 128]);
        let item = super::super::Item {
            image: Some(image),
            text: None,
        };
        assert_eq!(
            item.description(true)["platformTypes"],
            serde_json::json!(["PNG", "CF_DIBV5"])
        );
    }
}

#[test]
fn unsupported_cicp_and_hdr_metadata_are_refused() {
    for (cicp, hdr) in [
        ([9, 13, 0, 1], None),
        ([1, 16, 0, 1], None),
        ([1, 13, 1, 1], None),
        ([1, 13, 0, 0], None),
        ([1, 13, 0, 1], Some((png::chunk::mDCV, vec![0; 24]))),
        ([1, 13, 0, 1], Some((png::chunk::cLLI, vec![0; 8]))),
    ] {
        let mut input = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut input, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            let mut writer = encoder.write_header().unwrap();
            writer.write_chunk(png::chunk::cICP, &cicp).unwrap();
            if let Some((kind, bytes)) = hdr {
                writer.write_chunk(kind, &bytes).unwrap();
            }
            writer.write_image_data(&[200, 100, 50, 128]).unwrap();
            writer.finish().unwrap();
        }
        assert!(Image::decode(&input).is_err(), "{cicp:?}");
    }
}
