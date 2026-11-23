//! Canvas handles use the file's natural pixels for source rectangles and patterns.
use super::*;

pub(super) struct Read {
    source: Option<Arc<SourceOwner>>,
    request: Option<(RequestId, PixelSize)>,
    /// This read's own view in the cache, so what a worker asked for it is
    /// never another read's.
    view: u64,
    /// Cleared as the read goes, for a worker that asks for it (`Want`).
    alive: Arc<std::sync::atomic::AtomicBool>,
    /// Retired as the read goes, with whatever a worker asked under it.
    backend: std::sync::Weak<Backend>,
    canvas_generation: u64,
}

impl Default for Read {
    fn default() -> Self {
        static VIEWS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            source: None,
            request: None,
            view: VIEWS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            alive: Arc::new(true.into()),
            backend: std::sync::Weak::new(),
            canvas_generation: 0,
        }
    }
}

impl Drop for Read {
    fn drop(&mut self) {
        self.alive
            .store(false, std::sync::atomic::Ordering::Release);
        if let Some(backend) = self.backend.upgrade() {
            backend.session.retire_view(ViewKey {
                view: self.view,
                generation: self.canvas_generation,
            });
        }
    }
}

/// A canvas's ask for `source`: the whole picture, halved until its decode
/// fits `available`.
fn demand(
    header: png_decode::Header,
    view: ViewKey,
    source: u64,
    generation: u64,
    available: u64,
) -> Result<Demand, Refusal> {
    let natural = header.metadata.natural;
    let mut pixels = (natural.width, natural.height);
    let decode = loop {
        let plan = png_decode::DecodePlan::new(header, pixels);
        if plan
            .as_ref()
            .is_ok_and(|plan| plan.peak_bytes() <= available)
            || pixels == (1, 1)
        {
            break plan?;
        }
        pixels = (pixels.0.div_ceil(2), pixels.1.div_ceil(2));
    };
    Ok(Demand {
        view,
        key: RasterKey {
            source,
            generation,
            pixels: decode.pixels,
            variant: exact_raster::variant::SRGB8,
            crop: decode.crop,
        },
        metadata: header.metadata,
        cost: decode.cost,
        priority: Priority::Visible,
    })
}

impl Images {
    pub(crate) fn ask_canvas(&mut self, name: String) {
        if name.len() > source_limit(&name) {
            self.canvas_answers
                .push((name, Err("image source exceeds its length limit".into())));
        } else {
            self.canvas_reads.entry(name).or_default();
        }
    }

    pub(crate) fn cancel_canvas(&mut self, name: &str) {
        if let Some(request) = self.canvas_reads.remove(name).and_then(|r| r.request) {
            self.backend.cancel(request.0);
        }
        self.canvas_answers.retain(|(src, _)| src != name);
    }

    pub(crate) fn retain_canvas_reads(&mut self, mut named: impl FnMut(&str) -> bool) {
        self.canvas_reads.retain(|source, read| {
            if named(source) {
                return true;
            }
            if let Some(request) = read.request {
                self.backend.cancel(request.0);
            }
            false
        });
        self.start_canvas_reads();
    }

    pub(crate) fn start_canvas_reads(&mut self) {
        self.canvas_reads.retain(|name, read| {
            let result = read.poll(
                &self.backend,
                &self.assets,
                self.generation,
                self.canvas_generation,
                name,
            );
            match result {
                Ok(None) => true,
                Err(e) if passing(e) => true,
                finished => {
                    if let Some(request) = read.request {
                        self.backend.cancel(request.0);
                    }
                    let answer = finished
                        .map(|lease| lease.expect("completed canvas image"))
                        .map_err(|e| format!("{name}: {}", refusal_text(e)));
                    self.canvas_answers.push((name.clone(), answer));
                    false
                }
            }
        });
    }
}

impl Read {
    fn poll(
        &mut self,
        backend: &Arc<Backend>,
        assets: &Arc<Assets>,
        generation: u64,
        canvas_generation: u64,
        name: &str,
    ) -> Result<Option<RasterLease>, Refusal> {
        self.backend = Arc::downgrade(backend);
        self.canvas_generation = canvas_generation;
        let view = ViewKey {
            view: self.view,
            generation: canvas_generation,
        };
        // A capture's own pixels, still its file's: taken as asked, with no
        // header to read.
        if self.request.is_none() {
            if let Some(published) = backend.published(generation, name) {
                let demand = Demand {
                    view,
                    priority: Priority::Visible,
                    ..published
                };
                if let Ok(request) = backend.session.request_cached(demand) {
                    self.request = Some((request, published.key.pixels));
                    return Ok(backend.session.take_ready(request));
                }
            }
        }
        if self.source.is_none() {
            let source = backend.source(generation, name, assets, false)?;
            source
                .canvas
                .store(true, std::sync::atomic::Ordering::Relaxed);
            self.source = Some(source);
        }
        let source = self.source.as_ref().unwrap().id;
        let available = match self.request {
            Some((request, _)) => match backend.session.status(request) {
                Some(RequestStatus::Ready) => return Ok(backend.session.take_ready(request)),
                Some(RequestStatus::Failed(e)) => {
                    backend.cancel(request);
                    self.request = None;
                    return Err(e);
                }
                Some(RequestStatus::WaitingBudget) => backend.session.available_bytes(),
                Some(_) => return Ok(None),
                None => {
                    backend.forget(request);
                    self.request = None;
                    backend.session.budget()
                }
            },
            None => backend.session.budget(),
        };
        let mut prepared = backend.prepared(source);
        if matches!(prepared, Some(Prepared::Queued | Prepared::Reading)) {
            let want = Want {
                alive: self.alive.clone(),
                demand: Box::new(move |backend, header| {
                    let budget = backend.session.budget();
                    demand(header, view, source, generation, budget).map(|d| (d, true))
                }),
            };
            if backend.want(self.source.as_ref().unwrap(), want) {
                return Ok(None);
            }
            // Read since: ask now.
            prepared = backend.prepared(source);
        }
        let header = match prepared {
            Some(Prepared::Ready(header)) => header,
            Some(Prepared::Failed(e)) => return Err(e),
            Some(Prepared::Queued | Prepared::Reading) => return Ok(None),
            None => {
                self.source = None;
                return Ok(None);
            }
        };
        let demand = demand(header, view, source, generation, available)?;
        if let Some((request, wanted)) = self.request {
            if wanted == demand.key.pixels {
                return Ok(None);
            }
            backend.cancel(request);
            self.request = None;
        }
        let pixels = demand.key.pixels;
        let request = backend.request(demand, true)?;
        self.request = Some((request, pixels));
        Ok(backend.session.take_ready(request))
    }
}
