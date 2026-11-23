//! Replays the recorded device streams through this crate alone. @ref LLP 1172.023.006 D5.
//!
//! `streams/` holds each scene's `<scene>.stream` and two images of every frame the
//! stream reads back: the CPU painter's, `<scene>.<frame>.png`, and the recording
//! device's own, `<scene>.<frame>.ink.png` (both straight alpha). `SHA256SUMS` has the
//! SHA-256 of each file. A renderer carried elsewhere or changed here must still draw
//! each stream to its images. `host/painted/src/ink/stream_tests.rs` records them and
//! names the command.
#![cfg(not(target_arch = "wasm32"))]

mod stream;

use exact_ink::device::{Error, Selected};
use exact_ink::{Frame, Kind};
use sha2::{Digest, Sha256};
use stream::{Record, Stream};

const STREAMS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/streams");

/// The most a channel of a replayed pixel may differ from the recording device's.
///
/// Measured on an Apple M5 Ultra, 2026-11-23, replaying all 39 frames recorded on wgpu
/// over Metal: wgpu over Metal returned every pixel exactly, and the direct Metal
/// device differed on four frames of `images`, in 716 of their 6,300 pixels and by 2 at
/// most, where the two generate mips differently. The bound is four times that. A
/// rounding difference passes and a draw that is missing does not. Another GPU family
/// may round further apart; measure there before raising it.
const STEPS: u8 = 8;

fn read(name: &str) -> Vec<u8> {
    std::fs::read(format!("{STREAMS}/{name}")).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Each recorded file's name and SHA-256.
fn manifest() -> Vec<(String, String)> {
    let sums = String::from_utf8(read("SHA256SUMS")).unwrap();
    (sums.lines())
        .map(|line| {
            let (sha256, name) = line.split_once("  ").expect("a hash and a name");
            (name.to_owned(), sha256.to_owned())
        })
        .collect()
}

fn open() -> Option<Selected> {
    match Selected::from_env() {
        Ok(device) => Some(device),
        Err(Error::NoAdapter(e)) => {
            assert_ne!(std::env::var("EXACT_REQUIRED").as_deref(), Ok("1"), "{e}");
            eprintln!("Ink unavailable: {e}");
            None
        }
        Err(e) => panic!("ink device: {e}"),
    }
}

/// The summed channel differences between `pixels` and the image `name`, and how many
/// pixels have a channel more than `steps` apart.
fn apart(name: &str, pixels: &[u8], steps: u8) -> (u64, usize) {
    let image = tiny_skia::Pixmap::decode_png(&read(name)).unwrap();
    assert_eq!(pixels.len(), image.data().len(), "{name}: its size");
    let (mut sum, mut over) = (0, 0);
    for (a, b) in (image.data().as_chunks::<4>().0.iter()).zip(pixels.as_chunks::<4>().0) {
        let differences = [0, 1, 2, 3].map(|channel| a[channel].abs_diff(b[channel]));
        sum += differences.iter().map(|d| u64::from(*d)).sum::<u64>();
        over += usize::from(differences.iter().any(|d| *d > steps));
    }
    (sum, over)
}

/// A replayed frame against its two recorded images.
struct Replayed {
    frame: String,
    /// From the CPU painter's image: the mean channel difference, and the percentage
    /// of pixels with a channel more than 32 apart.
    mean: f64,
    beyond: f64,
    /// Pixels with a channel more than `STEPS` from the recording device's.
    apart: usize,
    /// Exactly the recording device's pixels.
    exact: bool,
}

impl Replayed {
    /// The band `host/painted` allows between Ink and the CPU painter (`compare` in
    /// its `ink/tests.rs`). It shows the renderer draws what another painter draws,
    /// and is too wide to miss a small draw.
    fn near_cpu(&self) -> bool {
        self.mean < 5. && self.beyond < 6.
    }
    fn near_ink(&self) -> bool {
        self.apart == 0
    }
}

impl std::fmt::Display for Replayed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (frame, mean, beyond, apart) = (&self.frame, self.mean, self.beyond, self.apart);
        write!(
            f,
            "{frame}: mean {mean:.3}/255 and {beyond:.3}% beyond 32 from the CPU "
        )?;
        write!(
            f,
            "painter's, {apart} pixels more than {STEPS} from the recording device's"
        )
    }
}

