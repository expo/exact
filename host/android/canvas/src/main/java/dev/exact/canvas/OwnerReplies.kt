package dev.exact.canvas

/** What the reader hands the owner may arrive while an agent operation holds it: answers, and the topics, pictures
 * and journal lines extensions announce. Only that owner takes them, in the order they arrived, either inside the
 * operation's wait or in the posted handler turn ([deliver]); neither path dispatches other handler jobs. What an
 * extension did before answering is so taken with that answer, and the operation waits for whatever it asks again. */
internal class OwnerReplies(private val post: (() -> Unit) -> Unit, val deliver: (Handed) -> Unit) {
    sealed interface Handed {
        /** The answer to request [id], or its cancellation (null): [Native.answer]. */
        data class Answer(val id: Int, val value: String?) : Handed
        data class After(val timer: Int, val due: Double) : Handed
        data class Refused(val id: Int, val error: String, val message: String?) : Handed
        /** A topic an extension announced: [Native.changed]. */
        data class Changed(val topic: String) : Handed
        /** A picture an extension changed or revoked: [Native.pictureChanged]. */
        data class Picture(val source: String, val revoked: Boolean) : Handed
        /** A line for the session's journal: [Native.journal]. */
        data class Journal(val line: String) : Handed
    }

    private val pending = ArrayDeque<Handed>()

    fun hand(handed: Handed) {
        synchronized(pending) { pending.addLast(handed) }
        post {
            if (synchronized(pending) { pending.removeAll { it === handed } }) deliver(handed)
        }
    }

    fun take(): List<Handed> = synchronized(pending) {
        pending.toList().also { pending.clear() }
    }

    companion object {
        fun deliver(h: Handed) {
            when (h) {
                is Handed.After -> Native.moduleAfter(h.timer, h.due)
                is Handed.Answer -> Native.answer(h.id, h.value)
                is Handed.Refused -> Native.refuseModule(h.id, h.error, h.message)
                is Handed.Changed -> Native.changed(h.topic)
                is Handed.Picture -> Native.pictureChanged(h.source, h.revoked)
                is Handed.Journal -> Native.journal(h.line)
            }
        }
    }
}
