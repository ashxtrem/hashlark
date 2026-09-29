// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import io.github.ashxtrem.hashlark.core.Category
import io.github.ashxtrem.hashlark.core.ErrorKind
import io.github.ashxtrem.hashlark.core.ProviderProgress
import io.github.ashxtrem.hashlark.core.SortDirection
import io.github.ashxtrem.hashlark.core.SortOrder
import io.github.ashxtrem.hashlark.ui.search.FilterDraft
import io.github.ashxtrem.hashlark.ui.search.ProviderSummary
import io.github.ashxtrem.hashlark.ui.search.agoText
import io.github.ashxtrem.hashlark.ui.search.seedsText
import io.github.ashxtrem.hashlark.ui.search.sourcesText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The wording and bookkeeping behind the result rows, the provider status and the filter sheet. */
class SearchUiLogicTest {
    @Test
    fun `unknown seeders are not shown as zero`() {
        assertEquals("Seeds unknown", seedsText(null))
        assertEquals("Seeds 0", seedsText(0))
        assertNotEquals(seedsText(null), seedsText(0))
    }

    @Test
    fun `sources are counted in the singular and plural`() {
        assertEquals("1 source", sourcesText(1))
        assertEquals("3 sources", sourcesText(3))
    }

    @Test
    fun `a missing date gives no age`() {
        assertNull(agoText(null))
        assertNull(agoText("not a date"))
    }

    private val ok = ProviderProgress.Finished(3, 100)
    private val failed = ProviderProgress.Failed(ErrorKind.Network, "boom", 50)
    private val stopped = ProviderProgress.Failed(ErrorKind.Timeout, ProviderProgress.Failed.CANCELLED, 0)

    @Test
    fun `providers that only ended are not called successful`() {
        val allOk = ProviderSummary.of(mapOf("a" to ok, "b" to ok, "c" to ok))
        assertEquals("All 3 providers answered", allOk.label)

        // A failed request ended too, but it did not answer.
        val partial = ProviderSummary.of(mapOf("a" to ok, "b" to failed, "c" to ok, "d" to failed))
        assertEquals("2 of 4 answered · 2 failed", partial.label)
        assertEquals(2, partial.realFailures)

        val single = ProviderSummary.of(mapOf("a" to failed))
        assertEquals("0 of 1 answered · 1 failed", single.label)
    }

    @Test
    fun `progress while searching counts only providers that answered`() {
        val running = ProviderSummary.of(mapOf("a" to ok, "b" to ProviderProgress.Running, "c" to failed))
        assertEquals("Searching · 1 of 3 answered", running.label)
        assertEquals(true, running.searching)
    }

    @Test
    fun `a stopped search says stopped, not failed`() {
        val summary = ProviderSummary.of(mapOf("a" to ok, "b" to stopped))
        assertEquals("Stopped · 1 of 2 answered", summary.label)
        assertEquals(0, summary.realFailures)
    }

    @Test
    fun `a filter draft counts categories and a provider choice`() {
        assertEquals(0, FilterDraft.Default.activeCount)
        assertEquals(2, FilterDraft(categories = setOf(Category.Movies, Category.Tv)).activeCount)
        assertEquals(3, FilterDraft(categories = setOf(Category.Movies, Category.Tv), providerIds = setOf("a")).activeCount)
        // Sorting is a view setting, not a filter.
        assertEquals(0, FilterDraft(order = SortOrder.Size).activeCount)
    }

    @Test
    fun `picking a sort chip starts descending and picking it again reverses it`() {
        var draft = FilterDraft.Default
        assertEquals(SortOrder.Date to SortDirection.Descending, draft.order to draft.direction)
        draft = draft.pickSort(SortOrder.Seeders)
        assertEquals(SortOrder.Seeders to SortDirection.Descending, draft.order to draft.direction)
        draft = draft.pickSort(SortOrder.Seeders)
        assertEquals(SortDirection.Ascending, draft.direction)
        draft = draft.pickSort(SortOrder.Size)
        assertEquals(SortOrder.Size to SortDirection.Descending, draft.order to draft.direction)
    }
}
