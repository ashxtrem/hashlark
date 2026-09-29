// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import kotlin.concurrent.thread

/**
 * A tiny Torznab endpoint on the loopback interface, so tests run a real
 * search through the real engine without any network. Plain HTTP is fine: the
 * requests come from the native library, which is not subject to Android's
 * cleartext policy.
 */
class TorznabFixture(
    private val count: Int = 5,
    /** Time to wait before answering, to make a search that is still running. */
    private val delayMs: Long = 0,
) : AutoCloseable {
    private val server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
    @Volatile private var open = true

    val url: String get() = "http://127.0.0.1:${server.localPort}/api"

    init {
        thread(name = "torznab-fixture", isDaemon = true) {
            while (open) {
                val socket = try {
                    server.accept()
                } catch (_: Exception) {
                    return@thread
                }
                thread(isDaemon = true) { handle(socket) }
            }
        }
    }

    private fun handle(socket: Socket) = socket.use {
        try {
            val reader = it.getInputStream().bufferedReader()
            // Read the request line and headers; the body is empty for GET.
            while (reader.readLine().orEmpty().isNotEmpty()) Unit
            if (delayMs > 0) Thread.sleep(delayMs)
            val body = xml().toByteArray()
            it.getOutputStream().apply {
                write("HTTP/1.1 200 OK\r\nContent-Type: application/rss+xml\r\nContent-Length: ${body.size}\r\nConnection: close\r\n\r\n".toByteArray())
                write(body)
                flush()
            }
        } catch (_: Exception) {
            // The client gave up (a cancelled search): nothing to do.
        }
    }

    private fun xml(): String = buildString {
        append("""<?xml version="1.0" encoding="UTF-8"?><rss version="2.0" xmlns:torznab="http://torznab.com/schemas/2015/feed"><channel>""")
        for (i in 1..count) {
            val hash = i.toString(16).padStart(40, '0')
            append(
                """<item><title>Fixture Result $i</title><link>http://127.0.0.1:${server.localPort}/dl/$i.torrent</link>""" +
                    """<pubDate>Mon, 01 Sep 2026 10:00:00 +0000</pubDate><size>${1_000_000L * i}</size>""" +
                    """<torznab:attr name="seeders" value="${100 - i}"/><torznab:attr name="peers" value="${110 - i}"/>""" +
                    """<torznab:attr name="infohash" value="$hash"/></item>""",
            )
        }
        append("</channel></rss>")
    }

    override fun close() {
        open = false
        runCatching { server.close() }
    }
}
