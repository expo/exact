package dev.exact.canvas

import java.nio.IntBuffer
import org.junit.Assert.*
import org.junit.Test

class StructureTest {
    @Test fun clipMaskAndTintRemainInTheDrawingSignature() {
        // A two-child union (each child has its own fill rule), a mask layer,
        // and a transparent tint followed by both image draw forms.
        val union = intArrayOf(45, 10, 2, 0, 2, 0, 0, 0, 3, 1, 1, 3, 6)
        val mask = intArrayOf(7, 1f.toRawBits(), 46, 2, -1) + IntArray(12) + intArrayOf(6, 6)
        val images = intArrayOf(47, 0, 8, 7, 0, 0, 0, 0, 47, -65536, 14, 7) + IntArray(16)
        val words = union + mask + images
        val b = IntBuffer.wrap(words)
        val s = Structure()
        assertEquals(union.size - 1, s.end(b, 0))
        assertArrayEquals(words, s.read(b) { it })
    }

    @Test fun rowReplacementChangesOnlyTheDrawingThatPlacesIt() {
        val s = Structure()
        for (place in listOf(intArrayOf(17, 9, 0, 0), intArrayOf(55, 9, 1f.toBits(), 0, 0, 1f.toBits(), 0, 0))) {
            val group = IntBuffer.wrap(intArrayOf(20, 7) + place + 21)
            val root = s.read(group) { 1 }
            val before = s.groupSigs.getValue(7)
            assertFalse(s.updateRows { 2 })
            assertEquals(setOf(7), s.changedGroups)
            assertEquals(2, before[1])
            assertArrayEquals(root, s.read(group) { 2 })
            assertArrayEquals(before, s.groupSigs.getValue(7))
            val page = IntBuffer.wrap(place)
            val drawing = s.read(page) { 1 }
            assertTrue(s.updateRows { 2 })
            assertTrue(s.changedGroups.isEmpty())
            assertFalse(s.updateRows { 2 })
            assertArrayEquals(drawing, s.read(page) { 2 })
        }
    }

    @Test fun aMaskAndClipUnionDoNotEndTheirContainingOpacityLayer() {
        val words = intArrayOf(7, 1f.toRawBits(), 45, 1, 0, 46, 6, 6, 6)
        val s = Structure()
        val b = IntBuffer.wrap(words)
        assertEquals(8, s.layerEnd(b, 2, words.size))
        assertEquals(7, s.layerEnd(b, 5, words.size))
        assertEquals(6, s.layerEnd(b, 6, words.size))
        assertEquals(-1, s.layerEnd(b, 2, words.size - 1))
    }

    @Test fun nestedGroupsTrackIndependentSignaturesRangesAndParents() {
        val s = Structure()
        val beforeRow = intArrayOf(17, 10, 0, 0)
        val innerRow = intArrayOf(17, 20, 0, 40)
        val afterRow = intArrayOf(17, 30, 0, 200)
        val stream = IntBuffer.wrap(
            intArrayOf(20, 1) + beforeRow + intArrayOf(20, 2) + innerRow + intArrayOf(21) + afterRow + intArrayOf(21)
        )
        var tok10 = 1; var tok20 = 2; var tok30 = 3
        val token: (Int) -> Int = { when (it) { 10 -> tok10; 20 -> tok20; else -> tok30 } }
        val root = s.read(stream, token = token)
        val outerBefore = s.groupSigs.getValue(1).copyOf()
        val innerBefore = s.groupSigs.getValue(2).copyOf()
        assertEquals(listOf(1, 2), s.groups.map { it[0] })
        assertEquals(mapOf(2 to 1), s.groupParents)
        assertArrayEquals(intArrayOf(2, stream.limit() - 1), s.groupRange(stream, 1))
        assertArrayEquals(intArrayOf(8, 12), s.groupRange(stream, 2))

        val outerDirect = ArrayList<Int>()
        s.forEachDirectRow(stream, s.groupRange(stream, 1)!!) { outerDirect.add(stream.get(it + 1)) }
        assertEquals(listOf(10, 30), outerDirect)

        val innerDirect = ArrayList<Int>()
        s.forEachDirectRow(stream, s.groupRange(stream, 2)!!) { innerDirect.add(stream.get(it + 1)) }
        assertEquals(listOf(20), innerDirect)

        // Updating an inner row changes only the inner group, leaving outer and root untouched.
        tok20 = 22
        assertFalse(s.updateRows(token))
        assertEquals(setOf(2), s.changedGroups)
        assertArrayEquals(outerBefore, s.groupSigs.getValue(1))
        assertFalse(innerBefore.contentEquals(s.groupSigs.getValue(2)))
        assertArrayEquals(root, s.read(stream, token = token))
        assertArrayEquals(outerBefore, s.groupSigs.getValue(1))

        // Updating an outer row after the nested group changes only the outer group.
        val innerNow = s.groupSigs.getValue(2).copyOf()
        tok30 = 33
        assertFalse(s.updateRows(token))
        assertEquals(setOf(1), s.changedGroups)
        assertArrayEquals(innerNow, s.groupSigs.getValue(2))
        assertArrayEquals(root, s.read(stream, token = token))
    }
}
