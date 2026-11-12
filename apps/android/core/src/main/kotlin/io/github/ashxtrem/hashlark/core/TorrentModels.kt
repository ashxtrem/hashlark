// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/*
 * Mirrors crates/hashlark-torrent/src/model.rs and TorrentSettings in
 * crates/hashlark-core/src/settings.rs. Ids are info hashes (40 lowercase hex
 * characters), times are epoch milliseconds, sizes and rates are bytes.
 */

// ----- settings ---------------------------------------------------------------------------

/** Speed limits in KiB/s; null is unlimited. */
@Serializable
data class SpeedLimits(
    @SerialName("down_kib") val downKib: Long? = null,
    @SerialName("up_kib") val upKib: Long? = null,
)

/** When the device is kept awake. */
@Serializable
enum class KeepAwake {
    @SerialName("off") Off,
    @SerialName("downloading") Downloading,
    @SerialName("active") Active,
}

/** What to do when the device runs on battery. */
@Serializable
enum class OnBattery {
    @SerialName("nothing") Nothing,
    @SerialName("alt_speed") AltSpeed,
    @SerialName("pause") Pause,
}

/**
 * Settings of the built-in torrent client. Every field of the core's
 * `TorrentSettings` is here, with the core's defaults on Android; a field left
 * out would be reset to its default on the next save.
 */
@Serializable
data class TorrentSettings(
    val enabled: Boolean = true,
    @SerialName("download_dir") val downloadDir: String? = null,
    @SerialName("ask_on_add") val askOnAdd: Boolean = false,
    @SerialName("notice_accepted") val noticeAccepted: Boolean = false,
    @SerialName("listen_port") val listenPort: Int = 0,
    val upnp: Boolean = true,
    val dht: Boolean = true,
    val lsd: Boolean = true,
    val utp: Boolean = true,
    val limits: SpeedLimits = SpeedLimits(),
    @SerialName("alt_limits") val altLimits: SpeedLimits = SpeedLimits(downKib = 1024, upKib = 256),
    val turtle: Boolean = false,
    /** 168 hourly cells, Monday 00:00 first: `n` normal, `a` alternative, `s` seed only, `o` off. */
    val schedule: String? = null,
    @SerialName("max_active_downloads") val maxActiveDownloads: Int = 3,
    @SerialName("max_active_seeds") val maxActiveSeeds: Int = 5,
    @SerialName("peer_limit_global") val peerLimitGlobal: Int = 400,
    @SerialName("peer_limit_torrent") val peerLimitTorrent: Int = 100,
    @SerialName("keep_awake") val keepAwake: KeepAwake = KeepAwake.Downloading,
    @SerialName("on_battery") val onBattery: OnBattery = OnBattery.AltSpeed,
    /** Stop seeding at this share ratio; null seeds forever. */
    @SerialName("seed_ratio") val seedRatio: Float? = 0.0f,
    @SerialName("seed_minutes") val seedMinutes: Int? = null,
    val proxy: String? = null,
    @SerialName("blocklist_url") val blocklistUrl: String? = null,
    @SerialName("watch_dir") val watchDir: String? = null,
    @SerialName("unmetered_only") val unmeteredOnly: Boolean = true,
    @SerialName("close_to_tray") val closeToTray: Boolean = true,
)

/**
 * [settings] as the core reads it. The shared [HashlarkJson] leaves nulls out,
 * which the core reads as "use the default": for `seed_ratio`, whose default
 * is not null, that would turn "seed forever" back into the default ratio.
 */
internal fun encodeSettings(json: Json, settings: Settings): String {
    val tree = json.encodeToJsonElement(Settings.serializer(), settings).jsonObject
    val torrents = tree["torrents"]?.jsonObject ?: return tree.toString()
    if ("seed_ratio" in torrents) return tree.toString()
    return JsonObject(tree + ("torrents" to JsonObject(torrents + ("seed_ratio" to JsonNull)))).toString()
}

// ----- torrents ---------------------------------------------------------------------------

@Serializable
enum class RunMode {
    @SerialName("auto") Auto,
    @SerialName("paused") Paused,
    @SerialName("stopped") Stopped,
    @SerialName("forced") Forced,
}

@Serializable
enum class TorrentOrigin {
    @SerialName("magnet") Magnet,
    @SerialName("torrent") Torrent,
    @SerialName("result") Result,
    @SerialName("created") Created,
    @SerialName("watch") Watch,
}

