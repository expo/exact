//! Records the device calls the adapter makes for scenes the CPU painter also draws, and
//! checks that the calls alone redraw them. @ref LLP 1172.023.006 D5.
//!
//! `EXACT_INK_RECORD=1 cargo test -p exact-painted --lib ink::stream_tests` writes each
//! stream, the CPU painter's image and Ink's of every frame, and their SHA-256s to
//! `host/ink/tests/streams`, which `exact-ink` replays with no adapter. With
//! `EXACT_INK_RECORD_LARGE=<directory>` it writes the scenes too large to commit
//! there, and lists them in `host/ink/tests/streams.large`.
use super::*;
use crate::raster::Raster;
use exact_ink::device::{Submission, TextureDesc, TextureUpload};
use exact_ink::instance::Frame;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

#[path = "../../../ink/tests/stream/mod.rs"]
mod stream;
use stream::Record;

mod screens;

type Native = exact_ink::device::Selected;
type Buffer = <Native as Device>::Buffer;

pub struct Recorder {
    native: Native,
    records: Rc<RefCell<Vec<Record>>>,
    /// The stream's name for each of the device's textures.
    names: HashMap<TextureId, TextureId>,
    created: u64,
    submitted: Submission,
}

impl Recorder {
    fn name(&self, texture: TextureId) -> TextureId {
        if texture.0 == 0 {
            texture
        } else {
            self.names[&texture]
        }
    }
    fn created(&mut self, texture: TextureId) -> TextureId {
        self.created += 1;
        self.names.insert(texture, TextureId(self.created));
        TextureId(self.created)
    }
    fn record(&mut self, record: Record) {
        self.records.borrow_mut().push(record);
    }
    fn submit(&mut self, submission: Submission, record: Record) -> Result<Submission> {
        self.submitted = self.submitted.max(submission);
        self.record(record);
        Ok(submission)
    }
    fn named(
        &self,
        texture: TextureId,
        origin: [u32; 2],
        size: [u32; 2],
        data: &[u8],
    ) -> TextureUpload {
        TextureUpload {
            texture: self.name(texture),
            origin,
            size,
            data: data.to_vec(),
        }
    }
}

