// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.MediaScannerConnection
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import io.github.ashxtrem.hashlark.MainActivity
import io.github.ashxtrem.hashlark.core.BulkAction
import io.github.ashxtrem.hashlark.core.BulkRequest
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.HashlarkApi
import io.github.ashxtrem.hashlark.core.KeepAwake
import io.github.ashxtrem.hashlark.core.OnBattery
import io.github.ashxtrem.hashlark.core.RunMode
import io.github.ashxtrem.hashlark.core.SessionStats
import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.core.TorrentSummary
import io.github.ashxtrem.hashlark.hashlark
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.io.File

/**
 * Keeps the built-in torrent client running while the app is in the
 * background: a foreground service with a progress notification.
 *
 * - It starts when a torrent is added or the app opens with unfinished
 *   torrents, and stops itself once nothing is downloading, seeding or waiting
 *   for Wi-Fi, the schedule or the battery (paused and stopped torrents don't
 *   keep it alive).
 * - It runs the Wi-Fi gate ([NetworkGate]) and the battery gate ([PowerGate]).
 * - It holds a partial wake lock and a Wi-Fi lock while `keep_awake` says so.
 * - It polls the core every [POLL_MS] for the notification, and asks the
 *   media scanner to index files once a download completes.
 */
class TorrentService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /** Calls into the core from the gates and the notification actions, run one at a time and in order. */
    private val commands = Channel<suspend (HashlarkApi) -> Unit>(Channel.UNLIMITED)

    private lateinit var network: NetworkGate
    private lateinit var power: PowerGate
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var notification: Notification? = null

    @Volatile
    private var lastStartId = 0

    /** Whether each torrent had completed at the last poll, to spot new completions. */
    private val completed = HashMap<String, Boolean>()

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        goForeground(build(TorrentServiceLogic.Text("Downloads", "Starting…", null)))

        network = NetworkGate(this) { allowed -> commands.trySend { it.setNetworkAllowed(allowed) } }
        power = PowerGate(this) { onBattery -> commands.trySend { it.setTorrentPower(onBattery) } }
        network.start()
        power.start()

        scope.launch {
            for (command in commands) {
                try {
                    command(api())
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    Log.w(TAG, "torrent call failed", e)
                }
            }
        }
        scope.launch { pollLoop() }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Every startForegroundService call must be answered with startForeground.
        notification?.let(::goForeground)
        lastStartId = startId
        when (intent?.action) {
            ACTION_PAUSE_ALL -> commands.trySend { it.torrentBulk(BulkRequest(action = BulkAction.Pause)) }
            ACTION_STOP -> commands.trySend { it.torrentBulk(BulkRequest(action = BulkAction.Stop)) }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        scope.cancel()
        commands.close()
        network.stop()
        power.stop()
        holdLocks(false)
        super.onDestroy()
    }

    private suspend fun api(): HashlarkApi = hashlark.startTorrents()

    private suspend fun pollLoop() {
        var idleTicks = 0
        var failures = 0
        while (scope.isActive) {
            val keep = try {
                tick().also { failures = 0 }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.w(TAG, "torrent poll failed", e)
                failures += 1
                failures < MAX_FAILURES
            }
            // Two idle polls in a row: a torrent added a moment ago may not have a state yet.
            idleTicks = if (keep) 0 else idleTicks + 1
            if (idleTicks >= IDLE_TICKS_TO_STOP) {
                holdLocks(false)
                // False when a start (a new torrent) came in meanwhile: keep going.
                if (stopSelfResult(lastStartId)) return
                idleTicks = 0
                failures = 0
            }
            delay(POLL_MS)
        }
    }

    /** One poll; returns whether the service is still needed. */
    private suspend fun tick(): Boolean {
        val api = api()
        val settings = api.settings().torrents
        network.setUnmeteredOnly(settings.unmeteredOnly)
        val list = api.torrents()
        val stats = api.torrentStats()
        scanNewlyCompleted(api, list)
        if (!TorrentServiceLogic.keepsRunning(stats, list)) return false
        holdLocks(TorrentServiceLogic.keepAwake(settings.keepAwake, list))
        val built = build(TorrentServiceLogic.text(list, stats, settings.onBattery))
        notification = built
        if (TorrentServiceLogic.canNotify(this)) {
            @Suppress("MissingPermission") // checked by canNotify
            NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, built)
        }
        return true
    }

    /** Asks the media scanner to index the files of torrents that completed since the last poll. */
    private suspend fun scanNewlyCompleted(api: HashlarkApi, list: List<TorrentSummary>) {
        // Only a change seen while the service runs counts; older downloads were scanned before.
        val newly = list.filter { it.completedAt != null && completed[it.id] == false }
        completed.clear()
        list.forEach { completed[it.id] = it.completedAt != null }
        for (summary in newly) {
            val details = runCatching { api.torrent(summary.id) }.getOrNull() ?: continue
            val paths = details.files.filter { it.selected }.map { File(summary.saveDir, it.path).absolutePath }
            if (paths.isNotEmpty()) MediaScannerConnection.scanFile(this, paths.toTypedArray(), null, null)
        }
    }

    @Synchronized
    private fun holdLocks(hold: Boolean) {
        if (hold) {
            val wake = wakeLock ?: getSystemService(PowerManager::class.java)
                ?.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Hashlark:torrents")
                ?.apply { setReferenceCounted(false) }
            // A timeout renewed on every poll, so the lock can never outlive the service.
            wake?.acquire(LOCK_TIMEOUT_MS)
            wakeLock = wake
            val wifi = wifiLock ?: applicationContext.getSystemService(WifiManager::class.java)
                ?.let {
                    @Suppress("DEPRECATION") // still honoured; the replacement (low latency) needs the screen on
                    it.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "Hashlark:torrents")
                }
                ?.apply { setReferenceCounted(false) }
            if (wifi != null && !wifi.isHeld) wifi.acquire()
            wifiLock = wifi
        } else {
            wakeLock?.takeIf { it.isHeld }?.release()
            wifiLock?.takeIf { it.isHeld }?.release()
        }
    }

    private fun createChannel() {
        val channel = NotificationChannel(CHANNEL_ID, "Downloads", NotificationManager.IMPORTANCE_LOW).apply {
            description = "Progress of torrent downloads"
            setShowBadge(false)
        }
        getSystemService(NotificationManager::class.java)?.createNotificationChannel(channel)
    }

    private fun build(text: TorrentServiceLogic.Text): Notification {
        val open = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(text.title)
            .setContentText(text.text)
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .addAction(0, "Pause all", actionIntent(ACTION_PAUSE_ALL, 1))
            .addAction(0, "Stop", actionIntent(ACTION_STOP, 2))
        text.progress?.let { builder.setProgress(PROGRESS_MAX, it, false) }
        return builder.build()
    }

    private fun actionIntent(action: String, requestCode: Int): PendingIntent = PendingIntent.getService(
        this, requestCode,
        Intent(this, TorrentService::class.java).setAction(action),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun goForeground(notification: Notification) {
        this.notification = notification
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        } else {
            0
        }
        ServiceCompat.startForeground(this, NOTIFICATION_ID, notification, type)
    }

    companion object {
        private const val TAG = "TorrentService"
        const val CHANNEL_ID = "torrents"
        const val NOTIFICATION_ID = 0x7042
        const val ACTION_PAUSE_ALL = "io.github.ashxtrem.hashlark.action.TORRENTS_PAUSE_ALL"
        const val ACTION_STOP = "io.github.ashxtrem.hashlark.action.TORRENTS_STOP"

        /** How often the notification is refreshed. */
        const val POLL_MS = 2_000L
        private const val IDLE_TICKS_TO_STOP = 2
        private const val MAX_FAILURES = 5
        private const val LOCK_TIMEOUT_MS = 60_000L
        private const val PROGRESS_MAX = 1000

        /** Asked for on the first add (Android 13+); without it the service runs but shows nothing. */
        const val NOTIFICATION_PERMISSION = "android.permission.POST_NOTIFICATIONS"

        /** Whether the UI should ask for [NOTIFICATION_PERMISSION] before the first download. */
        fun needsNotificationPermission(context: Context): Boolean =
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !TorrentServiceLogic.canNotify(context)

        /** Starts the service (it stops itself when there is nothing to do). Returns false if Android refused. */
        fun start(context: Context): Boolean = try {
            ContextCompat.startForegroundService(context, Intent(context, TorrentService::class.java))
            true
        } catch (e: IllegalStateException) {
            // Android 12+ refuses to start a foreground service from the background.
            Log.w(TAG, "could not start the torrent service", e)
            false
        }
    }
}

