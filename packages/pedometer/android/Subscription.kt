package exact.pedometer

/** The process's Recording subscription. Play services keeps one across processes but lists none to the app
 * (`LocalRecordingClient` has no `listSubscriptions`), so the module stores when this installation first subscribed,
 * in [marker]: a file the platform neither backs up nor moves to another device (`Context.getNoBackupFilesDir`), so
 * an installation never takes another's start as its own. */
class Subscription(private val marker: java.io.File? = null) {
    /** The first subscription's start, once [complete]: no read reaches before it. */
    var since: Double? = null
        private set
    private var ready = false
    private var pending = false

    fun begin(): Boolean {
        if (ready || pending) return false
        pending = true
        return true
    }

    /** The start an earlier process of this installation stored; null for none, or for a marker it cannot read. */
    fun stored(): Double? = try {
        marker?.takeIf { it.isFile }?.readText()?.trim()?.toDoubleOrNull()?.takeIf { it > 0 }
    } catch (e: java.io.IOException) { null }

    /** Subscribed at [epoch]; [recorded] is the start an earlier process stored, null for none. The start to store. */
    fun complete(recorded: Double?, epoch: Double): Double {
        val start = recorded ?: epoch
        since = start
        ready = true
        pending = false
        return start
    }

    /** Keep [start] for this installation's later processes. */
    fun store(start: Double) {
        try { marker?.writeText(start.toLong().toString()) } catch (e: java.io.IOException) {}
    }

    fun failed() {
        pending = false
    }

    /** The Recording read [count] makes at [epoch]; none before the subscription is made, when a read could claim
     * steps from before any recording. */
    fun range(count: Count, epoch: Double): Count.Range? = since?.let { count.range(it, epoch) }
}
