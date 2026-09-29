// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.ActivityNotFoundException
import android.content.ComponentName
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import androidx.annotation.RequiresApi
import io.github.ashxtrem.hashlark.MainActivity
import io.github.ashxtrem.hashlark.ffi.sanitizeFileName

/** Saving `.torrent` files without any storage permission. */
object TorrentFiles {
    const val MIME_TYPE = "application/x-bittorrent"

    /** `Title.torrent`, named by the same rules as the desktop app (the core does the cleaning). */
    fun fileName(title: String): String = sanitizeFileName(title) + ".torrent"

    /** Whether files can go straight into Downloads (Android 10+, through MediaStore). */
    val canSaveToDownloads: Boolean get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q

    /**
     * Saves into the Downloads folder through MediaStore. MediaStore renames a
     * duplicate on its own, so nothing is overwritten. Returns the file's Uri.
     */
    @RequiresApi(Build.VERSION_CODES.Q)
    fun saveToDownloads(context: Context, bytes: ByteArray, fileName: String): Uri {
        val resolver = context.contentResolver
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, fileName)
            put(MediaStore.Downloads.MIME_TYPE, MIME_TYPE)
            put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
            put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            ?: error("could not create the file in Downloads")
        try {
            resolver.openOutputStream(uri)?.use { it.write(bytes) } ?: error("could not open the file")
            resolver.update(uri, ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }, null, null)
        } catch (e: Exception) {
            resolver.delete(uri, null, null)
            throw e
        }
        return uri
    }

    /** Writes into a document the user picked with the "Save as" dialog. */
    fun writeTo(context: Context, uri: Uri, bytes: ByteArray) {
        context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(bytes) }
            ?: error("could not open the file")
    }

    /** Offers the saved file to a torrent client. Returns `false` when no app can open it. */
    fun openWith(context: Context, uri: Uri): Boolean {
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, MIME_TYPE)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        // Hashlark opens .yml files, never torrents: keep it out of its own chooser.
        val chooser = Intent.createChooser(intent, "Open with").putExtra(
            Intent.EXTRA_EXCLUDE_COMPONENTS,
            arrayOf(ComponentName(context, MainActivity::class.java)),
        )
        return try {
            context.startActivity(chooser)
            true
        } catch (_: ActivityNotFoundException) {
            false
        }
    }
}