/** The service's decisions, kept free of Android so they can be unit-tested. */
object TorrentServiceLogic {
    /** Notification content; [progress] is 0 to 1000, or null for no bar. */
    data class Text(val title: String, val text: String, val progress: Int?)

    private val transferring = setOf(TorrentState.Metadata, TorrentState.Checking, TorrentState.Downloading)

    /** States the core gives a torrent that would transfer if the Wi-Fi, schedule or battery allowed it. */
    private val held = setOf(
        TorrentState.WaitingForNetwork,
        TorrentState.OnBattery,
        TorrentState.ScheduledOff,
        TorrentState.ScheduledSeedOnly,
    )

    /** Downloading (or fetching metadata, or checking). */
    fun isDownloading(t: TorrentSummary): Boolean = t.state in transferring

    /**
     * Waiting for Wi-Fi, the schedule or the battery. The core puts every torrent in these
     * states, so the user's Pause and Stop and finished torrents are left out.
     */
    fun isHeld(t: TorrentSummary): Boolean =
        t.state in held && (t.runMode == RunMode.Auto || t.runMode == RunMode.Forced) && !t.done

    /** Whether the service is still needed. */
    fun keepsRunning(stats: SessionStats, list: List<TorrentSummary>): Boolean =
        stats.running && list.any { isDownloading(it) || it.state == TorrentState.Seeding || isHeld(it) }

