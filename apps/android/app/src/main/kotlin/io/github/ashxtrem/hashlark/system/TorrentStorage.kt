// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import java.io.File

/**
 * Where the built-in client saves downloads. librqbit needs a real file path,
 * so MediaStore can't be used: files go to `Download/Hashlark`, or to the
 * app's own Downloads folder when writing there fails (no permission on
 * Android 8-9, or a device that refuses it).
 */
object TorrentStorage {
    const val FOLDER = "Hashlark"

    /** Asked for (on Android 8-9 only) before the first download into `Download/Hashlark`. */
    const val STORAGE_PERMISSION = Manifest.permission.WRITE_EXTERNAL_STORAGE

    /** `Download/Hashlark` on shared storage. */
    @Suppress("DEPRECATION") // still the real path; the app writes there through the file API
    fun publicDir(): File =
        File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), FOLDER)

    /** The app's own Downloads folder: always writable, removed with the app. */
    fun appDir(context: Context): File =
        context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: File(context.filesDir, "downloads")

    /** Whether the UI should ask for [STORAGE_PERMISSION] before downloading (Android 8-9). */
    fun needsStoragePermission(context: Context): Boolean =
        Build.VERSION.SDK_INT <= Build.VERSION_CODES.P &&
            ContextCompat.checkSelfPermission(context, STORAGE_PERMISSION) != PackageManager.PERMISSION_GRANTED

    /** The default download folder: `Download/Hashlark` when a test file can be written there. Does I/O. */
    fun resolve(context: Context): File {
        val public = publicDir()
        if (!needsStoragePermission(context) && canWrite(public)) return public
        return appDir(context).also { it.mkdirs() }
    }

    /** Creates [dir] if needed and writes and deletes a small file in it. */
    fun canWrite(dir: File): Boolean = runCatching {
        if (!dir.isDirectory && !dir.mkdirs()) return false
        val probe = File(dir, ".hashlark-write-probe")
        probe.writeBytes(byteArrayOf(1))
        probe.delete()
        true
    }.getOrDefault(false)

    /** The FileProvider authority (see `res/xml/file_paths.xml`). */
    fun authority(context: Context): String = context.packageName + ".files"

    /** A `content:` Uri other apps can open for a downloaded [file]. */
    fun uriFor(context: Context, file: File): Uri = FileProvider.getUriForFile(context, authority(context), file)
}