@Serializable
enum class TorrentState {
    @SerialName("metadata") Metadata,
    @SerialName("checking") Checking,
    @SerialName("downloading") Downloading,
    @SerialName("seeding") Seeding,
    @SerialName("queued") Queued,
    @SerialName("paused") Paused,
    @SerialName("stopped") Stopped,
    @SerialName("finished") Finished,
    @SerialName("done") Done,
    @SerialName("scheduled_off") ScheduledOff,
    @SerialName("scheduled_seed_only") ScheduledSeedOnly,
    @SerialName("on_battery") OnBattery,
    @SerialName("waiting_for_network") WaitingForNetwork,
    @SerialName("error") Error,
}

/** One row of the torrent list. */
@Serializable
data class TorrentSummary(
    val id: String,
    val name: String,
    val state: TorrentState,
    @SerialName("run_mode") val runMode: RunMode,
    val source: TorrentOrigin,
    /** 0.0 to 1.0 of the selected files. */
    val progress: Double = 0.0,
    val size: Long = 0,
    val have: Long = 0,
    @SerialName("downloaded_total") val downloadedTotal: Long = 0,
    @SerialName("uploaded_total") val uploadedTotal: Long = 0,
    val ratio: Double = 0.0,
    @SerialName("down_bps") val downBps: Long = 0,
    @SerialName("up_bps") val upBps: Long = 0,
    @SerialName("eta_secs") val etaSecs: Long? = null,
    val peers: Int = 0,
    val seeds: Int = 0,
    @SerialName("queue_pos") val queuePos: Long = 0,
    @SerialName("save_dir") val saveDir: String = "",
    @SerialName("added_at") val addedAt: Long = 0,
    @SerialName("completed_at") val completedAt: Long? = null,
    val done: Boolean = false,
    val error: String? = null,
)

@Serializable
data class TorrentFile(
    val index: Int,
    val path: String,
    val size: Long = 0,
    val have: Long = 0,
    val selected: Boolean = true,
)

/** Per-torrent seeding limits; null uses the global setting. */
@Serializable
data class SeedLimits(
    val ratio: Float? = null,
    val minutes: Int? = null,
)

/** A torrent with its files and limits. On the wire the summary's fields sit beside the others. */
@Serializable(with = TorrentDetailsSerializer::class)
data class TorrentDetails(
    val summary: TorrentSummary,
    val files: List<TorrentFile> = emptyList(),
    val trackers: List<String> = emptyList(),
    val limits: SpeedLimits = SpeedLimits(),
    val peerLimit: Int? = null,
    val seed: SeedLimits = SeedLimits(),
    val magnet: String? = null,
    val resultId: String? = null,
    val seedingSince: Long? = null,
) {
    val id: String get() = summary.id
}

/** The fields of the core's `TorrentDetails` besides the flattened summary. */
@Serializable
private data class DetailsExtras(
    val files: List<TorrentFile> = emptyList(),
    val trackers: List<String> = emptyList(),
    val limits: SpeedLimits = SpeedLimits(),
    @SerialName("peer_limit") val peerLimit: Int? = null,
    val seed: SeedLimits = SeedLimits(),
    val magnet: String? = null,
    @SerialName("result_id") val resultId: String? = null,
    @SerialName("seeding_since") val seedingSince: Long? = null,
)

/** kotlinx.serialization has no `flatten`, so the summary is read from the same object. */
object TorrentDetailsSerializer : KSerializer<TorrentDetails> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("TorrentDetails")

    override fun deserialize(decoder: Decoder): TorrentDetails {
        val input = decoder as JsonDecoder
        val obj = input.decodeJsonElement()
        val summary = input.json.decodeFromJsonElement(TorrentSummary.serializer(), obj)
        val extras = input.json.decodeFromJsonElement(DetailsExtras.serializer(), obj)
        return TorrentDetails(
            summary = summary,
            files = extras.files,
            trackers = extras.trackers,
            limits = extras.limits,
            peerLimit = extras.peerLimit,
            seed = extras.seed,
            magnet = extras.magnet,
            resultId = extras.resultId,
            seedingSince = extras.seedingSince,
        )
    }

    override fun serialize(encoder: Encoder, value: TorrentDetails) {
        val output = encoder as JsonEncoder
        val summary = output.json.encodeToJsonElement(TorrentSummary.serializer(), value.summary).jsonObject
        val extras = output.json.encodeToJsonElement(
            DetailsExtras.serializer(),
            DetailsExtras(
                value.files, value.trackers, value.limits, value.peerLimit,
                value.seed, value.magnet, value.resultId, value.seedingSince,
            ),
        ).jsonObject
        output.encodeJsonElement(JsonObject(summary + extras))
    }
}

