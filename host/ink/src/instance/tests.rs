use super::*;

fn quad() -> Instance {
    Instance::rounded([0., 0., 10., 10.], [2.; 4], [1.; 4])
}
fn clip(x: f32) -> RoundedClip {
    RoundedClip {
        rect: [x, x, 20., 20.],
        radii: [2.031, 3.5, 4.25, 5.],
    }
}

#[test]
fn batches_preserve_order_and_break_only_for_scissor_or_bindings() {
    let mut e = Encoder::new(100, 100);
    let a = Bindings::default();
    let b = Bindings {
        image: TextureId(7),
        ..a
    };
    e.push(quad(), a).unwrap();
    e.push(quad(), a).unwrap();
    e.push_clip(Clip::Rounded(clip(3.))).unwrap();
    e.push(quad(), a).unwrap();
    e.push(quad(), b).unwrap();
    e.push_clip(Clip::Scissor(Scissor {
        x: 10,
        y: 20,
        width: 100,
        height: 15,
    }))
    .unwrap();
    e.push(quad(), b).unwrap();
    e.pop_clip();
    e.push(quad(), b).unwrap();
    let f = e.finish();
    assert_eq!(
        f.batches
            .iter()
            .map(|b| b.instances.clone())
            .collect::<Vec<_>>(),
        [0..3, 3..4, 4..5, 5..6]
    );
    assert_eq!(
        f.batches[2].scissor,
        Scissor {
            x: 10,
            y: 20,
            width: 90,
            height: 15
        }
    );
    assert_eq!(f.instances[2].clip0, clip(3.).rect);
    assert_eq!(f.instances[0].flags & flags::CLIP0, 0);
}

#[test]
fn clips_fill_slots_refuse_overflow_and_restore_after_pop() {
    let mut e = Encoder::new(100, 100);
    let mask = CoverageClip {
        texture: TextureId(0),
        rect: [1., 2., 30., 40.],
        uv: [0.1, 0.2, 0.3, 0.4],
    };
    e.push_clip(Clip::Rounded(clip(1.))).unwrap();
    e.push_clip(Clip::Rounded(clip(2.))).unwrap();
    e.push_clip(Clip::Coverage(mask)).unwrap();
    e.push(quad(), Bindings::default()).unwrap();
    for extra in [
        Clip::Rounded(clip(3.)),
        Clip::Coverage(mask),
        Clip::Transformed,
    ] {
        assert_eq!(e.push_clip(extra), Err(NeedsLayer));
        assert_eq!(e.push(quad(), Bindings::default()), Err(NeedsLayer));
        e.pop_clip();
    }
    e.pop_clip();
    e.pop_clip();
    e.push(quad(), Bindings::default()).unwrap();
    let f = e.finish();
    assert_eq!(f.instances.len(), 2);
    assert_eq!(f.instances[0].clip0, clip(2.).rect);
    assert_eq!(f.instances[0].clip1, clip(1.).rect);
    assert_eq!(f.instances[0].mask_rect, mask.rect);
    assert_eq!(f.instances[1].flags & (flags::CLIP1 | flags::MASK), 0);
    assert_eq!(
        f.instances[0].clip_radii,
        [
            32 | (56 << 16),
            68 | (80 << 16),
            32 | (56 << 16),
            68 | (80 << 16)
        ]
    );
}

#[test]
fn replay_preserves_inner_clips_and_refuses_a_third_without_emitting() {
    let mut e = Encoder::new(20, 20);
    let mut row = quad();
    row.set_clip(0, clip(1.)).unwrap();
    e.push_clip(Clip::Rounded(clip(2.))).unwrap();
    e.push(row, Bindings::default()).unwrap();
    e.push_clip(Clip::Rounded(clip(3.))).unwrap();
    assert_eq!(e.push(row, Bindings::default()), Err(NeedsLayer));
    let f = e.finish();
    assert_eq!(f.instances.len(), 1);
    assert_eq!(f.instances[0].clip0, row.clip0);
    assert_eq!(f.instances[0].clip1, clip(2.).rect);
    assert_eq!(pack_radii([5000.; 4]), Err(NeedsLayer));
}

#[test]
fn disjoint_scissors_emit_nothing_and_pop_restores_the_viewport() {
    let mut e = Encoder::new(10, 10);
    e.push_clip(Clip::Scissor(Scissor {
        x: 20,
        y: 0,
        width: 10,
        height: 10,
    }))
    .unwrap();
    e.push(quad(), Bindings::default()).unwrap();
    e.pop_clip();
    e.push(quad(), Bindings::default()).unwrap();
    assert_eq!(e.finish().instances.len(), 1);
}

