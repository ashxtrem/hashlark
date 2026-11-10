// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.PowerManager
import androidx.core.content.ContextCompat

/**
 * Tells the torrent client when the phone counts as "on battery": battery
 * saver on or the level under 15%, while not charging. The core then applies
 * the `on_battery` setting (alternative speed, pause, or nothing).
 *
 * [onChange] gets the new answer whenever it changes, and once at the start.
 */
class PowerGate(
    private val context: Context,
    private val onChange: (onBattery: Boolean) -> Unit,
) {
    private var last: Boolean? = null
    private var receiver: BroadcastReceiver? = null

    fun start() {
        if (receiver != null) return
        val r = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                // Power saver changes carry no battery data: read the sticky battery intent again.
                val battery = if (intent.action == Intent.ACTION_BATTERY_CHANGED) intent else stickyBattery()
                publish(battery)
            }
        }
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_BATTERY_CHANGED)
            addAction(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED)
        }
        // System broadcasts only; the sticky ACTION_BATTERY_CHANGED comes back at once.
        val sticky = ContextCompat.registerReceiver(context, r, filter, ContextCompat.RECEIVER_NOT_EXPORTED)
        receiver = r
        publish(sticky)
    }

    fun stop() {
        val r = receiver ?: return
        receiver = null
        runCatching { context.unregisterReceiver(r) }
    }

    private fun stickyBattery(): Intent? =
        ContextCompat.registerReceiver(
            context, null, IntentFilter(Intent.ACTION_BATTERY_CHANGED), ContextCompat.RECEIVER_NOT_EXPORTED,
        )

    @Synchronized
    private fun publish(battery: Intent?) {
        val powerSaver = context.getSystemService(PowerManager::class.java)?.isPowerSaveMode == true
        val level = battery?.let {
            levelPercent(it.getIntExtra(BatteryManager.EXTRA_LEVEL, -1), it.getIntExtra(BatteryManager.EXTRA_SCALE, -1))
        }
        val charging = battery?.let { isCharging(it.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0)) } ?: false
        val now = onBattery(powerSaver, level, charging)
        if (now != last) {
            last = now
            onChange(now)
        }
    }

    companion object {
        /** Below this battery level (percent) the phone counts as on battery even without power saver. */
        const val LOW_LEVEL = 15

        fun onBattery(powerSaver: Boolean, levelPercent: Int?, charging: Boolean): Boolean =
            (powerSaver || (levelPercent != null && levelPercent < LOW_LEVEL)) && !charging

        /** The level as a percentage, or `null` when the battery doesn't report one. */
        fun levelPercent(level: Int, scale: Int): Int? =
            if (level < 0 || scale <= 0) null else level * 100 / scale

        /**
         * Plugged into any power source. The status field is not used: it can still say
         * "charging" for a moment after unplugging (and does under `dumpsys battery unplug`).
         */
        fun isCharging(plugged: Int): Boolean = plugged != 0
    }
}
