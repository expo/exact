//! Exercise the adapter's real pass boundaries with an ordered fake queue.
use super::*;
use exact_ink::device::{Submission, TextureDesc, TextureUpload};
use exact_ink::instance::Frame;
use std::collections::HashMap;
type Native = exact_ink::device::Selected;
type Buffer = <Native as Device>::Buffer;

pub enum Selected {
    Wgpu(Native),
    Other(Native),
    Fake(Fake),
}
impl From<Native> for Selected {
    fn from(device: Native) -> Self {
        if matches!(device, Native::Wgpu(_)) {
            Self::Wgpu(device)
        } else {
            Self::Other(device)
        }
    }
}
impl Selected {
    pub fn from_env() -> Result<Self> {
        Native::from_env().map(Into::into)
    }
    fn device(&self) -> &dyn Device<Buffer = Buffer> {
        match self {
            Self::Wgpu(d) | Self::Other(d) => d,
            Self::Fake(d) => d,
        }
    }
    fn device_mut(&mut self) -> &mut dyn Device<Buffer = Buffer> {
        match self {
            Self::Wgpu(d) | Self::Other(d) => d,
            Self::Fake(d) => d,
        }
    }
    #[cfg(target_os = "macos")]
    pub fn has_layer(&self) -> bool {
        match self {
            Self::Wgpu(d) | Self::Other(d) => d.has_layer(),
            Self::Fake(_) => false,
        }
    }
    #[cfg(target_os = "macos")]
    pub fn attach_layer(&mut self, layer: exact_ink::device::apple::Layer) -> Result<()> {
        match self {
            Self::Wgpu(d) | Self::Other(d) => d.attach_layer(layer),
            Self::Fake(_) => unreachable!(),
        }
    }
    #[cfg(target_os = "macos")]
    pub fn present(&mut self, texture: TextureId) -> Result<f64> {
        match self {
            Self::Wgpu(d) | Self::Other(d) => d.present(texture),
            Self::Fake(_) => unreachable!(),
        }
    }
}
macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)+) => {
        $(fn $name(&mut self, $($arg: $ty),*) -> $ret { self.device_mut().$name($($arg),*) })+
    };
}
impl Device for Selected {
    type Buffer = Buffer;
    fn info(&self) -> &Info {
        self.device().info()
    }
    fn max_texture_dimension(&self) -> u32 {
        self.device().max_texture_dimension()
    }
    fn create_buffer(&self, bytes: u64) -> Result<Buffer> {
        self.device().create_buffer(bytes)
    }
    fn last_use(&self, id: TextureId) -> Result<Submission> {
        self.device().last_use(id)
    }
    forward! {
        upload_buffer(buffer: &Buffer, offset: u64, data: &[u8]) -> Result<Submission>;
        create_texture(desc: TextureDesc) -> Result<TextureId>;
        upload_texture(id: TextureId, origin: [u32; 2], size: [u32; 2], data: &[u8]) -> Result<Submission>;
        upload_textures(uploads: &[TextureUpload]) -> Result<Submission>;
        generate_mips(id: TextureId) -> Result<Submission>;
        create_target(width: u32, height: u32) -> Result<TextureId>;
        resize_target(id: TextureId, width: u32, height: u32) -> Result<TextureId>;
        copy_region(source: TextureId, origin: [u32; 2], target: TextureId, dst: [u32; 2], size: [u32; 2]) -> Result<Submission>;
        render(frame: &Frame, target: TextureId, clear: Option<[f64; 4]>) -> Result<Submission>;
        readback(id: TextureId) -> Result<Vec<u8>>;
        wait(submission: Submission) -> Result<()>;
        completed() -> Result<Submission>;
        retire_texture(id: TextureId) -> Result<()>;
        trim() -> Result<()>;
    }
}

