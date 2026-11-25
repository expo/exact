package dev.exact.canvas

import android.graphics.Rect
import android.os.Looper
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowLayoutInfo
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class FoldReviewRegressionTest {
    private fun fold(band: Rect, axis: FoldingFeature.Orientation) = WindowLayoutInfo(listOf(object : FoldingFeature {
        override val bounds = band
        override val orientation = axis
        override val state = FoldingFeature.State.HALF_OPENED
        override val isSeparating = true
        override val occlusionType = FoldingFeature.OcclusionType.NONE
    }))

    private val portrait = Rect(0, 0, 900, 1402)
    private val landscape = Rect(0, 0, 1402, 900)
    private val book = fold(Rect(450, 0, 450, 1402), FoldingFeature.Orientation.VERTICAL)
    private val laptop = fold(Rect(0, 450, 1402, 450), FoldingFeature.Orientation.HORIZONTAL)
    private val empty = WindowLayoutInfo(emptyList())
    private val columns = intArrayOf(2, 1, 450, 450)
    private val rows = intArrayOf(2, 2, 450, 450)
    private val single = intArrayOf(1, 0, 0, 0)
    private val updates = MutableSharedFlow<WindowLayoutInfo>(extraBufferCapacity = 1)
    private var bounds = portrait
    private fun placement() = FoldWindow(bounds, 0, if (bounds == landscape) 1 else 0)
    private var current = book
    private lateinit var folds: WindowFolds
    private var told: IntArray? = null

    private fun refresh() { told = folds.report(placement(), hasHinge = true) }

    private var initialOnSubscribe = true
    private fun listen(snapshot: (() -> WindowLayoutInfo)?) {
        val layouts = flow {
            if (initialOnSubscribe) emit(current)
            emitAll(updates)
        }
        folds = WindowFolds(layouts, snapshot, ::placement, ::refresh)
        folds.start()
        refresh()
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun emit(layout: WindowLayoutInfo) {
        told = null
        assertTrue(updates.subscriptionCount.value == 1 && updates.tryEmit(layout))
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test fun queuedFeaturesCannotReplaceTheCurrentSnapshotAfterEitherTurn() {
        listen { current }
        try {
            assertArrayEquals(columns, told)
            bounds = landscape
            current = laptop
            refresh()
            assertArrayEquals(rows, told)
            emit(book)
            assertArrayEquals(rows, told)
            bounds = portrait
            current = book
            refresh()
            assertArrayEquals(columns, told)
            emit(laptop)
            assertArrayEquals(columns, told)
        } finally { folds.stop() }
    }

    @Test fun aWindowMovingOffAndBackAcrossTheHingeReadsItsCurrentSnapshot() {
        listen { current }
        try {
            assertArrayEquals(columns, told)
            bounds = Rect(portrait).apply { offset(900, 0) }
            current = empty
            refresh()
            assertArrayEquals(single, told)
            bounds = portrait
            current = book
            refresh()
            assertArrayEquals(columns, told)
        } finally { folds.stop() }
    }

    @Test fun aStreamOnlyWindowConfirmsAmbiguousEventsAfterAMoveInEitherDirection() {
        listen(null)
        try {
            assertArrayEquals(columns, told)
            bounds = Rect(portrait).apply { offset(900, 0) }
            current = empty
            refresh()
            shadowOf(Looper.getMainLooper()).idle()
            assertArrayEquals(single, told)
            emit(book)
            assertArrayEquals(single, told)
            bounds = portrait
            current = book
            refresh()
            shadowOf(Looper.getMainLooper()).idle()
            assertArrayEquals(columns, told)
            emit(empty)
            assertArrayEquals(columns, told)
        } finally { folds.stop() }
    }

    @Test fun aTrackerWithoutTheSnapshotMethodStillReportsItsFlow() {
        listen { throw UnsupportedOperationException() }
        try {
            assertArrayEquals(columns, told)
            emit(empty)
            assertArrayEquals(single, told)
        } finally { folds.stop() }
    }

    @Test fun aSilentStreamKeepsTheMeasuredCreaseAtItsScreenPosition() {
        listen(null)
        try {
            initialOnSubscribe = false
            for ((offset, expected) in listOf(10 to intArrayOf(2, 1, 440, 440), 900 to single, 0 to columns)) {
                bounds = Rect(portrait).apply { offset(offset, 0) }
                refresh()
                shadowOf(Looper.getMainLooper()).idle()
                assertArrayEquals(expected, told)
                emit(if (offset == 0) empty else book)
                assertArrayEquals(expected, told)
            }
        } finally { folds.stop() }
    }

    @Test fun aStreamCanBeTheFirstNoticeOfEitherTurnEvenIfTheNextRegistrationIsSilent() {
        listen(null)
        try {
            assertArrayEquals(columns, told)
            initialOnSubscribe = false
            bounds = landscape
            emit(laptop)
            assertArrayEquals(rows, told)
            emit(book)
            assertArrayEquals(rows, told)
            bounds = portrait
            emit(book)
            assertArrayEquals(columns, told)
            emit(laptop)
            assertArrayEquals(columns, told)
        } finally { folds.stop() }
    }
}
