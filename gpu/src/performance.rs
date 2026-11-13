//! Opt-in HUD snapshots, bounded independently of the surface's public record.
use crate::Module;

/// Maximum JSON bytes in one metrics snapshot.
const MAX_BYTES: usize = 16 * 1024;
#[derive(Default)]
pub(crate) struct Reporting {
    enabled: bool,
    last_ms: Option<f64>,
    /// The frame submissions this canvas rode since the last snapshot:
    /// milliseconds summed, how many, and the longest.
    submits: (f64, u32, f64),
}

impl Reporting {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
    /// A frame this canvas rendered into was submitted in `ms`.
    pub(crate) fn submitted(&mut self, ms: f64) {
        let (sum, count, max) = &mut self.submits;
        *sum += ms;
        *count += 1;
        *max = max.max(ms);
    }
}

/// Run `submit`, and when `timed`, say how long it took in milliseconds.
pub(crate) fn time<T>(timed: bool, submit: impl FnOnce() -> T) -> (T, Option<f64>) {
    let start = timed.then(now_ms);
    let out = submit();
    (out, start.map(|start| now_ms() - start))
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
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
            instance.performance.submits = Default::default();
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
        let (sum, count, max) = std::mem::take(&mut reporting.submits);
        let submits = (count > 0).then(|| (sum / f64::from(count), max));
        instance
            .surface
            .take_performance(now_ms)
            .map(|record| with_facts(record, submits))
            .filter(|record| record.len() <= MAX_BYTES)
    }
}

/// `record` with the facts the module owns for every surface added:
/// `processMemoryBytes`, the process's footprint; `gameHeapBytes`, the
/// module's own heap where it has one apart from the process (the web's
/// linear memory); and `cpuSubmitMs`, `cpuSubmitMaxMs`, the mean and longest
/// wall time of the queue submissions of the frames the canvas drew in the
/// window (`submits`). A fact with no measurement is left out.
fn with_facts(record: String, submits: Option<(f64, f64)>) -> String {
    let facts = [
        (
            "processMemoryBytes",
            process_footprint().map(|b| b.to_string()),
        ),
        ("gameHeapBytes", heap_bytes().map(|b| b.to_string())),
        ("cpuSubmitMs", submits.map(|s| s.0.to_string())),
        ("cpuSubmitMaxMs", submits.map(|s| s.1.to_string())),
    ];
    let mut out = record.trim_end().to_owned();
    if out.pop() != Some('}') {
        return record;
    }
    for (name, bytes) in facts {
        if let Some(bytes) = bytes {
            if !out.trim_end().ends_with('{') {
                out.push(',');
            }
            out.push_str(&format!("\"{name}\":{bytes}"));
        }
    }
    out.push('}');
    out
}

/// The process's physical footprint, `task_vm_info`'s `phys_footprint`: what
/// Xcode's memory gauge and the system's memory limits count.
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos"
))]
fn process_footprint() -> Option<u64> {
    extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
    }
    /// `TASK_VM_INFO` (<mach/task_info.h>).
    const TASK_VM_INFO: u32 = 22;
    /// `phys_footprint`'s index among the record's 64-bit words: after
    /// `virtual_size`, the two 32-bit counts, and sixteen sizes.
    const PHYS_FOOTPRINT: usize = 18;
    let mut info = [0u64; 64];
    let mut count = (size_of_val(&info) / 4) as u32;
    // SAFETY: `info` holds `count` 32-bit words, which the kernel fills at most.
    let status = unsafe {
        task_info(
            mach_task_self_,
            TASK_VM_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    (status == 0 && count as usize >= (PHYS_FOOTPRINT + 1) * 2).then_some(info[PHYS_FOOTPRINT])
}

/// The process's resident set (`VmRSS` in `/proc/self/status`).
#[cfg(any(target_os = "linux", target_os = "android"))]
fn process_footprint() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?;
    let kib: u64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
    Some(kib * 1024)
}

/// The browser reports no process memory to a page; Windows is not measured.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "linux",
    target_os = "android"
)))]
fn process_footprint() -> Option<u64> {
    None
}

