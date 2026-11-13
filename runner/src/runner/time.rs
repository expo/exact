//! Host-owned date answers; never baked or carried.
//! @ref LLP 1027.000.000
use super::{DataError, DataSource, Runner, RunnerError};
use crate::time::{Place, WallTime, SOURCE};
use exact_kernel::CommitReceipt;
use exact_plan::{TypeKind, Value};

/// The store's name for the language the app chose (`setLocale`), beside
/// its persisted state: no state is named `#locale`, which is the locale
/// slot's name. JSON text, as a persisted string is.
pub(crate) const CHOSEN: &str = "exact.state.#locale";

/// A tag `setLocale` takes: what [`crate::time::Place::validate`] takes for
/// the viewer's locale.
fn valid_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.len() <= 64 && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

impl<D: DataSource> Runner<D> {
    /// What the host last said about the date.
    pub fn wall_time(&self) -> WallTime {
        self.time
    }

    /// The language of the displayed strings. Without tables, language is
    /// unknown (HTML's empty `lang`), rather than the viewer's preferred locale.
    pub fn resolved_locale(&self) -> &str {
        self.plan
            .locale
            .and_then(|slot| self.slots.get(slot.0 as usize))
            .and_then(Value::as_str)
            .filter(|name| {
                self.plan
                    .locales
                    .iter()
                    .any(|row| self.plan.str(row.name) == *name)
            })
            .or_else(|| self.plan.locales.first().map(|row| self.plan.str(row.name)))
            .unwrap_or("")
    }

