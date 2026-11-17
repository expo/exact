package dev.exact.canvas

import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import org.junit.Assert.*
import org.junit.Test

class PickersTest {
    /** A local date and time joins its day and time by `T` or a space, as HTML's grammar does (#275): a value the
     * agent typed with a space reaches the app as typed, and the picker opens on it, held to bounds written so. */
    @Test fun aLocalDateAndTimeTakesASpaceAsItsSeparator() {
        val at = LocalDateTime.of(2026, 3, 4, 10, 30, 45, 500_000_000)
        assertEquals(at, ExactPicker.parseDateTime("2026-03-04 10:30:45.500"))
        assertEquals(at, ExactPicker.parseDateTime("2026-03-04T10:30:45.5"))
        assertNull(ExactPicker.parseDateTime("2026-03-04  10:30"))
        assertNull(ExactPicker.parseDateTime("2026-03-04"))
        val picker = ExactPicker("datetime-local", "2026-03-04 10:30", "2026-03-04 09:00", "", "any")
        assertEquals(LocalDate.of(2026, 3, 4), picker.heldDay)
        assertEquals(LocalTime.of(10, 30), picker.time())
        assertFalse(picker.allows("2026-03-04T08:59"))
        assertTrue(picker.allows("2026-03-04 09:00"))
    }
}