/// The module's linear memory: the simulation's and the renderer's heap.
#[cfg(target_arch = "wasm32")]
fn heap_bytes() -> Option<u64> {
    Some(core::arch::wasm32::memory_size(0) as u64 * 65536)
}

/// Natively the module shares the process's heap, which the footprint counts.
#[cfg(not(target_arch = "wasm32"))]
fn heap_bytes() -> Option<u64> {
    None
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
    fn memory_facts_join_the_surface_record() {
        let joined: std::collections::BTreeMap<String, f64> =
            serde_json_lite(&with_facts("{\"frames\":3}".into(), None));
        assert_eq!(joined["frames"], 3.0);
        assert!(
            !joined.contains_key("gameHeapBytes"),
            "native has no heap apart"
        );
        let empty = with_facts("{}".into(), None);
        assert!(
            empty.starts_with('{') && !empty.starts_with("{,"),
            "{empty}"
        );
        assert_eq!(with_facts("not json".into(), None), "not json");
        assert!(
            !joined.contains_key("cpuSubmitMs"),
            "no submission, no time"
        );
        let timed = serde_json_lite(&with_facts("{}".into(), Some((1.5, 4.0))));
        assert_eq!((timed["cpuSubmitMs"], timed["cpuSubmitMaxMs"]), (1.5, 4.0));
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let bytes = joined["processMemoryBytes"];
            assert!(bytes > 1e6 && bytes < 1e13, "{bytes}");
        }
    }

    /// The flat number record [`with_facts`] makes, as name → value.
    fn serde_json_lite(text: &str) -> std::collections::BTreeMap<String, f64> {
        let body = text.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
        body.split(',')
            .filter(|p| !p.is_empty())
            .map(|pair| {
                let (name, value) = pair.split_once(':').unwrap();
                (name.trim_matches('"').to_owned(), value.parse().unwrap())
            })
            .collect()
    }

    /// `phys_footprint` read at the offset [`process_footprint`] assumes
    /// matches `proc_pid_rusage`'s, which names the field.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_footprint_is_the_systems() {
        extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
        }
        // `rusage_info_v2`: a 16-byte UUID, then 64-bit counters; the eighth is
        // `ri_phys_footprint`.
        let mut usage = [0u64; 32];
        // SAFETY: `usage` is larger than `rusage_info_v2`.
        assert_eq!(
            unsafe { proc_pid_rusage(std::process::id() as i32, 2, usage.as_mut_ptr()) },
            0
        );
        let system = usage[9] as f64;
        let ours = process_footprint().unwrap() as f64;
        assert!(
            (ours - system).abs() < system * 0.1,
            "{ours} against {system}"
        );
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
            serde_json_lite(&module.take_performance(id, 0.0).unwrap())["clock"],
            0.0
        );
        for now in [0.0, 999.9, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(module.take_performance(id, now), None);
        }
        assert_eq!(
            serde_json_lite(&module.take_performance(id, 1000.0).unwrap())["clock"],
            1000.0
        );
        let reporting = &mut module.instances.get_mut(&id).unwrap().performance;
        reporting.submitted(2.0);
        reporting.submitted(4.0);
        let snapshot = serde_json_lite(&module.take_performance(id, 2000.0).unwrap());
        assert_eq!(
            (snapshot["cpuSubmitMs"], snapshot["cpuSubmitMaxMs"]),
            (3.0, 4.0)
        );
        module
            .instances
            .get_mut(&id)
            .unwrap()
            .performance
            .submitted(9.0);
        assert!(module.set_performance_reporting(id, false));
        assert!(module.set_performance_reporting(id, true));
        assert!(module.set_performance_reporting(id, false));
        assert_eq!(module.take_performance(id, 3000.0), None);
        assert_eq!(*calls.lock().unwrap(), (vec![true, false, true, false], 3));
        assert!(module.set_performance_reporting(id, true));
        assert_eq!(module.take_performance(id, 2001.0), None);
        let snapshot = serde_json_lite(&module.take_performance(id, 3000.0).unwrap());
        assert!(
            !snapshot.contains_key("cpuSubmitMs"),
            "a toggle drops the window"
        );
        module.destroy(id);
        assert_eq!(module.take_performance(id, 10000.0), None);
    }
}
