//! Opt-in HUD snapshots, bounded independently of the surface's public record.
use crate::Module;

/// Maximum JSON bytes in one metrics snapshot.
const MAX_BYTES: usize = 16 * 1024;
#[derive(Default)]
pub(crate) struct Reporting {
    enabled: bool,
    last_ms: Option<f64>,
}

impl Module {
    /// Start or stop this canvas's performance reporting. No collector is called
    /// while disabled; the cadence survives toggles until the canvas is destroyed.
    pub fn set_performance_reporting(&mut self, id: u32, enabled: bool) -> bool {
        let Some(instance) = self.instances.get_mut(&id) else {
            return false;
        };
        if instance.performance.enabled != enabled {
            instance.performance.enabled = enabled;
            instance.surface.set_performance_reporting(enabled);
        }
        true
    }

    /// Read metrics at most once per second on the host's monotonic clock.
    /// Invalid or backwards clock samples cannot create a publication storm.
    pub fn take_performance(&mut self, id: u32, now_ms: f64) -> Option<String> {
        if !now_ms.is_finite() {
            return None;
        }
        let instance = self.instances.get_mut(&id)?;
        let reporting = &mut instance.performance;
        if !reporting.enabled {
            return None;
        }
        if reporting.last_ms.is_some_and(|last| now_ms < last + 1000.0) {
            return None;
        }
        reporting.last_ms = Some(now_ms);
        instance
            .surface
            .take_performance(now_ms)
            .filter(|record| record.len() <= MAX_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{wgpu, Frame, Registry, Surface, SurfaceError, Value};
    use std::sync::{Arc, Mutex};

    struct Probe(Arc<Mutex<(Vec<bool>, usize)>>);
    impl Surface for Probe {
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn render(
            &mut self,
            _: &Frame,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: &mut wgpu::CommandEncoder,
            _: &wgpu::TextureView,
            _: wgpu::TextureFormat,
        ) -> bool {
            false
        }
        fn set_performance_reporting(&mut self, enabled: bool) {
            self.0.lock().unwrap().0.push(enabled);
        }
        fn take_performance(&mut self, now_ms: f64) -> Option<String> {
            self.0.lock().unwrap().1 += 1;
            Some(format!("{{\"clock\":{now_ms}}}"))
        }
    }

    #[test]
    fn reporting_is_opt_in_bounded_and_released_on_disable() {
        static REGISTRY: Registry = Registry {
            surfaces: &[],
            shaders: &[],
        };
        let calls = Arc::new(Mutex::new((vec![], 0)));
        let mut module = Module::new(&REGISTRY);
        let id = module.insert(Box::new(Probe(calls.clone())), None).unwrap();
        assert_eq!(module.take_performance(id, 0.0), None);
        assert_eq!(*calls.lock().unwrap(), (vec![], 0));
        assert!(module.set_performance_reporting(id, true));
        assert!(module.set_performance_reporting(id, true));
        assert_eq!(
            module.take_performance(id, 0.0).as_deref(),
            Some("{\"clock\":0}")
        );
        for now in [0.0, 999.9, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(module.take_performance(id, now), None);
        }
        assert_eq!(
            module.take_performance(id, 1000.0).as_deref(),
            Some("{\"clock\":1000}")
        );
        assert!(module.set_performance_reporting(id, false));
        assert_eq!(module.take_performance(id, 2000.0), None);
        assert_eq!(*calls.lock().unwrap(), (vec![true, false], 2));
        assert!(module.set_performance_reporting(id, true));
        assert_eq!(module.take_performance(id, 1001.0), None);
        assert!(module.take_performance(id, 2000.0).is_some());
        module.destroy(id);
        assert_eq!(module.take_performance(id, 10000.0), None);
    }
}