/** Where a torrent to add comes from. On the wire: `{"magnet": ".."}`. */
@Serializable(with = AddSourceSerializer::class)
sealed interface AddSource {
    data class Magnet(val uri: String) : AddSource

    /** A `.torrent` file, base64-encoded. */
    data class Torrent(val base64: String) : AddSource

    /** A result of a recent search, or a favourite. */
    data class ResultId(val id: String) : AddSource
}

object AddSourceSerializer : KSerializer<AddSource> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("AddSource")

    override fun serialize(encoder: Encoder, value: AddSource) {
        val (key, text) = when (value) {
            is AddSource.Magnet -> "magnet" to value.uri
            is AddSource.Torrent -> "torrent" to value.base64
            is AddSource.ResultId -> "result_id" to value.id
        }
        (encoder as JsonEncoder).encodeJsonElement(buildJsonObject { put(key, text) })
    }

    override fun deserialize(decoder: Decoder): AddSource {
        val obj = (decoder as JsonDecoder).decodeJsonElement().jsonObject
        val (key, value) = obj.entries.singleOrNull()
            ?: throw IllegalArgumentException("an add source has exactly one key")
        val text = value.jsonPrimitive.content
        return when (key) {
            "magnet" -> AddSource.Magnet(text)
            "torrent" -> AddSource.Torrent(text)
            "result_id" -> AddSource.ResultId(text)
            else -> throw IllegalArgumentException("unknown add source `$key`")
        }
    }
}

@Serializable
data class AddRequest(
    val source: AddSource,
    /** Folder to save into; the default download folder when null. */
    @SerialName("save_dir") val saveDir: String? = null,
    /** File indices to download; all when null. */
    val files: List<Int>? = null,
    /** Add in the paused run mode. */
    val paused: Boolean = false,
)

@Serializable
data class AddResponse(
    val torrent: TorrentSummary,
    /** The info hash was already in the client; nothing was added. */
    val already: Boolean = false,
)

@Serializable
enum class TorrentAction {
    @SerialName("pause") Pause,
    @SerialName("resume") Resume,
    @SerialName("stop") Stop,
    @SerialName("start") Start,
    @SerialName("force_start") ForceStart,
}

enum class QueueStep(val wire: String) {
    Top("top"),
    Up("up"),
    Down("down"),
    Bottom("bottom"),
}

/** A queue move: a step, or an absolute 0-based index. On the wire: `"top"` or `3`. */
@Serializable(with = QueueMoveSerializer::class)
sealed interface QueueMove {
    data class Step(val step: QueueStep) : QueueMove
    data class Index(val index: Int) : QueueMove
}

object QueueMoveSerializer : KSerializer<QueueMove> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("QueueMove")

    override fun serialize(encoder: Encoder, value: QueueMove) {
        val element: JsonElement = when (value) {
            is QueueMove.Step -> JsonPrimitive(value.step.wire)
            is QueueMove.Index -> JsonPrimitive(value.index)
        }
        (encoder as JsonEncoder).encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): QueueMove {
        val primitive = (decoder as JsonDecoder).decodeJsonElement().jsonPrimitive
        primitive.intOrNull?.let { return QueueMove.Index(it) }
        val step = QueueStep.entries.firstOrNull { it.wire == primitive.content }
            ?: throw IllegalArgumentException("unknown queue step `${primitive.content}`")
        return QueueMove.Step(step)
    }
}

/** Changes to a torrent. Only the fields that are set are sent. */
@Serializable
data class TorrentPatch(
    val action: TorrentAction? = null,
    /** Selected file indices (at least one). */
    val files: List<Int>? = null,
    /** Per-torrent speed limits; replaces both values. */
    val limits: SpeedLimits? = null,
    /** Own connection limit, 10 to 2000; 0 returns to the global default. */
    @SerialName("peer_limit") val peerLimit: Int? = null,
    /** Seeding limits; replaces both values. */
    val seed: SeedLimits? = null,
    @SerialName("queue_pos") val queuePos: QueueMove? = null,
    /** Replaces the tracker list; an empty list leaves DHT and PEX only. */
    val trackers: List<String>? = null,
)

/** An action on many torrents. On the wire: `"pause"`, `{"remove":{"delete_files":true}}`, `{"turtle":true}`. */
@Serializable(with = BulkActionSerializer::class)
sealed interface BulkAction {
    data object Pause : BulkAction
    data object Resume : BulkAction
    data object Stop : BulkAction
    data object Start : BulkAction
    data object ForceStart : BulkAction
    data class Remove(val deleteFiles: Boolean = false) : BulkAction

