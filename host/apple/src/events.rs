//! Deliver events on the Apple host's input clock.

use super::*;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

const WAITING: u8 = 0;
const COMMITTED: u8 = 1;
const REFUSED: u8 = 2;

pub(super) struct NativeTextReceipt {
    result: Arc<AtomicU8>,
    through: f64,
}

enum NativeTextWork {
    Held(f64),
    Immediate,
}

impl<D: DataSource> Host<D> {
    /// Retain the arrival cutoff while the runner may need to publish earlier
    /// changes. After the input commits, immediate work uses the runner clock.
    pub(crate) fn dispatch_native_text(
        &mut self,
        view: ViewId,
        event: Event,
        now_ms: f64,
    ) -> String {
        self.now_ms = now_ms.max(self.now_ms);
        let result = Arc::new(AtomicU8::new(WAITING));
        self.native_text_receipt = Some(NativeTextReceipt {
            result: result.clone(),
            through: self.now_ms,
        });
        let advanced =
            self.runner
                .dispatch_at_with_receipt(view, event, self.now_ms, move |accepted| {
                    result.store(
                        if accepted { COMMITTED } else { REFUSED },
                        Ordering::Relaxed,
                    );
                });
        let error = advanced.error.map(|e| format!("{e:?}"));
        self.in_inert_entry(|host| host.commit(&advanced.receipts, error))
    }

    fn native_text_work(&self) -> Option<NativeTextWork> {
        let receipt = self.native_text_receipt.as_ref()?;
        match receipt.result.load(Ordering::Relaxed) {
            WAITING if Arc::strong_count(&receipt.result) > 1 => {
                Some(NativeTextWork::Held(receipt.through))
            }
            COMMITTED if self.runner.immediate_work_due() => Some(NativeTextWork::Immediate),
            _ => None,
        }
    }

    pub(super) fn native_text_pending(&self) -> bool {
        self.native_text_work().is_some()
    }

    pub(crate) fn advance_native_text(&mut self) -> String {
        match self.native_text_work() {
            Some(NativeTextWork::Held(through)) => self.advance(through),
            Some(NativeTextWork::Immediate) => self.land_then(false),
            None => self.refused(None),
        }
    }
}
