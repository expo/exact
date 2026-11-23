//! Accumulate each shadow's coverage before blending its colour onto the page.
use super::*;

impl Raster {
    pub(super) fn gaussian_shadow(
        &mut self,
        shape: &Shape,
        color: [u8; 4],
        sigma: f32,
        outer: &Shape,
        ts: Transform,
    ) -> bool {
        let dev = self.device(ts);
        // A uniform scale or rotation keeps the Gaussian circular. For a skew or
        // unequal scale, transform the contours of the shadow in the box's space.
        if !(dev.sx == dev.sy && dev.kx == -dev.ky || dev.sx == -dev.sy && dev.kx == dev.ky) {
            return false;
        }
        let sigma = sigma * dev.sx.hypot(dev.ky);
        let pad = (3. * sigma).ceil() + 2.;
        if sigma <= 0. || !pad.is_finite() || pad > self.width.max(self.height) as f32 {
            return false;
        }
        let (Some(source), Some(knockout)) = (rounded_rect(shape), rounded_rect(outer)) else {
            return false;
        };
        let Some(extent) = source
            .bounds()
            .transform(dev)
            .and_then(|r| r.outset(pad, pad))
        else {
            return false;
        };
        let viewport = Rect::from_xywh(0., 0., self.width as f32, self.height as f32).unwrap();
        if extent.intersect(&viewport).is_none() {
            return true;
        }
        // Keep the source outside the frame when its blur can reach a visible pixel.
        let Some(bounds) = viewport
            .outset(pad, pad)
            .and_then(|r| r.intersect(&extent))
            .and_then(|r| r.round_out())
        else {
            return false;
        };
        let (w, h) = (bounds.width(), bounds.height());
        if w > MAX_FRAME_SIDE || h > MAX_FRAME_SIDE {
            return false;
        }
        let (Some(mut pixels), Some(mut inside)) = (Pixmap::new(w, h), Mask::new(w, h)) else {
            return false;
        };
        let local = dev.post_translate(-(bounds.x() as f32), -(bounds.y() as f32));
        let mut solid = Paint::default();
        solid.set_color(Color::WHITE);
        pixels.fill_path(&source, &solid, FillRule::Winding, local, None);
        exact_svg_raster::backdrop_blur(pixels.data_mut(), w as usize, h as usize, sigma);
        inside.fill_path(&knockout, FillRule::Winding, true, local);
        let [r, g, b, opacity] = color;
        for (pixel, cut) in pixels.pixels_mut().iter_mut().zip(inside.data()) {
            let alpha = u32::from(pixel.alpha()) * u32::from(opacity) * u32::from(255 - cut);
            *pixel = tiny_skia::ColorU8::from_rgba(r, g, b, ((alpha + 32512) / 65025) as u8)
                .premultiply();
        }
        self.place_shadow(bounds, &pixels);
        true
    }

    pub(super) fn composite_bands(&mut self, mut bands: Vec<BorderFill>, ts: Transform) {
        let Some(first) = bands.first() else { return };
        if bands.len() == 1 {
            self.fill_border(first, ts);
            return;
        }

        // Subsequent contours lie inside the first. Limit the temporary mask
        // to the visible part, including partial device pixels at its edges.
        let dev = self.device(ts);
        let Some(bounds) = visible_band(first, dev, self.width, self.height) else {
            return;
        };
        let mut coverage = Raster::transparent();
        coverage.begin(bounds.width() as f32, bounds.height() as f32, 1.);
        let local = dev.post_translate(-(bounds.x() as f32), -(bounds.y() as f32));
        let [r, g, b, _] = first.color;
        for band in &mut bands {
            band.color[..3].fill(0);
            coverage.fill_border(band, local);
        }
        let Ok(mut pixels) = coverage.finish() else {
            return;
        };
        for pixel in pixels.pixels_mut() {
            *pixel = tiny_skia::ColorU8::from_rgba(r, g, b, pixel.alpha()).premultiply();
        }
        self.place_shadow(bounds, &pixels);
    }

    fn place_shadow(&mut self, bounds: tiny_skia::IntRect, pixels: &Pixmap) {
        let mask = self.active_clip().cloned();
        if let Some(target) = self
            .target
            .bounded_pixels(bounds.to_rect(), Transform::identity())
        {
            target.draw_pixmap(
                bounds.x(),
                bounds.y(),
                pixels.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                mask.as_deref(),
            );
        }
    }
}

fn visible_band(
    band: &BorderFill,
    dev: Transform,
    width: u32,
    height: u32,
) -> Option<tiny_skia::IntRect> {
    let mut bounds = tiny_path(&band.region)?.bounds();
    if let Some(clip) = &band.clip {
        bounds = bounds.intersect(&tiny_path(clip)?.bounds())?;
    }
    let viewport = Rect::from_xywh(0., 0., width as f32, height as f32)?;
    bounds.transform(dev)?.intersect(&viewport)?.round_out()
}