fn replay(device: &mut Selected, scene: &str, stream: &Stream) -> Vec<Replayed> {
    let frames = stream::replay(device, &stream.records).unwrap();
    (frames.iter().zip(stream.readbacks()).enumerate())
        .map(|(frame, (pixels, recorded))| {
            let frame = format!("{scene}.{frame}");
            let (sum, beyond) = apart(&format!("{frame}.png"), pixels, 32);
            Replayed {
                mean: sum as f64 / pixels.len() as f64,
                beyond: beyond as f64 * 400. / pixels.len() as f64,
                apart: apart(&format!("{frame}.ink.png"), pixels, STEPS).1,
                exact: Sha256::digest(pixels)[..] == recorded,
                frame,
            }
        })
        .collect()
}

/// The first frame of the stream `name`, with the instance `at` left out.
fn without_an_instance(name: &str, at: impl Fn(&Frame) -> usize) -> Stream {
    let mut stream = Stream::decode(&read(name));
    let frame = (stream.records.iter_mut())
        .find_map(|record| match record {
            Record::Render { frame, .. } => Some(frame),
            _ => None,
        })
        .unwrap();
    let at = at(frame);
    frame.instances.remove(at);
    for batch in &mut frame.batches {
        let after = |index: u32| index - u32::from(index as usize > at);
        batch.instances = after(batch.instances.start)..after(batch.instances.end);
    }
    stream
}

#[test]
fn every_file_is_the_one_recorded_and_every_stream_encodes_to_its_bytes() {
    let mut wrong = Vec::new();
    for (name, sha256) in manifest() {
        let bytes = read(&name);
        let actual: String = (Sha256::digest(&bytes).iter())
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if actual != sha256 {
            wrong.push(format!("{name} is not the file recorded"));
        } else if name.ends_with(".stream") && Stream::decode(&bytes).encode() != bytes {
            wrong.push(format!("{name} does not encode to its own bytes"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn every_stream_replays_to_its_images() {
    let mut outside = Vec::new();
    for (name, _) in manifest() {
        let Some(scene) = name.strip_suffix(".stream") else {
            continue;
        };
        // A device that has drawn nothing, as the recording's was.
        let Some(mut device) = open() else {
            return;
        };
        let stream = Stream::decode(&read(&name));
        let frames = replay(&mut device, scene, &stream);
        println!(
            "{scene}: {} frames on {}, {} of them exactly the pixels of {}",
            frames.len(),
            stream::device_name(&device),
            frames.iter().filter(|frame| frame.exact).count(),
            stream.device
        );
        let outside_either = |f: &&Replayed| !(f.near_cpu() && f.near_ink());
        outside.extend(
            frames
                .iter()
                .filter(outside_either)
                .map(Replayed::to_string),
        );
    }
    assert!(outside.is_empty(), "{outside:#?}");
}

/// The faults the replay has to catch: a frame that loses one instance, a large one
/// and then a small one that only the recording device's image shows.
#[test]
fn a_stream_with_one_instance_dropped_is_outside_its_band() {
    let Some(mut device) = open() else {
        return;
    };
    // The first draw of the first frame is the fill under its blended images.
    let fill = replay(
        &mut device,
        "blends",
        &without_an_instance("blends.stream", |_| 0),
    );
    println!("without the fill, {}", fill[0]);
    assert!(!fill[0].near_cpu() && !fill[0].near_ink());

    // One glyph of the 120 instances in the list's first frame.
    let glyph = |frame: &Frame| {
        let glyph = |i: &exact_ink::Instance| i.kind_brush & 65535 == Kind::Glyph as u32;
        frame.instances.iter().position(glyph).unwrap()
    };
    let glyph = replay(
        &mut device,
        "list",
        &without_an_instance("list.stream", glyph),
    );
    println!("without one glyph, {}", glyph[0]);
    assert!(glyph[0].near_cpu() && !glyph[0].near_ink());
    assert!(glyph[1..].iter().all(|f| f.near_cpu() && f.near_ink()));
}
