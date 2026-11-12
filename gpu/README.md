# GPU module ABI

The host and module ship together. Before 1.0, missing required symbols refuse
loading; there is no compatibility shim. See LLP 1009 and LLP 1014.000.

| Version / revision | Child delivery export | Contract |
|---|---|---|
| GPU ABI 1 / exact-gpu 0.1.0, before `126daba5` | `gpu_child` | Unnamed child pixels and geometry. |
| GPU ABI 1 / exact-gpu 0.1.0, from `126daba5` | `gpu_child_view` | **Breaking host ABI change:** adds UTF-8 `testId` and its byte length before geometry and pixel arguments, on native and Wasm. All embedders must rebuild against this revision. |

GPU tests use `exact_gpu::fixture::device_or_skip`: only `no adapter:` skips;
request-device, configuration, validation, and device-loss errors fail.

Windows rendered modules also export `gpu_window_frame` and `gpu_window_detach`;
rebuild the host and module together. A full-window canvas can present directly
from its module-owned GPU, composing premultiplied RGBA UI layers below and above
the game. The HWND and HINSTANCE remain valid until detach or module unload.
No GPU device or texture object crosses the DLL boundary. Transformed, clipped,
multiple-canvas and backdrop-dependent layouts use ordinary capture instead.
Canvas ID zero presents the complete CPU UI frame after the game is unmounted,
so menus continue through the same swapchain instead of switching back to GDI.
The final flags argument uses bit 0 for changed UI and bit 1 for a solid underlay.
With bit 1 set, the below pointer holds only four premultiplied RGBA bytes;
otherwise it holds the full pixel buffer. The above pointer always holds `len`
bytes. A solid underlay is cleared on the GPU without a CPU pixel upload.
The final two arguments, `above_top` and `above_bottom`, bound all nontransparent
overlay pixels in device rows (end exclusive). Unknown bounds use the whole
height. The uploader compares the union of current and previous bounds, so
moving or removing UI also clears its old pixels. Rebuild both host and module
for this signature.
Capture reuses its render target and staging buffer until size, format or device
changes. The Windows host follows the current monitor's refresh rate.

For native performance measurements, set `EXACT_AGENT_WINDOW_FRAMES` (1–10000)
and optionally `EXACT_AGENT_WINDOW_TAP` to a launch button's test ID.
`EXACT_AGENT_WINDOW_FULLSCREEN=1` uses borderless fullscreen;
`EXACT_AGENT_WINDOW_UNPACED=1` removes pacing to measure rendering capacity.
These switches require a bounded development run and are stripped in production.

## Frame pacing and timing

A surface that rests returns `false` from `render`; the host then asks for no
frame until the canvas is dirty (an input, a bind, an asset), its box or scale
changes, or the agent's clock moves. `Surface::max_fps` caps its drawn frames (0
for none). Each display frame a pacing host asks `gpu_due(id, now_ms)` before it
would render a canvas, and skips a canvas that is not due while still asking for
the next frame: the deadline sits on the lattice of the cap's interval, so a cap
of 30 on a 120 Hz display draws every fourth frame, a missed slot is skipped
rather than caught up, and the first frame after a rest draws at once. Under the
agent's clock (`gpu_seekable(true)`) every canvas is due. A cap below 5 draws
5: a longer gap than the live clock carries in one frame (250 ms) would slow a
world's ticks. `gpu_max_fps(id)`
(native) reports the cap, which the Apple host uses to lower its `CADisplayLink`'s
preferred rate while nothing else on screen asks for more.

