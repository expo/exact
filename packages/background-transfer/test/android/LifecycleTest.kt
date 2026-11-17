package exact.backgroundtransfer

import exact.backgroundtransfer.Lifecycle.Limits
import exact.backgroundtransfer.Lifecycle.Phase
import exact.backgroundtransfer.LifecycleModel.Config
import exact.backgroundtransfer.LifecycleModel.Graph
import exact.backgroundtransfer.LifecycleModel.Reboot
import exact.backgroundtransfer.LifecycleModel.World
import exact.backgroundtransfer.LifecycleModel.app
import exact.backgroundtransfer.LifecycleModel.drive
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * Every interleaving of the lifecycle's events with death, reboot, force-stop, update, a person's
 * stop, retirement, abort and a second namespace, from every state [LifecycleModel] reaches.
 * The platform answers a schedule in a later move, and a death may fall inside a step's writes
 * and before any effect. [LifecycleModel.check] and [LifecycleModel.Sim] hold the invariants:
 *
 * - only the holder of an unfinished transfer's lease starts sending, one exchange a transfer,
 *   and nothing sends before recovery or after a force-stop, a person's stop, an abort or a
 *   retirement;
 * - a result, a run and a delivery each end once;
 * - a transfer that owes work has a platform job, or is parked, or is being given one; a job is
 *   released only after the record says so, a job that replaces another only after the record
 *   names it, and none while its wake is in flight;
 * - an answer to a schedule that a later one replaced, and a session event that names no live
 *   session, change nothing;
 * - a start is answered; work that is stopped again and again ends or parks its transfer;
 * - a registration's files are removed only when no operation of its can still write;
 * - no step reads or writes another namespace.
 *
 * The graph checks say a path exists from every state: to an empty store, and, with no person
 * acting, to every transfer settled. They do not say a fair platform takes that path.
 *
 * The search is bounded. The limits are small; the function reads them only by comparison. A
 * step's effects are performed with it, except in the three searches named for an effect
 * waiting: there a [Lifecycle.Transmit], a [Lifecycle.Finish] or a [Lifecycle.Cancel] and what
 * follows it stay in the queue, one or two deep, while other events come. Deeper queues do not
 * fit the test's memory.
 */
class LifecycleTest {
    private val limits = Limits(stops = 2, networkRetries = 2, wakes = 2, completedWakes = 2)

    private fun explore(config: Config, least: Int): Graph {
        val graph = LifecycleModel.explore(config)
        assertTrue("only ${graph.states.size} states: the search no longer covers the lifecycle", graph.states.size >= least)
        return graph
    }

    /** No registration is wedged: its id can always be freed. */
    private fun everyStateCanBeCleared(graph: Graph) {
        val reached = LifecycleModel.reaching(graph, backgroundOnly = false) { it.durable.records.isEmpty() }
        val stuck = graph.states.indices.firstOrNull { !reached[it] }
        if (stuck != null) fail("no path clears ${graph.states[stuck]}")
    }

    private fun settled(w: World) = w.wakes.isEmpty() && w.durable.records.values.all {
        it.phase == Phase.Admitting || it.phase == Phase.Retired || (it.phase == Phase.Ended && it.job == null)
    }

    /**
     * Unless a person stopped the app or the transfer's job, the transfer is parked for a launch,
     * or the platform refused to store its work, a transfer can reach its ending and its
     * handler's wake with nobody opening the app: by the jobs the platform holds and the
     * recovery a boot or an update owes.
     */
    private fun everyTransferCanSettleInTheBackground(graph: Graph, starved: Boolean) {
        val reached = LifecycleModel.reaching(graph, backgroundOnly = true, ::settled)
        // A refusal parks the transfer when its answer comes; until then, and after a death before it, the record names no job.
        fun unstored(w: World) = starved && w.durable.records.values.any { r ->
            r.phase != Phase.Ended && r.phase != Phase.Retired && r.job == null && w.jobs.none { it.first == r.key }
        }
        val stuck = graph.states.indices.firstOrNull {
            val w = graph.states[it]
            !reached[it] && !w.person && !w.halted && w.tasked.isEmpty() && !unstored(w) && w.durable.records.values.none { r -> r.parked }
        }
        if (stuck != null) fail("only a launch moves ${graph.states[stuck]}")
    }

