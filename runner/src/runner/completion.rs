//! Runner-issued request and callback completions (LLP 1172.019 §2 item 1, Carry).

use super::{CommitReceipt, DataSource, Outcome, RequestOut, Runner, RunnerError, Target};
use std::fmt;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(super) struct Identity {
    session: Arc<()>,
    generation: Arc<()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Owner {
    Ui(Target),
    Mapper { stream: u64, target: Target },
    ModuleWork,
    Background,
    Tool,
    NativeTimer { index: u32, generation: u64 },
    Canvas { lifetime: u64, generation: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    UiOwned,
    DurableProcessLocal,
}

#[derive(Clone, Copy, Debug)]
enum Protocol {
    Reply,
    Subscription,
    DurableOperation,
}

struct Binding {
    identity: Identity,
    ticket: u64,
    owner: Owner,
    class: Class,
    protocol: Protocol,
    taken: bool,
}

impl fmt::Debug for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding")
            .field("session", &Arc::as_ptr(&self.identity.session))
            .field("generation", &Arc::as_ptr(&self.identity.generation))
            .field("ticket", &self.ticket)
            .field("owner", &self.owner)
            .field("class", &self.class)
            .field("protocol", &self.protocol)
            .field("taken", &self.taken)
            .finish()
    }
}

/// Authority for one issued delivery. Only the runner can mint it. A one-shot
/// reply moves it; a subscription borrows its binding for each delivery.
/// Carry records identity without adding any refusal policy.
///
/// ```compile_fail
/// use exact_runner::CompletionCapability;
/// fn duplicate(reply: CompletionCapability) -> CompletionCapability { reply.clone() }
/// ```
///
/// ```compile_fail
/// use exact_runner::{CompletionCapability, CompletionReply};
/// fn forge(binding: CompletionReply) -> CompletionCapability {
///     CompletionCapability { binding }
/// }
/// ```
#[derive(Debug)]
pub struct CompletionCapability {
    binding: CompletionReply,
}

/// Authority for a subscription: a stream, long poll, native timer arming
/// or canvas generation. Clones share the runner's original binding.
/// Deliveries follow calls to [`Runner::deliver`]; each family's existing
/// sequence, generation and mapper rules still apply.
///
/// ```compile_fail
/// use exact_runner::CompletionSubscription;
/// let subscription = CompletionSubscription::default();
/// ```
///
/// ```compile_fail
/// use exact_runner::{CompletionReply, CompletionSubscription};
/// fn forge(binding: CompletionReply) -> CompletionSubscription {
///     CompletionSubscription { binding }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct CompletionSubscription {
    binding: CompletionReply,
}

impl From<&CompletionSubscription> for CompletionCapability {
    fn from(subscription: &CompletionSubscription) -> Self {
        Self {
            binding: subscription.binding.clone(),
        }
    }
}

/// Authority shared by an outgoing request and its lifetime record. One-shot
/// and durable-round clones share one take; subscriptions keep their binding.
#[derive(Clone, Debug)]
pub struct CompletionReply(Arc<Mutex<Binding>>);

impl CompletionReply {
    pub(super) fn callback(identity: Identity, ticket: u64, owner: Owner) -> Self {
        Self(Arc::new(Mutex::new(Binding {
            identity,
            ticket,
            owner,
            class: Class::UiOwned,
            protocol: Protocol::Subscription,
            taken: false,
        })))
    }

    /// Take the runner's authority once, for [`Runner::deliver`].
    /// A subscription has no one-shot authority; use [`Self::subscription`].
    pub fn take(&self) -> Option<CompletionCapability> {
        let mut binding = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(binding.protocol, Protocol::Subscription) || binding.taken {
            return None;
        }
        binding.taken = true;
        Some(CompletionCapability {
            binding: self.clone(),
        })
    }

    /// The runner's reusable subscription binding, for [`Runner::deliver`].
    /// Borrow it on each delivery; neither an event nor its commit consumes it.
    pub fn subscription(&self) -> Option<CompletionSubscription> {
        let binding = self.0.lock().unwrap_or_else(|e| e.into_inner());
        matches!(binding.protocol, Protocol::Subscription).then(|| CompletionSubscription {
            binding: self.clone(),
        })
    }

    pub(super) fn retain(&self) {
        let mut binding = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // The host may already have extracted its capability. Transfer the
        // shared binding, never mint a replacement (LLP 1172.019 Carry).
        binding.owner = Owner::ModuleWork;
        binding.class = Class::DurableProcessLocal;
        binding.protocol = Protocol::DurableOperation;
    }

    pub(super) fn delivery(&self) -> Option<CompletionCapability> {
        let mut binding = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(binding.protocol, Protocol::Subscription) {
            if binding.taken {
                return None;
            }
            binding.taken = true;
        }
        Some(CompletionCapability {
            binding: self.clone(),
        })
    }

    pub(super) fn completion(
        &self,
        outcome: Outcome,
        timing: Option<crate::CompletionTiming>,
    ) -> Completion {
        match self.0.lock().unwrap_or_else(|e| e.into_inner()).protocol {
            Protocol::Reply => Completion::Reply { outcome, timing },
            Protocol::Subscription => Completion::Subscription { outcome, timing },
            Protocol::DurableOperation => Completion::DurableOperation { outcome, timing },
        }
    }
}