#[derive(Debug, PartialEq)]
enum Event {
    Upload(TextureId),
    Draw(Vec<TextureId>),
}
#[derive(Default)]
pub struct Fake {
    textures: HashMap<TextureId, Submission>,
    events: Vec<Event>,
    submitted: Submission,
    copies: Vec<([u32; 2], [u32; 2])>,
}
impl Device for Fake {
    type Buffer = Buffer;
    fn info(&self) -> &Info {
        unreachable!()
    }
    fn create_buffer(&self, _: u64) -> Result<Buffer> {
        unreachable!()
    }
    fn upload_buffer(&mut self, _: &Buffer, _: u64, _: &[u8]) -> Result<Submission> {
        unreachable!()
    }
    fn create_texture(&mut self, _: TextureDesc) -> Result<TextureId> {
        let id = TextureId(self.textures.len() as u64 + 1);
        self.textures.insert(id, Submission(0));
        Ok(id)
    }
    fn upload_texture(
        &mut self,
        id: TextureId,
        _: [u32; 2],
        _: [u32; 2],
        _: &[u8],
    ) -> Result<Submission> {
        self.submitted.0 += 1;
        self.textures.insert(id, self.submitted);
        self.events.push(Event::Upload(id));
        Ok(self.submitted)
    }
    fn last_use(&self, id: TextureId) -> Result<Submission> {
        Ok(self.textures[&id])
    }
    fn generate_mips(&mut self, _: TextureId) -> Result<Submission> {
        Ok(self.submitted)
    }
    fn resize_target(&mut self, id: TextureId, _: u32, _: u32) -> Result<TextureId> {
        Ok(id)
    }
    fn copy_region(
        &mut self,
        _: TextureId,
        origin: [u32; 2],
        _: TextureId,
        dst: [u32; 2],
        size: [u32; 2],
    ) -> Result<Submission> {
        assert_eq!(dst, [0, 0]);
        self.copies.push((origin, size));
        Ok(self.submitted)
    }
    fn render(&mut self, f: &Frame, target: TextureId, _: Option<[f64; 4]>) -> Result<Submission> {
        let mut sampled = Vec::new();
        for batch in &f.batches {
            for id in [batch.bindings.glyph, batch.bindings.color] {
                if id.0 != 0 {
                    assert!(
                        self.events.contains(&Event::Upload(id)),
                        "sampled before upload"
                    );
                    sampled.push(id);
                }
            }
        }
        self.events.push(Event::Draw(sampled));
        self.submitted.0 += 1;
        self.textures.insert(target, self.submitted);
        Ok(self.submitted)
    }
    fn readback(&mut self, _: TextureId) -> Result<Vec<u8>> {
        unreachable!()
    }
    fn wait(&mut self, _: Submission) -> Result<()> {
        panic!("the pass must not wait")
    }
    fn completed(&mut self) -> Result<Submission> {
        Ok(self.submitted)
    }
    fn retire_texture(&mut self, _: TextureId) -> Result<()> {
        Ok(())
    }
    fn trim(&mut self) -> Result<()> {
        Ok(())
    }
}

fn glyph(ink: &mut Ink, id: u16) -> TextureId {
    let font = exact_ink::glyph::Font {
        id: [1, 0],
        data: include_bytes!("../../../../scripts/fixtures/fonts/assets/DejaVuSans.ttf"),
        index: 0,
        coords: &[],
        skew: false,
    };
    let g = ink
        .glyphs
        .get(&mut ink.device, &font, id, 24., 0, 0.)
        .unwrap()
        .unwrap();
    let mut i = Instance::new(
        Kind::Glyph,
        Brush::Solid,
        [0., 0., g.size[0] as f32, g.size[1] as f32],
    );
    i.uv = g.entry.uv;
    ink.emit(
        i,
        Bindings {
            glyph: g.entry.texture,
            ..Default::default()
        },
    )
    .unwrap();
    g.entry.texture
}
fn checkpoint(ink: &Ink, before: usize, texture: TextureId) -> usize {
    let Selected::Fake(d) = &ink.device else {
        unreachable!()
    };
    let events = &d.events[before..];
    assert_eq!(events.first(), Some(&Event::Upload(texture)));
    assert_eq!(events.get(1), Some(&Event::Draw(vec![texture])));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Upload(_)))
            .count(),
        1
    );
    for e in &events[2..] {
        assert!(matches!(e, Event::Draw(ids) if ids.is_empty()), "{e:?}");
    }
    d.events.len()
}

