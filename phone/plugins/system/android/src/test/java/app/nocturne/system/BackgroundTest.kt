package app.nocturne.system

import android.os.BatteryManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class BackgroundTest {
    @Test
    fun onlyAPhoneChargingOrFullCountsAsCharging() {
        val usb = BatteryManager.BATTERY_PLUGGED_USB
        assertEquals(true, Power.charging(usb, BatteryManager.BATTERY_STATUS_CHARGING))
        assertEquals(true, Power.charging(usb, BatteryManager.BATTERY_STATUS_FULL))
        assertEquals(false, Power.charging(usb, BatteryManager.BATTERY_STATUS_NOT_CHARGING))
        assertEquals(false, Power.charging(0, BatteryManager.BATTERY_STATUS_CHARGING))
        assertNull(Power.charging(-1, -1))
    }

    @Test
    fun aWindowIsOpenUntilTheCoreIsDone() {
        assertFalse(Window.isOpen)
        val latch = Window.begin()
        assertTrue(Window.isOpen)
        Window.done()
        assertFalse(Window.isOpen)
        assertEquals(0L, latch.count)
        Window.done()
        assertFalse(Window.isOpen)
    }
}
