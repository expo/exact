//! Hand dialogs to a posted window message and consume their queued answers.
use super::*;

impl<D: DataSource + Default + 'static> App<D> {
    pub(super) fn file_dialog_answers(&mut self) {
        let Some(p) = &mut self.presenter else { return };
        for answer in self.dialogs.take_answers() {
            for line in answer.lines {
                p.host_mut().log(line);
            }
            p.answer_file_dialog(answer.id, answer.outcome);
        }
    }

    pub(super) fn queue_file_dialogs(&mut self) {
        let Some(p) = &mut self.presenter else { return };
        for (id, request) in p.take_file_dialogs() {
            self.dialogs.queue(id, request);
        }
    }
}
