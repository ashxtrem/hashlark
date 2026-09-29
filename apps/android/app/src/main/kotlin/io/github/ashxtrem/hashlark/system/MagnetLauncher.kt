// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri

/** An installed app that opens magnet links. */
data class MagnetHandler(val packageName: String, val label: String)

/** Hands magnet links to a torrent client (ADR 0008). */
object MagnetLauncher {
    enum class Outcome { Opened, NoHandler }

    /** A well-formed magnet used only to ask Android who handles the scheme. */
    private const val PROBE = "magnet:?xt=urn:btih:0000000000000000000000000000000000000000"

    private fun viewIntent(magnet: String) = Intent(Intent.ACTION_VIEW, Uri.parse(magnet))
        .addCategory(Intent.CATEGORY_BROWSABLE)

    /** Torrent clients installed on this device (needs the `magnet:` entry in `<queries>`). */
    fun handlers(context: Context): List<MagnetHandler> {
        val pm = context.packageManager
        return pm.queryIntentActivities(viewIntent(PROBE), PackageManager.MATCH_ALL)
            .map { MagnetHandler(it.activityInfo.packageName, it.loadLabel(pm).toString()) }
            .distinctBy { it.packageName }
            .sortedBy { it.label.lowercase() }
    }

    /**
     * Opens [magnet] in [preferredPackage] when that app is still installed
     * and handles magnets, else in whatever Android picks.
     */
    fun open(context: Context, magnet: String, preferredPackage: String? = null): Outcome {
        val intent = viewIntent(magnet)
        if (preferredPackage != null && handlers(context).any { it.packageName == preferredPackage }) {
            intent.setPackage(preferredPackage)
        }
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
