//! GPU time by wgpu's timestamp queries, on request (`Device::begin_gpu_span`).
//! @ref LLP 1172.023.006 M2.
//!
//! Each `render` pass in a span writes a timestamp at its start and its end. Apple
//! GPUs sample one nowhere else, so the passes are what every backend can time:
//! <https://developer.apple.com/documentation/metal/mtlcountersamplingpoint/atstageboundary>.
//!
//! A span's timestamps are resolved only once its passes have completed. Metal
//! resolves a pass still in flight to the values its queries held before: measured on
//! an Apple M5 Ultra, a resolve submitted straight after the pass gave ends earlier
//! than their starts.

use super::super::{Error, GpuTime, Result, Submission};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The pairs of queries, one a pass. A span that runs past the last reports no time.
const PASSES: u32 = ::wgpu::QUERY_SET_MAX_QUERIES / 2;

struct Span {
    /// Its first pass's pair, and how many follow it.
    first: u32,
    passes: u32,
}

enum Ended {
    /// A span with no pass, or one whose pairs ran out or were written again before
    /// they were resolved.
    Untimed,
    /// Its passes are complete once this submission is.
    Drawing(Span, Submission),
    /// The buffer its ticks are copied to, and whether that buffer is mapped yet.
    Reading(::wgpu::Buffer, Arc<AtomicBool>),
}

pub(super) struct Timing {
    set: ::wgpu::QuerySet,
    resolved: ::wgpu::Buffer,
    /// Nanoseconds in a tick.
    period: f64,
    /// The pair after the last span's. Spans take pairs in turn, so that a span's
    /// stay unwritten until they are resolved.
    next: u32,
    open: Option<Span>,
    /// Oldest first.
    ended: VecDeque<Ended>,
}

impl Timing {
    pub(super) fn new(device: &::wgpu::Device, queue: &::wgpu::Queue) -> Result<Self> {
        if !(device.features()).contains(::wgpu::Features::TIMESTAMP_QUERY) {
            return Err(Error::Unsupported("GPU timing: no timestamp queries here"));
        }
        Ok(Self {
            set: device.create_query_set(&::wgpu::QuerySetDescriptor {
                label: Some("ink gpu time"),
                ty: ::wgpu::QueryType::Timestamp,
                count: PASSES * 2,
            }),
            resolved: device.create_buffer(&::wgpu::BufferDescriptor {
                label: Some("ink gpu time"),
                size: u64::from(PASSES) * 16,
                usage: ::wgpu::BufferUsages::QUERY_RESOLVE | ::wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            period: f64::from(queue.get_timestamp_period()),
            next: 0,
            open: None,
            ended: VecDeque::new(),
        })
    }

    pub(super) fn begin(&mut self) -> Result<()> {
        if self.open.is_some() {
            return Err(Error::Invalid("a GPU span is already open".into()));
        }
        if self.next > PASSES / 2 {
            // The pairs are used again from the first, so a span still drawing
            // could have its own overwritten.
            self.next = 0;
            for span in &mut self.ended {
                if matches!(span, Ended::Drawing(..)) {
                    *span = Ended::Untimed;
                }
            }
        }
        self.open = Some(Span {
            first: self.next,
            passes: 0,
        });
        Ok(())
    }

    /// The open span's next pass, where a pair is left for it.
    pub(super) fn pass(&mut self) -> Option<u32> {
        let span = self.open.as_mut()?;
        span.passes += 1;
        let pass = span.first + span.passes - 1;
        (pass < PASSES).then_some(pass)
    }

    pub(super) fn writes(&self, pass: u32) -> ::wgpu::RenderPassTimestampWrites<'_> {
        ::wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(pass * 2),
            end_of_pass_write_index: Some(pass * 2 + 1),
        }
    }

    /// End the span, whose passes are complete once `last` is.
    pub(super) fn end(&mut self, last: Submission) -> Result<()> {
        let span = self.open.take();
        let span = span.ok_or_else(|| Error::Invalid("no GPU span is open".into()))?;
        self.next = (span.first + span.passes).min(PASSES);
        self.ended
            .push_back(if span.passes == 0 || span.first + span.passes > PASSES {
                Ended::Untimed
            } else {
                Ended::Drawing(span, last)
            });
        Ok(())
    }

    /// The times that have arrived, and the commands to submit that copy out the
    /// ticks of the spans `done` completes and map them once run.
    pub(super) fn times(
        &mut self,
        device: &::wgpu::Device,
        done: Submission,
    ) -> Result<(Vec<Option<GpuTime>>, Vec<::wgpu::CommandBuffer>)> {
        let mut commands = Vec::new();
        for ended in &mut self.ended {
            let Ended::Drawing(span, last) = ended else {
                continue;
            };
            if *last > done {
                break;
            }
            let bytes = u64::from(span.passes) * 16;
            let ticks = device.create_buffer(&::wgpu::BufferDescriptor {
                label: Some("ink gpu time"),
                size: bytes,
                usage: ::wgpu::BufferUsages::COPY_DST | ::wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let queries = span.first * 2..(span.first + span.passes) * 2;
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.resolve_query_set(&self.set, queries, &self.resolved, 0);
            encoder.copy_buffer_to_buffer(&self.resolved, 0, &ticks, 0, bytes);
            let mapped = Arc::new(AtomicBool::new(false));
            let done = mapped.clone();
            encoder.map_buffer_on_submit(&ticks, ::wgpu::MapMode::Read, .., move |result| {
                done.store(result.is_ok(), Ordering::Release);
            });
            commands.push(encoder.finish());
            *ended = Ended::Reading(ticks, mapped);
        }
        let mut times = Vec::new();
        loop {
            match self.ended.front() {
                Some(Ended::Untimed) => times.push(None),
                Some(Ended::Reading(ticks, mapped)) if mapped.load(Ordering::Acquire) => {
                    let bytes = ticks.slice(..).get_mapped_range();
                    let bytes = bytes.map_err(|e| Error::Device(e.to_string()))?;
                    let ticks = bytes.as_chunks::<8>().0.iter();
                    let ticks: Vec<_> = ticks.map(|tick| u64::from_le_bytes(*tick)).collect();
                    times.push(GpuTime::of_ticks(&ticks, self.period / 1e6));
                }
                _ => break,
            }
            self.ended.pop_front();
        }
        Ok((times, commands))
    }
}
