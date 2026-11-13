// The Android pedometer's ranges and sums (LLP 1133.026 §15 stage 3), fed clocks, reads and counter events, since an emulator
// need not have a step counter or current Play services. `count.mjs` compiles this with the Android count and subscription.
import exact.pedometer.Count
import exact.pedometer.Subscription
import kotlin.system.exitProcess

var failed = 0

fun check(name: String, answer: Count.Answer, steps: Double, since: Double) {
    val ok = answer.steps == steps && answer.since == since
    if (!ok) failed++
    println("${if (ok) "PASS" else "FAIL"} count $name${if (ok) "" else ": ${answer.steps} since ${answer.since}, not $steps since $since"}")
}

fun checkThat(name: String, ok: Boolean) {
    if (!ok) failed++
    println("${if (ok) "PASS" else "FAIL"} count $name")
}

fun main() {
    // The sensor path: the counter's increase from its first event.
    Count(0.0).run {
        counter(1000.0, 10.0)
        check("sensor: the first event is the zero", sensor(10.0), 0.0, 0.0)
        counter(1050.0, 20.0)
        check("sensor: steps since the first event", sensor(20.0), 50.0, 0.0)
        // A hidden stretch: no event arrives meanwhile, so the return takes a new zero and moves `since`.
        hidden()
        counter(1200.0, 100.0)
        check("sensor: after a hidden stretch, a new zero and since", sensor(100.0), 0.0, 100.0)
        counter(1230.0, 110.0)
        check("sensor: counting on after the return", sensor(110.0), 30.0, 100.0)
        // A reboot: the counter falls and counts on from its new value.
        counter(5.0, 200.0)
        check("sensor: a reboot's fall is a new zero", sensor(200.0), 0.0, 200.0)
        counter(25.0, 210.0)
        check("sensor: counting on after the reboot", sensor(210.0), 20.0, 200.0)
    }
    // The Recording path: a total, then the counter's increase until the next read.
    Count(0.0).run {
        counter(500.0, 1.0)
        checkThat("recording: the first answer reads the API", due(1.0))
        check("recording: the total read", recorded(300.0, 0.0, 1.0, 1.0), 300.0, 0.0)
        counter(520.0, 2.0)
        checkThat("recording: a fresh total is not read again", !due(2.0))
        check("recording: the total and the counter's increase", between(2.0), 320.0, 0.0)
        checkThat("recording: a total a minute old is read again", due(1.0 + Count.READ_EVERY_MS))
        // A hidden stretch: the counter's last value is not trusted, so the return reads the API again.
        hidden()
        checkThat("recording: the return reads the API", due(3.0))
        check("recording: the return's total", recorded(410.0, 0.0, 4.0, 4.0), 410.0, 0.0)
        check("recording: no counter event yet adds nothing", between(5.0), 410.0, 0.0)
        counter(900.0, 6.0)
        counter(915.0, 7.0)
        check("recording: the counter counts from its first event after the read", between(7.0), 425.0, 0.0)
        // A reboot between reads: the counter falls; the total stands and the counter counts on from its new value.
        counter(3.0, 8.0)
        counter(13.0, 9.0)
        check("recording: a reboot's fall adds no negative steps", between(9.0), 435.0, 0.0)
    }
    val epoch = 1794268800000.0
    Subscription().run {
        checkThat("subscription: the first request subscribes", begin())
        checkThat("subscription: concurrent requests share the pending subscription", !begin())
        val first = epoch - 12 * 86_400_000
        checkThat("subscription: an earlier process's start is kept", complete(first, epoch) == first)
        val start = epoch - 30 * 60_000
        val restored = Count(start).range(since, epoch)!!
        checkThat("subscription: a restored walk reads from its saved start",
            restored.from == start && restored.endSeconds - restored.startSeconds == 1801L)
        checkThat("subscription: a later request keeps the stored start", since == first && !begin())
        val retained = Count(epoch - 11 * 86_400_000).range(since, epoch)!!
        checkThat("subscription: a subscription older than ten days retains the last ten",
            retained.from == epoch - 10 * 86_400_000 && retained.endSeconds - retained.startSeconds == 864001L)
    }
    Subscription().run {
        begin()
        complete(epoch - 60 * 60_000, epoch)
        val older = Count(epoch - 2 * 60 * 60_000).range(since, epoch)!!
        checkThat("subscription: a walk older than the stored start reads from that start", older.from == epoch - 60 * 60_000)
    }
    Subscription().run {
        val walk = Count(epoch - 30 * 60_000)
        checkThat("subscription: no Recording read before the subscription is asked", range(walk, epoch) == null)
        begin()
        checkThat("subscription: none while it is pending, so no empty total is kept", range(walk, epoch) == null)
        complete(null, epoch)
        checkThat("subscription: a read once it is made", range(walk, epoch + 60_000)?.from == epoch)
    }
    run {
        val dir = kotlin.io.path.createTempDirectory("pedometer-marker").toFile()
        try {
            val marker = java.io.File(dir, "subscribed")
            checkThat("marker: a new installation has no start", Subscription(marker).stored() == null)
            Subscription(marker).store(epoch)
            checkThat("marker: a later process reads the start stored", Subscription(marker).stored() == epoch)
            marker.writeText("garbled")
            checkThat("marker: one it cannot read is no start", Subscription(marker).stored() == null)
            Subscription(java.io.File(dir, "gone/subscribed")).store(epoch)
            checkThat("marker: a store that cannot write is no start, and no failure",
                Subscription(java.io.File(dir, "gone/subscribed")).stored() == null)
        } finally { dir.deleteRecursively() }
    }
    Subscription().run {
        begin()
        failed()
        checkThat("subscription: a failed subscription can be retried", begin())
        checkThat("subscription: the first subscription's start is now", complete(null, epoch) == epoch)
        val fresh = Count(epoch - 30 * 60_000).range(since, epoch + 60_000)!!
        checkThat("subscription: a new subscription bounds an older walk",
            fresh.from == epoch && fresh.endSeconds - fresh.startSeconds == 61L)
        checkThat("subscription: later requests keep the new subscription's start", since == epoch && !begin())
    }
    Count(epoch).run {
        counter(500.0, epoch)
        val first = recorded(120.0, epoch, 1500.0, epoch + 60_000)
        check("clocks: a Recording total starts at an epoch date", first, 120.0, epoch)
        checkThat("clocks: a Recording answer has an epoch timestamp", first.at == epoch + 60_000)
        checkThat("clocks: the session clock expires a total at sixty seconds", !due(61_499.0) && due(61_500.0))

        counter(510.0, epoch + 61_000)
        val next = between(epoch + 61_000)
        check("clocks: steps accumulate between reads", next, 130.0, epoch)
        checkThat("clocks: an interim answer has an epoch timestamp", next.at == epoch + 61_000)

        val adjustedEpoch = epoch - 3_600_000
        checkThat("clocks: the session clock makes a read due after an epoch correction", due(61_500.0))
        checkThat("clocks: no inverted request is built after a clock correction", range(epoch, adjustedEpoch) == null)
        checkThat("clocks: a held Recording total is available", hasRecording)
        val adjusted = between(adjustedEpoch)
        check("clocks: a correction keeps the total and the counter's increase", adjusted, 130.0, epoch)
        checkThat("clocks: a wall-clock correction reaches the answer", adjusted.at == adjustedEpoch)
        counter(515.0, adjustedEpoch + 1000)
        check("clocks: steps keep accumulating while the range is inverted", between(adjustedEpoch + 1000), 135.0, epoch)
        checkThat("clocks: deferring a read leaves it due", due(62_500.0))

        val resumed = range(epoch, epoch + 1)!!
        checkThat("clocks: a read resumes as soon as the epoch catches up", due(62_500.0) && resumed.endSeconds > resumed.startSeconds)
        recorded(240.0, resumed.from, 62_500.0, epoch + 1)
        checkThat("clocks: only a completed read resets the refresh interval", !due(122_499.0) && due(122_500.0))

        hidden()
        counter(600.0, epoch + 120_000)
        counter(610.0, epoch + 121_000)
        val returned = sensor(epoch + 121_000)
        check("clocks: a foreground return sets an epoch since", returned, 10.0, epoch + 120_000)
        checkThat("clocks: a sensor answer has an epoch timestamp", returned.at == epoch + 121_000)
        counter(5.0, epoch + 122_000)
        check("clocks: a counter reset sets an epoch since", sensor(epoch + 122_000), 0.0, epoch + 122_000)
    }
    Count(epoch).run {
        val first = range(null, epoch)!!
        checkThat("ranges: an omitted start at the first ask has a valid request",
            first.from == epoch && first.startSeconds == 1794268800L && first.endSeconds == 1794268801L && first.bucketSeconds == 1)
        val read = range(epoch, epoch + 60_500)!!
        checkThat("ranges: a normal refresh uses epoch seconds and one complete bucket",
            read.startSeconds == 1794268800L && read.endSeconds == 1794268861L && read.bucketSeconds == 61)
        checkThat("ranges: a subscription can move the boundary", range(epoch + 10_000, epoch + 60_000)!!.from == epoch + 10_000)
        checkThat("ranges: a clock correction before the subscription defers the read", range(epoch + 10_000, epoch + 5_000) == null)
        checkThat("ranges: a future start in the same second defers the read", range(null, epoch - 1) == null)
        checkThat("ranges: retention clamps an old boundary", range(epoch, epoch + 11 * 86_400_000)!!.from == epoch + 86_400_000)

        counter(100.0, epoch - 3_600_000)
        counter(110.0, epoch - 3_599_000)
        checkThat("ranges: a correction before the first total leaves the sensor available",
            !hasRecording && due(1500.0) && range(null, epoch - 3_599_000) == null)
        check("ranges: the sensor counts while the first range is inverted", sensor(epoch - 3_599_000), 10.0, epoch)
    }
    println(if (failed > 0) "$failed failed" else "all passed")
    exitProcess(if (failed > 0) 1 else 0)
}
