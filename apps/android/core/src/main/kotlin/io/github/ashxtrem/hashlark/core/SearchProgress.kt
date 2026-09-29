// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

/** How one provider is doing in the running search. */
sealed interface ProviderProgress {
    data object Running : ProviderProgress
    data class Finished(val count: Int, val latencyMs: Long) : ProviderProgress
    data class Failed(val kind: ErrorKind, val message: String, val latencyMs: Long) : ProviderProgress {
        /** The user stopped the search before this provider answered; it did not fail. */
        val wasCancelled: Boolean get() = message == CANCELLED

        companion object {
            const val CANCELLED = "Cancelled"
        }
    }
}

/**
 * The state of one search, built up from its event stream. Immutable, so it
 * can be a `StateFlow` value; [reduce] returns the next state.
 */
data class SearchProgress(
    val results: Map<String, MergedResult> = emptyMap(),
    val providers: Map<String, ProviderProgress> = emptyMap(),
    val done: Boolean = false,
    val durationMs: Long? = null,
) {
    val failed: Map<String, ProviderProgress.Failed>
        get() = providers.mapNotNull { (id, p) -> (p as? ProviderProgress.Failed)?.let { id to it } }.toMap()

    /** Providers still answering. */
    val pending: Int get() = providers.count { it.value is ProviderProgress.Running }

    /** The state after the user stopped the search: providers still running count as cancelled. */
    fun cancelled(): SearchProgress = copy(
        done = true,
        providers = providers.mapValues { (_, p) ->
            if (p is ProviderProgress.Running) ProviderProgress.Failed(ErrorKind.Timeout, ProviderProgress.Failed.CANCELLED, 0) else p
        },
    )

    fun reduce(event: SearchEvent): SearchProgress = when (event) {
        is SearchEvent.ProviderStarted ->
            copy(providers = providers + (event.provider to ProviderProgress.Running))
        is SearchEvent.Results -> {
            // A result whose id was seen before replaces the earlier version.
            val merged = LinkedHashMap(results)
            event.items.forEach { merged[it.id] = it }
            copy(results = merged)
        }
        is SearchEvent.ProviderFinished ->
            copy(providers = providers + (event.provider to ProviderProgress.Finished(event.count, event.latencyMs)))
        is SearchEvent.ProviderFailed ->
            copy(
                providers = providers + (
                    event.provider to ProviderProgress.Failed(event.errorKind, event.message, event.latencyMs)
                    ),
            )
        is SearchEvent.Done -> copy(done = true, durationMs = event.durationMs)
    }
}