    /**
     * Whether opening the app starts the service: something is active, or an unfinished torrent
     * the user hasn't paused or stopped (the core may not have given it a state yet).
     */
    fun startsOnOpen(stats: SessionStats, list: List<TorrentSummary>): Boolean =
        keepsRunning(stats, list) ||
            stats.running && list.any {
                it.completedAt == null && it.state != TorrentState.Error &&
                    (it.runMode == RunMode.Auto || it.runMode == RunMode.Forced)
            }

    /** Whether to hold the wake and Wi-Fi locks. */
    fun keepAwake(setting: KeepAwake, list: List<TorrentSummary>): Boolean = when (setting) {
        KeepAwake.Off -> false
        KeepAwake.Downloading -> list.any(::isDownloading)
        KeepAwake.Active -> list.any { isDownloading(it) || it.state == TorrentState.Seeding }
    }

    /** Why transfers are held back, for the notification; null when nothing holds them. */
    fun reason(stats: SessionStats, onBattery: OnBattery): String? = when {
        !stats.networkAllowed -> "Waiting for Wi-Fi"
        stats.onBattery && onBattery == OnBattery.Pause -> "Paused: battery"
        stats.onBattery && onBattery == OnBattery.AltSpeed -> "On battery: alternative speed"
        else -> null
    }

    /** Overall progress of the unfinished torrents that are downloading or waiting, 0 to 1000. */
    fun progress(list: List<TorrentSummary>): Int? {
        val active = list.filter { (isDownloading(it) || isHeld(it)) && it.completedAt == null }
        val total = active.sumOf { it.size }
        if (active.isEmpty() || total <= 0) return null
        val have = active.sumOf { it.progress.coerceIn(0.0, 1.0) * it.size }
        return (have / total * 1000).toInt().coerceIn(0, 1000)
    }

    fun text(list: List<TorrentSummary>, stats: SessionStats, onBattery: OnBattery): Text {
        val downloading = list.count { isDownloading(it) || (isHeld(it) && it.completedAt == null) }
        val seeding = list.count { it.state == TorrentState.Seeding }
        val title = when {
            downloading > 0 -> "Downloading " + plural(downloading)
            seeding > 0 -> "Seeding " + plural(seeding)
            else -> "Downloads"
        }
        val progress = progress(list)
        val parts = listOfNotNull(
            reason(stats, onBattery),
            progress?.let { "${it / 10}%" },
            "↓ ${Format.bytes(stats.downBps)}/s  ↑ ${Format.bytes(stats.upBps)}/s",
        )
        return Text(title, parts.joinToString(" · "), progress)
    }

    private fun plural(n: Int) = if (n == 1) "1 torrent" else "$n torrents"

    fun canNotify(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, TorrentService.NOTIFICATION_PERMISSION) ==
            PackageManager.PERMISSION_GRANTED
}
