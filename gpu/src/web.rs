//! The web ABI: wasm-bindgen exports the glue calls after the module's
//! wasm is fetched on demand (LLP 1009 D2).

use crate::{json, Frame, Module, Registry, SurfaceRef};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! {
    static MODULE: RefCell<Option<Module>> = const { RefCell::new(None) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn with<T>(f: impl FnOnce(&mut Module) -> T) -> Option<T> {
    MODULE.with(|m| m.borrow_mut().as_mut().map(f))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["globalThis", "exact", "gpu"], js_name = deviceLost)]
    fn device_lost();
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    pub(crate) fn console_error(message: &str);
}

pub(crate) fn notify_loss(lost: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let current = MODULE.with(|m| {
        m.try_borrow().ok().is_some_and(|m| {
            m.as_ref()
                .is_some_and(|m| std::sync::Arc::ptr_eq(lost, &m.device_lost))
        })
    });
    if current {
        device_lost();
    }
}

/// Drain requested asset paths as JSON.
pub fn assets(id: u32) -> String {
    with(|m| m.take_assets(id).json()).unwrap_or_else(|| crate::AssetChanges::default().json())
}

/// Deliver named bytes, including a missing file, without requiring a device.
pub fn asset(id: u32, name: &str, bytes: Option<&[u8]>) -> bool {
    with(|m| m.asset(id, name, bytes.ok_or(crate::AssetError::Missing))).unwrap_or(false)
}

/// Deliver a terminal host transport failure by name.
pub fn asset_failed(id: u32, name: &str, reason: &str) -> bool {
    with(|m| m.asset(id, name, Err(crate::AssetError::Failed(reason.into())))).unwrap_or(false)
}

/// Create the device and the module (asynchronous: WebGPU's adapter and
/// device requests are).
pub async fn load(registry: &'static Registry) -> Result<(), JsValue> {
    crate::report_panics();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    match crate::load_gpu(instance, None).await {
        Ok(gpu) => {
            MODULE.with(|m| {
                let mut m = m.borrow_mut();
                m.get_or_insert_with(|| Module::new(registry)).set_gpu(gpu);
            });
            Ok(())
        }
        Err(e) => {
            let failure = crate::recovery::RecoveryFailure::device(e);
            let message = failure.message().to_string();
            ERROR.with(|s| *s.borrow_mut() = message.clone());
            Err(JsValue::from_str(&failure.json()))
        }
    }
}

/// Replacement canvases use the existing IDs and surface objects. No module load.
pub async fn recover(
    ids: &[u32],
    canvases: Vec<web_sys::HtmlCanvasElement>,
) -> Result<String, JsValue> {
    let mut module = MODULE
        .with(|m| m.borrow_mut().take())
        .ok_or_else(|| JsValue::from_str("GPU module not loaded"))?;
    let result: Result<String, crate::recovery::RecoveryFailure> = async {
        if ids.len() != canvases.len() || ids.iter().any(|id| !module.instances.contains_key(id)) {
            return Err(crate::recovery::RecoveryFailure::other(
                "recovery canvas table mismatch",
            ));
        }
        let outcome = module.recover().await?;
        if !outcome.contains("\"status\":\"recovered\"") {
            return Ok(outcome);
        }
        // Stage the whole replacement table. Configurations remain on instances,
        // even when adapter creation, attachment, or a second loss fails.
        let gpu = module
            .gpu
            .as_ref()
            .ok_or_else(|| crate::recovery::RecoveryFailure::other("no device"))?;
        let mut targets = Vec::new();
        for (id, canvas) in ids.iter().zip(canvases) {
            let inst = module.instances.get_mut(id).unwrap();
            let config = inst.config.as_mut().ok_or_else(|| {
                crate::recovery::RecoveryFailure::other("missing presentation configuration")
            })?;
            let target = gpu
                .instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
                .map_err(|e| crate::recovery::RecoveryFailure::other(e.to_string()))?;
            let caps = target.get_capabilities(&gpu.adapter);
            inst.hdr = crate::target::configure(&*inst.surface, &gpu.adapter, &caps, config);
            inst.alpha_modes = caps.alpha_modes;
            target.configure(&gpu.device, config);
            targets.push((*id, target));
        }
        for (id, target) in targets {
            let inst = module.instances.get_mut(&id).unwrap();
            let config = inst.config.as_ref().unwrap();
            inst.surface.device_ready(gpu.device.features());
            inst.surface
                .color_space_granted(crate::target::granted(Some(config)));
            inst.surface
                .prepare_assets(&gpu.device, &gpu.queue, crate::view_format(config));
            inst.presentation = Some(SurfaceRef::new(target));
        }
        if module
            .device_lost
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(crate::recovery::RecoveryFailure::other(
                "replacement device was lost during recovery",
            ));
        }
        Ok(module.recovery_report())
    }
    .await;
    if let Err(error) = &result {
        module.lose_device();
        module.error = format!("device recovery: {}", error.message());
    }
    MODULE.with(|m| *m.borrow_mut() = Some(module));
    Ok(match result {
        Ok(report) => report,
        Err(error) => error.json(),
    })
}

