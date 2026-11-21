package dev.exact.canvas

import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class AccessTest {
    /** One node of a statement, in the words `presenter::access::Presenter::reader_access` writes. */
    private class Said(
        val id: Int, val parent: Int, val role: Int, val name: String = "",
        val set: Pair<Int, Int> = 0 to 0, val grid: List<Int> = listOf(0, 0, 0, 0),
        val flags: Int = 0, val range: List<Float> = listOf(0f, 0f, 0f),
    )

    /** A statement of the page telling [nodes] whole, as the reader is given it. */
    private fun statement(nodes: List<Said>): IntArray {
        val w = mutableListOf(1, 0, 1, 0)
        fun string(s: String) {
            val bytes = s.toByteArray(Charsets.UTF_8)
            w.add(bytes.size)
            for (part in bytes.toList().chunked(4)) w.add(part.foldIndexed(0) { i, word, b -> word or ((b.toInt() and 255) shl (i * 8)) })
        }
        string("")
        w.add(0)
        w.add(nodes.size)
        for (n in nodes) {
            w.addAll(listOf(n.id, n.parent, n.id, n.role, n.flags, 0, 0, 100, 20))
            w.addAll(n.range.map { java.lang.Float.floatToIntBits(it) })
            w.addAll(listOf(n.set.first, n.set.second))
            w.addAll(n.grid)
            for (s in listOf(n.name, "", "")) string(s)
        }
        return w.toIntArray()
    }

    private fun told(nodes: List<Said>): Access {
        val access = Access(FrameLayout(RuntimeEnvironment.getApplication()))
        access.told(Access.Told(statement(nodes)))
        return access
    }

    /** A table of a header row whose first cell spans two columns, and a body whose row header spans two rows: the
     * table says its rows and columns, each cell its place and its spans, a header cell is a heading, and a row is
     * no item. */
    @Test fun aTablesCellsSayTheirPlacesAndSpans() {
        val cell = Access.CELL; val col = Access.COLUMN_HEADER; val row = Access.ROW
        val access = told(listOf(
            Said(1, 0, Access.TABLE, grid = listOf(0, 3, 0, 3)),
            Said(2, 1, Access.CAPTION),
            Said(3, 1, row, grid = listOf(1, 1, 1, 3)),
            Said(4, 3, col, "Move", grid = listOf(1, 1, 1, 2)),
            Said(5, 3, col, "Power", grid = listOf(1, 1, 3, 1)),
            Said(6, 1, row, grid = listOf(2, 1, 1, 3)),
            Said(7, 6, Access.ROW_HEADER, "Tackle", grid = listOf(2, 2, 1, 1)),
            Said(8, 6, cell, "Normal", grid = listOf(2, 1, 2, 1)),
            Said(9, 6, cell, "40", grid = listOf(2, 1, 3, 1)),
            Said(10, 1, row, grid = listOf(3, 1, 1, 3)),
            Said(11, 10, cell, "Physical", grid = listOf(3, 1, 2, 1)),
            Said(12, 10, cell, "35", grid = listOf(3, 1, 3, 1)),
        ))
        val info = { id: Int -> access.provider.createAccessibilityNodeInfo(id)!! }
        val table = info(1).collectionInfo!!
        assertEquals(listOf(3, 3, AccessibilityNodeInfo.CollectionInfo.SELECTION_MODE_NONE), listOf(table.rowCount, table.columnCount, table.selectionMode))
        assertFalse(table.isHierarchical)
        @Suppress("DEPRECATION")
        fun place(id: Int) = info(id).collectionItemInfo!!.let { listOf(it.rowIndex, it.rowSpan, it.columnIndex, it.columnSpan, if (it.isHeading) 1 else 0) }
        assertEquals(listOf(0, 1, 0, 2, 1), place(4))
        assertEquals(listOf(0, 1, 2, 1, 1), place(5))
        assertEquals(listOf(1, 2, 0, 1, 1), place(7))
        assertEquals(listOf(1, 1, 1, 1, 0), place(8))
        assertEquals(listOf(2, 1, 1, 1, 0), place(11))
        assertEquals(listOf(2, 1, 2, 1, 0), place(12))
        assertTrue(info(4).isHeading); assertTrue(info(7).isHeading); assertFalse(info(8).isHeading)
        for (id in listOf(2, 3, 6)) assertNull("node $id", info(id).collectionItemInfo)
        assertNull(info(2).collectionInfo)
    }

    /** A table whose rows the statement does not place (no table formation laid it out) is no collection, and its
     * cells are no items: TalkBack reads them as groups rather than in places that are not theirs. */
    @Test fun aTableWithNoPlacesIsNoCollection() {
        val access = told(listOf(
            Said(1, 0, Access.TABLE),
            Said(2, 1, Access.ROW),
            Said(3, 2, Access.CELL, "40"),
        ))
        assertNull(access.provider.createAccessibilityNodeInfo(1)!!.collectionInfo)
        assertNull(access.provider.createAccessibilityNodeInfo(3)!!.collectionItemInfo)
    }

    /** A description list is a list of its terms: each term its place among them, a definition none. */
    @Test fun aDescriptionListCountsItsTerms() {
        val access = told(listOf(
            Said(1, 0, Access.DESCRIPTION_LIST, set = 0 to 2),
            Said(2, 1, Access.TERM, "Type", set = 1 to 2),
            Said(3, 1, Access.DEFINITION, "Electric"),
            Said(4, 1, Access.TERM, "Weight", set = 2 to 2),
            Said(5, 1, Access.DEFINITION, "6 kg"),
        ))
        val info = { id: Int -> access.provider.createAccessibilityNodeInfo(id)!! }
        val list = info(1).collectionInfo!!
        assertEquals(listOf(2, 1), listOf(list.rowCount, list.columnCount))
        assertNull(info(1).collectionItemInfo)
        val second = info(4).collectionItemInfo!!
        assertEquals(listOf(1, 1, 0, 1), listOf(second.rowIndex, second.rowSpan, second.columnIndex, second.columnSpan))
        assertNull(info(3).collectionItemInfo)
    }

    /** A drawn spin button is what TalkBack steps by swiping up and down, a SeekBar, which only steps (a drawn range
     * takes no value set to it); its value is its number, not a float's "3.0". */
    @Test fun aSpinButtonIsAdjustableAndSaysItsNumber() {
        val access = told(listOf(
            Said(1, 0, Access.SPIN_BUTTON, "Floor", flags = Access.RANGE or Access.FORWARD or Access.BACKWARD, range = listOf(1f, 100f, 3f)),
        ))
        val info = access.provider.createAccessibilityNodeInfo(1)!!
        assertEquals("android.widget.SeekBar", info.className)
        assertEquals(listOf(1f, 100f, 3f), info.rangeInfo!!.let { listOf(it.min, it.max, it.current) })
        assertEquals("3", info.stateDescription)
        val actions = info.actionList.map { it.id }
        assertTrue(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD.id in actions)
        assertTrue(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD.id in actions)
        assertFalse(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.id in actions)
    }

    /** A node a held press drags offers TalkBack's Start drag (LLP 1094.000 D9), which asks exact2 for the drag. */
    @Test @Config(sdk = [33], shadows = [Acts::class]) fun aNodeThatDragsOffersStartDrag() {
        Acts.asked.clear()
        val access = told(listOf(Said(1, 0, Access.GROUP, "Ochre dunes", flags = Access.DRAG), Said(2, 0, Access.BUTTON, "Open", flags = Access.CLICK)))
        val start = android.R.id.accessibilityActionDragStart
        assertTrue(start in access.provider.createAccessibilityNodeInfo(1)!!.actionList.map { it.id })
        assertFalse(start in access.provider.createAccessibilityNodeInfo(2)!!.actionList.map { it.id })
        assertTrue(access.provider.performAction(1, start, null))
        assertFalse(access.provider.performAction(2, start, null))
        assertEquals(listOf(1 to 6), Acts.asked)
    }

    /** Android 12, which has no Start drag (API 32 adds it), offers none. */
    @Test fun noStartDragBeforeApi32() {
        val access = told(listOf(Said(1, 0, Access.GROUP, "Ochre dunes", flags = Access.DRAG)))
        assertTrue(access.provider.createAccessibilityNodeInfo(1)!!.actionList.none { it.id == android.R.id.accessibilityActionDragStart })
    }

    /** exact2's side of an act: each node and act asked, all taken. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Acts {
        companion object {
            val asked = ArrayList<Pair<Int, Int>>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun accessAct(id: Int, action: Int, value: Float): Boolean { asked += id to action; return true }
        }
    }
}
