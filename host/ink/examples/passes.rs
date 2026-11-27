//! Lists the render passes of recorded device streams. @ref LLP 1172.023.006 M2.
//!
//! ```text
//! cargo run -p exact-ink --example passes -- [--list] <stream or directory>...
//! ```
//!
//! A pass's cost on a tile-based GPU follows its target's size, so each pass is given
//! with its target's pixels beside the pixels its draws can write. What a pass is for
//! is read from the stream alone: the kinds it draws and how later calls of the frame
//! bind its target. Per stream it prints each class's passes and pixels a frame, the
//! targets made a frame and the bytes of targets held at a frame's end; `--list` adds
//! every pass of every frame. It opens no device.

// It reads streams and replays none.
#[allow(dead_code)]
#[path = "../tests/stream/mod.rs"]
mod stream;

use exact_ink::instance::{flags, Scissor, TextureId};
use exact_ink::{Frame, Kind};
use std::collections::{BTreeMap, HashMap};
use stream::{Record, Stream};

fn kind(instance: &exact_ink::Instance) -> u32 {
    instance.kind_brush & 65535
}

/// How the calls of a frame after `after` first sample `texture`.
fn sampled(frame: &[Record], after: usize, texture: TextureId) -> Option<&'static str> {
    for record in &frame[after + 1..] {
        let Record::Render { frame, .. } = record else {
            continue;
        };
        for batch in &frame.batches {
            let b = batch.bindings;
            let range = batch.instances.start as usize..batch.instances.end as usize;
            let first = &frame.instances[range][0];
            if b.mask == texture {
                return Some("mask");
            }
            if b.color == texture && first.flags & flags::LAYER_MASK != 0 {
                return Some("alpha");
            }
            if b.image == texture {
                return Some(if first.flags & flags::REPLACE != 0 {
                    "replace"
                } else if kind(first) == Kind::Layer as u32 {
                    "layer"
                } else {
                    "effect"
                });
            }
        }
    }
    None
}

/// What the pass at `at` of `frame` is for.
fn class(frame: &[Record], at: usize, drawn: &Frame, target: TextureId) -> &'static str {
    let has = |k: Kind| drawn.instances.iter().any(|i| kind(i) == k as u32);
    // A pass is cut for a read when a copy of its target follows it. A copy's own
    // target is made between the pass and the copy.
    let made = |r: &&Record| matches!(r, Record::CreateTarget { .. } | Record::ResizeTarget { .. });
    let next = frame[at + 1..].iter().find(|r| !made(r));
    let read = matches!(next, Some(Record::Copy { source, .. }) if *source == target);
    let main = matches!(frame.last(), Some(Record::Readback { texture, .. }) if *texture == target);
    let last = !(frame[at + 1..].iter())
        .any(|r| matches!(r, Record::Render { target: t, .. } if *t == target));
    if has(Kind::Blur) {
        "backdrop blur"
    } else if has(Kind::ColorPass) {
        "backdrop colour"
    } else if has(Kind::Downsample) {
        "backdrop reduce"
    } else if main && read {
        "window, cut for a read"
    } else if main && !last {
        "window, cut for no read"
    } else if main {
        "window"
    } else {
        match sampled(frame, at, target) {
            Some("mask") if drawn.batches.iter().any(|b| b.bindings.image.0 != 0) => {
                "clip mask, intersection"
            }
            Some("mask") => "clip mask",
            Some("alpha") => "alpha mask",
            Some("replace") | Some("effect") => "backdrop enlarge",
            Some("layer") if read => "layer, cut for a read",
            Some("layer") => "layer",
            _ => "unread",
        }
    }
}

fn bytes([width, height]: [u32; 2]) -> u64 {
    u64::from(width) * u64::from(height) * 4
}

#[derive(Default, Clone, Copy)]
struct Total {
    passes: usize,
    target: u64,
    drawn: u64,
    instances: usize,
}