/// Create a canvas's surface on a `<canvas>` element. The id, or 0.
pub fn create(name: &str, canvas: web_sys::HtmlCanvasElement, width: u32, height: u32) -> u32 {
    with(|m| {
        let gpu = m.gpu()?;
        match gpu
            .instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        {
            Ok(t) => m.create(name, t, width, height),
            Err(e) => {
                ERROR.with(|s| *s.borrow_mut() = format!("{e}"));
                None
            }
        }
    })
    .flatten()
    .unwrap_or(0)
}

/// Register the text of shader `name` (LLP 1030 D8): validated, its
/// interface checked against the one this module's Rust binds. `true` on
/// success; the refusal is [`error`]'s.
pub fn shader(name: &str, text: &str) -> bool {
    match with(|m| m.set_shader(name, text.to_string())) {
        Some(ok) => ok,
        None => {
            ERROR.with(|s| *s.borrow_mut() = "gpu_shader: the module is not loaded".into());
            false
        }
    }
}

/// Validate a candidate registration without changing the live registry.
pub async fn shader_check(name: &str, text: &str) -> bool {
    let device = with(|m| {
        m.expected_digest(name)?;
        m.gpu().map(|gpu| gpu.device.clone())
    })
    .flatten();
    let Some(device) = device else {
        ERROR.with(|s| *s.borrow_mut() = format!("unknown shader `{name}` or GPU unavailable"));
        return false;
    };
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(name),
        source: wgpu::ShaderSource::Wgsl(text.into()),
    });
    let validation = scope.pop();
    let info = module.get_compilation_info().await;
    let error = validation.await.map(|error| error.to_string()).or_else(|| {
        info.messages
            .into_iter()
            .find(|message| message.message_type == wgpu::CompilationMessageType::Error)
            .map(|message| message.message)
    });
    if let Some(error) = error {
        ERROR.with(|s| *s.borrow_mut() = format!("shader `{name}` refused: {error}"));
        false
    } else {
        true
    }
}

/// Clear the namespace before installing a fully checked replacement.
pub fn shaders_clear() {
    crate::shaders::clear_shaders();
}

/// The shaders this module's surfaces bind against, as a JSON list of names
/// — what the glue fetches (`./shaders/<name>.wgsl`) and registers before a
/// surface is created. `[]` before the module is loaded.
pub fn shader_names() -> String {
    json_names(&with(|m| m.shader_names()).unwrap_or_default())
}

/// The registry's surfaces, as a JSON list of names: what the web build
/// compiles to this module rather than to Canvas 2D. Needs no load.
pub fn surface_names(registry: &Registry) -> String {
    let names: Vec<&str> = registry.surfaces.iter().map(|(n, _, _)| *n).collect();
    json_names(&names)
}