/// Completion families admitted by the runner's typed delivery entry.
/// Other host callbacks retain their current entries until their carry slices
/// land (LLP 1172.019 §2 item 1).
#[derive(Debug)]
pub enum Completion {
    /// An ordinary native timer firing under its arming subscription.
    NativeTimer {
        /// The plan timer row.
        index: u32,
        /// The arming generation supplied to the platform.
        generation: u64,
        /// Delivery time in the runner's clock domain.
        cutoff: f64,
    },
    /// A draw under a canvas generation's subscription. Existing stamp and
    /// structural checks still decide whether its lists apply.
    CanvasDraw {
        /// The process-local canvas lifetime.
        lifetime: u64,
        /// The backing-store generation.
        generation: u32,
        /// The in-flight draw within that generation.
        seq: u64,
        /// Recorded lists and the source's next-frame request.
        reply: super::DrawReply,
    },
    /// A one-shot request outcome, with optional executor clock measurements.
    Reply {
        /// HTTP, storage, surface, module or failure outcome of the issued work.
        outcome: Outcome,
        /// Execution and delivery wait, apart from the seekable agent clock.
        timing: Option<crate::CompletionTiming>,
    },
    /// A stream/long-poll event, batch, control reply or terminal outcome.
    Subscription {
        /// Delivered in arrival order under the subscription's original binding.
        outcome: Outcome,
        /// Execution and delivery wait, apart from the seekable agent clock.
        timing: Option<crate::CompletionTiming>,
    },
    /// A background/tool round or retained module request. Its authority
    /// survives UI retirement, but is process-local: no crash recovery or
    /// blind retry is added by carry (LLP 1172.019 §3 inventory).
    DurableOperation {
        /// The one outcome of this round; a successor gets fresh authority.
        outcome: Outcome,
        /// Execution and delivery wait, apart from the seekable agent clock.
        timing: Option<crate::CompletionTiming>,
    },
}

impl<D: DataSource> Runner<D> {
    fn completion_record(&mut self, ticket: u64) -> Option<(Owner, &mut Option<CompletionReply>)> {
        if self.is_background(ticket) {
            return Some((Owner::Background, &mut self.background.completion));
        }
        if self.is_tool(ticket) {
            return Some((Owner::Tool, self.tool_completion(ticket)?));
        }
        if self.is_module_work(ticket) {
            return Some((Owner::ModuleWork, self.module_work.completion(ticket)?));
        }
        if let Some((owner, slot)) = self.mapper_turns.completion(ticket) {
            return Some((owner, slot));
        }
        let p = self.pending.iter_mut().find(|p| p.ticket == ticket)?;
        Some((Owner::Ui(p.target), &mut p.completion))
    }

    pub(super) fn request_completion(&mut self, ticket: u64) -> Option<CompletionReply> {
        self.completion_record(ticket)?.1.clone()
    }

    pub(super) fn issue_completions(&mut self, requests: &mut [RequestOut]) {
        for request in requests {
            let subscription = self
                .pending
                .iter()
                .any(|p| p.ticket == request.ticket && p.stream.is_some());
            let identity = self
                .completion_identity
                .get_or_init(Identity::default)
                .clone();
            let Some((owner, slot)) = self.completion_record(request.ticket) else {
                continue;
            };
            let reply = slot.get_or_insert_with(|| {
                CompletionReply(Arc::new(Mutex::new(Binding {
                    identity,
                    ticket: request.ticket,
                    owner,
                    class: match owner {
                        Owner::Ui(_)
                        | Owner::Mapper { .. }
                        | Owner::NativeTimer { .. }
                        | Owner::Canvas { .. } => Class::UiOwned,
                        _ => Class::DurableProcessLocal,
                    },
                    protocol: match owner {
                        Owner::ModuleWork | Owner::Background | Owner::Tool => {
                            Protocol::DurableOperation
                        }
                        _ if subscription => Protocol::Subscription,
                        _ => Protocol::Reply,
                    },
                    taken: false,
                })))
            });
            request.completion = Some(reply.clone());
        }
    }

    /// Deliver one issued completion: move a one-shot capability or borrow a
    /// [`CompletionSubscription`] for each ordered delivery. Carry preserves
    /// today's ticket semantics and host guards; it adds no identity refusal.
    /// A timer returns its commit. A canvas draw returns no commit, or its
    /// existing validation error as [`RunnerError::CanvasDraw`].
    pub fn deliver(
        &mut self,
        capability: impl Into<CompletionCapability>,
        completion: Completion,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let capability = capability.into();
        let binding = capability
            .binding
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let ticket = binding.ticket;
        drop(binding);
        match completion {
            Completion::NativeTimer {
                index,
                generation,
                cutoff,
            } => {
                let mut result = self.fire_native_timer_now(index, generation, cutoff);
                match result.error {
                    Some(error) => Err(error),
                    None => Ok(result.receipts.pop().map(|timed| timed.receipt)),
                }
            }
            Completion::CanvasDraw {
                lifetime,
                generation,
                seq,
                reply,
            } => self
                .canvas_reply_now(lifetime, generation, seq, reply)
                .map(|()| None)
                .map_err(RunnerError::CanvasDraw),
            reply => self.deliver_reply(ticket, reply),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod callback_tests;