#[test]
fn source_over_never_batches_with_replace_or_blend() {
    let mut encoder = Encoder::new(8, 8);
    for flags in [0, flags::REPLACE, 0, 1 << flags::BLEND_SHIFT, 0] {
        let mut i = Instance::rounded([0., 0., 8., 8.], [0.; 4], [1.; 4]);
        i.flags = flags;
        encoder
            .push(
                i,
                Bindings {
                    destination: TextureId(7),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let frame = encoder.finish();
    assert_eq!(frame.batches.len(), 5);
}

#[test]
fn snapshot_bounds_cover_transformed_quads_and_intersect_the_effective_clips() {
    let mut encoder = Encoder::new(1920, 1080);
    let mut image = Instance::new(Kind::RoundedRect, Brush::Image, [10., 20., 30., 40.]);
    image.xform = [0., 1., -1., 0., 300., 200.];
    assert_eq!(
        encoder.bounds(&image),
        Scissor::from_rect([239., 209., 42., 32.])
    );
    let scissor = Scissor::from_rect([250., 220., 15., 10.]);
    encoder.push_clip(Clip::Scissor(scissor)).unwrap();
    assert_eq!(encoder.bounds(&image), scissor);
    encoder.pop_clip();
    encoder
        .push_clip(Clip::Rounded(RoundedClip {
            rect: [250., 220., 15., 10.],
            radii: [2.; 4],
        }))
        .unwrap();
    assert_eq!(
        encoder.bounds(&image),
        Scissor::from_rect([248., 218., 19., 14.])
    );
    encoder
        .push_clip(Clip::Coverage(CoverageClip {
            texture: TextureId(1),
            rect: [252., 221., 8., 7.],
            uv: [0., 0., 1., 1.],
        }))
        .unwrap();
    assert_eq!(
        encoder.bounds(&image),
        Scissor::from_rect([252., 221., 8., 7.])
    );
    image = Instance::new(Kind::Coverage, Brush::Image, [254., 222., 3., 2.]);
    image.xform = [2., 1., 1., 3., 99., 17.];
    assert_eq!(encoder.bounds(&image), Scissor::from_rect(image.rect));
}

#[test]
fn draw_bounds_unite_clipped_draws_and_stop_at_the_limit() {
    let mut e = Encoder::new(100, 100);
    e.push(quad(), Bindings::default()).unwrap();
    e.push_clip(Clip::Scissor(Scissor {
        x: 50,
        y: 50,
        width: 10,
        height: 10,
    }))
    .unwrap();
    e.push(
        Instance::rounded([40., 40., 30., 30.], [0.; 4], [1.; 4]),
        Bindings::default(),
    )
    .unwrap();
    let frame = e.take_frame();
    let full = Scissor::full([100, 100]);
    let clipped = Scissor {
        x: 50,
        y: 50,
        width: 10,
        height: 10,
    };
    let quad_bounds = Scissor::from_rect(Encoder::device_rect(&quad()));
    assert_eq!(
        frame.draw_bounds(None, full),
        Some(quad_bounds.union(clipped))
    );
    // The walk stops once the bound covers the limit, before the clipped draw.
    assert_eq!(frame.draw_bounds(None, quad_bounds), Some(quad_bounds));

    // Every backend refuses a range past the instances; it adds nothing here.
    let mut refused = frame.clone();
    refused.batches[0].instances = 0..9;
    assert_eq!(refused.draw_bounds(None, full), Some(clipped));

    // A skewed draw's vertices are uncertain: it takes its batch's scissor.
    let mut e = Encoder::new(100, 100);
    let scissor = Scissor {
        x: 30,
        y: 30,
        width: 40,
        height: 40,
    };
    e.push_clip(Clip::Scissor(scissor)).unwrap();
    let mut skewed = Instance::rounded([40., 40., 10., 10.], [0.; 4], [1.; 4]);
    skewed.xform = [1., 0.5, 0., 1., 0., 0.];
    e.push(skewed, Bindings::default()).unwrap();
    assert_eq!(e.take_frame().draw_bounds(None, full), Some(scissor));
    // So does a draw too far out for the CPU and the shader to agree.
    e.push(
        Instance::rounded([70000., 40., 10., 10.], [0.; 4], [1.; 4]),
        Bindings::default(),
    )
    .unwrap();
    assert_eq!(e.take_frame().draw_bounds(None, full), Some(scissor));
    // A coverage quad is in device space: its brush transform does not move it.
    let mut coverage = Instance::new(Kind::Coverage, Brush::Solid, [40., 40., 10., 10.]);
    coverage.xform = [1., 0.5, 0., 1., 0., 0.];
    e.push(coverage, Bindings::default()).unwrap();
    assert_eq!(
        e.take_frame().draw_bounds(None, full),
        Some(Scissor::from_rect([40., 40., 10., 10.]))
    );

    let empty = Scissor {
        x: 200,
        y: 200,
        width: 0,
        height: 0,
    };
    assert_eq!(empty.union(clipped), clipped);
    assert_eq!(clipped.union(empty), clipped);
}
