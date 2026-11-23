package app.nocturne.system

import android.os.BatteryManager

/** Whether the phone charges, from the battery's own report. */
object Power {
    /**
     * Plugged in and charging or full: true; on battery: false; null when the phone gave no
     * report. A phone on a charger that is not charging counts as on battery.
     */
    fun charging(plugged: Int, status: Int): Boolean? {
        if (plugged < 0 || status < 0) return null
        if (plugged == 0) return false
        return status == BatteryManager.BATTERY_STATUS_CHARGING ||
            status == BatteryManager.BATTERY_STATUS_FULL
    }
}