    private fun whole(config: Config, least: Int) {
        val graph = explore(config, least)
        everyStateCanBeCleared(graph)
        everyTransferCanSettleInTheBackground(graph, config.starved)
    }

    // Every fault and every answer an operation can give; a schedule is answered in a later step; effects are performed with their step.
    private val broad = Config(api = 35, limits = limits, persons = listOf(true), lag = 0)

    @Test fun aPersonsTransferHoldsEveryInvariantWhereAUserInitiatedJobExists() = whole(broad, least = 50_000)

    @Test fun aPersonsTransferHoldsEveryInvariantWhateverARebootDoesToItsJob() =
        whole(broad.copy(reboots = Reboot.values().toList(), updates = false), least = 50_000)

    @Test fun aPersonsTransferHoldsEveryInvariantWhereOnlyWorkExists() = whole(broad.copy(api = 33), least = 40_000)

    @Test fun aPersonsTransferHoldsEveryInvariantWhenThePlatformRefusesItsJob() = whole(broad.copy(denied = true), least = 30_000)

    @Test fun aPersonsTransferHoldsEveryInvariantWhenWorkCannotBeStored() =
        whole(broad.copy(starved = true, updates = false), least = 50_000)

    @Test fun aProgramsTransferHoldsEveryInvariant() = whole(broad.copy(persons = listOf(false)), least = 15_000)

    // Here a handler that completes without retiring its ending has one background wake, the other reading of D10.
    @Test fun aTransferOfTwoRequestsHoldsEveryInvariant() =
        whole(broad.copy(persons = listOf(false), requests = 2, limits = limits.copy(completedWakes = 1)), least = 25_000)

    // Effects wait in the queue while other events come. The person's searches take one of each kind of answer to stay in memory.
    private val slim = Config(api = 35, limits = Limits(stops = 1, networkRetries = 1, wakes = 1, completedWakes = 1), persons = listOf(true),
        responses = listOf(Lifecycle.Response.Ok), failures = listOf(Lifecycle.Failure.Cancelled), stops = listOf(Lifecycle.Stop.System),
        outcomes = listOf(Lifecycle.Outcome.Completed, Lifecycle.Outcome.Expired), updates = false)

    @Test fun aProgramsTransferHoldsEveryInvariantWithAnEffectWaiting() = whole(broad.copy(persons = listOf(false), lag = 1), least = 40_000)

    @Test fun aPersonsTransferHoldsEveryInvariantWithAnEffectWaitingWhereAUserInitiatedJobExists() = whole(slim.copy(lag = 1), least = 40_000)

    @Test fun aPersonsTransferHoldsEveryInvariantWithTwoEffectsWaitingWhereOnlyWorkExists() = whole(slim.copy(api = 33, lag = 2), least = 40_000)

    // An id is started again only when its retired generation's record is gone; that generation's jobs and runs may outlive it.
    @Test fun aReusedIdHoldsEveryInvariantAfterItsRetiredGeneration() =
        whole(broad.copy(persons = listOf(false), generations = 2), least = 100_000)

    // Two registrations side by side multiply the states, so this search takes one fault a path.
    private val narrow = slim.copy(persons = listOf(false), faults = 1, lag = 0)

    @Test fun anAgentDriveAndTheAppHoldEveryInvariantSideBySide() =
        everyStateCanBeCleared(explore(narrow.copy(namespaces = listOf(app, drive), straight = true, failures = emptyList(),
            stops = emptyList(), outcomes = listOf(Lifecycle.Outcome.Completed)), least = 50_000))
}
