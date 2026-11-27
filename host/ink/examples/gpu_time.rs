//! Times the renderer alone on recorded device streams. @ref LLP 1172.023.006 M2.
//!
//! ```text
//! cargo run --release -p exact-ink --example gpu_time -- \
//!     [--replays N] [--warm N] [--back-to-back] [--json FILE] <stream or directory>...
//! ```
//!
//! Each stream is replayed `--warm` times unmeasured and `--replays` times measured, on
//! the device `EXACT_INK_DEVICE` (and `WGPU_BACKEND`) selects. A frame is the calls up
//! to a readback. Its CPU time is the wall time the calling thread spent inside those
//! calls and in ending the frame's span, where the wgpu device submits what the calls
//! encoded: it leaves out a driver's own threads, and includes a call's wait for the
//! GPU. Its GPU time is the device's (`Device::gpu_times`): the span from the start of
//! its first render pass to the end of its last, and the sum of its passes. Its target
//! pixels are those of each pass's target, whatever part of it the pass draws. With
//! `EXACT_INK_COUNTERS` set, its submits are the wgpu device's queue submits in that
//! time.
//!
//! The replay is offscreen, so it measures drawing and not presentation: no frame is
//! copied to a swap chain or a drawable, composed, or paced to a display. Every replay
//! begins with no textures, so a stream's first frame uploads its atlases and images,
//! and each frame makes its own target; `by_frame` separates them. A frame is read
//! back once its times are taken, which waits for the GPU, so no frame's work overlaps
//! the next's and the GPU idles while the next is encoded. `--back-to-back` reads no
//! frame back: the GPU has the next frame before it finishes this one, as far ahead
//! as the device lets a caller run, and a frame's CPU time includes that wait.

// The timer reads streams and writes none.
#[allow(dead_code)]
#[path = "../tests/stream/mod.rs"]
mod stream;

use exact_ink::counters::{self, Counter};
use exact_ink::device::{Device, Result, Selected};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use stream::{Record, Replay, Stream};

/// One frame's times over the measured replays, in milliseconds.
#[derive(Default)]
struct Samples {
    cpu: Vec<f64>,
    span: Vec<f64>,
    passes: Vec<f64>,
    /// Over the replays together.
    submits: u64,
}

/// The wgpu device's queue submits so far, where they are counted.
fn submits() -> Option<u64> {
    Some(counters::snapshot()?.values[Counter::QueueSubmits as usize])
}

/// The submits a frame of `frames`, where they are counted.
fn submits_a_frame(frames: &[Samples]) -> String {
    let total: u64 = frames.iter().map(|frame| frame.submits).sum();
    let count: usize = frames.iter().map(|frame| frame.cpu.len()).sum();
    match submits() {
        Some(_) => format!("{:.2}", total as f64 / count as f64),
        None => "null".into(),
    }
}

/// The median, the 95th percentile and the largest of `values`.
type Summary = Option<[f64; 3]>;

fn summary(values: impl Iterator<Item = f64>) -> Summary {
    let mut sorted: Vec<_> = values.collect();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q).round() as usize];
    (!sorted.is_empty()).then(|| [at(0.5), at(0.95), at(1.)])
}

/// `frames` taken together: their CPU times, GPU spans and GPU passes.
fn summaries(frames: &[Samples]) -> [Summary; 3] {
    let all = |of: fn(&Samples) -> &Vec<f64>| summary(frames.iter().flat_map(of).copied());
    [all(|s| &s.cpu), all(|s| &s.span), all(|s| &s.passes)]
}

