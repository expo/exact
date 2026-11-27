//! Offscreen targets reserved per frame; the device orders their GPU writes.
//! @ref LLP 1149 D2.5, D11.
use crate::counters::{self, Counter};
use crate::device::{Device, Result};
use crate::instance::TextureId;

#[derive(Default)]
pub struct Pool {
    size: [u32; 2],
    targets: Vec<(TextureId, u64, [u32; 2])>,
    frame: u64,
    used: usize,
}

impl Pool {
    pub fn used(&self) -> usize {
        self.used
    }
    pub fn retained(&self) -> usize {
        self.targets.len()
    }
    pub fn bytes(&self) -> u64 {
        self.targets
            .iter()
            .map(|(_, _, s)| u64::from(s[0]) * u64::from(s[1]) * 4)
            .sum()
    }

    pub fn begin(&mut self, device: &mut impl Device, size: [u32; 2]) -> Result<()> {
        let result = (|| {
            self.frame += 1;
            for (target, _, _) in self
                .targets
                .extract_if(.., |(_, seen, _)| self.frame.saturating_sub(*seen) > 2)
            {
                device.retire_texture(target)?;
                counters::add(Counter::TargetsRetired, 1);
            }
            if self.size != size {
                for (target, _, _) in self.targets.drain(..) {
                    device.retire_texture(target)?;
                    counters::add(Counter::TargetsRetired, 1);
                }
                self.size = size;
            }
            self.used = 0;
            Ok(())
        })();
        self.observe_ownership();
        result
    }

    /// Keep at most 16 MiB of unused targets between frames. Active layers are
    /// bounded by the scene; retirement still follows the device's last use.
    pub fn end(&mut self, device: &mut impl Device) -> Result<()> {
        let mut spare_bytes = 0;
        let result = (|| {
            for (target, _, _) in self.targets.extract_if(self.used.., |(_, _, size)| {
                let bytes = u64::from(size[0]) * u64::from(size[1]) * 4;
                if spare_bytes + bytes > 16 * 1024 * 1024 {
                    true
                } else {
                    spare_bytes += bytes;
                    false
                }
            }) {
                device.retire_texture(target)?;
                counters::add(Counter::TargetsRetired, 1);
            }
            Ok(())
        })();
        self.observe_ownership();
        result
    }

    pub fn trim(&mut self, device: &mut impl Device) -> Result<()> {
        let result = (|| {
            for (target, _, _) in self.targets.drain(..) {
                device.retire_texture(target)?;
                counters::add(Counter::TargetsRetired, 1);
            }
            self.used = 0;
            device.trim()
        })();
        self.observe_ownership();
        result
    }

    fn observe_ownership(&self) {
        counters::observe(|s| self.ownership(s));
    }

    fn ownership(&self, s: &mut counters::Snapshot) {
        s.gauge(Counter::TargetsRetained, self.retained() as u64);
        s.gauge(Counter::TargetBytes, self.bytes());
        s.gauge(
            Counter::TargetsReserved,
            self.used.min(self.retained()) as u64,
        );
    }

    pub fn take(&mut self, device: &mut impl Device) -> Result<TextureId> {
        self.take_sized(device, self.size)
    }

    /// A target that holds `size`, and the size it has: each dimension the next
    /// power of two from 64, up to the frame's. A pass costs a tile-based GPU the
    /// load and store of its whole target, so content smaller than the frame is
    /// drawn into a target near its own size. The size follows from the content
    /// alone, so the same content draws the same pixels whatever the pool holds;
    /// the steps let content that grows or moves a few pixels a frame keep its
    /// target.
    pub fn take_fitting(
        &mut self,
        device: &mut impl Device,
        size: [u32; 2],
    ) -> Result<(TextureId, [u32; 2])> {
        let size = [0, 1].map(|axis| {
            let step = size[axis].max(64).next_power_of_two();
            step.min(self.size[axis].max(size[axis]))
        });
        Ok((self.take_sized(device, size)?, size))
    }

