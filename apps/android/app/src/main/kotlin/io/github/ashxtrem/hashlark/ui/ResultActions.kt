// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import io.github.ashxtrem.hashlark.challenge.ChallengeActivity
import io.github.ashxtrem.hashlark.core.DownloadTarget
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.core.TargetPreference
import io.github.ashxtrem.hashlark.hashlark
import io.github.ashxtrem.hashlark.system.MagnetLauncher
import io.github.ashxtrem.hashlark.system.TorrentFiles
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * What happens when the user opens, copies, saves or shares a result. One
 * instance serves every screen, so a result behaves the same in the list, the
 * details pane, favourites and the drag handle.
 */
@Stable
class ResultActions(
    private val context: Context,
    private val scope: CoroutineScope,
    private val app: AppViewModel,
    private val startChallenge: (providerId: String, name: String, url: String) -> Unit,
    private val startSaveAs: (fileName: String) -> Unit,
    private val keepBytes: (ByteArray?) -> Unit,
) {
    /** A magnet no app could open; the UI offers to copy or share it. */
    var noHandlerMagnet: String? by mutableStateOf(null)

    /** Opens the result in the user's torrent client (ADR 0008). */
    fun open(result: MergedResult) {
        scope.launch {
            val target = app.withApi("Could not open this result") { it.resolve(result.id) } ?: return@launch
            when (target) {
                is DownloadTarget.Magnet -> openMagnet(target.url)
                is DownloadTarget.TorrentFile -> saveTorrentFile(result, target.url, saveAs = false, openAfter = true)
            }
        }
    }

    private suspend fun openMagnet(magnet: String) {
        val preferred = context.hashlark.prefs.magnetPackage.first()
        if (MagnetLauncher.open(context, magnet, preferred) == MagnetLauncher.Outcome.NoHandler) {
            noHandlerMagnet = magnet
        }
    }

    fun copyMagnet(result: MergedResult) {
        scope.launch {
            val target = app.withApi("Could not copy the magnet link") { it.resolve(result.id, TargetPreference.Magnet) }
                ?: return@launch
            if (target is DownloadTarget.Magnet) {
                copyText("Magnet link", target.url)
                app.say("Magnet link copied")
            } else {
                app.say("This result has no magnet link; use the .torrent instead.")
            }
        }
    }

    /** Saves the `.torrent`; with [saveAs] the user picks the place (needed before Android 10). */
    fun saveTorrent(result: MergedResult, saveAs: Boolean = false) {
        scope.launch {
            val target = app.withApi("Could not get the .torrent") { it.resolve(result.id, TargetPreference.TorrentFile) }
                ?: return@launch
            if (target is DownloadTarget.TorrentFile) {
                saveTorrentFile(result, target.url, saveAs, openAfter = false)
            } else {
                app.say("This result has no .torrent file; use the magnet link instead.")
            }
        }
    }

    private suspend fun saveTorrentFile(result: MergedResult, url: String, saveAs: Boolean, openAfter: Boolean) {
        val bytes = app.withApi("Could not get the .torrent") { it.fetchTorrent(url) } ?: return
        val name = TorrentFiles.fileName(result.primary.title)
        if (!saveAs && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            try {
                val uri = TorrentFiles.saveToDownloads(context, bytes, name)
                if (openAfter) {
                    if (!TorrentFiles.openWith(context, uri)) app.say("Saved $name to Downloads")
                } else {
                    app.say("Saved $name to Downloads", "Open with") {
                        if (!TorrentFiles.openWith(context, uri)) app.say("No app can open .torrent files")
                    }
                }
            } catch (e: Exception) {
                app.report(e, "Could not save the .torrent")
            }
            return
        }
        // Android 9 and older (no MediaStore downloads), or the user asked to choose the place.
        keepBytes(bytes)
        startSaveAs(name)
    }

    /** The user picked a place in the "Save as" dialog. */
    fun finishSaveAs(uri: Uri?, bytes: ByteArray?) {
        keepBytes(null)
        if (uri == null || bytes == null) return
        try {
            TorrentFiles.writeTo(context, uri, bytes)
            app.say("Saved the .torrent", "Open with") {
                if (!TorrentFiles.openWith(context, uri)) app.say("No app can open .torrent files")
            }
        } catch (e: Exception) {
            app.report(e, "Could not save the .torrent")
        }
    }

    fun share(result: MergedResult) {
        scope.launch {
            val target = app.withApi("Could not share this result") { it.resolve(result.id, TargetPreference.Magnet) }
                ?: return@launch
            shareText(result.primary.title, target.url)
        }
    }

    fun shareText(subject: String, text: String) {
        val send = Intent(Intent.ACTION_SEND)
            .setType("text/plain")
            .putExtra(Intent.EXTRA_SUBJECT, subject)
            .putExtra(Intent.EXTRA_TEXT, text)
        context.startActivity(Intent.createChooser(send, "Share").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    fun copyText(label: String, text: String) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newPlainText(label, text))
    }

    fun viewOnSite(url: String) {
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (_: Exception) {
            app.say("No app can open this link")
        }
    }

    /** Starts the browser-check flow for a provider that shows a challenge page. */
    fun challenge(providerId: String) {
        val provider = app.providers.value.orEmpty().firstOrNull { it.id == providerId }
        val url = provider?.siteUrl
        if (provider == null || url == null) {
            app.say("This provider has no web page to open.")
            return
        }
        startChallenge(provider.id, provider.name, url)
    }

    /**
     * The text a drag of this result carries into another app (the magnet), or
     * `null` when it is not known without asking the provider.
     */
    fun dragText(result: MergedResult): String? =
        result.primary.magnet ?: result.primary.infoHash?.let { MagnetLauncher.fromInfoHash(it, result.primary.title) }

    /** Copies the magnet on Ctrl+C. */
    fun copyMagnetOffline(result: MergedResult) {
        val text = dragText(result)
        if (text != null) {
            copyText("Magnet link", text)
            app.say("Magnet link copied")
        } else {
            copyMagnet(result)
        }
    }
}

@Composable
fun rememberResultActions(app: AppViewModel, onChallengeDone: () -> Unit): ResultActions {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var bytes by remember { mutableStateOf<ByteArray?>(null) }
    // The launchers are created before the actions that use them, so the
    // callbacks reach the instance through this holder.
    val holder = remember { arrayOfNulls<ResultActions>(1) }

    val challengeLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == android.app.Activity.RESULT_OK) onChallengeDone()
    }
    val saveAs = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(TorrentFiles.MIME_TYPE)) { uri ->
        holder[0]?.finishSaveAs(uri, bytes)
    }
    return remember(context, app) {
        ResultActions(
            context = context,
            scope = scope,
            app = app,
            startChallenge = { id, name, url -> challengeLauncher.launch(ChallengeActivity.intent(context, id, name, url)) },
            startSaveAs = { fileName -> saveAs.launch(fileName) },
            keepBytes = { bytes = it },
        ).also { holder[0] = it }
    }
}