fn json_names(names: &[impl AsRef<str>]) -> String {
    let mut s = String::from("[");
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(n.as_ref());
        s.push('"');
    }
    s.push(']');
    s
}

/// Bind inputs (a JSON array). `true` on success.
pub fn bind(id: u32, values: &str) -> bool {
    bind_at(id, values, None)
}

/// Bind inputs at an optional host commit clock.
pub fn bind_at(id: u32, values: &str, at_ms: Option<f64>) -> bool {
    with(|m| m.bind_json(id, values, at_ms)).unwrap_or(false)
}

/// Bind the named inputs that changed since the last accepted bind
/// ([`Module::bind_changed_json`]).
pub fn bind_changed_at(id: u32, changed: &str, at_ms: Option<f64>) -> bool {
    with(|m| m.bind_changed_json(id, changed, at_ms)).unwrap_or(false)
}

/// The canvas's size in points, before a bind. `true` on success.
pub fn size(id: u32, width: f32, height: f32) -> bool {
    with(|m| m.size(id, width, height)).unwrap_or(false)
}

/// Record one frame into the open frame (shown at [`flush`]): 1 = wants
/// another, 0 = done, 2 = failed.
pub fn render(id: u32, width: f32, height: f32, scale: f32, now_ms: f64) -> u32 {
    let frame = Frame {
        width,
        height,
        scale,
        now_ms,
        children_generation: 0,
        seekable: false,
        period_ms: 0.0,
        shader_generation: 0,
        headroom: 1.0,
    };
    with(|m| match m.render(id, &frame) {
        Some(true) => 1,
        Some(false) => 0,
        None if m.gpu().is_none() => 3,
        None => 2,
    })
    .unwrap_or(2)
}

/// Submit the frame the canvases recorded, once (LLP 1009 D7): before the
/// task that rendered them ends, which is when the browser presents.
/// `true` on success; a surface's failure is [`error`]'s.
pub fn flush() -> bool {
    with(|m| m.flush()).unwrap_or(true)
}

/// Whether the host should supply per-child kernel frames.
pub fn children_mode(id: u32) -> u32 {
    with(|m| m.children_mode(id).code()).unwrap_or(0)
}
/// Host-composited child: no pixel texture on the browser.
pub fn child(id: u32, index: u32, name: &str, frame: [f32; 4]) -> bool {
    with(|m| m.child(id, index as usize, name, frame, [0, 0], &[])).unwrap_or(false)
}
/// Retire departed direct children.
pub fn children_count(id: u32, count: u32) -> bool {
    with(|m| m.children_count(id, count as usize)).unwrap_or(false)
}
/// Same encoding as native: 0 kernel frame, 1 ten floats (sixteen with the
/// two clip rows when `out` has room), 2 hidden (out untouched).
pub fn placement(id: u32, index: u32, out: &mut [f32]) -> u32 {
    if out.len() < 10 {
        return 0;
    }
    match with(|m| m.placement(id, index as usize)).flatten() {
        Some(p) if p.hidden => 2,
        Some(p) => {
            out[..9].copy_from_slice(&p.homography);
            out[9] = p.depth;
            if out.len() >= 16 {
                out[10..16].copy_from_slice(p.clip_depth.as_flattened());
            }
            1
        }
        None => 0,
    }
}

/// Whether a canvas wants raw input.
pub fn wants_input(id: u32) -> bool {
    with(|m| m.wants_input(id)).unwrap_or(false)
}

/// Whether a canvas draws above SDR white (LLP 1100 D12b).
pub fn high_dynamic_range(id: u32) -> bool {
    with(|m| m.high_dynamic_range(id)).unwrap_or(false)
}

/// The headroom an HDR canvas draws its next frames to (LLP 1100 D12b).
pub fn headroom(id: u32, headroom: f32) {
    with(|m| m.set_headroom(id, headroom));
}

/// Deliver a JSON device event. True on success.
pub fn input(id: u32, event: &str) -> bool {
    with(|m| m.input_json(id, event)).unwrap_or(false)
}

