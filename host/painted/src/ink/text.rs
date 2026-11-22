use super::*;
use exact_ink::glyph::{subpixel, Font};

impl Ink {
    pub(super) fn glyph_runs(
        &mut self,
        text: &mut TextEngine,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        origin: (f32, f32),
        ts: Transform,
    ) -> Result<()> {
        let transform = self
            .device_transform(ts)
            .pre_scale(1. / self.scale, 1. / self.scale);
        // A retained row may replay at another scroll position. Blur and the
        // DirectWrite audit also have ink outside the Swash placement bounds.
        let clip = (self.rows.capture.is_empty()
            && self.groups.is_empty()
            && self.text_sigma == 0.
            && self.glyphs.swash_placements())
        .then(|| (transform, self.encoder.clip_bounds().rect()));
        for run in text.physical_glyph_runs(paragraph, palette, origin, self.scale, clip) {
            if run.paint.color[3] == 0 {
                continue;
            }
            let font = Font {
                id: [run.font.data.id(), u64::from(run.font.index)],
                data: run.font.data.data(),
                index: run.font.index,
                coords: &run.coords,
                skew: run.synthetic_italic,
            };
            for (id, x, y) in run.glyphs {
                self.observe_glyph(x, y, transform);
                let (x, phase) = subpixel(x);
                // Down, as the CPU raster places it and so a text stroke's
                // band: a whole-pixel move then moves a glyph above the
                // origin by the same pixels as one below it.
                let y = y.floor() as i32;
                let Some(glyph) = self.glyphs.get(
                    &mut self.device,
                    &font,
                    id as u16,
                    run.size * self.scale,
                    phase,
                    self.text_sigma * self.scale,
                )?
                else {
                    continue;
                };
                let mut i = Instance::new(
                    Kind::Glyph,
                    Brush::Solid,
                    [
                        (x + glyph.left) as f32,
                        (y - glyph.top) as f32,
                        glyph.size[0] as f32,
                        glyph.size[1] as f32,
                    ],
                );
                i.xform = affine(transform);
                i.uv = glyph.entry.uv;
                i.color = color(run.paint.color);
                let mut b = Bindings::default();
                if glyph.color {
                    i.flags |= flags::COLOR_GLYPH;
                    b.color = glyph.entry.texture;
                } else {
                    b.glyph = glyph.entry.texture;
                }
                self.emit(i, b)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn clipped_gpu_text_matches_replaying_the_complete_row() {
    let mut ink = match Ink::new() {
        Ok(ink) => ink,
        Err(e) => {
            assert_ne!(std::env::var("EXACT_REQUIRED").as_deref(), Ok("1"), "{e}");
            eprintln!("no Ink device: {e}");
            return;
        }
    };
    let mut engine = crate::text::fixtures::engine();
    let spec = crate::paint::text_spec(&Default::default(), &"abc אבג é W ".repeat(800));
    let p = engine.paragraph(&spec, None);
    let palette = [RunPaint {
        color: [0x30, 0x60, 0x90, 255],
        source: 1,
    }];
    for x in [0., -p.width * 0.5, 200. - p.width] {
        let origin = (x + 0.25, 5.);
        let draw = |ink: &mut Ink, engine: &mut TextEngine, record| {
            ink.begin(200., 40., 1.25);
            if record {
                ink.record_row(1, (0., 0.));
            }
            ink.glyph_runs(engine, &p, &palette, origin, Transform::identity())
                .unwrap();
            if record {
                let row = ink.keep_row();
                ink.draw_row(row, (0., 0.)).unwrap();
            }
            ink.finish().unwrap()
        };
        let selected: usize = engine
            .physical_glyph_runs(
                &p,
                &palette,
                origin,
                1.25,
                Some((Transform::identity(), [0., 0., 250., 50.])),
            )
            .iter()
            .map(|r| r.glyphs.len())
            .sum();
        assert!((1..=384).contains(&selected), "{selected} glyphs at {x}");
        let expected = draw(&mut ink, &mut engine, true);
        let actual = draw(&mut ink, &mut engine, false);
        assert!(
            actual.pixels().iter().any(|p| p.red() < 200),
            "no text at {x}"
        );
        assert!(actual.data() == expected.data(), "pixels differ at {x}");
    }
}
