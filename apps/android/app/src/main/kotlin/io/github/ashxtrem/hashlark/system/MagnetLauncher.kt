// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri

/** An installed app that opens magnet links. */
data class MagnetHandler(val packageName: String, val label: String)

/**
 * Hands magnet links to another torrent client, used when the built-in client
 * is off (ADR 0008, amended by ADR 0017). Hashlark itself opens `magnet:`
 * links, so it is left out of the handlers and never picked for a hand-off.
 */
object MagnetLauncher {
    enum class Outcome { Opened, NoHandler }

    /** A well-formed magnet used only to ask Android who handles the scheme. */
    private const val PROBE = "magnet:?xt=urn:btih:0000000000000000000000000000000000000000"

    private fun viewIntent(magnet: String) = Intent(Intent.ACTION_VIEW, Uri.parse(magnet))
        .addCategory(Intent.CATEGORY_BROWSABLE)

    /** Other torrent clients installed on this device (needs the `magnet:` entry in `<queries>`). */
    fun handlers(context: Context): List<MagnetHandler> {
        val pm = context.packageManager
        return pm.queryIntentActivities(viewIntent(PROBE), PackageManager.MATCH_ALL)
            .map { MagnetHandler(it.activityInfo.packageName, it.loadLabel(pm).toString()) }
            .let { withoutSelf(it, context.packageName) }
    }

    /** [found] without [ownPackage], one entry per app, sorted by label. */
    fun withoutSelf(found: List<MagnetHandler>, ownPackage: String): List<MagnetHandler> =
        found.filter { it.packageName != ownPackage }
            .distinctBy { it.packageName }
            .sortedBy { it.label.lowercase() }

    /**
     * Opens [magnet] in [preferredPackage] when that app is still installed
     * and handles magnets, else in the first other client. Always targets an
     * app explicitly, so the link never comes back to Hashlark.
     */
    fun open(context: Context, magnet: String, preferredPackage: String? = null): Outcome {
        val intent = viewIntent(magnet)
        val handlers = handlers(context)
        val target = handlers.firstOrNull { it.packageName == preferredPackage } ?: handlers.firstOrNull()
            ?: return Outcome.NoHandler
        intent.setPackage(target.packageName)
        return try {
            context.startActivity(intent)
            Outcome.Opened
        } catch (_: ActivityNotFoundException) {
            Outcome.NoHandler
        }
    }

    /** Whether at least one app can open magnet links. */
    fun hasHandler(context: Context): Boolean = handlers(context).isNotEmpty()

    /** Builds a magnet from an infohash when the provider gave none (for drag and drop). */
    fun fromInfoHash(infoHash: String, title: String): String =
        "magnet:?xt=urn:btih:$infoHash&dn=${Uri.encode(title)}"
}