impl Device for Recorder {
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
    fn upload_buffer(&mut self, _: &Buffer, _: u64, _: &[u8]) -> Result<Submission> {
        unreachable!("the adapter uploads no buffer, and a stream has no record for one")
    }
    fn create_texture(&mut self, desc: TextureDesc) -> Result<TextureId> {
        let texture = self.native.create_texture(desc)?;
        let id = self.created(texture);
        self.record(Record::CreateTexture { id, desc });
        Ok(texture)
    }
    fn upload_texture(
        &mut self,
        texture: TextureId,
        origin: [u32; 2],
        size: [u32; 2],
        data: &[u8],
    ) -> Result<Submission> {
        let submission = self.native.upload_texture(texture, origin, size, data)?;
        let upload = self.named(texture, origin, size, data);
        self.submit(submission, Record::Upload(upload))
    }
    fn upload_textures(&mut self, uploads: &[TextureUpload]) -> Result<Submission> {
        let submission = self.native.upload_textures(uploads)?;
        let uploads = (uploads.iter())
            .map(|u| self.named(u.texture, u.origin, u.size, &u.data))
            .collect();
        self.submit(submission, Record::Uploads(uploads))
    }
    fn last_use(&self, texture: TextureId) -> Result<Submission> {
        self.native.last_use(texture)
    }
    fn generate_mips(&mut self, texture: TextureId) -> Result<Submission> {
        let submission = self.native.generate_mips(texture)?;
        self.submit(submission, Record::GenerateMips(self.name(texture)))
    }
    fn create_target(&mut self, width: u32, height: u32) -> Result<TextureId> {
        let texture = self.native.create_target(width, height)?;
        let id = self.created(texture);
        self.record(Record::CreateTarget {
            id,
            size: [width, height],
        });
        Ok(texture)
    }
    fn resize_target(&mut self, texture: TextureId, width: u32, height: u32) -> Result<TextureId> {
        let resized = self.native.resize_target(texture, width, height)?;
        let target = self.name(texture);
        let id = if resized == texture {
            target
        } else {
            self.names.remove(&texture);
            self.created(resized)
        };
        self.record(Record::ResizeTarget {
            id,
            target,
            size: [width, height],
        });
        Ok(resized)
    }
    fn copy_region(
        &mut self,
        source: TextureId,
        origin: [u32; 2],
        target: TextureId,
        dst: [u32; 2],
        size: [u32; 2],
    ) -> Result<Submission> {
        let submission = (self.native).copy_region(source, origin, target, dst, size)?;
        let (source, target) = (self.name(source), self.name(target));
        self.submit(
            submission,
            Record::Copy {
                source,
                origin,
                target,
                dst,
                size,
            },
        )
    }
    fn render(
        &mut self,
        frame: &Frame,
        target: TextureId,
        clear: Option<[f64; 4]>,
    ) -> Result<Submission> {
        let submission = self.native.render(frame, target, clear)?;
        let record = Record::Render {
            frame: stream::renamed(frame, |id| self.name(id)),
            target: self.name(target),
            clear,
        };
        self.submit(submission, record)
    }
    fn readback(&mut self, texture: TextureId) -> Result<Vec<u8>> {
        let pixels = self.native.readback(texture)?;
        self.record(Record::Readback {
            texture: self.name(texture),
            sha256: sha256(&pixels),
        });
        Ok(pixels)
    }
    fn wait(&mut self, submission: Submission) -> Result<()> {
        self.native.wait(submission)
    }
    /// Every submission, so that the atlas page and pooled target the adapter
    /// chooses next follow from the drawing and not from how far the GPU has got.
    fn completed(&mut self) -> Result<Submission> {
        self.native.wait(self.submitted)?;
        self.native.completed()
    }
    fn retire_texture(&mut self, texture: TextureId) -> Result<()> {
        self.native.retire_texture(texture)?;
        // The device may give its id to a later texture.
        let name = self.names.remove(&texture).expect("a live texture");
        // The adapter retires the pictures a frame left unused in a hash map's order,
        // which no two runs share. Retirements of textures with no call between them
        // are recorded in name order; any order of them draws the same.
        let mut records = self.records.borrow_mut();
        let made = |name: TextureId| {
            let made = |r: &Record| matches!(r, Record::CreateTexture { id, .. } if *id == name);
            records.iter().any(made)
        };
        let later = |r: &&Record| matches!(r, Record::Retire(n) if n.0 > name.0 && made(*n));
        let later = records.iter().rev().take_while(later).count();
        let at = records.len() - if made(name) { later } else { 0 };
        records.insert(at, Record::Retire(name));
        Ok(())
    }
    fn trim(&mut self) -> Result<()> {
        self.native.trim()?;
        self.record(Record::Trim);
        Ok(())
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn open() -> Option<Native> {
    match Native::from_env() {
        Ok(device) => Some(device),
        Err(Error::NoAdapter(e)) => {
            assert_ne!(std::env::var("EXACT_REQUIRED").as_deref(), Ok("1"), "{e}");
            eprintln!("Ink unavailable: {e}");
            None
        }
        Err(e) => panic!("ink device: {e}"),
    }
}

/// A scene draws its frames on Ink and on the CPU painter, and hands each frame's
/// pixels over: the CPU's, then Ink's.
type Scene = fn(Ink, &mut dyn FnMut(Pixmap, Pixmap));

fn clips(mut ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    let mut cpu = Raster::new();
    let identity = Transform::identity();
    let masks = [
        Shape::new((-2.75, 3.25, 54., 41.), [11., 7., 10., 12.]),
        Shape::new((2.375, 1.625, 50., 41.5), [9., 6., 12., 8.]),
        Shape::elliptical((0.125, 5.25, 49., 37.), [(13.5, 8.25); 4]),
        Shape::elliptical((3.5, 6.375, 44., 36.), [(11., 9.); 4]),
    ];
    let bounds = (0., 0., 68., 50.);
    for scale in [1., 1.25, 2.] {
        for nested in [false, true] {
            // Ink draws the second order into a row and replays the row.
            for (order, row) in [([2, 0, 1], false), ([0, 1, 2], true)] {
                for backend in [&mut cpu as &mut dyn Backend, &mut ink] {
                    let row = row && backend.rows();
                    backend.page([0; 4]);
                    backend.begin(bounds.2, bounds.3, scale);
                    if row {
                        backend.row_begin(1, (0., 0.), None);
                    }
                    for index in order {
                        backend.push_clip(&masks[index], identity);
                    }
                    if nested {
                        backend.push_clip(&masks[3], identity);
                    }
                    backend.fill(&Shape::rect(bounds), [210, 62, 97, 255], identity);
                    let shadow = Shape::new((12.25, 13.5, 19., 13.), [4.; 4]);
                    let color = [81, 101, 163, 128];
                    assert!(backend.blurred_shadow(
                        &shadow,
                        color,
                        3.,
                        &shadow.inset(1.),
                        identity
                    ));
                    for _ in 0..order.len() + usize::from(nested) {
                        backend.pop_clip();
                    }
                    if row {
                        backend.row_end(bounds);
                        backend.begin(bounds.2, bounds.3, scale);
                        backend.row_draw(1, (0., 0.), bounds);
                    }
                }
                frame(cpu.finish().unwrap(), ink.finish().unwrap());
                ink.row_free(1);
            }
        }
    }
}

fn layers(mut ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    let mut cpu = Raster::new();
    let identity = Transform::identity();
    let bounds = (0., 0., 32., 24.);
    let outer = |b: &mut dyn Backend| {
        b.push_clip(&Shape::new((2.25, 1.25, 34., 28.), [4.; 4]), identity);
        b.push_opacity(0.5);
        b.fill(&Shape::rect((0., 6., 14.25, 7.)), [255; 4], identity);
    };
    let inner = |b: &mut dyn Backend| {
        b.push_opacity(0.5);
        b.fill(&Shape::rect((27., 18., 8., 8.)), [255; 4], identity);
        b.pop_opacity();
        b.pop_opacity();
        b.pop_clip();
    };
    // Ink draws the first frame in two passes and the second from a row.
    for row in [false, true] {
        cpu.page([0; 4]);
        cpu.begin(bounds.2, bounds.3, 1.);
        outer(&mut cpu);
        inner(&mut cpu);
        ink.page([0; 4]);
        ink.begin(bounds.2, bounds.3, 1.);
        if row {
            ink.row_begin(1, (0., 0.), None);
        }
        outer(&mut ink);
        if !row {
            ink.flush().unwrap();
        }
        inner(&mut ink);
        if row {
            ink.row_end(bounds);
            ink.begin(bounds.2, bounds.3, 1.);
            ink.row_draw(1, (0., 0.), bounds);
        }
        frame(cpu.finish().unwrap(), ink.finish().unwrap());
    }
}

fn backdrop(mut ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    use exact_kernel::style::{BackdropFilter, BackdropOp};
    let mut cpu = Raster::new();
    let identity = Transform::identity();
    let skew = Transform::from_row(0.9, 0.2, -0.15, 1., 8., -7.);
    for (scale, ts, filter, clip) in [
        (1., identity, vec![BackdropOp::Saturate(0.4)], None),
        (
            1.,
            skew,
            vec![BackdropOp::Blur(4.), BackdropOp::Saturate(0.4)],
            None,
        ),
        (
            1.5,
            skew,
            vec![BackdropOp::Saturate(1.8), BackdropOp::Blur(13.)],
            Some((40., 32., 96., 72.)),
        ),
    ] {
        for backend in [&mut cpu as &mut dyn Backend, &mut ink] {
            backend.begin(160., 120., scale);
            backend.push_opacity(0.73);
            for n in 0..20 {
                backend.fill(
                    &Shape::rect((n as f32 * 8., 0., 8., 120.)),
                    [(n * 51) as u8, (n * 17) as u8, 190, 210],
                    identity,
                );
            }
            if let Some(clip) = clip {
                backend.push_clip(&Shape::rect(clip), identity);
            }
            backend.backdrop_filter(
                &Shape::new((5.25, -3.5, 150.5, 120.), [13.; 4]),
                &BackdropFilter(filter.clone()),
                ts,
            );
            if clip.is_some() {
                backend.pop_clip();
            }
            backend.pop_opacity();
        }
        frame(cpu.finish().unwrap(), ink.finish().unwrap());
    }
}

/// A list of text rows, scrolled: glyphs, and rows Ink keeps from frame to frame.
fn list(ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    let rows: String = (0..24)
        .map(|n| {
            let shade = if n % 2 == 0 { "#ffffff" } else { "#e8eef7" };
            format!(
                "        row height=36 padding=8 gap=8 background-color=\"{shade}\" border-bottom=\"1px solid #c0c8d4\"
          box width=20 height=20 border-radius=10 background-color=\"#2f6fde\"
          text \"Row {n}: the quick brown fox\" font-size=14
"
            )
        })
        .collect();
    let plan = crate::test_plan(&format!(
        "component App
  view
    scroll testId=\"port\" width=240 height=200
      column
{rows}"
    ));
    let mut app = screens::Pair::<()>::boot(&plan, (240., 200.), "", ink);
    for offset in [0., 37.5, 130.] {
        app.scroll(Some("port"), offset);
        app.frame(frame);
    }
}

/// The scene's stream, and the CPU painter's image of each frame with Ink's.
fn record(scene: Scene) -> Option<(stream::Stream, Vec<[Pixmap; 2]>)> {
    let native = open()?;
    let device = stream::device_name(&native);
    let records = Rc::new(RefCell::new(Vec::new()));
    let ink = Ink::from_device(Selected::Recorded(Box::new(Recorder {
        native,
        records: records.clone(),
        names: HashMap::new(),
        created: 0,
        submitted: Submission(0),
    })));
    let (mut images, mut drawn) = (Vec::new(), Vec::new());
    scene(ink, &mut |cpu, ink| {
        tests::compare(&cpu, &ink);
        drawn.push(sha256(ink.data()));
        images.push([cpu, ink]);
    });
    let records = records.take();
    let stream = stream::Stream { device, records };
    assert!(stream.readbacks().eq(drawn), "one readback for each frame");
    Some((stream, images))
}

/// Each file's SHA-256 and name, a line each, with its size where `sizes`.
fn sums(files: &BTreeMap<String, Vec<u8>>, sizes: bool) -> String {
    let line = |(name, bytes): (&String, &Vec<u8>)| {
        let hex: String = sha256(bytes).iter().map(|b| format!("{b:02x}")).collect();
        let size = if sizes {
            format!("  {}", bytes.len())
        } else {
            String::new()
        };
        format!("{hex}  {name}{size}\n")
    };
    files.iter().map(line).collect()
}

fn write(directory: &str, files: &BTreeMap<String, Vec<u8>>) {
    std::fs::create_dir_all(directory).unwrap();
    for (name, bytes) in files {
        std::fs::write(format!("{directory}/{name}"), bytes).unwrap();
    }
}

const STREAMS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../ink/tests/streams");

/// The head of `host/ink/tests/streams.large`, which lists the scenes kept outside
/// the repository: a stream holds each frame's instances whole, and the heavy list's
/// its decoded pictures.
const LARGE: &str = "# Recorded device streams too large to commit: each file's SHA-256, name and bytes.
# `tests/replay.rs` replays them from the directory EXACT_INK_STREAMS names. To record
# them again, at the commit that last changed this file:
#   bench/heavy-list/prepare.sh && EXACT_INK_RECORD_LARGE=<directory> cargo test -p exact-painted --lib ink::stream_tests
# The bytes are the recording device's, which each stream names, and the heavy list's
# pictures are Pillow 11.3.0's (bench/heavy-list/data.sha256).
";

/// Record `scenes` and give their files by name: each stream, and two images of each
/// of its frames. `wrong` gains each scene whose stream alone draws other pixels.
fn files(
    scenes: &[(&'static str, Scene)],
    wrong: &mut Vec<&'static str>,
) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for &(name, scene) in scenes {
        let (stream, images) = record(scene)?;
        // Replayed from the file's bytes, on a device that has drawn nothing.
        let bytes = stream.encode();
        let stream = stream::Stream::decode(&bytes);
        let mut device = open().unwrap();
        let replayed = stream::replay(&mut device, &stream.records).unwrap();
        let replayed = replayed.iter().map(|pixels| sha256(pixels));
        if !stream.readbacks().eq(replayed) {
            wrong.push(name);
        }
        let mut pictures = 0;
        for (at, [cpu, ink]) in images.iter().enumerate() {
            let (cpu, ink) = (cpu.encode_png().unwrap(), ink.encode_png().unwrap());
            pictures += cpu.len() + ink.len();
            files.insert(format!("{name}.{at}.png"), cpu);
            files.insert(format!("{name}.{at}.ink.png"), ink);
        }
        println!(
            "{name}: {} records, {} frames, {} bytes, and {pictures} of images",
            stream.records.len(),
            images.len(),
            bytes.len()
        );
        files.insert(format!("{name}.stream"), bytes);
    }
    Some(files)
}

#[test]
fn the_recorded_device_calls_redraw_each_scene() {
    let scenes: [(&str, Scene); 9] = [
        ("geometry", |mut ink, frame| {
            tests::geometry(&mut Raster::new(), &mut ink, frame)
        }),
        ("images", |mut ink, frame| {
            tests::images(&mut Raster::new(), &mut ink, frame)
        }),
        ("blends", |mut ink, frame| {
            tests::blends(&mut Raster::new(), &mut ink, frame)
        }),
        ("clips", clips),
        ("layers", layers),
        ("backdrop", backdrop),
        ("list", list),
        ("stations", screens::stations),
        ("sheet", screens::sheet),
    ];
    let mut wrong = Vec::new();
    let Some(committed) = files(&scenes, &mut wrong) else {
        return;
    };
    // Too large to commit. The heavy list's data is not committed either, so it is
    // drawn only to be recorded: `bench/heavy-list/prepare.sh` makes the data.
    let mut scenes: Vec<(&str, Scene)> = vec![("caltrain", screens::caltrain)];
    let outside = std::env::var("EXACT_INK_RECORD_LARGE").ok();
    if outside.is_some() {
        scenes.push(("heavy", screens::heavy));
    }
    let large = files(&scenes, &mut wrong).unwrap();
    assert!(
        wrong.is_empty(),
        "the calls alone draw other pixels: {wrong:?}"
    );
    if std::env::var_os("EXACT_INK_RECORD").is_some() {
        let _ = std::fs::remove_dir_all(STREAMS);
        write(STREAMS, &committed);
        std::fs::write(format!("{STREAMS}/SHA256SUMS"), sums(&committed, false)).unwrap();
    }
    if let Some(outside) = outside {
        write(&outside, &large);
        let listed = LARGE.to_owned() + &sums(&large, true);
        std::fs::write(format!("{STREAMS}.large"), listed).unwrap();
    }
}