    pub fn take_sized(&mut self, device: &mut impl Device, size: [u32; 2]) -> Result<TextureId> {
        let observing = counters::enabled();
        // Earlier frames have submitted every read. Reusing a target orders its
        // next write after those reads; waiting for completion would duplicate
        // the whole pool whenever the CPU starts a frame ahead of the GPU.
        let available = self.targets[self.used..]
            .iter()
            .position(|t| t.2 == size)
            .map(|i| i + self.used)
            .or_else(|| (self.used < self.targets.len()).then_some(self.used));
        if let Some(i) = available {
            if self.targets[i].2 != size {
                let target = device.resize_target(self.targets[i].0, size[0], size[1])?;
                self.targets[i] = (target, self.frame, size);
                counters::add(Counter::TargetsRetired, 1);
                counters::add(Counter::TargetsAllocated, 1);
            }
            self.targets.swap(self.used, i);
        } else {
            self.targets
                .push((device.create_target(size[0], size[1])?, self.frame, size));
            let last = self.targets.len() - 1;
            self.targets.swap(self.used, last);
            counters::add(Counter::TargetsAllocated, 1);
        }
        self.targets[self.used].1 = self.frame;
        let target = self.targets[self.used].0;
        self.used += 1;
        if observing {
            let completed = device.completed()?;
            self.observe(device, completed)?;
        }
        Ok(target)
    }

    /// Observation only: no completion poll, allocation or retirement is added.
    pub fn observe(
        &self,
        device: &impl Device,
        completed: crate::device::Submission,
    ) -> Result<()> {
        counters::observe(|s| {
            self.ownership(s);
            let mut busy = 0;
            let mut idle_bytes = 0;
            for &(target, _, size) in &self.targets[self.used..] {
                if device.last_use(target)? > completed {
                    busy += 1;
                } else {
                    idle_bytes += u64::from(size[0]) * u64::from(size[1]) * 4;
                }
            }
            s.gauge(Counter::TargetsBusy, busy);
            s.gauge(
                Counter::TargetsIdle,
                (self.retained() - self.used) as u64 - busy,
            );
            s.gauge(Counter::TargetIdleBytes, idle_bytes);
            s.gauge(Counter::TargetPartitionCompleted, completed.0);
            s.gauge(
                Counter::TargetPartitionAt,
                counters::timestamp(std::time::Instant::now()) as u64,
            );
            Ok(())
        })
        .unwrap_or(Ok(()))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    #[test]
    fn unused_targets_retire_after_the_idle_window() {
        let mut d = crate::Wgpu::new().unwrap();
        let mut pool = Pool::default();
        pool.begin(&mut d, [16, 16]).unwrap();
        pool.take(&mut d).unwrap();
        assert_gauges(&pool, 1);
        pool.begin(&mut d, [16, 16]).unwrap();
        assert_gauges(&pool, 0);
        for _ in 0..3 {
            pool.begin(&mut d, [16, 16]).unwrap();
        }
        assert_eq!(pool.retained(), 0);
        assert_gauges(&pool, 0);
        pool.take(&mut d).unwrap();
        pool.begin(&mut d, [32, 32]).unwrap();
        assert_eq!(pool.retained(), 0);
        assert_gauges(&pool, 0);
        pool.take(&mut d).unwrap();
        pool.trim(&mut d).unwrap();
        assert_eq!(pool.retained(), 0);
        assert_gauges(&pool, 0);
    }

    fn assert_gauges(pool: &Pool, reserved: u64) {
        if let Some(s) = counters::snapshot() {
            assert_eq!(
                s.values[Counter::TargetsRetained as usize],
                pool.retained() as u64
            );
            assert_eq!(s.values[Counter::TargetBytes as usize], pool.bytes());
            assert_eq!(s.values[Counter::TargetsReserved as usize], reserved);
        }
    }
}