    /** Asks trackers and DHT for peers now; only for downloading or seeding torrents. */
    data object Reannounce : BulkAction

    /** Re-verifies all downloaded data. */
    data object Recheck : BulkAction

    /** Turtle mode (alternative speed) on or off; the ids are ignored. */
    data class Turtle(val on: Boolean) : BulkAction
}

object BulkActionSerializer : KSerializer<BulkAction> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("BulkAction")

    override fun serialize(encoder: Encoder, value: BulkAction) {
        val element: JsonElement = when (value) {
            BulkAction.Pause -> JsonPrimitive("pause")
            BulkAction.Resume -> JsonPrimitive("resume")
            BulkAction.Stop -> JsonPrimitive("stop")
            BulkAction.Start -> JsonPrimitive("start")
            BulkAction.ForceStart -> JsonPrimitive("force_start")
            BulkAction.Reannounce -> JsonPrimitive("reannounce")
            BulkAction.Recheck -> JsonPrimitive("recheck")
            is BulkAction.Remove -> buildJsonObject {
                put("remove", buildJsonObject { put("delete_files", value.deleteFiles) })
            }
            is BulkAction.Turtle -> buildJsonObject { put("turtle", value.on) }
        }
        (encoder as JsonEncoder).encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): BulkAction {
        val element = (decoder as JsonDecoder).decodeJsonElement()
        if (element is JsonObject) {
            val (key, value) = element.entries.singleOrNull()
                ?: throw IllegalArgumentException("a bulk action has exactly one key")
            return when (key) {
                "remove" -> BulkAction.Remove(value.jsonObject["delete_files"]?.jsonPrimitive?.content == "true")
                "turtle" -> BulkAction.Turtle(value.jsonPrimitive.content == "true")
                else -> throw IllegalArgumentException("unknown bulk action `$key`")
            }
        }
        return when (val name = element.jsonPrimitive.content) {
            "pause" -> BulkAction.Pause
            "resume" -> BulkAction.Resume
            "stop" -> BulkAction.Stop
            "start" -> BulkAction.Start
            "force_start" -> BulkAction.ForceStart
            "reannounce" -> BulkAction.Reannounce
            "recheck" -> BulkAction.Recheck
            else -> throw IllegalArgumentException("unknown bulk action `$name`")
        }
    }
}

@Serializable
data class BulkRequest(
    /** Torrents to act on; every torrent when null. */
    val ids: List<String>? = null,
    val action: BulkAction,
)

@Serializable
data class BulkFailure(val id: String, val message: String)

@Serializable
data class BulkResult(
    val ok: List<String> = emptyList(),
    val failed: List<BulkFailure> = emptyList(),
)

/** One connected peer. */
@Serializable
data class PeerInfo(
    val addr: String,
    val client: String? = null,
    /** `tcp`, `utp` or `socks`. */
    @SerialName("conn_kind") val connKind: String? = null,
    val state: String = "",
    val downloaded: Long = 0,
    val uploaded: Long = 0,
)

/** What the weekly schedule says now. */
@Serializable
enum class ScheduleMode {
    @SerialName("normal") Normal,
    @SerialName("alt") Alt,
    @SerialName("seed_only") SeedOnly,
    @SerialName("off") Off,
}

/** Torrents per state, for the filter tabs. */
@Serializable
data class StateCounts(
    val all: Int = 0,
    val downloading: Int = 0,
    val seeding: Int = 0,
    val queued: Int = 0,
    val paused: Int = 0,
    val stopped: Int = 0,
    val finished: Int = 0,
    val error: Int = 0,
)

/** Session-wide numbers. */
@Serializable
data class SessionStats(
    /** The client is on and its session runs. */
    val running: Boolean = false,
    @SerialName("down_bps") val downBps: Long = 0,
    @SerialName("up_bps") val upBps: Long = 0,
    @SerialName("listen_port") val listenPort: Int? = null,
    @SerialName("dht_nodes") val dhtNodes: Long? = null,
    @SerialName("schedule_mode") val scheduleMode: ScheduleMode = ScheduleMode.Normal,
    val turtle: Boolean = false,
    @SerialName("on_battery") val onBattery: Boolean = false,
    @SerialName("network_allowed") val networkAllowed: Boolean = true,
    @SerialName("peers_connected") val peersConnected: Int = 0,
    val counts: StateCounts = StateCounts(),
)
