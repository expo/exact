//! GPU time by Direct3D 11's timestamp queries, on request (`Device::begin_gpu_span`).
//! @ref LLP 1172.023.006 M2.
//!
//! A span is one `D3D11_QUERY_TIMESTAMP_DISJOINT` query begun and ended around
//! `D3D11_QUERY_TIMESTAMP` queries, one before and one after each `render` pass. The
//! ticks count only where the disjoint query says the counter ran undisturbed:
//! <https://learn.microsoft.com/windows/win32/api/d3d11/ne-d3d11-d3d11_query>.

use super::super::{Error, GpuTime, Result};
use std::collections::VecDeque;
use windows::Win32::Graphics::Direct3D11::*;

struct Span {
    disjoint: ID3D11Query,
    stamps: Vec<ID3D11Query>,
}

#[derive(Default)]
pub(super) struct Timing {
    open: Option<Span>,
    /// Ended spans whose results have not been read, oldest first.
    ended: VecDeque<Span>,
    /// Timestamp queries read and free to issue again.
    spare: Vec<ID3D11Query>,
    /// Every query issued. A device that was never asked for a time has issued none.
    pub(super) issued: u64,
}

fn query(device: &ID3D11Device, kind: D3D11_QUERY) -> Result<ID3D11Query> {
    let desc = D3D11_QUERY_DESC {
        Query: kind,
        MiscFlags: 0,
    };
    let mut query = None;
    // SAFETY: the description and the output outlive the call.
    unsafe { device.CreateQuery(&desc, Some(&mut query))? };
    Ok(query.unwrap())
}

/// A query's result where it has one, without flushing. `value` is left as it was
/// where the result is not ready: `GetData` then returns `S_FALSE`, which is no error.
fn read<T>(context: &ID3D11DeviceContext, query: &ID3D11Query, value: &mut T) -> Result<()> {
    let flags = D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32;
    // SAFETY: `value` is the plain data this kind of query writes, at its size.
    unsafe {
        context.GetData(
            query,
            Some(std::ptr::from_mut(value).cast()),
            size_of::<T>() as u32,
            flags,
        )?
    };
    Ok(())
}

impl Timing {
    pub(super) fn begin(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
    ) -> Result<()> {
        if self.open.is_some() {
            return Err(Error::Invalid("a GPU span is already open".into()));
        }
        let disjoint = query(device, D3D11_QUERY_TIMESTAMP_DISJOINT)?;
        // SAFETY: the immediate context is used on the device's owning thread.
        unsafe { context.Begin(&disjoint) };
        self.issued += 1;
        self.open = Some(Span {
            disjoint,
            stamps: Vec::new(),
        });
        Ok(())
    }

    /// Stamp this point of the command stream, inside a span: a pass's start or end.
    pub(super) fn stamp(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
    ) -> Result<()> {
        let Some(span) = &mut self.open else {
            return Ok(());
        };
        let stamp = match self.spare.pop() {
            Some(stamp) => stamp,
            None => query(device, D3D11_QUERY_TIMESTAMP)?,
        };
        // SAFETY: as in `begin`. A timestamp query has no `Begin`.
        unsafe { context.End(&stamp) };
        self.issued += 1;
        span.stamps.push(stamp);
        Ok(())
    }

    pub(super) fn end(&mut self, context: &ID3D11DeviceContext) -> Result<()> {
        let span = self.open.take();
        let span = span.ok_or_else(|| Error::Invalid("no GPU span is open".into()))?;
        // SAFETY: as in `begin`.
        unsafe { context.End(&span.disjoint) };
        self.ended.push_back(span);
        Ok(())
    }

    /// The times of the ended spans whose results are ready.
    pub(super) fn times(&mut self, context: &ID3D11DeviceContext) -> Result<Vec<Option<GpuTime>>> {
        let mut times = Vec::new();
        while let Some(span) = self.ended.front() {
            // A ready result has the counter's frequency, which is never zero, and a
            // ready timestamp is a tick count since the counter started.
            let mut disjoint = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
            read(context, &span.disjoint, &mut disjoint)?;
            let mut ticks = vec![0u64; span.stamps.len()];
            for (stamp, tick) in span.stamps.iter().zip(&mut ticks) {
                read(context, stamp, tick)?;
            }
            if disjoint.Frequency == 0 || ticks.contains(&0) {
                break;
            }
            let time = GpuTime::of_ticks(&ticks, 1000. / disjoint.Frequency as f64);
            times.push(time.filter(|_| !disjoint.Disjoint.as_bool()));
            let span = self.ended.pop_front().unwrap();
            self.spare.extend(span.stamps);
        }
        Ok(times)
    }
}
