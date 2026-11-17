//! Resource ownership across a screenshot, a resizing canvas and a hidden window.
use super::*;
use exact_ink::device::{Submission, TextureDesc};
use exact_ink::instance::Frame;
use std::collections::HashMap;

type Native = exact_ink::device::Selected;
type Buffer = <Native as Device>::Buffer;

pub struct Counted {
    native: Native,
    textures: HashMap<TextureId, TextureDesc>,
    retired: Vec<(Submission, TextureDesc)>,
    created: usize,
    freed: usize,
}

fn bytes(d: TextureDesc) -> u64 {
    (0..d.levels())
        .map(|level| {
            u64::from((d.width >> level).max(1))
                * u64::from((d.height >> level).max(1))
                * u64::from(d.format.channels())
        })
        .sum()
}

impl Counted {
    fn open() -> Self {
        Self {
            native: Native::from_env().unwrap(),
            textures: HashMap::new(),
            retired: Vec::new(),
            created: 0,
            freed: 0,
        }
    }
    fn report(&mut self, phase: &str) -> u64 {
        self.completed().unwrap();
        let live: u64 = self.textures.values().copied().map(bytes).sum();
        let retiring: u64 = self.retired.iter().map(|(_, desc)| bytes(*desc)).sum();
        println!(
            "{phase}: owned={live}, retiring={retiring}, created={}, freed={}",
            self.created, self.freed
        );
        live + retiring
    }
}

macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)+) => {
        $(fn $name(&mut self, $($arg: $ty),*) -> $ret { self.native.$name($($arg),*) })+
    };
}
impl Device for Counted {
    type Buffer = Buffer;
    fn info(&self) -> &Info {
        self.native.info()
    }
    fn max_texture_dimension(&self) -> u32 {
        self.native.max_texture_dimension()
    }
    fn create_buffer(&self, bytes: u64) -> Result<Buffer> {
        self.native.create_buffer(bytes)
    }
    fn last_use(&self, id: TextureId) -> Result<Submission> {
        self.native.last_use(id)
    }
    fn create_texture(&mut self, desc: TextureDesc) -> Result<TextureId> {
        let id = self.native.create_texture(desc)?;
        self.textures.insert(id, desc);
        self.created += 1;
        Ok(id)
    }
    fn resize_target(&mut self, id: TextureId, width: u32, height: u32) -> Result<TextureId> {
        let desc = self.textures[&id];
        if [desc.width, desc.height] == [width, height] {
            return Ok(id);
        }
        let next = self.create_target(width, height)?;
        self.retire_texture(id)?;
        Ok(next)
    }
    fn retire_texture(&mut self, id: TextureId) -> Result<()> {
        let last = self.last_use(id)?;
        self.native.retire_texture(id)?;
        self.retired
            .push((last, self.textures.remove(&id).unwrap()));
        Ok(())
    }
    fn completed(&mut self) -> Result<Submission> {
        let done = self.native.completed()?;
        let before = self.retired.len();
        self.retired.retain(|(last, _)| *last > done);
        self.freed += before - self.retired.len();
        Ok(done)
    }
    forward! {
        upload_buffer(buffer: &Buffer, offset: u64, data: &[u8]) -> Result<Submission>;
        upload_texture(id: TextureId, origin: [u32; 2], size: [u32; 2], data: &[u8]) -> Result<Submission>;
        upload_textures(uploads: &[exact_ink::device::TextureUpload]) -> Result<Submission>;
        generate_mips(id: TextureId) -> Result<Submission>;
        copy_region(source: TextureId, origin: [u32; 2], target: TextureId, dst: [u32; 2], size: [u32; 2]) -> Result<Submission>;
        render(frame: &Frame, target: TextureId, clear: Option<[f64; 4]>) -> Result<Submission>;
        readback(id: TextureId) -> Result<Vec<u8>>;
        wait(submission: Submission) -> Result<()>;
        trim() -> Result<()>;
    }
}

fn report(ink: &mut Ink, phase: &str) -> u64 {
    assert!(ink.error.is_none(), "{:?}", ink.error);
    let Selected::Counted(d) = &mut ink.device else {
        unreachable!()
    };
    d.report(phase)
}

fn canvas(size: [u32; 2], draw: impl FnOnce(&exact_canvas::Context2d)) -> Arc<Pixmap> {
    let c = exact_canvas::Context2d::new();
    draw(&c);
    crate::canvas2d::replay(
        &c.take_lists(),
        (size[0] as f32, size[1] as f32),
        1.,
        &crate::canvas2d::Env {
            images: &Default::default(),
            text: None,
        },
    )
    .unwrap()
    .unwrap()
}