fn json(summaries: [Summary; 3]) -> String {
    let [cpu, span, passes] = summaries.map(|summary| match summary {
        Some([median, p95, max]) => {
            format!(r#"{{"median":{median:.4},"p95":{p95:.4},"max":{max:.4}}}"#)
        }
        None => "null".into(),
    });
    format!(r#""cpu_ms":{cpu},"gpu_span_ms":{span},"gpu_passes_ms":{passes}"#)
}

/// The first `count` numbers of a summary as a table's columns.
fn columns(summary: Summary, count: usize) -> String {
    let numbers = summary.iter().flat_map(|numbers| &numbers[..count]);
    let columns: String = numbers.map(|number| format!(" {number:>8.3}")).collect();
    format!("{columns:>width$}", width = count * 9)
}

/// A frame's instances and render passes.
fn drawn(frame: &[Record]) -> (usize, usize) {
    let renders = frame.iter().filter_map(|record| match record {
        Record::Render { frame, .. } => Some(frame.instances.len()),
        _ => None,
    });
    (renders.clone().sum(), renders.count())
}

/// The pixels of each frame's render targets, a target counted once for each pass
/// into it: what a tile-based GPU loads or clears, and stores.
fn target_pixels(frames: &[&[Record]]) -> Vec<u64> {
    let mut sizes = HashMap::new();
    let pixels = |frame: &&[Record]| {
        let mut pixels = 0;
        for record in *frame {
            match record {
                Record::CreateTarget { id, size } | Record::ResizeTarget { id, size, .. } => {
                    sizes.insert(*id, u64::from(size[0]) * u64::from(size[1]));
                }
                Record::Render { target, .. } => pixels += sizes[target],
                _ => {}
            }
        }
        pixels
    };
    frames.iter().map(pixels).collect()
}

/// Move the GPU times that have arrived to the frames awaiting them, oldest first. A
/// warm-up frame awaits its time as `None`.
fn collect(
    device: &mut Selected,
    awaited: &mut VecDeque<Option<usize>>,
    samples: &mut [Samples],
) -> Result<()> {
    for time in device.gpu_times()? {
        let frame = awaited.pop_front().expect("a time for an ended span");
        if let (Some(frame), Some(time)) = (frame, time) {
            samples[frame].span.push(time.span_ms);
            samples[frame].passes.push(time.passes_ms);
        }
    }
    Ok(())
}

/// Replay `frames`, each ending with its readback unless `back_to_back`, and return
/// each one's samples.
fn time(
    device: &mut Selected,
    frames: &[&[Record]],
    warm: usize,
    replays: usize,
    back_to_back: bool,
) -> Result<Vec<Samples>> {
    let mut samples: Vec<_> = frames.iter().map(|_| Samples::default()).collect();
    let mut awaited = VecDeque::new();
    for replay in 0..warm + replays {
        let mut textures = Replay::default();
        for (index, frame) in frames.iter().enumerate() {
            let measured = (replay >= warm).then_some(index);
            let (readback, calls) = frame.split_last().unwrap();
            let timed = device.begin_gpu_span().is_ok();
            assert!(timed || !back_to_back, "only a span's end submits a frame");
            let before = submits();
            let start = Instant::now();
            for record in calls {
                textures.step(device, record)?;
            }
            if timed {
                device.end_gpu_span()?;
                awaited.push_back(measured);
            }
            let cpu = start.elapsed().as_secs_f64() * 1000.;
            if let Some(index) = measured {
                samples[index].cpu.push(cpu);
                samples[index].submits += submits().zip(before).map_or(0, |(now, b)| now - b);
            }
            if !back_to_back {
                textures.step(device, readback)?;
            }
            collect(device, &mut awaited, &mut samples)?;
        }
        // The stream leaves its last textures to a device that is dropped.
        for (name, texture) in textures.0 {
            if name.0 != 0 {
                device.retire_texture(texture)?;
            }
        }
    }
    let until = Instant::now() + Duration::from_secs(10);
    while !awaited.is_empty() && Instant::now() < until {
        collect(device, &mut awaited, &mut samples)?;
        std::thread::yield_now();
    }
    Ok(samples)
}

fn main() {
    let (mut warm, mut replays, mut output, mut paths) = (20, 200, None, Vec::new());
    let mut back_to_back = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().expect("a value");
        match arg.as_str() {
            "--warm" => warm = value().parse().expect("--warm N"),
            "--replays" => replays = value().parse().expect("--replays N"),
            "--json" => output = Some(value()),
            "--back-to-back" => back_to_back = true,
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
    let mut device = Selected::from_env().expect("ink device");
    let name = stream::device_name(&device);
    let spacing = if back_to_back { ", back to back" } else { "" };
    println!("{name}: {replays} replays after {warm}{spacing}; milliseconds a frame");
    println!(
        "{:<10} {:>6} {:>9} {:>6} {:>9} {:>7} {:>18} {:>27} {:>27}",
        "stream",
        "frames",
        "instances",
        "passes",
        "target px",
        "submits",
        "CPU p50 p95",
        "GPU span p50 p95 max",
        "GPU passes p50 p95 max"
    );
    let mut streams = Vec::new();
    for path in &paths {
        let stream = Stream::decode(&std::fs::read(path).expect("a stream"));
        let is_readback = |record: &Record| matches!(record, Record::Readback { .. });
        let frames = stream.records.split_inclusive(is_readback);
        let frames: Vec<_> = frames
            .filter(|f| f.last().is_some_and(is_readback))
            .collect();
        let samples = time(&mut device, &frames, warm, replays, back_to_back).expect("replay");
        let scene = path.file_stem().unwrap().to_string_lossy().into_owned();
        let a_frame = |total: usize| total as f64 / frames.len() as f64;
        let instances = a_frame(frames.iter().map(|frame| drawn(frame).0).sum());
        let passes = a_frame(frames.iter().map(|frame| drawn(frame).1).sum());
        let pixels = target_pixels(&frames);
        let target = a_frame(pixels.iter().sum::<u64>() as usize);
        let [cpu, span, in_passes] = summaries(&samples);
        let submits = submits_a_frame(&samples);
        println!(
            "{scene:<10} {:>6} {instances:>9.0} {passes:>6.1} {target:>9.0} {submits:>7} {} {} {}",
            frames.len(),
            columns(cpu, 2),
            columns(span, 3),
            columns(in_passes, 3)
        );
        let by_frame: Vec<_> = (frames.iter().zip(&samples).zip(&pixels))
            .map(|((frame, samples), target)| {
                let (instances, passes) = drawn(frame);
                let samples = std::slice::from_ref(samples);
                format!(
                    r#"{{"instances":{instances},"passes":{passes},"target_pixels":{target},"submits":{},{}}}"#,
                    submits_a_frame(samples),
                    json(summaries(samples))
                )
            })
            .collect();
        streams.push(format!(
            r#"{{"name":{scene:?},"frames":{},"instances_per_frame":{instances:.1},"passes_per_frame":{passes:.2},"target_pixels_per_frame":{target:.0},"submits_per_frame":{submits},{},"by_frame":[{}]}}"#,
            frames.len(),
            json([cpu, span, in_passes]),
            by_frame.join(",")
        ));
    }
    if let Some(file) = output {
        let document = format!(
            "{{\"device\":{name:?},\"replays\":{replays},\"warm\":{warm},\"back_to_back\":{back_to_back},\"streams\":[\n{}\n]}}\n",
            streams.join(",\n")
        );
        std::fs::write(&file, document).expect("the JSON file");
    }
}