#[test]
fn every_glyph_pass_flushes_before_sampling_including_masks_found_later() {
    let mut ink = Ink::from_device(Selected::Fake(Fake::default()));
    ink.begin(64., 64., 1.);
    ink.open_group(1.);
    let t = glyph(&mut ink, 36);
    ink.close_group().unwrap();
    let a = checkpoint(&ink, 0, t);
    ink.push_opacity(0.5);
    let t = glyph(&mut ink, 37);
    ink.pop_opacity();
    let b = checkpoint(&ink, a, t);
    let t = glyph(&mut ink, 38);
    ink.backdrop(
        &Shape::rect((0., 0., 32., 32.)),
        &exact_kernel::style::BackdropFilter(vec![exact_kernel::style::BackdropOp::Saturate(0.5)]),
        Transform::identity(),
    )
    .unwrap();
    let c = checkpoint(&ink, b, t);
    let t = glyph(&mut ink, 39);
    ink.flush().unwrap();
    let end = checkpoint(&ink, c, t);
    assert!(ink.error.is_none());
    println!("ordered glyph passes: intermediate group, opacity, backdrop, later masks; {end} queue events");
}

#[test]
fn small_effects_copy_their_region_in_a_large_viewport() {
    let mut ink = Ink::from_device(Selected::Fake(Fake::default()));
    ink.begin(1920., 1080., 1.);
    let pixels = Arc::new(Pixmap::new(1, 1).unwrap());
    ink.island_image(pixels, (120., 80., 20., 10.), Transform::identity(), 1);
    ink.backdrop_filter(
        &Shape::rect((300., 200., 30., 20.)),
        &exact_kernel::style::BackdropFilter(vec![exact_kernel::style::BackdropOp::Blur(4.)]),
        Transform::identity(),
    );
    assert!(ink.error.is_none(), "{:?}", ink.error);
    let Selected::Fake(d) = &ink.device else {
        unreachable!()
    };
    assert_eq!(d.copies.len(), 2);
    let area: u32 = d.copies.iter().map(|(_, size)| size[0] * size[1]).sum();
    println!("small effects: copied {area} pixels, {:?}", d.copies);
    assert!(area < 2000, "copied {area} pixels: {:?}", d.copies);
}

#[test]
fn destination_copies_follow_transforms_clips_and_filter_support() {
    use exact_kernel::style::{BackdropFilter, BackdropOp};
    let mut ink = Ink::from_device(Selected::Fake(Fake::default()));
    ink.begin(1920., 1080., 1.);
    let pixels = Arc::new(Pixmap::new(1, 1).unwrap());
    let clip = Shape::rect((250., 220., 15., 10.));
    ink.push_clip(&clip, Transform::identity());
    ink.island_image(
        pixels.clone(),
        (10., 20., 30., 40.),
        Transform::from_row(0., 1., -1., 0., 300., 200.),
        1,
    );
    ink.backdrop_filter(
        &Shape::rect((100., 100., 600., 400.)),
        &BackdropFilter(vec![BackdropOp::Blur(4.)]),
        Transform::identity(),
    );
    ink.backdrop_filter(
        &Shape::rect((100., 100., 600., 400.)),
        &BackdropFilter(vec![BackdropOp::Blur(f32::MAX)]),
        Transform::identity(),
    );
    ink.pop_clip();
    ink.push_clip(&Shape::rect((0., 0., 1., 1.)), Transform::identity());
    ink.island_image(pixels, (100., 100., 50., 50.), Transform::identity(), 1);
    ink.pop_clip();
    assert!(ink.error.is_none(), "{:?}", ink.error);
    let Selected::Fake(d) = &ink.device else {
        unreachable!()
    };
    assert_eq!(d.copies.len(), 3, "invisible effects need no snapshot");
    assert_eq!(d.copies[0], ([250, 220], [15, 10]));
    let (origin, size) = d.copies[1];
    assert!(
        origin[0] < 250 && origin[1] < 220,
        "blur samples beyond its clip"
    );
    assert!(origin[0] + size[0] > 265 && origin[1] + size[1] > 230);
    assert!(
        size[0] < 60 && size[1] < 60,
        "sampling support: {:?}",
        d.copies[1]
    );
    assert_eq!(d.copies[2], ([100, 100], [600, 400]));
}