#[test]
fn visible_trim_preserves_the_compose_target_for_presentation() {
    let mut ink = Ink::from_device(Selected::Counted(Box::new(Counted::open())));
    ink.begin(64., 48., 1.);
    ink.push_opacity(0.5);
    ink.fill(
        &Shape::rect((0., 0., 64., 48.)),
        [40, 80, 120, 255],
        Transform::identity(),
    );
    ink.pop_opacity();
    let pixels = ink.finish().unwrap();
    let target = ink.target.unwrap();
    ink.trim(false);
    assert_eq!(
        ink.target,
        Some(target),
        "the visible frame can still repeat"
    );
    assert_eq!(report(&mut ink, "visible after trim"), 64 * 48 * 4);
    assert_eq!(ink.device.readback(target).unwrap(), pixels.data());
}

#[test]
fn large_canvases_release_drawing_storage_when_hidden() {
    let device = Counted::open();
    let mut ink = Ink::from_device(Selected::Counted(Box::new(device)));
    let desktop = canvas([6000, 1440], |c| {
        let mut pixels = c.create_image_data_with_sw_and_sh(6000., 1440.).unwrap();
        for p in pixels.data.as_chunks_mut::<4>().0 {
            *p = [24, 48, 96, 255];
        }
        c.put_image_data(&pixels, 0., 0.).unwrap();
    });
    let mut overlay = None;
    for frame in 0..16 {
        if frame > 0 {
            let size = [1000 + frame.min(10) * 90, 600 + frame.min(10) * 40];
            overlay = Some(canvas(size, |c| {
                c.clear_rect(0., 0., f64::from(size[0]), f64::from(size[1]));
                if frame > 10 {
                    c.set_stroke_style_str("#ff0000");
                    c.set_line_width(4.);
                    c.stroke_rect(10., 10., 1600., 300.);
                }
            }));
        }
        ink.begin(6000., 1440., 1.);
        ink.canvas(&desktop, (0., 0., 6000., 1440.), &[], Transform::identity());
        if let Some(p) = &overlay {
            ink.canvas(
                p,
                (0., 0., p.width() as f32, p.height() as f32),
                &[],
                Transform::identity(),
            );
        }
        let pixels = ink.finish().unwrap();
        assert_eq!(pixels.pixel(5900, 1300).unwrap().red(), 24);
        if frame > 10 {
            assert_eq!(pixels.pixel(10, 100).unwrap().red(), 255);
        }
        if frame == 0 {
            report(&mut ink, "shown");
        }
    }
    let scene = report(&mut ink, "drawn");
    ink.trim(true);
    let hidden = report(&mut ink, "hidden");
    assert_eq!(scene, (6000 * 1440 * 2 + 1900 * 1000) * 4);
    assert_eq!(hidden, 0, "hidden without another paint");
    // An app can keep its CPU capture while the hidden painter releases its copy.
    assert_eq!(desktop.pixel(5900, 1300).unwrap().red(), 24);
    ink.begin(6000., 1440., 1.);
    ink.fill(
        &Shape::rect((0., 0., 6000., 1440.)),
        [40, 80, 120, 255],
        Transform::identity(),
    );
    assert_eq!(ink.finish().unwrap().pixel(100, 100).unwrap().red(), 40);
}

#[test]
fn large_layers_keep_one_set_of_targets_across_submitted_frames() {
    let mut ink = Ink::from_device(Selected::Counted(Box::new(Counted::open())));
    for frame in 0..3 {
        ink.begin(6000., 1440., 1.);
        for layer in 0..6 {
            ink.push_opacity(0.5);
            ink.fill(
                &Shape::rect((0., 0., 6000., 1440.)),
                [frame * 40, layer * 30, 90, 255],
                Transform::identity(),
            );
            ink.pop_opacity();
        }
        ink.flush().unwrap();
        report(&mut ink, "six submitted layers");
    }
    let targets = ink.pool.retained();
    let pixels = ink.finish().unwrap();
    assert!((78..=85).contains(&pixels.pixel(100, 100).unwrap().red()));
    assert_eq!(report(&mut ink, "layers drawn"), 7 * 6000 * 1440 * 4);
    ink.trim(true);
    assert_eq!(report(&mut ink, "layers hidden"), 0);
    assert_eq!(targets, 6);
}
