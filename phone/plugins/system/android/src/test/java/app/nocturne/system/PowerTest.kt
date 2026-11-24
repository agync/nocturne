package app.nocturne.system

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PowerTest {
    @Test
    fun theChargeIsAWholePerCentRoundedDown() {
        assertEquals(50, Power.percent(50, 100))
        assertEquals(49, Power.percent(99, 200))
        assertEquals(33, Power.percent(1, 3))
        assertEquals(0, Power.percent(0, 100))
        assertEquals(100, Power.percent(255, 255))
    }

    @Test
    fun aChargeBeyondTheScaleStopsAtTheEnds() {
        assertEquals(100, Power.percent(120, 100))
    }

    @Test
    fun aReportThatMakesNoSenseGivesNothing() {
        assertNull(Power.percent(-1, 100))
        assertNull(Power.percent(50, 0))
        assertNull(Power.percent(50, -1))
        assertNull(Power.percent(-1, -1))
    }
}
