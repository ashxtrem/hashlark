// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.content.Intent
import android.net.Uri
import io.github.ashxtrem.hashlark.core.AddSource
import io.github.ashxtrem.hashlark.ui.AppEvent
import io.github.ashxtrem.hashlark.ui.Destination
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

// A plain Application: the real one schedules background work, which these tests do not need.
@Config(application = android.app.Application::class)
@RunWith(RobolectricTestRunner::class)
class IntentRouterTest {
    private fun route(intent: Intent, files: Map<String, String> = emptyMap()) =
        IntentRouter.route(intent, readText = { files[it.toString()] })

    @Test
    fun `shared text starts a search`() {
        val event = route(Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_TEXT, "  night of the living dead "))
        assertEquals("night of the living dead", (event as AppEvent.RunSearch).query.text)
    }

    @Test
    fun `a shared imdb link becomes an exact search`() {
        val event = route(Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_TEXT, "https://www.imdb.com/title/tt0063350/"))
        assertEquals("tt0063350", (event as AppEvent.RunSearch).query.imdbId)
    }

    @Test
    fun `the selection menu starts a search`() {
        val event = route(Intent(Intent.ACTION_PROCESS_TEXT).putExtra(Intent.EXTRA_PROCESS_TEXT, "ubuntu 24.04"))
        assertEquals("ubuntu 24.04", (event as AppEvent.RunSearch).query.text)
    }

    @Test
    fun `empty shares do nothing`() {
        assertNull(route(Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_TEXT, "   ")))
        assertNull(route(Intent(Intent.ACTION_SEND)))
    }

    @Test
    fun `hashlark links add a repository or search`() {
        val repo = route(Intent(Intent.ACTION_VIEW, Uri.parse("hashlark://repo?url=https%3A%2F%2Fexample.org%2Fdefs%2F")))
        assertEquals("https://example.org/defs/", (repo as AppEvent.ImportRepo).url)

        val search = route(Intent(Intent.ACTION_VIEW, Uri.parse("hashlark://search?q=arch+linux")))
        assertEquals("arch linux", (search as AppEvent.RunSearch).query.text)
    }

    @Test
    fun `a repository link must be http or https`() {
        assertNull(route(Intent(Intent.ACTION_VIEW, Uri.parse("hashlark://repo?url=file%3A%2F%2F%2Fetc%2Fpasswd"))))
        assertNull(route(Intent(Intent.ACTION_VIEW, Uri.parse("hashlark://repo"))))
        assertNull(route(Intent(Intent.ACTION_VIEW, Uri.parse("hashlark://other"))))
    }

    @Test
    fun `a yaml file offers a definition import`() {
        val uri = "content://files/provider.yml"
        val event = route(Intent(Intent.ACTION_VIEW, Uri.parse(uri)), mapOf(uri to "id: x\nname: X"))
        assertEquals("id: x\nname: X", (event as AppEvent.ImportDefinition).yaml)
        assertNull(route(Intent(Intent.ACTION_VIEW, Uri.parse(uri)), emptyMap()))
    }

    @Test
    fun `a magnet link adds a torrent`() {
        val magnet = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Ubuntu"
        val event = route(Intent(Intent.ACTION_VIEW, Uri.parse(magnet)))
        assertEquals(AddSource.Magnet(magnet), (event as AppEvent.AddTorrent).source)
    }

    @Test
    fun `a torrent file adds a torrent by its type or name`() {
        val bytes = "d4:infod4:name1:aee".toByteArray()
        val read = { _: Uri -> bytes }
        val expected = AddSource.Torrent(java.util.Base64.getEncoder().encodeToString(bytes))

        // A file manager gives an opaque content Uri with the type.
        val typed = Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse("content://docs/document/42"), "application/x-bittorrent")
        assertEquals(expected, (IntentRouter.route(typed, { null }, read) as AppEvent.AddTorrent).source)

        // A browser download often has only the name.
        val named = Intent(Intent.ACTION_VIEW, Uri.parse("content://downloads/files/Ubuntu.Torrent"))
        assertEquals(expected, (IntentRouter.route(named, { null }, read) as AppEvent.AddTorrent).source)
    }

    @Test
    fun `an unreadable or too big torrent file does nothing`() {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse("file:///sdcard/Download/big.torrent"))
        assertNull(IntentRouter.route(intent, { "not yaml" }, { null }))
        assertNull(IntentRouter.route(intent, { null }, { ByteArray(0) }))
    }

    @Test
    fun `a yaml file is not taken for a torrent`() {
        val uri = Uri.parse("content://files/provider.yml")
        assertTrue(!IntentRouter.isTorrentFile(uri, "text/yaml"))
        assertTrue(IntentRouter.isTorrentFile(uri, "application/x-bittorrent"))
    }

    @Test
    fun `launcher shortcuts open their screen`() {
        assertEquals(Destination.Favorites, (route(Intent(IntentRouter.ACTION_FAVORITES)) as AppEvent.Navigate).destination)
        assertEquals(Destination.History, (route(Intent(IntentRouter.ACTION_HISTORY)) as AppEvent.Navigate).destination)
        assertEquals(Destination.Search, (route(Intent(IntentRouter.ACTION_NEW_SEARCH)) as AppEvent.Navigate).destination)
    }

    @Test
    fun `other intents are ignored`() {
        assertNull(route(Intent(Intent.ACTION_MAIN)))
        assertTrue(IntentRouter.searchUri(io.github.ashxtrem.hashlark.core.SearchQuery("a b")).toString().startsWith("hashlark://search"))
    }
}
