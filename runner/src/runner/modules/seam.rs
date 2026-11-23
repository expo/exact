//! The module seam, linked: each entry the core reaches, answered by the
//! modules' code (`module_seam.rs`).

use super::super::module_seam::{Modules, Readers};
use super::super::provenance::CallOrigin;
use super::super::{CommitReceipt, DataError, DataSource, Runner, RunnerError, Target};
use super::AppState;
use crate::request::{Answer, Dispatch, Outcome, Request};
use crate::HoldAnswer;
use exact_kernel::ViewId;
use exact_plan::{ModulesRow, TypesId, Value};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The module seam, linked.
pub(crate) struct LinkedModules;

impl<D: DataSource> Modules<D> for LinkedModules {
    fn state(&self) -> Box<dyn std::any::Any> {
        Box::new(super::Extensions::default())
    }

    fn abort_held_of(&self, r: &mut Runner<D>, id: u64, message: &str) {
        r.abort_held_of_linked(id, message)
    }

    fn admit_picture(&self, r: &mut Runner<D>, source: &str, target: &str) -> Option<Json> {
        r.admit_picture_linked(source, target)
    }

    fn arrival_visibility(&self, r: &mut Runner<D>, hidden: bool) {
        r.arrival_visibility_linked(hidden)
    }

    fn bind_extensions(&self, r: &mut Runner<D>, real: &[&str], agent: &[&str]) {
        r.bind_extensions_linked(real, agent)
    }

    fn bound_here(&self, r: &Runner<D>, extension: &str) -> bool {
        r.bound_here_linked(extension)
    }

    fn check_picture(&self, r: &mut Runner<D>, source: &str, target: &str) -> Option<Json> {
        r.check_picture_linked(source, target)
    }

    fn declaration_topics(&self, r: &Runner<D>, topics: Vec<String>) -> Vec<String> {
        r.declaration_topics_linked(topics)
    }

    fn declared_values(&self, r: &Runner<D>) -> Json {
        r.declared_values_linked()
    }

    fn declares_action(&self, r: &Runner<D>, extension: &str, action: &Json) -> bool {
        r.declares_action_linked(extension, action)
    }

    fn deliver_module_events(
        &self,
        r: &mut Runner<D>,
    ) -> (Vec<CommitReceipt>, Option<RunnerError>) {
        r.deliver_module_events_linked()
    }

    fn discard_failed_tools(&self, r: &mut Runner<D>) {
        r.discard_failed_tools_linked()
    }

    fn disclosed_told(&self, r: &mut Runner<D>, body: &str) {
        r.disclosed_told_linked(body)
    }

    fn dispose(&self, r: &mut Runner<D>, handles: (String, Vec<String>), why: &str) {
        r.dispose_linked(handles, why)
    }

    fn dispose_dropped(&self, r: &mut Runner<D>) {
        r.dispose_dropped_linked()
    }

    fn drop_statements_of(&self, r: &mut Runner<D>, id: u64) {
        r.drop_statements_of_linked(id)
    }

    fn extension_agent(
        &self,
        r: &mut Runner<D>,
        request: &str,
    ) -> Option<(Vec<CommitReceipt>, Result<String, String>)> {
        r.extension_agent_linked(request)
    }

    fn extensions_json(&self, r: &Runner<D>) -> Option<String> {
        r.extensions_json_linked()
    }

    fn fail_tool(&self, r: &mut Runner<D>, ticket: u64, message: &str) {
        r.fail_tool_linked(ticket, message)
    }

    fn failed_tool_tickets(&self, r: &Runner<D>) -> Vec<(u64, String)> {
        r.failed_tool_tickets_linked()
    }

    fn fed_canvases(&self, r: &mut Runner<D>, updates: &[crate::SurfaceUpdate]) {
        r.fed_canvases_linked(updates)
    }

    fn foreground_service_told(&self, r: &mut Runner<D>, body: &str) {
        r.foreground_service_told_linked(body)
    }

    fn forget_module_row(&self, r: &mut Runner<D>, i: usize) {
        r.forget_module_row_linked(i)
    }

    fn handled_work(&self, r: &mut Runner<D>, body: &[u8]) -> Option<Dispatch> {
        r.handled_work_linked(body)
    }

    fn has_ordered_tool_refusal(&self, r: &Runner<D>) -> bool {
        r.has_ordered_tool_refusal_linked()
    }

    fn has_tool_refusal(&self, r: &Runner<D>, allow_ordered: bool) -> bool {
        r.has_tool_refusal_linked(allow_ordered)
    }

    fn implemented(&self, r: &Runner<D>, extension: &str) -> bool {
        r.implemented_linked(extension)
    }

    fn index_entities(&self, r: &Runner<D>, kind: &str, matching: Option<&str>) -> Vec<Json> {
        r.index_entities_linked(kind, matching)
    }

    fn invoke_module(&self, r: &mut Runner<D>, body: &str, reply: Box<dyn FnOnce(String) + Send>) {
        r.invoke_module_linked(body, reply)
    }