Before each frame `Surface::timing` receives a `FrameTiming`: the interval since
the canvas's previous drawn frame (0 after a rest), the display period
(`gpu_period`), the host's callback (`gpu_host_frame(raw_ms, callback_ms,
fastest_period_ms, thermal)`: its own timestamp and when it began to run, on the
frame clock, and the display's fastest period) and a `ThermalState` (0 unknown,
1 nominal, 2 fair, 3 serious, 4 critical). Apple reports `ProcessInfo.thermalState`;
the web, Android, Linux and Windows report unknown.

| host | rests | caps | timing |
|---|---|---|---|
| web | yes | `gpu_due` in the `requestAnimationFrame` loop | callback times; thermal unknown |
| iOS, tvOS, macOS | yes | capped `CADisplayLink` range and `gpu_due` per canvas | callback times, fastest period, thermal state |
| Android | yes | `gpu_due` per canvas; the cap asked of the platform: `Surface.setFrameRate` as a fixed source for a canvas with a window of its own (Android 11 and later), `View.setRequestedFrameRate` for one in the reader's window (Android 15 and later) | `gpu_period`: Choreographer's vsync spacing (Android 13 and later), else the display's refresh |
| Linux (display) | no: a rendered canvas repaints every display frame | no | `gpu_period`: the DRM mode's pixel clock over its total pixels |
| Linux headless, Windows (readback and window frame) | no: a rendered canvas repaints every display frame | no | none |

## Performance

Three things decide what a shader-heavy module costs on a phone. The figures
below are one reporter's, from a Solana Seeker (Android 16, Mali, 120 Hz), and
were not measured here (#300).

**Shader runtime checks.** `Device::create_shader_module` compiles with every
runtime check of wgpu's [`ShaderRuntimeChecks`](https://docs.rs/wgpu-types/30.0.1/wgpu_types/struct.ShaderRuntimeChecks.html)
on. `force_loop_bounding` puts a counter in every loop so that a loop that never
ends cannot hang the GPU, and a long ray march pays for it on each iteration.
`unsafe { device.create_shader_module_trusted(desc, checks) }` with only that
check off is sound only when every loop in the module ends for every input the
shader can see, uniforms and textures included: wgpu's documentation lets a
backend compiler treat a loop that does not as unreachable, which is undefined
behaviour. Keep the other checks (`bounds_checks`, `int_div_checks`) on unless
you hold their own conditions. Turning off loop bounding alone took the
reporter's gallery of ray-marched models from 54 to 63 fps (p90 24.9 ms to
16.6 ms); bounds checks cost nothing measurable there. The browser ignores
these settings and applies WebGPU's own.

**`Surface::opaque`.** A full-window canvas that returns `true` (and not
`high_dynamic_range`) is presented on Android in a `SurfaceView` beneath the UI,
a layer the system compositor takes as it is. Otherwise the canvas is composed
into the window, so each frame it draws makes HWUI's RenderThread redraw the
window: the reporter's full-window gallery cost 32 % of a CPU core there until
it declared itself opaque. Return `true` when every pixel the surface writes
has alpha one.

**The pipeline cache.** `exact_gpu::PipelineCache::new(device)` gives a renderer
the cache Exact installs for its module's device. Pass its `handle` as the
`cache` of every render and compute pipeline descriptor, and after each submit
call `save(work)` with the number of pipelines the renderer has created so far;
a write happens on a worker thread only when that number has grown. On Android
(Vulkan) the cache is kept on disk per app build and GPU, so a warm launch
builds its pipelines from it: the reporter's 90 pipelines took 0.32 s warm and
5.9 s cold; the log says `GPU module pipeline cache: loaded`, `miss` or
`unavailable` when the module loads. On Vulkan elsewhere the cache lives only in
memory. Other backends have no wgpu cache `handle`; Apple uses the Metal archive
below through the same `PipelineCache` owner. `game/render` is a whole user
(`pipeline.rs`, `renderer/draw.rs`).

### Metal pipeline archives

Apple GPU modules keep a Metal binary archive in the app's cache directory,
shared by its surfaces and written after submitted frames. The cache key includes
the module name/version, app identity, GPU registry identity, OS build and archive
encoding version. Metal matches shader functions and complete pipeline descriptors;
a shader or specialization change misses the archive and compiles normally.
Invalid or incompatible data falls back to an empty archive. Writes use the same
checksummed envelope and atomic replacement as Vulkan's pipeline cache.

On Apple, the default directory is `~/Library/Caches/exact-gpu-<app hash>`;
`XDG_CACHE_HOME` replaces its parent for a controlled run. To preseed a matching
installation, copy its `exact-*.bin` files into the app bundle's resources under
`exact-pipelines/`, or point `EXACT_GPU_CACHE_SEED` at that directory. The seed is
read-only and is used when no valid writable cache exists. It must match the cache
key, including device and OS build; this is not a portable archive for every GPU.
A fresh installation without a matching seed still compiles its initial pipelines,
but not their MSL when the build shipped it compiled (below).

**Shader libraries compiled at build time.** wgpu writes each pipeline's MSL at run
time. A development build on a Mac or a simulator records each source it compiled in
the app's `.exact/metal/` (the bundle's `ExactMetalSources`); the next Apple build
compiles those for its SDK (`host/apple/shader-libraries.mjs`, kept in
`target/metal-kept/shaders`) into the bundle's `exact-metal/`, and the module loads
a library in place of a source whose text and options match its name
(`vendor/wgpu-hal/EXACT-PATCHES.md` 16). A changed shader, layout or OS that writes
different MSL compiles at run time and is recorded for the next build. So a build
ships what development runs drew: run the app's screens before building a release.
`EXACT_METAL_LIBRARIES` and `EXACT_METAL_SOURCES` replace either directory. On an
M5 Max, a fresh install's 28 first-frame pipelines took 0.8 to 1.1 s instead of 3.7
to 3.9 s; on the iOS simulator, whose GPU compile no archive keeps, 3.8 to 4.0 s
instead of 4.0 to 4.3.
Render and compute pipelines participate; mesh-shader pipelines do not yet.

The backend's `BinaryArchive::counts` reports successful binary lookups and misses.
The process-isolated Metal test renders and reads pixels after both cold and warm
loads, and covers a seed, a changed shader, a changed key and corrupt data.

## A module's own audio output

A surface posts the string `"exact:audio"` through `Surface::messages` to ask
for the app's audio session. The host consumes it without dispatching it to
the app. On iOS and tvOS, ExactKit applies the app's `audio_session` category
and options and activates the session. A first successful request sends
nothing back. A failed activation sends `Lifecycle::Interrupted` to the
host's surfaces; recovery sends `Lifecycle::Resumed`. Failure retries after
300 live display-link frames, on a Hidden → Visible transition or a key/pointer
gesture. An interruption ending without `shouldResume` blocks automatic
retries, including repeated `exact:audio` requests, until visibility or input
allows recovery. Hidden surfaces do not retry.

Immediately before a GPU module makes a surface on iOS or tvOS, the owner
sets an unclaimed session to `.ambient`, without activating it. Output opened
before its request is handled therefore mixes with other apps' audio and
obeys the ring/silent switch until the request takes the app's configuration.
Under `"playback-mix"` the owner sets the app's own configuration instead
(`.playback` with `.mixWithOthers`), so output may start before the request
and the request only activates: a category change while output plays
interrupts other apps' audio, even toward a mixing category. Under
`"playback"` an early `.playback` would stop other apps' audio before the
request, so that app is prepared in `.ambient`; its category rises when the
request is handled, which is what `"playback"` does anyway.
A held or claimed session stays as it is. This preparation does not run for
Canvas 2D, an app with no GPU canvas, or the driver's frozen clock, which
makes surfaces seekable and opens no output. Under `EXACT_AGENT_TIMING=platform`
the owner prepares too: surfaces can open output, but the driver drops
`exact:audio` requests, so they do not raise the prepared ambient session.
macOS, the web, Linux and Android consume the request without configuring a session.
On Android that means a module's output gets no audio focus and no interruption
report from Exact: the module asks Android for focus and handles its loss itself.

## A module's own haptics

A surface plays haptics from its own callbacks, where the gesture and the
physics are, by returning `exact_gpu::haptics::Haptic::message()` strings from
`Surface::messages` (LLP 1009 D10). The host consumes them; the app never sees
them. The names are Core Haptics':

- `Haptic::texture(intensity, sharpness)` starts the surface's texture, a
  continuous vibration, or moves it to new values. Send it as often as every
  frame while the finger drags; each value is 0–1. `Haptic::Stop` stops it.
  A surface has one texture.
- `Haptic::Pattern` plays a short pattern now, beside the texture: transient
  events (taps) and continuous ones, each at its time with its intensity and
  sharpness, and curves over the pattern that multiply its intensity or add to
  its sharpness (`CHHapticParameterCurve`). A pattern lasts at most 30 s.

The host owns their life. A surface's texture stops when the surface is
destroyed, and every texture stops while the canvases are hidden or interrupted
(iOS) or the activity is paused (Android); a later texture message starts it
again, so a surface that sends one each frame resumes by itself. The person's
system haptics setting applies, as it does to `haptic()`.

- **iOS**: Core Haptics, one `CHHapticEngine` for the process, haptics only, so
  no audio session is involved. A texture is a looping continuous event moved by
  `hapticIntensityControl` and `hapticSharpnessControl`; a pattern is a
  `CHHapticPattern` played at once. A device without a haptic engine plays
  nothing.
- **Android**: the vibrator (`host/android/README.md`, "Haptics"). A texture is
  an amplitude, so its sharpness is not played, and a device without amplitude
  control plays no texture; a pattern is click and tick primitives, or a
  waveform. The shell's manifest asks for `VIBRATE`.
- **macOS, tvOS, the web, Linux, Windows**, and every host under the driver,
  consume the messages and play nothing.
