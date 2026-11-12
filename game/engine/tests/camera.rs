use exact_game::*;

#[test]
fn image_shift_is_depth_and_aspect_independent_and_keeps_zero_records() {
    #[derive(Default, Data)]
    struct CenteredCamera {
        projection: Projection,
        fov_y_degrees: f32,
        near: f32,
        far: f32,
        active: bool,
    }
    for camera in [Camera::default(), Camera::orthographic(12.).integer_scale()] {
        let centered = CenteredCamera {
            projection: camera.projection,
            fov_y_degrees: camera.fov_y_degrees,
            near: camera.near,
            far: camera.far,
            active: camera.active,
        };
        assert_eq!(bin::to_vec(&camera), bin::to_vec(&centered));
        assert_eq!(hash::of(&camera), hash::of(&centered));
        let shifted = Camera {
            shift: Vec2::new(-0.2, 0.25),
            ..camera
        };
        assert_ne!(hash::of(&camera), hash::of(&shifted));
        for size in [Vec2::new(600., 300.), Vec2::new(300., 600.)] {
            let original = camera.matrix(size);
            if camera.projection == Projection::Perspective {
                assert_eq!(
                    original,
                    glam::camera::rh::proj::directx::perspective(
                        camera.fov_y_degrees.to_radians(),
                        size.x / size.y,
                        camera.near,
                        camera.far,
                    )
                );
            }
            for depth in [5., 10.] {
                let point = Vec3::new(0.5, -0.5, -depth);
                let expected = original.project_point3(point) + shifted.shift.extend(0.);
                assert!((shifted.matrix(size).project_point3(point) - expected).length() < 1e-6);
            }
        }
        let mut back: Camera = bin::from_slice(&bin::to_vec(&shifted)).unwrap();
        assert_eq!(back, shifted);
        json::read_into("{\"near\":0.25}", &mut back).unwrap();
        assert_eq!(
            back.shift, shifted.shift,
            "an overlay preserves omitted fields"
        );
        assert_eq!(back.near, 0.25);
        assert!(json::read_into("{\"near\":\"invalid\"}", &mut back).is_err());
        assert_eq!(back.shift, shifted.shift);
        bin::read_into(&bin::to_vec(&camera), &mut back).unwrap();
        assert_eq!(
            back, shifted,
            "binary record patches also preserve omitted fields"
        );
        assert_eq!(
            bin::from_slice::<Camera>(&bin::to_vec(&camera)).unwrap(),
            camera
        );
        assert!(!Camera {
            shift: Vec2::splat(f32::NAN),
            ..camera
        }
        .valid());
        assert!(!Camera {
            shift: Vec2::splat(f32::INFINITY),
            ..camera
        }
        .valid());
    }
}

#[derive(Default, Args)]
struct Settings {
    #[live]
    x: f32,
    #[live]
    y: f32,
    orthographic: bool,
}
struct Shifted;
impl Game for Shifted {
    const ID: &'static str = "camera-shift";
    type Args = Settings;
    fn setup(w: &mut World, a: &Settings) {
        let camera = if a.orthographic {
            Camera::orthographic(6.)
        } else {
            Camera::default()
        };
        w.spawn_named("camera", (Transform::default(), camera));
        w.spawn_named("near", (Transform::at(-1., 0., -5.), Mesh::cube(0.5)));
        w.spawn_named("far", (Transform::at(1., 0., -10.), Mesh::cube(0.5)));
    }
    fn tick(w: &mut World, _: &Input, a: &Settings) {
        w.require_mut::<Camera>("camera").shift = Vec2::new(a.x, a.y);
    }
}
#[test]
fn live_shifts_layout_pick_and_restore_without_moving_the_camera() {
    for orthographic in [false, true] {
        let mut sim = Sim::<Shifted>::new(Settings {
            orthographic,
            ..Default::default()
        })
        .unwrap();
        sim.viewport(600., 400.);
        let centered_save = sim.save().unwrap();
        let pose = *sim.world().require::<Transform>("camera");
        let initial = ["near", "far"].map(|n| sim.layout(n).unwrap().screen);
        sim.bind(
            &[
                Value::Number(0.3),
                Value::Number(0.25),
                Value::Bool(orthographic),
            ],
            None,
        )
        .unwrap();
        sim.run(20.);
        for (name, before) in ["near", "far"].into_iter().zip(initial) {
            let rect = sim.layout(name).unwrap().screen;
            assert!((rect.x - before.x - 90.).abs() < 0.001);
            assert!((rect.y - before.y + 50.).abs() < 0.001);
            assert_eq!(
                sim.pick(rect.center()).unwrap().entity,
                sim.world().named(name).unwrap()
            );
        }
        assert_eq!(*sim.world().require::<Transform>("camera"), pose);
        let saved = sim.save().unwrap();
        sim.bind(
            &[
                Value::Number(-0.2),
                Value::Number(-0.25),
                Value::Bool(orthographic),
            ],
            None,
        )
        .unwrap();
        sim.run(20.);
        sim.restore(&saved).unwrap();
        assert_eq!(
            sim.world().require::<Camera>("camera").shift,
            Vec2::new(0.3, 0.25)
        );
        assert_eq!(*sim.world().require::<Transform>("camera"), pose);
        sim.restore(&centered_save).unwrap();
        assert_eq!(sim.world().require::<Camera>("camera").shift, Vec2::ZERO);
        assert_eq!(*sim.world().require::<Transform>("camera"), pose);
    }
}