    /// CSS direction of the resolved table, compiled from CLDR likely subtags.
    pub fn direction(&self) -> &'static str {
        if self
            .plan
            .locales
            .iter()
            .any(|row| row.rtl && self.plan.str(row.name) == self.resolved_locale())
        {
            "rtl"
        } else {
            "ltr"
        }
    }

    /// Re-answer every `exactTime` resource in one commit. An invalid fact
    /// is journaled without changing anything.
    pub fn set_time(
        &mut self,
        epoch_at_zero: f64,
        utc_offset: f64,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let time = WallTime {
            epoch_at_zero,
            utc_offset,
        };
        if let Err(error) = time.validate() {
            self.log(format!("time refused: {error:?}"));
            return Err(error);
        }
        // A clock that drifts by less than a second is the same fact.
        if (time.epoch_at_zero - self.time.epoch_at_zero).abs() < 1000.0
            && time.utc_offset == self.time.utc_offset
        {
            return Ok(None);
        }
        self.replace_fact(
            |r| &mut r.time,
            time,
            SOURCE,
            |r, _, readers| {
                if readers.is_empty() && !r.live_times.is_empty() {
                    r.tick_live_times()
                } else {
                    r.recommit(readers, "time")
                }
            },
        )
    }

    /// What the host last said about the viewer and this launch.
    pub fn place(&self) -> &Place {
        &self.place
    }

    /// Re-answer every `exactTime` resource with the viewer's locale and
    /// zone, and write the table that locale resolves to into the plan's
    /// locale slot, in one commit; the same fact again commits nothing.
    /// The launch seed arrives with them; `None` keeps the current seed.
    pub fn set_place(
        &mut self,
        locale: &str,
        time_zone: &str,
        seed: Option<f64>,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        self.set_place_formatted(locale, time_zone, seed, None)
    }

    /// [`Runner::set_place`] with the regional format ([`Place`]'s
    /// `format_locale`, `""` for none apart from the locale); `None` keeps
    /// the one last told, as a `None` seed does.
    pub fn set_place_formatted(
        &mut self,
        locale: &str,
        time_zone: &str,
        seed: Option<f64>,
        format_locale: Option<&str>,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let place = Place {
            locale: locale.into(),
            time_zone: time_zone.into(),
            seed: seed.unwrap_or(self.place.seed),
            format_locale: format_locale
                .map_or_else(|| self.place.format_locale.clone(), str::to_owned),
        };
        if let Err(error) = place.validate() {
            self.log(format!("place refused: {error:?}"));
            return Err(error);
        }
        self.replace_fact(
            |r| &mut r.place,
            place,
            SOURCE,
            |r, _, mut readers| {
                // The locale slot is part of this fact's commit (LLP 1060 D4):
                // the table the app's choice resolves to, else the viewer's.
                let written = r.write_locale();
                if !written.is_empty() && readers.is_empty() {
                    readers = r
                        .resource_rows()
                        .enumerate()
                        .filter_map(|(i, row)| (r.plan.str(row.source) == SOURCE).then_some(i))
                        .collect();
                }
                let result = if readers.is_empty() && written.is_empty() {
                    if r.live_times.is_empty() {
                        Ok(None)
                    } else {
                        r.tick_live_times()
                    }
                } else {
                    r.commit_again(readers, "place").map(Some)
                };
                if result.is_err() {
                    for (i, old) in written {
                        r.slots[i] = old;
                    }
                }
                result
            },
        )
    }

    /// The language the app chose with `setLocale`, kept across launches;
    /// `None` while it follows the host's, and for a plan that never runs
    /// `setLocale` (a choice an earlier version kept chooses nothing).
    pub fn chosen_locale(&self) -> Option<String> {
        let text = self.store.kept(CHOSEN)?;
        let tag = super::persist::decode_string(text).filter(|t| valid_tag(t))?;
        crate::uses::chooses_locale(&self.plan).then_some(tag)
    }

    /// The tag the strings table is resolved from: the app's choice, else
    /// the viewer's locale.
    pub(super) fn requested_locale(&self) -> String {
        self.chosen_locale()
            .unwrap_or_else(|| self.place.locale.clone())
    }

    /// Write the table [`Runner::requested_locale`] resolves to into the
    /// locale slot, and the format locale into the format slot: the index
    /// and old value of each slot that changed.
    fn write_locale(&mut self) -> Vec<(usize, Value)> {
        let mut changed = Vec::new();
        let Some(i) = self.plan.locale.map(|s| s.0 as usize) else {
            return changed;
        };
        let requested = self.requested_locale();
        let Some(table) = self.plan.resolve_locale(&requested) else {
            return changed;
        };
        let value = Value::str(table);
        if self.slots[i] != value {
            changed.push((i, std::mem::replace(&mut self.slots[i], value)));
        }
        if let Some(f) = self.plan.format.map(|s| s.0 as usize) {
            let value = Value::str(&self.format_locale_of(i));
            if self.slots[f] != value {
                changed.push((f, std::mem::replace(&mut self.slots[f], value)));
            }
        }
        changed
    }

    /// The format locale (LLP 1054.000.003 D6): the table in the locale slot
    /// `i`, with the hour cycle the viewer's regional format forces.
    pub(super) fn format_locale_of(&self, i: usize) -> String {
        let table = self.slots[i].as_str().unwrap_or_default();
        let place = &self.place;
        let format = if place.format_locale.is_empty() {
            &place.locale
        } else {
            &place.format_locale
        };
        exact_plan::strings::format_tag(table, format)
    }

    /// The commit's `setLocale`s from `from` on, each applied and taken
    /// out, before its settlement: the app's choice is kept in the store
    /// (so the next launch's first frame reads it), the locale slot takes
    /// the table it resolves to, and every `exactTime` is asked again for
    /// `resolvedLocale`. `setLocale("")` forgets the choice. No host is
    /// told: each reads the resolved table after the commit, as after
    /// `set_place`.
    pub(super) fn take_set_locale(&mut self, from: usize) {
        let mut i = from;
        while i < self.commands.len() {
            if self.commands[i].name != "setLocale" {
                i += 1;
                continue;
            }
            let args = self.commands.remove(i).args;
            let Some(tag) = (match &args[..] {
                [tag] => tag.as_str().filter(|t| t.is_empty() || valid_tag(t)),
                _ => None,
            }) else {
                let mut said = String::new();
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        said.push_str(", ");
                    }
                    crate::agent::untyped_json(a, &mut said);
                }
                self.log(format!(
                    "setLocale({said}) refused: a BCP 47 tag (\"de\", \"pt-BR\"), or \"\" for the host's locale"
                ));
                continue;
            };
            if tag.is_empty() {
                self.store.forget_kept(CHOSEN);
            } else {
                let mut text = String::new();
                crate::agent::quote(tag, &mut text);
                self.store.keep(CHOSEN, text.into_bytes());
            }
            if !self.write_locale().is_empty() {
                let readers: Vec<usize> = self
                    .resource_rows()
                    .enumerate()
                    .filter_map(|(i, row)| (self.plan.str(row.source) == SOURCE).then_some(i))
                    .collect();
                for r in readers {
                    self.force_refresh(r);
                }
            }
        }
    }

    /// Take the topics the source's native module announces (LLP 1016.002),
    /// calling `wake` from the announcing thread; the host then calls
    /// [`Runner::apply_announced`] on its own. A source with no native
    /// handle announces nothing.
    pub fn listen(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        // An abort heard off this thread wakes the host to collect it.
        self.data.abort_wake(wake.clone());
        let native = self.native_slot();
        let queue = self.announced.clone();
        native.on_changed(Some(std::sync::Arc::new(move |topic: &str| {
            let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
            if !queue.iter().any(|t| t == topic) {
                queue.push(topic.to_owned());
            }
            drop(queue);
            wake();
        })));
    }

    /// Every topic a current answer watches, once each, sorted: what a host
    /// that announces on its own behalf (a document watch, LLP 1069.010.000
    /// D1) must listen for after a commit.
    pub fn watched_topics(&self) -> Vec<String> {
        let mut topics: Vec<String> = crate::vm::Column::rows(&self.resources, |row| &row.watching)
            .iter()
            .flatten()
            .cloned()
            .collect();
        topics.sort();
        topics.dedup();
        topics
    }

    /// Whether a topic was announced since the last [`Runner::apply_announced`].
    pub fn has_announced(&self) -> bool {
        !self
            .announced
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    /// Ask again every resource watching a topic announced since the last
    /// call: one commit per topic that anything watches. Announcements
    /// between two calls coalesce, so a fast producer costs one re-ask.
    pub fn apply_announced(&mut self) -> (Vec<CommitReceipt>, Option<RunnerError>) {
        // A module's reply that carried handles may have left its
        // implementation for a request no longer in flight (LLP 1133.009 D6).
        self.dispose_dropped();
        let topics = std::mem::take(&mut *self.announced.lock().unwrap_or_else(|e| e.into_inner()));
        let mut receipts = Vec::new();
        for topic in topics {
            // An event a bound implementation emitted, said through its
            // module's `changed` (LLP 1133.004 D6): raised, not a topic.
            if let Some(event) = topic.strip_prefix(super::modules::EMITTED) {
                self.raise_emitted(event);
                continue;
            }
            // The platform expired a wake an implementation raised.
            if let Some(expired) = topic.strip_prefix(super::modules::EXPIRED) {
                let (more, error) = self.expire_told_wake(expired);
                receipts.extend(more);
                if let Some(error) = error {
                    return (receipts, Some(error));
                }
                continue;
            }
            // A wake a bound implementation raised (LLP 1133.017 D8).
            if let Some(wake) = topic.strip_prefix(super::modules::TOLD) {
                self.raise_told_wake(wake);
                continue;
            }
            // A person's act on a bound implementation's own screen, on a
            // list the runner keeps (LLP 1133.033 §13).
            if let Some(choice) = topic.strip_prefix(super::modules::CHOSEN) {
                self.listed_choice(choice);
                continue;
            }
            // An implementation's own objects, for `state.extensions` (LLP 1133.008 D8 item 9).
            if let Some(state) = topic.strip_prefix(super::modules::STATED) {
                self.disclosed_told(state);
                continue;
            }
            // A foreground-service claim under the agent (LLP 1133.033 D21).
            if let Some(claim) = topic.strip_prefix(super::modules::CLAIMED) {
                self.foreground_service_told(claim);
                continue;
            }
            match self.changed(&topic) {
                Ok(Some(receipt)) => receipts.push(receipt),
                Ok(None) => {}
                Err(error) => return (receipts, Some(error)),
            }
        }
        // Then the module events raised meanwhile (LLP 1133.004 D6).
        let (events, error) = self.deliver_module_events();
        receipts.extend(events);
        (receipts, error)
    }

    /// The device says `topic` changed (LLP 1016.002): every resource whose
    /// answer watches it is asked again, in one commit; none, no commit.
    ///
    /// @ref LLP 1016.002 D4 — a resource with a request in flight is not
    /// asked now: forgetting that request would drop its reply, and a topic
    /// that changes faster than the source answers would starve the
    /// resource. The request is marked instead; its reply lands as any
    /// reply does, and that commit asks the resource again, forced, once
    /// however many announcements came meanwhile.
    pub fn changed(&mut self, topic: &str) -> Result<Option<CommitReceipt>, RunnerError> {
        let mut which = Vec::new();
        for i in 0..self.resources.len() {
            // An upload is asked once for its arguments (LLP 1016.000 D10).
            if !self.resources[i].watching.iter().any(|t| t == topic) || self.upload_stands(i) {
                continue;
            }
            // A stream keeps no reply to wait for, and a refused request
            // never ran: those are asked now, as before.
            let mut in_flight = false;
            let mut log = None;
            for p in self.pending.iter_mut().filter(|p| {
                p.target == crate::Target::Resource(i)
                    && p.stream.is_none()
                    && !p.refused
                    && p.refusal.is_none()
            }) {
                in_flight = true;
                if !p.ask_again {
                    p.ask_again = true;
                    log.get_or_insert(p.ticket);
                }
            }
            if let Some(ticket) = log {
                let name = self.target_name(crate::Target::Resource(i));
                self.log(super::lines::waits_for(topic, ticket, &name));
            }
            if !in_flight {
                which.push(i);
            }
        }
        // Each reader asks again under its own last ask's origin (LLP 1133.002 D12).
        self.reasking(|r| r.recommit(which, "changed"))
    }

    pub(super) fn time_answer(&self, i: usize) -> Result<Value, DataError> {
        let row = self.resource_row(i);
        if row.args.len > 0 {
            return Err(DataError::BadArguments(format!(
                "{SOURCE} takes no arguments"
            )));
        }
        let ty = self.plan.type_(row.ty);
        if ty.kind != TypeKind::Record {
            return Err(DataError::Unavailable(format!(
                "{SOURCE} answers a record; this one is declared `{}`",
                self.plan.str(ty.name)
            )));
        }
        let mut fields = Vec::with_capacity(ty.fields.len as usize);
        for f in ty.fields.iter() {
            let name = self.plan.str(self.plan.field(f).name);
            match self
                .time
                .field(name)
                .or_else(|| self.place.field(name))
                .or_else(|| (name == "resolvedLocale").then(|| Value::str(self.resolved_locale())))
            {
                Some(v) => fields.push(v),
                None => {
                    return Err(DataError::Unavailable(format!(
                        "{SOURCE} has no field `{name}`"
                    )))
                }
            }
        }
        Ok(Value::record(fields))
    }
}