/// Capture state; None is distinct from a zero-byte carry.
pub fn carry(id: u32) -> Result<Option<Vec<u8>>, wasm_bindgen::JsValue> {
    with(|m| m.carry(id))
        .unwrap_or_else(|| Err(crate::SurfaceError("GPU module is not loaded".into())))
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.0))
}

/// Restore state. False leaves the surface unchanged; error explains why.
pub fn restore(id: u32, bytes: &[u8], mode: u32) -> bool {
    let Ok(mode) = crate::Restore::from_code(mode) else {
        return false;
    };
    with(|m| m.restore(id, bytes, mode)).unwrap_or(false)
}

/// Take the latest changed public record, if any.
pub fn published(id: u32) -> Option<String> {
    with(|m| m.take_published(id)).flatten()
}

/// Enable or disable bounded HUD performance reporting.
pub fn set_performance_reporting(id: u32, enabled: bool) -> bool {
    with(|m| m.set_performance_reporting(id, enabled)).unwrap_or(false)
}

/// Take one metrics snapshot, at most once per second on the host clock.
pub fn performance(id: u32, now_ms: f64) -> Option<String> {
    with(|m| m.take_performance(id, now_ms)).flatten()
}

/// Drain posted messages as a JSON array.
pub fn messages(id: u32) -> Option<String> {
    let messages = with(|m| m.take_messages(id)).unwrap_or_default();
    (!messages.is_empty()).then(|| json::strings(&messages))
}

/// Ask the surface; an empty string means no answer.
pub fn agent(id: u32, request: &str) -> String {
    with(|m| m.agent(id, request)).flatten().unwrap_or_default()
}

/// Deliver a host lifecycle notification without advancing the surface.
pub fn lifecycle(id: u32, code: u32) {
    with(|m| m.lifecycle(id, code));
}

/// Set the host's clock ownership.
pub fn seekable(on: bool) {
    with(|m| m.set_seekable(on));
}

/// The display's frame period in milliseconds (0 = unknown), for every frame after.
pub fn period(period_ms: f64) {
    with(|m| m.set_period(period_ms));
}

/// The host's display callback for the frames after: its raw timestamp, when
/// it began to run and the display's fastest period, in milliseconds on the
/// frame clock, and the thermal state's code ([`crate::ThermalState`]).
pub fn host_frame(raw_ms: f64, callback_ms: f64, fastest_period_ms: f64, thermal: u32) {
    let host = crate::HostTiming {
        raw_ms,
        callback_ms,
        fastest_period_ms,
    };
    with(|m| m.set_host_timing(host, crate::ThermalState::from_code(thermal)));
}

/// Whether a canvas's next frame is due at `now_ms` under its cap ([`crate::Module::due`]).
pub fn due(id: u32, now_ms: f64) -> bool {
    with(|m| m.due(id, now_ms)).unwrap_or(true)
}

/// Whether a canvas has unrendered inputs.
pub fn dirty(id: u32) -> bool {
    with(|m| m.dirty(id)).unwrap_or(false)
}

/// Drop all surfaces and release the module's device.
pub fn unload() {
    MODULE.with(|m| {
        if let Some(module) = m.borrow_mut().take() {
            module.unload();
        }
    });
    crate::shaders::clear_shaders();
}

/// A canvas's raster scale ([`crate::Module::render_scale`]).
pub fn render_scale(id: u32) -> f32 {
    with(|m| m.render_scale(id)).unwrap_or(1.0)
}

/// Every canvas takes the told tier ([`crate::Module::retier`]).
pub fn retier() {
    with(|m| m.retier());
}

/// Drop a canvas's surface.
pub fn destroy(id: u32) {
    with(|m| m.destroy(id));
}

/// The last failure's text.
pub fn error() -> String {
    let own = ERROR.with(|s| std::mem::take(&mut *s.borrow_mut()));
    if !own.is_empty() {
        return own;
    }
    with(|m| m.take_error()).unwrap_or_default()
}

