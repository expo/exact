//! D11's wall-clock costs, collected only for an active performance resource.
//!
//! `exactCanvasPerformance("modelViewer")` with the optional `triangles` field
//! enables collection. Detailed phases are in `take_performance`'s JSON and the
//! surface agent's `{"op":"state","performance":true}` reply while enabled;
//! Contract's metrics schema admits only its standard fields. Values describe
//! the latest operation. Occlusion totals count updates since asset delivery.
//! Disabling collection drops every sample, including worker results still in flight.
//! `parseMs` includes BVH construction, on the same executor as parsing.
//! Detailed state also reports the BVH's heap allocation as `bvhBytes`.
//! `parseWaitMs` includes delivery copying, worker dispatch and the wait until a
//! frame consumes the result. `uploadMs` includes CPU packing, buffers, pipelines
//! and attachments. `loadToSubmitMs` ends at submission, before presentation.
//!
//! D11 and section 3's cost measurements, 2026-11-18
//!
//! Apple M5 Ultra, 36 logical CPUs, Xcode 27.2. Native: host-dev (release
//! optimizations), iPhone 17 Pro simulator, iOS 27.0, Metal, real clock. Web:
//! pinned full Chromium, WebGPU, web profile. Canvas: 320 x 320 points, 4x MSAA.
//! The generated GLB has two concentric UV spheres, radii 1 and 0.7, each with
//! 500 x 250 quads: 500,000 triangles, 251,502 vertices. Housing is an occluder;
//! Impeller is revealed. Twenty hotspot anchors form a 5 x 4 grid on Impeller,
//! 0.18 m apart, centred on x=y=0 at z=0.5 m. A centre pick returns Impeller.
//! glTF-Transform CLI 4.2.1 (SDK 4.5.1), meshoptimizer 0.25.0: optimize with
//! meshopt/medium and quantization; flatten, instance, join, palette, prune,
//! simplify, weld and texture-compress disabled. The model is generated in the
//! worktree's target directory, not committed: 1,228,000 bytes, SHA-256
//! 8796ffb6716bcbf599400cf690190f25fddf91f4e41eedbd2053dd3f0f68d2c1.
//!
//! Each batch starts a fresh process, then replaces the source seven times with
//! identical bytes under alternating asset names. The drive checks 500,000
//! triangles and the Impeller pick. Load is the machine's 1/5/15-minute average
//! sampled before each trial. All durations below are milliseconds.
//!
//! Native presentation is bracketed: submission is a lower bound; the elapsed
//! time read immediately after the host reports presented=true is an upper
//! bound, including observer polling and dispatch. An upper bound above 200
//! does not establish a miss. The initial shader-cache trial did miss: submit
//! 1433.48, observed presented by 1523.51, parse 21.28, preparation 3.09, GPU
//! setup 307.20, one occlusion update 40.31, at load 30.37/42.66/50.40. GPU setup
//! alone exceeded 200; the remaining cold scheduling delay was not attributed.
//! Later fresh processes reuse Metal's shader cache:
//!
//! | Trial | Load 1/5/15 | Parse | Wait | GPU setup | Present bounds | Pick | 20 hotspots |
//! |---|---|---:|---:|---:|---|---:|---:|
//! | A first | 81.33/62.10/55.67 | 25.34 | 16.05 | 31.03 | 120.07–188.76 | — | 40.03 |
//! | A1 | 81.33/62.10/55.67 | 25.23 | 2.82 | 27.24 | 100.93–157.36 | 7.50 | 40.00 |
//! | A2 | 81.33/62.10/55.67 | 23.06 | 44.35 | 27.42 | 140.50–196.84 | 7.56 | 39.93 |
//! | A3 | 81.33/62.10/55.67 | 20.82 | 0.86 | 28.06 | 95.68–151.77 | 7.62 | 40.07 |
//! | A4 | 81.33/62.10/55.67 | 26.22 | 44.32 | 28.32 | 145.74–202.71 | 7.45 | 40.33 |
//! | A5 | 81.33/62.10/55.67 | 26.97 | 34.63 | 28.29 | 135.77–192.00 | 7.57 | 40.03 |
//! | A6 | 81.33/62.10/55.67 | 23.31 | 3.72 | 27.91 | 100.60–155.56 | 7.46 | 39.92 |
//! | A7 | 81.33/62.10/55.67 | 24.38 | 46.64 | 27.32 | 143.66–203.78 | 7.37 | 39.88 |
//! | B first | 61.09/60.57/55.65 | 26.90 | 16.13 | 31.85 | 123.54–191.15 | — | 40.41 |
//! | B1 | 61.09/60.57/55.65 | 22.56 | 49.04 | 26.98 | 144.78–200.77 | 7.49 | 40.29 |
//! | B2 | 61.09/60.57/55.65 | 21.03 | 45.86 | 27.33 | 140.95–198.21 | 7.52 | 40.28 |
//! | B3 | 61.09/60.57/55.65 | 20.72 | 48.85 | 29.56 | 145.67–202.14 | 7.55 | 40.46 |
//! | B4 | 61.09/60.57/55.65 | 27.16 | 42.09 | 34.22 | 151.33–209.28 | 7.49 | 40.78 |
//! | B5 | 61.09/60.57/55.65 | 28.23 | 27.15 | 30.18 | 132.83–191.41 | 7.44 | 40.16 |
//! | B6 | 61.09/60.57/55.65 | 25.61 | 40.03 | 30.17 | 143.06–201.84 | 7.51 | 40.54 |
//! | B7 | 61.09/60.57/55.65 | 20.96 | 1.49 | 28.74 | 97.33–153.34 | 7.42 | 40.26 |
//! | C first | 69.83/74.51/69.11 | 22.35 | 2.13 | 31.39 | 105.70–323.57 | — | 40.49 |
//! | C1 | 69.83/74.51/69.11 | 30.18 | 26.11 | 28.42 | 132.94–189.38 | 7.66 | 40.78 |
//! | C2 | 69.83/74.51/69.11 | 21.67 | 42.16 | 28.99 | 139.22–195.10 | 7.61 | 40.41 |
//! | C3 | 69.83/74.51/69.11 | 23.04 | 28.24 | 28.52 | 126.56–182.37 | 7.79 | 40.52 |
//! | C4 | 69.83/74.51/69.11 | 21.79 | 30.45 | 28.37 | 127.70–183.81 | 7.55 | 40.58 |
//! | C5 | 69.83/74.51/69.11 | 22.66 | 29.01 | 27.87 | 125.82–182.04 | 7.64 | 40.52 |
//! | C6 | 69.83/74.51/69.11 | 23.34 | 29.49 | 29.16 | 127.98–183.58 | 8.64 | 40.26 |
//! | C7 | 69.83/74.51/69.11 | 23.76 | 27.93 | 28.05 | 126.29–181.27 | 7.64 | 40.60 |
//!
//! The original renderer rejects rays outside each node's box before scanning
//! triangles (koolkao/occlusion-viewer b63cff7, src/pick.rs); Exact's adaptation
//! omitted that rejection. The old 0.4–0.9 ms native figure used another model.
//! These concentric spheres cross both boxes at the centre, so box rejection
//! alone cannot accelerate that pick. The picker scans 500,000 triangles once
//! across its two disjoint primitive classes, not twice. Hotspot rays stop at
//! their first blocker. A fresh optimized macOS CPU run below is much faster
//! than the simulator rows; those rows do not isolate simulator, scheduling or
//! code-generation costs. A debug CPU run took 46.49/189.28 ms (pick/labels),
//! and optimizing only the viewer for size took 3.17/13.68 ms, versus the
//! host-dev baseline below. Neither establishes the cause of the old 7.5 ms.
//!
//! Amended 2026-11-18: D11's trigger is met. Picking and occlusion now share a
//! triangle BVH per primitive, built with the world-transformed geometry on the
//! parser's executor. Changing visibility, opacity or clipping filters queries;
//! replacing geometry rebuilds the tree. A pending source no longer invalidates
//! the retained picture's occlusion: a replacement with unchanged camera and
//! styles tests only the new picture. Geometry changes still force a retest.
//!
//! Fresh before/after runs use the same sphere construction and optimization
//! settings above: 1,227,964 bytes, SHA-256
//! 512ee0b92911723aa752bff4de0329b6f473994e1c6377de8d43357b026e184a.
//! Native rows are macOS CPU surface tests, host-dev, 25 operations after one
//! warm-up; values are median/max. Web rows use the production web profile in
//! pinned full Chromium/WebGPU, eight picks and eight 20-label updates; the
//! seven warm updates move the camera by 0.001 degrees with decay zero.
//! Native canvas: 400 x 400; web: 320 x 320. Every centre pick is Impeller.
//!
//! | Target | Load 1/5/15 | Pick before | Pick BVH | 20 labels before | 20 labels BVH |
//! |---|---|---:|---:|---:|---:|
//! | macOS A | 46.37/50.98/55.83 before; 45.62/50.18/53.70 after | 1.2638/1.5160 | 0.0010/0.0385 | 4.9225/5.5839 | 0.0030/0.0046 |
//! | macOS B | 50.79/58.46/57.05 | — | 0.0011/0.0044 | — | 0.0030/0.0045 |
//! | macOS C | 50.79/58.46/57.05 | — | 0.0011/0.0039 | — | 0.0031/0.0045 |
//! | macOS D | 40.36/41.67/49.31 | — | 0.0010/0.0025 | — | 0.0031/0.0046 |
//! | macOS E | 42.88/42.91/48.27 | — | 0.0015/0.0088 | — | 0.0045/0.0064 |
//! | macOS F | 138.33/98.26/73.44 | — | 0.0033/0.0102 | — | 0.0091/0.0113 |
//! | macOS G | 141.16/101.76/75.39 | — | 0.0036/0.0111 | — | 0.0094/0.0118 |
//! | web cold | 95.32/65.64/59.23 before; 58.22/60.30/57.63 after | 8.0 | <0.1 | 52.4 | <0.1 |
//! | web warm range | same | 4.4–4.7 | <0.1 | 13.0–13.6 | <0.1–0.2 |
//! | web recheck cold | 109.18/98.60/75.33 | — | 0.1 | — | 0.4 |
//! | web recheck warm | same | — | ≤0.1 | — | ≤0.1 |
//!
//! F, G and the web recheck use bounds expanded for floating-point rounding
//! so triangle-edge hits reach the unchanged triangle test.
//!
//! A second browser run at load 33.98/50.15/53.98 measured 0.2 ms for its
//! cold pick and at most 0.1 ms for subsequent picks and label updates.
//! Repeating the conservative-bounds recheck at load 84.00/92.33/75.99 gave
//! the same ray timings, with parse/build 187.40 ms and submission at 287.00 ms.
//!
//! BVH heap bytes: 6,194,336 native, 6,194,288 wasm32. Partition scratch is
//! released before the parser delivers the result. Native parse plus build:
//! 62.32, 53.70, 54.34, 57.20, 60.35, 91.00 and 218.89 ms. Web cold parse plus build: 187.70 ms, versus
//! 78.30 ms parsing before; load-to-submit was 285.10 versus 227.30 ms.
//! The web recheck parsed and built in 222.50 ms and submitted at 343.70 ms.
//! Both ray budgets pass. These results do not qualify the simulator's cold
//! first-frame budget, and no budget has been raised. The web still parses
//! and builds inline; moving that work to a worker remains outside this change.
//!
//! Piece 8's pre-BVH web parsing blocks the page once; camera preparation
//! blocks it afterwards.
//! The first trial includes cold Wasm/JIT paths; the seven replacements warm
//! those paths. GPU setup and hotspot work also block, outside these parse rows.
//!
//! | Trial | Load 1/5/15 | GLB parse | Camera preparation |
//! |---|---|---:|---:|
//! | first | 60.18/77.66/69.40 | 82.00 | 28.00 |
//! | 1 | 60.18/77.66/69.40 | 44.90 | 9.80 |
//! | 2 | 60.18/77.66/69.40 | 45.20 | 10.00 |
//! | 3 | 60.18/77.66/69.40 | 43.70 | 9.70 |
//! | 4 | 60.18/77.66/69.40 | 44.00 | 9.90 |
//! | 5 | 60.18/77.66/69.40 | 43.90 | 9.70 |
//! | 6 | 60.18/77.66/69.40 | 43.50 | 9.90 |
//! | 7 | 60.18/77.66/69.40 | 43.80 | 10.30 |
//!
//! Piece 8's pre-BVH module bytes (shared host code and model assets excluded):
//!
//! | Artifact | Raw | Gzip-9 | Brotli-11 |
//! |---|---:|---:|---:|
//! | Web Wasm | 468173 | 211325 | 177306 |
//! | Generated web bindings | 90977 | 16532 | 13804 |
//! | macOS dylib, host-dev | 5661672 | — | — |
//! | iOS simulator packaged dylib, host-dev | 5536832 | — | — |
//!
//! Native counts are development artifacts, not production-size claims. Current
//! iOS packaging places the GPU dylib in the app's Frameworks directory. Web
//! counts are the web build's on-demand package artifacts; metrics.mjs
//! --web-bytes enumerates its fixed app list, not package GPU modules. Nothing
//! here enters the eager page closure or an app without this package.
//!
//! Unqualified: macOS live presentation (no model frame arrived on the desktop),
//! physical devices and the reporter's model. Unit GPU readback and both Apple
//! builds passed. Presentation latency needs the same drive on an unlocked Mac.
#[derive(Default)]
pub struct Timing {
    pub arrival: Option<Stamp>,
    pub parse_ms: Option<f64>,
    pub parse_wait_ms: Option<f64>,
    pub prepare_ms: Option<f64>,
    pub upload_ms: Option<f64>,
    pub pick_ms: Option<f64>,
    pub load_to_submit_ms: Option<f64>,
}
impl Timing {
    pub fn snapshot(&self, hotspots: &crate::hotspots::Hotspots) -> serde_json::Value {
        let mut fields = serde_json::Map::new();
        for (name, ms) in [
            ("parseMs", self.parse_ms),
            ("parseWaitMs", self.parse_wait_ms),
            ("prepareMs", self.prepare_ms),
            ("uploadMs", self.upload_ms),
            ("pickMs", self.pick_ms),
            ("occlusionMs", hotspots.occlusion_ms),
            // Submission is observable by a surface; presentation belongs to the host.
            ("loadToSubmitMs", self.load_to_submit_ms),
        ] {
            if let Some(ms) = ms {
                fields.insert(name.into(), ms.into());
            }
        }
        if hotspots.occlusion_ms.is_some() {
            fields.insert(
                "occlusionTotalMs".into(),
                hotspots.occlusion_total_ms.into(),
            );
            fields.insert("occlusionUpdates".into(), hotspots.occlusion_updates.into());
        }
        serde_json::Value::Object(fields)
    }
}

pub struct Stamp {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
    #[cfg(target_arch = "wasm32")]
    start: f64,
}
impl Stamp {
    pub fn now() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            start: std::time::Instant::now(),
            #[cfg(target_arch = "wasm32")]
            start: now_ms(),
        }
    }
    pub fn elapsed(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start.elapsed().as_secs_f64() * 1000.
        }
        #[cfg(target_arch = "wasm32")]
        {
            now_ms() - self.start
        }
    }
}
#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .expect("modelViewer requires window.performance")
        .now()
}
