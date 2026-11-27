//! The modelViewer GPU surface (LLP 1133.039 D1–D12).
mod arguments;
mod bvh;
mod camera;
mod hotspots;
mod input;
mod labels;
mod loading;
mod pick;
mod presentation;
mod renderer;
mod styles;
mod timing;

use arguments::Arguments;
use camera::Camera;
use exact_gpu::{
    wgpu, AssetChanges, AssetError, ChildrenMode, Frame, InputEvent, Lifecycle, Placement,
    PointerPhase, Surface, SurfaceError, Value,
};
use exact_model_gltf::scene::Scene;
use input::Controls;
use loading::{Job, Source};
use pick::Clip;
use renderer::Renderer;
use std::{collections::HashMap, sync::mpsc};
use styles::Effective;

struct Parsed {
    sequence: u64,
    name: String,
    scene: Result<(Scene, bvh::Bvh), String>,
    timing_session: u64,
    parse_ms: Option<f64>,
}
struct Model {
    scene: Scene,
    bvh: bvh::Bvh,
    camera: Camera,
    effective: Vec<Effective>,
    clip: Clip,
    renderer: Option<Renderer>,
}
impl Model {
    fn style(&mut self, a: &Arguments) {
        let styles = styles::resolve(&self.scene, &a.nodes);
        self.effective = self
            .scene
            .primitives
            .iter()
            .map(|p| Effective::new(p, &styles[p.node]))
            .collect();
        self.clip = Clip {
            position: arguments::resolve(a.clip_position, self.scene.centre),
            normal: a.clip_normal,
        };
    }
}

pub struct ModelViewer {
    arguments: Arguments,
    source: Source,
    previous: Option<Box<Model>>,
    jobs: HashMap<String, Job>,
    drawn_generation: Option<u64>,
    #[cfg(not(target_arch = "wasm32"))]
    parsers: Vec<std::thread::JoinHandle<()>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    parser_joins: Option<mpsc::Sender<Option<std::thread::Result<()>>>>,
    #[cfg(not(target_arch = "wasm32"))]
    sender: mpsc::Sender<Parsed>,
    receiver: mpsc::Receiver<Parsed>,
    assets: AssetChanges,
    controls: Controls,
    hotspots: hotspots::Hotspots,
    last_frame: Option<Frame>,
    size: [f32; 2],
    seekable: bool,
    loaded: bool,
    error: String,
    generation: u64,
    publication: Option<String>,
    messages: Vec<String>,
    max_buffer_size: u64,
    force_samples: Option<u32>,
    timing_session: u64,
    timing: Option<timing::Timing>,
}
impl Default for ModelViewer {
    fn default() -> Self {
        let (_sender, receiver) = mpsc::channel();
        Self {
            arguments: Arguments::parse(&[]).unwrap(),
            source: Source::None,
            previous: None,
            drawn_generation: None,
            jobs: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            parsers: vec![],
            #[cfg(all(test, not(target_arch = "wasm32")))]
            parser_joins: None,
            #[cfg(not(target_arch = "wasm32"))]
            sender: _sender,
            receiver,
            assets: AssetChanges::default(),
            controls: Controls::default(),
            hotspots: hotspots::Hotspots::default(),
            last_frame: None,
            size: [1., 1.],
            seekable: false,
            loaded: false,
            error: String::new(),
            generation: 0,
            publication: None,
            messages: vec![],
            max_buffer_size: 256 * 1024 * 1024,
            force_samples: None,
            timing_session: 0,
            timing: None,
        }
    }
}
impl ModelViewer {
    fn model(&self) -> Option<&Model> {
        match &self.source {
            Source::Ready { scene, .. } => scene.as_deref(),
            _ => self.previous.as_deref(),
        }
    }
    fn model_mut(&mut self) -> Option<&mut Model> {
        match &mut self.source {
            Source::Ready { scene, .. } => scene.as_deref_mut(),
            _ => self.previous.as_deref_mut(),
        }
    }
    fn pick(&mut self, [x, y]: [f32; 2]) {
        let start = self.timing.as_ref().map(|_| timing::Stamp::now());
        let name = self
            .model()
            .map(|m| {
                let (origin, direction) = m.camera.pose.ray(
                    x,
                    y,
                    self.size[0],
                    self.size[1],
                    m.camera.vertical_fov(self.size[0] / self.size[1]),
                );
                pick::pick(
                    &m.scene,
                    &m.bvh,
                    &m.effective,
                    m.clip,
                    origin,
                    direction,
                    m.camera.ray_depth(direction),
                )
            })
            .unwrap_or_default();
        self.messages.push(name);
        if let Some(timing) = &mut self.timing {
            timing.pick_ms = start.map(|s| s.elapsed());
        }
    }
}