    fn is_listed(&self, r: &Runner<D>, reference: u64) -> bool {
        r.is_listed_linked(reference)
    }

    fn is_tool(&self, r: &Runner<D>, ticket: u64) -> bool {
        r.is_tool_linked(ticket)
    }

    fn journal_unheard_events(&self, r: &mut Runner<D>) {
        r.journal_unheard_events_linked()
    }

    fn keep_send(&self, r: &mut Runner<D>, ticket: u64) {
        r.keep_send_linked(ticket)
    }

    fn landed_handles(&self, r: &mut Runner<D>, ticket: u64) -> Option<(String, Vec<String>)> {
        r.landed_handles_linked(ticket)
    }

    fn landed_send(&self, r: &mut Runner<D>, ticket: u64) {
        r.landed_send_linked(ticket)
    }

    fn launch_listed(
        &self,
        r: &mut Runner<D>,
        launch: &str,
        entry: &str,
    ) -> Result<String, String> {
        r.launch_listed_linked(launch, entry)
    }

    fn launch_login(&self, r: &mut Runner<D>, hidden: bool) -> Result<String, String> {
        r.launch_login_linked(hidden)
    }

    fn listed_choice(&self, r: &mut Runner<D>, body: &str) {
        r.listed_choice_linked(body)
    }

    fn listed_due(&self, r: &mut Runner<D>, now_ms: f64) {
        r.listed_due_linked(now_ms)
    }

    fn listed_op(
        &self,
        r: &mut Runner<D>,
        op: &str,
        request: &str,
    ) -> Option<Result<String, String>> {
        r.listed_op_linked(op, request)
    }