/// The exports for one app's registry, wasm-bindgen (see LLP 1009 D2).
#[macro_export]
macro_rules! module {
    ($registry:expr) => {
        /// Create the device.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub async fn gpu_load() -> Result<(), ::wasm_bindgen::JsValue> {
            $crate::web::load(&$registry).await
        }

        /// Recover device/context bindings while keeping every surface ID.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub async fn gpu_recover(
            ids: &[u32],
            canvases: Vec<::web_sys::HtmlCanvasElement>,
        ) -> Result<String, ::wasm_bindgen::JsValue> {
            $crate::web::recover(ids, canvases).await
        }

        /// Create a canvas's surface on a `<canvas>`. The id, or 0.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_create(
            name: &str,
            canvas: ::web_sys::HtmlCanvasElement,
            width: u32,
            height: u32,
        ) -> u32 {
            $crate::web::create(name, canvas, width, height)
        }

        /// Register a shader's text (LLP 1030 D8). `true` on success.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_shader(name: &str, text: &str) -> bool {
            $crate::web::shader(name, text)
        }

        /// Validate one registration without changing the live registry.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub async fn gpu_shader_check(name: &str, text: &str) -> bool {
            $crate::web::shader_check(name, text).await
        }

        /// Replace the namespace, including removals.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_shaders_clear() {
            $crate::web::shaders_clear()
        }

        /// The registry's surfaces, as a JSON list of names (the web build reads it).
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_surfaces() -> String {
            $crate::web::surface_names(&$registry)
        }

        /// The shaders to register, as a JSON list of names.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_shader_names() -> String {
            $crate::web::shader_names()
        }

        /// Bind inputs (a JSON array). `true` on success.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_bind(id: u32, values: &str) -> bool {
            $crate::web::bind(id, values)
        }

        /// Bind inputs at an optional host commit clock.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_bind_at(id: u32, values: &str, at_ms: Option<f64>) -> bool {
            $crate::web::bind_at(id, values, at_ms)
        }

        /// Bind the named inputs that changed since the last accepted bind.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_bind_changed_at(id: u32, changed: &str, at_ms: Option<f64>) -> bool {
            $crate::web::bind_changed_at(id, changed, at_ms)
        }

        /// The canvas's size in points, before a bind. `true` on success.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_size(id: u32, width: f32, height: f32) -> bool {
            $crate::web::size(id, width, height)
        }

        /// Record one frame (shown at `gpu_flush`): 1 = wants another, 0 = done, 2 = failed.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_render(id: u32, width: f32, height: f32, scale: f32, now_ms: f64) -> u32 {
            $crate::web::render(id, width, height, scale, now_ms)
        }

        /// Submit the recorded frame once; before the rendering task ends.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_flush() -> bool {
            $crate::web::flush()
        }

        /// Supply child frames when requested; the browser composites their elements.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_children_mode(id: u32) -> u32 {
            $crate::web::children_mode(id)
        }
        /// Supply one direct child frame for browser composition.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_child_view(
            id: u32,
            index: u32,
            name: &str,
            x: f32,
            y: f32,
            w: f32,
            h: f32,
        ) -> bool {
            $crate::web::child(id, index, name, [x, y, w, h])
        }
        /// Retire direct child frames past the new count.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_children_count(id: u32, count: u32) -> bool {
            $crate::web::children_count(id, count)
        }
        /// 0 kernel frame, 1 homography/depth, 2 hidden with out untouched.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_placement(id: u32, index: u32, out: &mut [f32]) -> u32 {
            $crate::web::placement(id, index, out)
        }

        /// Whether a canvas wants raw input.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_wants_input(id: u32) -> bool {
            $crate::web::wants_input(id)
        }

        /// Whether a canvas draws above SDR white (LLP 1100 D12b).
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_high_dynamic_range(id: u32) -> bool {
            $crate::web::high_dynamic_range(id)
        }

        /// The headroom an HDR canvas draws its next frames to.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_headroom(id: u32, headroom: f32) {
            $crate::web::headroom(id, headroom)
        }

        /// Deliver one JSON event; true on success.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_input(id: u32, event_json: &str) -> bool {
            $crate::web::input(id, event_json)
        }

        /// Drain requested asset paths as JSON.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_assets(id: u32) -> String {
            $crate::web::assets(id)
        }
        /// Deliver one requested asset, or null/undefined for a missing file.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_asset(id: u32, name: &str, bytes: Option<Vec<u8>>) -> bool {
            $crate::web::asset(id, name, bytes.as_deref())
        }
        /// Deliver a terminal host transport failure.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_asset_failed(id: u32, name: &str, reason: &str) -> bool {
            $crate::web::asset_failed(id, name, reason)
        }

        /// Capture state, or undefined when this surface carries nothing.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_carry(id: u32) -> Result<Option<Vec<u8>>, wasm_bindgen::JsValue> {
            $crate::web::carry(id)
        }

        /// Restore state, reporting a refusal through gpu_error.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_restore(id: u32, bytes: &[u8], mode: u32) -> bool {
            $crate::web::restore(id, bytes, mode)
        }

        /// Release every surface and the device before replacing this module.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_unload() {
            $crate::web::unload();
        }

        /// Take the changed public record, if any.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_published(id: u32) -> Option<String> {
            $crate::web::published(id)
        }

        /// Enable or disable bounded HUD performance reporting.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_performance_reporting(id: u32, enabled: bool) -> bool {
            $crate::web::set_performance_reporting(id, enabled)
        }

        /// Take one metrics snapshot, at most once per second on the host clock.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_performance(id: u32, now_ms: f64) -> Option<String> {
            $crate::web::performance(id, now_ms)
        }

        /// Drain messages as a JSON array.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_messages(id: u32) -> Option<String> {
            $crate::web::messages(id)
        }

        /// Ask the surface; empty when it has no answer.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_agent(id: u32, request_json: &str) -> String {
            $crate::web::agent(id, request_json)
        }

        /// Host lifecycle code; unknown codes are ignored.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_lifecycle(id: u32, code: u32) {
            $crate::web::lifecycle(id, code);
        }
        /// Set the host clock ownership.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_seekable(on: bool) {
            $crate::web::seekable(on)
        }

        /// The display's frame period in milliseconds, 0 while unknown.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_period(period_ms: f64) {
            $crate::web::period(period_ms)
        }

        /// The page's frame callback, on the frame clock, and the thermal state's code (0 unknown).
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_host_frame(raw_ms: f64, callback_ms: f64, fastest_period_ms: f64, thermal: u32) {
            $crate::web::host_frame(raw_ms, callback_ms, fastest_period_ms, thermal)
        }

        /// Whether a canvas's next frame is due at `now_ms` under its frame-rate cap.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_due(id: u32, now_ms: f64) -> bool {
            $crate::web::due(id, now_ms)
        }

        /// The page's GPU tier, told before `gpu_load` (`GpuTier::code`: 0
        /// high, 1 mid, 2 low).
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_tier(code: u32) {
            $crate::set_tier($crate::GpuTier::from_code(code))
        }

        /// A canvas's raster relative to its device pixels: the page sizes
        /// the canvas's backing store by it.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_render_scale(id: u32) -> f32 {
            $crate::web::render_scale(id)
        }

        /// Remake each canvas's renderer at the told tier, keeping its world.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_retier() {
            $crate::web::retier()
        }

        /// Whether a canvas has unrendered inputs.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_dirty(id: u32) -> bool {
            $crate::web::dirty(id)
        }

        /// Drop a canvas's surface.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_destroy(id: u32) {
            $crate::web::destroy(id)
        }

        /// The last failure's text.
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub fn gpu_error() -> String {
            $crate::web::error()
        }
    };
}