/// A fresh surface for one canvas.
pub fn surface() -> Box<dyn Surface> {
    Box::<ModelViewer>::default()
}

impl Surface for ModelViewer {
    fn arguments(&self) -> Vec<(&'static str, Value)> {
        arguments::NAMES
            .into_iter()
            .zip(arguments::defaults())
            .collect()
    }
    fn bind(&mut self, inputs: &[Value], at_ms: Option<f64>) -> Result<(), SurfaceError> {
        let next = Arguments::parse(inputs)?;
        let source_changed = next.src != self.arguments.src;
        let camera_changed = next.orbit != self.arguments.orbit
            || next.target != self.arguments.target
            || next.fov != self.arguments.fov;
        if !next.controls {
            self.controls.clear();
        }
        self.hotspots.dirty |= next.hotspots != self.arguments.hotspots
            || next.nodes != self.arguments.nodes
            || next.clip_position != self.arguments.clip_position
            || next.clip_normal != self.arguments.clip_normal;
        self.arguments = next;
        if source_changed {
            self.request();
        }
        let arguments = self.arguments.clone();
        if let Some(model) = self.model_mut() {
            if let Some(at) = at_ms {
                model.camera.step(&model.scene, at);
            }
            model
                .camera
                .rebind(&model.scene, &arguments, camera_changed);
            model.style(&arguments);
        }
        Ok(())
    }
    fn clock(&mut self, seekable: bool) {
        self.seekable = seekable;
    }
    fn size(&mut self, width: f32, height: f32) -> bool {
        let size = [width.max(1.), height.max(1.)];
        let changed = self.size != size;
        self.size = size;
        changed
    }
    fn assets(&mut self) -> AssetChanges {
        std::mem::take(&mut self.assets)
    }
    fn asset(&mut self, name: &str, bytes: Result<&[u8], AssetError>) {
        self.deliver(name, bytes);
    }
    fn accepts_asset_update(&self, name: &str) -> bool {
        self.arguments.src.starts_with("assets/") && self.source.name() == Some(name)
    }
    fn preparing(&self) -> bool {
        self.source.load().is_some()
    }
    fn frame_ready(&self) -> bool {
        let sequence = match self.source {
            Source::None => self.generation,
            Source::Ready { sequence, .. } => sequence,
            _ => return false,
        };
        self.drawn_generation == Some(sequence)
    }
    fn generation(&self) -> u64 {
        self.generation
    }
    fn srgb_target(&self) -> bool {
        true
    }
    fn wants_input(&self) -> bool {
        self.arguments.controls
    }
    fn wants_pads(&self) -> bool {
        false
    }
    fn device_ready(&mut self, _: wgpu::Features, limits: wgpu::Limits) {
        self.max_buffer_size = limits.max_buffer_size;
    }
    fn device_lost(&mut self) {
        self.hotspots.device_lost();
        if let Some(model) = self.model_mut() {
            model.renderer = None;
        }
        if let Some(name) = match &self.source {
            Source::Pending(load) => Some(&load.name),
            Source::Parsing { load, job } if load.sequence != *job => Some(&load.name),
            _ => None,
        } {
            if !self.assets.requests.iter().any(|r| r == name) {
                self.assets.requests.push(name.clone());
            }
        }
    }
    fn lifecycle(&mut self, event: Lifecycle) {
        if matches!(event, Lifecycle::Hidden | Lifecycle::Interrupted) {
            self.controls.clear();
        }
        if let Some(model) = self.model_mut() {
            model.camera.last_ms = None;
        }
    }
    fn input(&mut self, event: &InputEvent) {
        if matches!(
            event,
            InputEvent::Blur { .. }
                | InputEvent::Pointer {
                    phase: PointerPhase::Cancel,
                    ..
                }
        ) {
            self.controls.clear();
            return;
        }
        if let InputEvent::Message { text, at_ms } = event {
            if text == "reset" {
                if let Some(model) = self.model_mut() {
                    model.camera.step(&model.scene, *at_ms);
                    model.camera.reset(&model.scene);
                }
            } else if let Some(coordinates) = text.strip_prefix("pick ") {
                let p: Vec<_> = coordinates
                    .split_whitespace()
                    .map(str::parse::<f32>)
                    .collect();
                if let [Ok(x), Ok(y)] = p.as_slice() {
                    if x.is_finite() && y.is_finite() {
                        self.pick([*x, *y]);
                    }
                }
            }
            return;
        }
        if !self.arguments.controls {
            return;
        }
        let arguments = self.arguments.clone();
        let size = self.size;
        let model = match &mut self.source {
            Source::Ready { scene, .. } => scene.as_deref_mut(),
            _ => self.previous.as_deref_mut(),
        };
        if let Some(model) = model {
            let time = match event {
                InputEvent::Key { at_ms, .. }
                | InputEvent::Wheel { at_ms, .. }
                | InputEvent::Pointer { at_ms, .. } => Some(*at_ms),
                _ => None,
            };
            if let Some(time) = time {
                model.camera.step(&model.scene, time);
            }
            if let Some(point) =
                self.controls
                    .event(event, &model.scene, &mut model.camera, &arguments, size)
            {
                self.pick(point);
            }
        }
    }
    fn messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }
    fn children_mode(&self) -> ChildrenMode {
        ChildrenMode::Each
    }
    fn child(
        &mut self,
        index: usize,
        name: &str,
        texture: Option<&wgpu::TextureView>,
        frame: [f32; 4],
    ) {
        self.hotspots.child(index, name, texture, frame);
        // A host can supply new child slots after rendering, then read placements immediately.
        if let Some(frame) = self.last_frame {
            let model = match &self.source {
                Source::Ready { scene, .. } => scene.as_deref(),
                _ => self.previous.as_deref(),
            };
            self.hotspots.update(model, &self.arguments, &frame);
        }
    }
    fn placement(&self, index: usize) -> Option<Placement> {
        self.hotspots.children.get(index)?.placement
    }
    fn placement_opacity(&self, index: usize) -> f32 {
        self.hotspots.children.get(index).map_or(1., |c| c.opacity)
    }
    fn agent(&mut self, request: &str) -> Option<String> {
        let request: serde_json::Value = serde_json::from_str(request).ok()?;
        if request["op"] == "state" && request["performance"] == true {
            let timing = self.timing.as_ref()?;
            let mut metrics = timing.snapshot(&self.hotspots);
            if let Some(model) = self.model() {
                metrics["bvhBytes"] = model.bvh.bytes().into();
            }
            if let Some(arrival) = &timing.arrival {
                metrics["sinceArrivalMs"] = arrival.elapsed().into();
            }
            return Some(serde_json::json!({ "performance": metrics }).to_string());
        }
        if request["op"] != "clock" {
            return None;
        }
        let camera = self.model().map(|m| &m.camera);
        let moving = camera.is_some_and(Camera::moving);
        let mut reply = serde_json::json!({
            "quiescent": !moving && !self.preparing(),
            "changing": if self.preparing() { vec!["loading"] }
                else if moving { vec!["camera"] } else { vec![] }
        });
        if moving {
            let camera = camera.unwrap();
            let now = request["now"].as_f64().or(camera.last_ms).unwrap_or(0.);
            reply["settleAt"] = camera.settle_at(now).into();
        }
        Some(reply.to_string())
    }
    fn published(&mut self) -> Option<String> {
        let model = self.model();
        let camera = model.map(|m| m.camera.pose.strings());
        let nodes: Vec<_> = model
            .map(|m| {
                m.scene
                    .nodes
                    .iter()
                    .map(|n| {
                        serde_json::json!({
                            "name": n.name,
                            "parent": n.parent.map(|i| m.scene.nodes[i].name.as_str()).unwrap_or("")
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut value = serde_json::json!({ "src": self.arguments.src, "loaded": self.loaded,
            "error": self.error, "triangles": model.map_or(0, |m| m.scene.triangles()), "nodes": nodes });
        // A moving camera keeps the last settled values in its public record.
        let settled = model.is_none_or(|m| !m.camera.moving());
        let previous = self
            .publication
            .as_ref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        let (orbit, target, fov, view) = camera.unwrap_or_default();
        for (name, text) in [
            ("cameraOrbit", orbit),
            ("cameraTarget", target),
            ("fieldOfView", fov),
            ("cameraView", view),
        ] {
            value[name] = if settled {
                text.into()
            } else {
                previous
                    .as_ref()
                    .and_then(|p| p.get(name))
                    .cloned()
                    .unwrap_or_else(|| "".into())
            };
        }
        let json = value.to_string();
        if self.publication.as_ref() == Some(&json) {
            None
        } else {
            self.publication = Some(json.clone());
            Some(json)
        }
    }
    fn set_performance_reporting(&mut self, enabled: bool) {
        if enabled != self.timing.is_some() {
            self.timing_session += 1;
            self.timing = enabled.then(timing::Timing::default);
            self.hotspots.reporting = enabled;
            self.hotspots.reset_timing();
        }
    }
    fn take_performance(&mut self, _now_ms: f64) -> Option<String> {
        let timing = self.timing.as_ref()?;
        let mut metrics = timing.snapshot(&self.hotspots);
        if let Some(model) = self.model() {
            metrics["triangles"] = model.scene.triangles().into();
        }
        Some(metrics.to_string())
    }
    fn submitted(&mut self) {
        if self.loaded && self.frame_ready() {
            if let Some(timing) = &mut self.timing {
                if let Some(arrival) = &timing.arrival {
                    if timing.load_to_submit_ms.is_none() {
                        timing.load_to_submit_ms = Some(arrival.elapsed());
                    }
                }
            }
        }
    }
    fn render(
        &mut self,
        frame: &Frame,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> bool {
        self.poll();
        self.size(frame.width, frame.height);
        self.last_frame = Some(*frame);
        let complete = !self.preparing();
        let size = frame.pixels();
        let samples = self.force_samples.unwrap_or_else(|| {
            renderer::samples(format.guaranteed_format_features(device.features()).flags)
        });
        let model = match &mut self.source {
            Source::Ready { scene, .. } => scene.as_deref_mut(),
            _ => self.previous.as_deref_mut(),
        };
        let Some(model) = model else {
            self.hotspots.update(None, &self.arguments, frame);
            renderer::clear(encoder, target);
            self.drawn_generation = complete.then_some(self.generation);
            return self.preparing();
        };
        let moving = model.camera.step(&model.scene, frame.now_ms);
        self.hotspots.update(Some(model), &self.arguments, frame);
        if model
            .renderer
            .as_ref()
            .is_none_or(|r| r.size != size || r.format != format || r.samples != samples)
        {
            let start = self.timing.as_ref().map(|_| timing::Stamp::now());
            match Renderer::new(device, &model.scene, size, format, samples) {
                Ok(renderer) => model.renderer = Some(renderer),
                Err(SurfaceError(error)) => {
                    self.fail(error);
                    self.hotspots.update(None, &self.arguments, frame);
                    renderer::clear(encoder, target);
                    self.drawn_generation = Some(self.generation);
                    return false;
                }
            }
            if let Some(timing) = &mut self.timing {
                timing.upload_ms = start.map(|s| s.elapsed());
            }
        }
        model.renderer.as_ref().unwrap().draw(
            queue,
            encoder,
            &model.scene,
            &model.camera,
            &model.effective,
            model.clip,
        );
        model.renderer.as_mut().unwrap().presentation.draw(
            device,
            encoder,
            target,
            &mut self.hotspots,
            frame,
        );
        self.loaded = complete;
        self.drawn_generation = complete.then_some(self.generation);
        moving || self.preparing()
    }
}

#[cfg(test)]
mod tests;