fn main() {
    let (mut list, mut paths) = (false, Vec::new());
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--list" => list = true,
            path if std::path::Path::new(path).is_dir() => {
                let streams = std::fs::read_dir(path).unwrap().map(|f| f.unwrap().path());
                let mut streams: Vec<_> = streams
                    .filter(|file| file.extension().is_some_and(|e| e == "stream"))
                    .collect();
                streams.sort();
                paths.extend(streams);
            }
            path => paths.push(path.into()),
        }
    }
    for path in &paths {
        let stream = Stream::decode(&std::fs::read(path).expect("a stream"));
        let scene = path.file_stem().unwrap().to_string_lossy().into_owned();
        let is_readback = |record: &Record| matches!(record, Record::Readback { .. });
        let frames: Vec<_> = (stream.records.split_inclusive(is_readback))
            .filter(|f| f.last().is_some_and(is_readback))
            .collect();
        let mut sizes: HashMap<TextureId, [u32; 2]> = HashMap::new();
        let mut classes: BTreeMap<&str, Total> = BTreeMap::new();
        let (mut made, mut copies) = (0, 0);
        let mut held: HashMap<TextureId, u64> = HashMap::new();
        let (mut held_sum, mut held_most) = (0, 0);
        for (number, frame) in frames.iter().enumerate() {
            if list {
                println!("{scene}.{number}");
            }
            for (at, record) in frame.iter().enumerate() {
                match record {
                    Record::CreateTarget { id, size } => {
                        made += usize::from(number > 0);
                        sizes.insert(*id, *size);
                        held.insert(*id, bytes(*size));
                    }
                    Record::ResizeTarget { id, target, size } => {
                        made += usize::from(number > 0 && sizes.get(target) != Some(size));
                        sizes.insert(*id, *size);
                        held.remove(target);
                        held.insert(*id, bytes(*size));
                    }
                    Record::Retire(id) => {
                        held.remove(id);
                    }
                    Record::Copy { .. } => copies += 1,
                    Record::Render {
                        frame: drawn,
                        target,
                        ..
                    } => {
                        let size = sizes[target];
                        let bounds = drawn.draw_bounds(None, Scissor::full(size));
                        let bounds = bounds.unwrap_or(Scissor::full([0, 0]));
                        let class = class(frame, at, drawn, *target);
                        let total = classes.entry(class).or_default();
                        total.passes += 1;
                        total.target += u64::from(size[0]) * u64::from(size[1]);
                        total.drawn += u64::from(bounds.width) * u64::from(bounds.height);
                        total.instances += drawn.instances.len();
                        if list {
                            println!(
                                "  {class:<28} target {:>3} {:>4}x{:<4} draws {:>4}x{:<4} at {:>4},{:<4} {:>5} instances",
                                target.0,
                                size[0],
                                size[1],
                                bounds.width,
                                bounds.height,
                                bounds.x,
                                bounds.y,
                                drawn.instances.len()
                            );
                        }
                    }
                    _ => {}
                }
            }
            let now: u64 = held.values().sum();
            held_sum += now;
            held_most = now.max(held_most);
        }
        let count = frames.len() as f64;
        let all = classes.values().fold(Total::default(), |a, b| Total {
            passes: a.passes + b.passes,
            target: a.target + b.target,
            drawn: a.drawn + b.drawn,
            instances: a.instances + b.instances,
        });
        println!(
            "{scene}: {} frames; a frame: {:.2} passes, {:.0} target pixels, {:.0} drawn, {:.2} copies; {:.2} targets made a frame after the first; targets held at a frame's end: {:.0} bytes, at most {held_most}",
            frames.len(),
            all.passes as f64 / count,
            all.target as f64 / count,
            all.drawn as f64 / count,
            copies as f64 / count,
            made as f64 / (count - 1.).max(1.),
            held_sum as f64 / count
        );
        for (class, total) in &classes {
            println!(
                "  {class:<28} {:>6.2} passes {:>9.0} target pixels {:>9.0} drawn {:>7.1} instances",
                total.passes as f64 / count,
                total.target as f64 / count,
                total.drawn as f64 / count,
                total.instances as f64 / count
            );
        }
    }
}
