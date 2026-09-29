// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import java.text.Collator

enum class SortDirection { Ascending, Descending }

/**
 * Sorts merged results the way the core does (`ranking::sort`): the chosen
 * key first, unknown values last in either direction, then relevance.
 */
object ResultSorter {
    private val collator: Collator = Collator.getInstance().apply { strength = Collator.PRIMARY }

    fun sort(
        results: Collection<MergedResult>,
        order: SortOrder,
        direction: SortDirection = SortDirection.Descending,
    ): List<MergedResult> = results.sortedWith { a, b ->
        val primary = when (order) {
            SortOrder.Relevance -> 0
            SortOrder.Title -> {
                val cmp = collator.compare(a.primary.title, b.primary.title)
                if (direction == SortDirection.Descending) -cmp else cmp
            }
            SortOrder.Seeders -> nullable(a.seeders?.toLong(), b.seeders?.toLong(), direction)
            SortOrder.Peers -> nullable(a.leechers?.toLong(), b.leechers?.toLong(), direction)
            SortOrder.Size -> nullable(a.primary.sizeBytes, b.primary.sizeBytes, direction)
            SortOrder.Date -> nullable(
                Format.parseInstant(a.primary.published)?.toEpochMilli(),
                Format.parseInstant(b.primary.published)?.toEpochMilli(),
                direction,
            )
        }
        if (primary != 0) return@sortedWith primary
        val byScore = b.score.compareTo(a.score)
        if (byScore != 0) byScore else nullable(a.seeders?.toLong(), b.seeders?.toLong(), SortDirection.Descending)
    }

    private fun nullable(a: Long?, b: Long?, direction: SortDirection): Int = when {
        a == null && b == null -> 0
        a == null -> 1
        b == null -> -1
        direction == SortDirection.Descending -> b.compareTo(a)
        else -> a.compareTo(b)
    }
}
