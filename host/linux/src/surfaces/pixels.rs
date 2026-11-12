//! The portable native display baseline: GPU canvas readback composed by the
//! same painter as Canvas 2D. Device-free agent runs never enter this path.
use super::*;
use std::sync::Arc;
use tiny_skia::{IntSize, Pixmap};

impl Surfaces {
    pub(crate) fn has_rendered_canvas(&self) -> bool {
        self.canvases
            .values()
            .any(|c| self.abis.get(&c.artifact).is_some_and(|a| a.rendered))
    }
    pub(crate) fn pixels<D: DataSource>(
        &mut self,
        host: &mut Host<D>,
        scale: f32,
        mut previous: BTreeMap<u32, crate::canvas2d::CanvasPaint>,
        skip: Option<u32>,
    ) -> BTreeMap<u32, crate::canvas2d::CanvasPaint> {
        let mut result = BTreeMap::new();
        let mut shown = Vec::new();
        for (&view, canvas) in &self.canvases {
            if skip == Some(view) {
                continue;
            }
            let Some(abi) = self.abis.get(&canvas.artifact).filter(|a| a.rendered) else {
                continue;
            };
            let Some(node) = host.kernel().node(view) else {
                continue;
            };
            let (width, height) = (node.frame.width, node.frame.height);
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            let (w, h) = (
                (width * scale).round() as u32,
                (height * scale).round() as u32,
            );
            let Some(size) = IntSize::from_wh(w, h) else {
                continue;
            };
            let Some(len) = (w as usize)
                .checked_mul(h as usize)
                .and_then(|n| n.checked_mul(4))
                .filter(|&n| n <= LIMIT)
            else {
                host.log("GPU canvas exceeds native readback limit");
                continue;
            };
            // The previous CPU canvas is no longer being painted. Reuse its
            // allocation only when there are no retained readers.
            let mut bytes = previous
                .remove(&view)
                .and_then(|c| Arc::try_unwrap(c.pixels).ok())
                .filter(|p| p.width() == w && p.height() == h)
                .map(Pixmap::take)
                .unwrap_or_else(|| vec![0; len]);
            let code = unsafe {
                abi.symbol::<unsafe extern "C" fn(u32, f32, f32, f32, f64, *mut u8, usize) -> u32>(
                    b"gpu_readback",
                )(
                    canvas.id,
                    width,
                    height,
                    scale,
                    host.now(),
                    bytes.as_mut_ptr(),
                    bytes.len(),
                )
            };
            if matches!(code, 0 | 2) {
                shown.push(view);
                if let Some(pixels) = Pixmap::from_vec(bytes, size) {
                    result.insert(
                        view,
                        crate::canvas2d::CanvasPaint {
                            pixels: Arc::new(pixels),
                        },
                    );
                }
            } else {
                host.log(format!(
                    "GPU canvas readback failed ({code}): {}",
                    abi.error().unwrap_or_default()
                ));
            }
        }
        // These pixels are this frame's picture of each canvas: the module
        // counts a complete one presented (`gpu_readback`).
        self.owe_records(shown);
        result
    }

    /// Take `views`' canvases' records now, for the next sync to give the
    /// app: a frame just presented may have made a record's `presented` true
    /// (LLP 1009), and a canvas that wants no further frame would not be
    /// synced again to hear it. A presenter runs another frame while records
    /// are owed ([`Surfaces::owes_records`]).
    pub(crate) fn owe_records(&mut self, views: impl IntoIterator<Item = u32>) {
        for view in views {
            let Some(c) = self.canvases.get(&view) else {
                continue;
            };
            let Some(bytes) = self.abis[&c.artifact].read(b"gpu_published", c.id) else {
                continue;
            };
            if c.owner {
                let record = String::from_utf8_lossy(&bytes).into_owned();
                self.owed.push((c.name.clone(), record));
            }
        }
    }

    /// Records are owed to the app ([`Surfaces::owe_records`]).
    pub(crate) fn owes_records(&self) -> bool {
        !self.owed.is_empty()
    }

    /// Give the app the owed records, first thing in a sync, before a canvas
    /// that went releases its name; whether there were any.
    pub(super) fn forward_owed<D: DataSource>(&mut self, host: &mut Host<D>) -> bool {
        let owed = std::mem::take(&mut self.owed);
        let any = !owed.is_empty();
        for (name, record) in owed {
            self.error = self
                .error
                .take()
                .or(host.surface_record(&name, Some(&record)));
        }
        any
    }
}