    fn module_answer(
        &self,
        r: &mut Runner<D>,
        i: usize,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, RunnerError> {
        r.module_answer_linked(i, source, args)
    }

    fn module_answered(&self, r: &mut Runner<D>, hold: u64, answer: &HoldAnswer) {
        r.module_answered_linked(hold, answer)
    }

    fn module_call(&self, r: &mut Runner<D>, native: &crate::Request) -> Option<Dispatch> {
        r.module_call_linked(native)
    }

    fn module_check(&self, r: &Runner<D>, ticket: u64, text: &str) -> Option<Result<(), String>> {
        r.module_check_linked(ticket, text)
    }

    fn module_deadlines(&self, r: &mut Runner<D>, now_ms: f64) {
        r.module_deadlines_linked(now_ms)
    }

    fn module_declared_state(&self, r: &Runner<D>) -> BTreeMap<String, Json> {
        r.module_declared_state_linked()
    }

    fn module_dispatch(&self, r: &mut Runner<D>, token: u64) -> Option<Dispatch> {
        r.module_dispatch_linked(token)
    }

    fn module_parse(
        &self,
        r: &mut Runner<D>,
        target: Target,
        ty: TypesId,
        source: &str,
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        r.module_parse_linked(target, ty, source, outcome)
    }

    fn module_release(&self, r: &mut Runner<D>) -> Vec<(u64, Dispatch)> {
        r.module_release_linked()
    }

    fn module_send(
        &self,
        r: &mut Runner<D>,
        target: Target,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        r.module_send_linked(target, source, args)
    }

    fn module_topic(&self, r: &Runner<D>, body: &[u8]) -> Option<String> {
        r.module_topic_linked(body)
    }

    fn modules_own(&self, r: &Runner<D>, id: u64) -> bool {
        r.modules_own_linked(id)
    }

    fn modules_quiet(&self, r: &Runner<D>) -> bool {
        r.modules_quiet_linked()
    }

    fn note_handles(&self, r: &mut Runner<D>, ticket: u64, source: &str, body: &[u8]) {
        r.note_handles_linked(ticket, source, body)
    }

    fn note_send(&self, r: &mut Runner<D>, ticket: u64, body: &[u8]) {
        r.note_send_linked(ticket, body)
    }

    fn number_data_call(&self, r: &mut Runner<D>, request: &mut Request) {
        r.number_data_call_linked(request)
    }

    fn picture_schemes(&self, r: &Runner<D>) -> Vec<String> {
        r.picture_schemes_linked()
    }

    fn poll(&self, r: &mut Runner<D>) -> Result<Vec<CommitReceipt>, RunnerError> {
        r.poll_linked()
    }

    fn poll_due_ms(&self, r: &Runner<D>) -> Option<f64> {
        r.poll_due_ms_linked()
    }

    fn polls_asking(&self, r: &Runner<D>) -> bool {
        r.polls_asking_linked()
    }

    fn raise_emitted(&self, r: &mut Runner<D>, body: &str) {
        r.raise_emitted_linked(body)
    }

    fn raise_module_event(
        &self,
        r: &mut Runner<D>,
        extension: &str,
        name: &str,
        args: &str,
        origin: CallOrigin,
    ) -> Result<(), String> {
        r.raise_module_event_linked(extension, name, args, origin)
    }

    fn refuse_tool(&self, r: &mut Runner<D>, ticket: u64, reason: &'static str, ordered: bool) {
        r.refuse_tool_linked(ticket, reason, ordered)
    }

    fn release_sampled(&self, r: &mut Runner<D>) {
        r.release_sampled_linked()
    }

    fn report_views(&self, r: &mut Runner<D>) {
        r.report_views_linked()
    }

    fn retire_modules(&self, r: &mut Runner<D>) {
        r.retire_modules_linked()
    }

    fn run_module_effects(&self, r: &mut Runner<D>, effects: Vec<(ModulesRow, Vec<Value>)>) {
        r.run_module_effects_linked(effects)
    }

    fn samples_refused(&self, r: &mut Runner<D>) {
        r.samples_refused_linked()
    }

    fn samples_stood(&self, r: &mut Runner<D>) {
        r.samples_stood_linked()
    }

    fn samples_visibility(&self, r: &mut Runner<D>, hidden: bool) {
        r.samples_visibility_linked(hidden)
    }

    fn share_app_state(&self, r: &mut Runner<D>, state: AppState) {
        r.share_app_state_linked(state)
    }

    fn span_due(&self, r: &Runner<D>) -> Option<f64> {
        r.span_due_linked()
    }

    fn surface_sensor_events(&self, r: &mut Runner<D>, to_ms: f64) -> Vec<(String, String)> {
        r.surface_sensor_events_linked(to_ms)
    }

    fn surface_sensor_inputs(&self, r: &Runner<D>) -> bool {
        r.surface_sensor_inputs_linked()
    }

    fn take_module_effects(
        &self,
        r: &mut Runner<D>,
        first: usize,
    ) -> Vec<(ModulesRow, Vec<Value>)> {
        r.take_module_effects_linked(first)
    }

    fn take_stale(&self, r: &mut Runner<D>, key: &str, body: Vec<u8>) -> Vec<u8> {
        r.take_stale_linked(key, body)
    }

    fn tap_listed(&self, r: &mut Runner<D>, reference: u64) -> Result<String, String> {
        r.tap_listed_linked(reference)
    }

    fn tap_listed_as(
        &self,
        r: &mut Runner<D>,
        reference: u64,
        choice: Option<&str>,
    ) -> Result<String, String> {
        r.tap_listed_as_linked(reference, choice)
    }

    fn tell_foreground(&self, r: &Runner<D>) {
        r.tell_foreground_linked()
    }

    fn tool_completion<'a>(
        &self,
        r: &'a mut Runner<D>,
        ticket: u64,
    ) -> Option<&'a mut Option<crate::CompletionReply>> {
        r.tool_completion_linked(ticket)
    }

    fn tool_effects(&self, r: &Runner<D>) -> bool {
        r.tool_effects_linked()
    }

    fn tool_landed(&self, r: &mut Runner<D>, ticket: u64, outcome: Outcome) {
        r.tool_landed_linked(ticket, outcome)
    }

    fn tool_cause(&self, r: &Runner<D>) -> Option<crate::runner::provenance::Cause> {
        r.tool_cause_linked()
    }

    fn tool_refusal(&self, r: &mut Runner<D>, allow_ordered: bool) -> Option<(u64, &'static str)> {
        r.tool_refusal_linked(allow_ordered)
    }

    fn tool_tickets(&self, r: &Runner<D>) -> Vec<(String, u64)> {
        r.tool_tickets_linked()
    }

    fn view_origin(&self, r: &Runner<D>, id: ViewId, prop: Option<&str>) -> Option<CallOrigin> {
        r.view_origin_linked(id, prop)
    }

    fn views_committed(&self, r: &mut Runner<D>) {
        r.views_committed_linked()
    }

    fn wake_entry_ended(&self, r: &mut Runner<D>, reference: u64, state: &str, delivered: bool) {
        r.wake_entry_ended_linked(reference, state, delivered)
    }

    fn wake_entry_running(&self, r: &mut Runner<D>, reference: u64) {
        r.wake_entry_running_linked(reference)
    }

    fn module_substitute(&self, r: &Runner<D>, row: &ModulesRow) -> Arc<Json> {
        r.substitute(row)
    }

    fn module_value_of(&self, r: &Runner<D>, row: &ModulesRow) -> Option<Json> {
        r.value_of(row)
    }

    fn module_permission_state(&self, r: &Runner<D>, name: &str) -> String {
        r.permission_state(name)
    }

    fn set_module_permission(&self, r: &mut Runner<D>, name: &str, state: &str) {
        r.set_permission(name, state)
    }

    fn module_key(&self, r: &Runner<D>, row: &ModulesRow) -> String {
        r.key(row)
    }

    fn local_network_substitute(&self, r: &Runner<D>) -> Option<crate::local_network::State> {
        r.local_network_substitute()
    }

    fn holds_token(&self, r: &Runner<D>, token: Option<u64>) -> bool {
        r.extensions.holds_token(token)
    }

    fn readers_saved(&self, r: &Runner<D>) -> Readers {
        r.extensions.readers.clone()
    }

    fn readers_restore(&self, r: &mut Runner<D>, saved: Readers) {
        r.extensions.readers = saved;
    }

    fn retarget(&self, r: &mut Runner<D>) {
        r.retarget_linked()
    }
}
