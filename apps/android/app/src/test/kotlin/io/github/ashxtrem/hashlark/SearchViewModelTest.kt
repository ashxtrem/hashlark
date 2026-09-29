// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import io.github.ashxtrem.hashlark.core.Category
import io.github.ashxtrem.hashlark.core.ErrorKind
import io.github.ashxtrem.hashlark.core.HashlarkFailure
import io.github.ashxtrem.hashlark.core.HashlarkRuntime
import io.github.ashxtrem.hashlark.core.ProviderProgress
import io.github.ashxtrem.hashlark.core.SearchEvent
import io.github.ashxtrem.hashlark.core.SearchQuery
import io.github.ashxtrem.hashlark.core.SortDirection
import io.github.ashxtrem.hashlark.core.SortOrder
import io.github.ashxtrem.hashlark.ui.search.SearchViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SearchViewModelTest {
    private val fake = FakeApi()
    private fun viewModel() = SearchViewModel(HashlarkRuntime { fake })

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun page(vararg ids: String) = listOf(
        SearchEvent.ProviderStarted("alpha"),
        SearchEvent.Results("alpha", ids.map { mergedResult(it) }),
        SearchEvent.ProviderFinished("alpha", ids.size, 50),
        SearchEvent.Done(ids.size, 60),
    )

    @Test
    fun `a search streams its events into the state`() = runTest {
        fake.script = { page("a", "b") }
        val vm = viewModel()
        vm.setText("ubuntu")
        vm.toggleCategory(Category.Software)
        vm.run()

        val state = vm.state.value
        assertEquals(setOf("a", "b"), state.progress.results.keys)
        assertTrue(state.progress.done)
        assertFalse(state.running)
        assertEquals("ubuntu", fake.queries.single().text)
        assertEquals(listOf(Category.Software), fake.queries.single().categories)
        assertEquals(SortOrder.Date, fake.queries.single().sort)
    }

    @Test
    fun `an empty query does not search`() = runTest {
        val vm = viewModel()
        vm.setText("   ")
        vm.run()
        assertTrue(fake.queries.isEmpty())
        assertTrue(vm.state.value.idle)
    }

    @Test
    fun `an imdb search runs from a shared link`() = runTest {
        fake.script = { page("a") }
        val vm = viewModel()
        vm.run(SearchQuery(text = "", imdbId = "tt0063350"))
        assertEquals("tt0063350", fake.queries.single().imdbId)
        assertTrue(vm.state.value.canSearch)
    }

    @Test
    fun `stopping a running search cancels it and marks the providers still running`() = runTest {
        fake.stayOpen = true
        fake.script = { listOf(SearchEvent.ProviderStarted("alpha"), SearchEvent.ProviderStarted("beta"), SearchEvent.ProviderFinished("alpha", 0, 5)) }
        val vm = viewModel()
        vm.setText("x")
        vm.run()
        assertTrue(vm.state.value.running)

        vm.cancel()
        val state = vm.state.value
        assertFalse(state.running)
        assertTrue(state.progress.done)
        assertEquals(1, fake.cancelledSearches)
        assertEquals("Cancelled", (state.progress.providers.getValue("beta") as ProviderProgress.Failed).message)
    }

    @Test
    fun `a new search replaces the old results and clears the selection`() = runTest {
        fake.script = { q -> if (q.text == "one") page("a") else page("b") }
        val vm = viewModel()
        vm.setText("one")
        vm.run()
        vm.select("a")
        vm.setText("two")
        vm.run()
        assertEquals(setOf("b"), vm.state.value.progress.results.keys)
        assertNull(vm.state.value.selectedId)
    }

    @Test
    fun `more results are added to the ones shown, keeping the selection`() = runTest {
        fake.script = { q -> if (q.page == 1) page("a") else page("b") }
        val vm = viewModel()
        vm.setText("x")
        vm.run()
        vm.select("a")
        vm.loadMore()

        assertEquals(listOf(1, 2), fake.queries.map { it.page })
        assertEquals(setOf("a", "b"), vm.state.value.progress.results.keys)
        assertEquals("a", vm.state.value.selectedId)
        assertEquals(2, vm.state.value.page)
    }

    @Test
    fun `a failing search shows an error instead of crashing`() = runTest {
        fake.failWith = HashlarkFailure("internal", "the database is locked")
        val vm = viewModel()
        vm.setText("x")
        vm.run()
        assertEquals("the database is locked", vm.state.value.error)
        assertFalse(vm.state.value.running)
    }

    @Test
    fun `sorting toggles direction on the same column and resets on a new one`() {
        val vm = viewModel()
        assertEquals(SortOrder.Date, vm.state.value.order)
        vm.toggleSort(SortOrder.Seeders)
        assertEquals(SortOrder.Seeders to SortDirection.Descending, vm.state.value.order to vm.state.value.direction)
        vm.toggleSort(SortOrder.Seeders)
        assertEquals(SortDirection.Ascending, vm.state.value.direction)
        vm.toggleSort(SortOrder.Size)
        assertEquals(SortOrder.Size to SortDirection.Descending, vm.state.value.order to vm.state.value.direction)
        vm.pickSort(SortOrder.Title)
        assertEquals(SortDirection.Descending, vm.state.value.direction)
    }

    @Test
    fun `the sorted list follows the chosen order`() = runTest {
        fake.script = {
            listOf(
                SearchEvent.Results("alpha", listOf(mergedResult("low", seeders = 1), mergedResult("high", seeders = 99), mergedResult("none", seeders = null))),
                SearchEvent.Done(3, 1),
            )
        }
        val vm = viewModel()
        vm.setText("x")
        vm.run()
        vm.pickSort(SortOrder.Seeders)
        assertEquals(listOf("high", "low", "none"), vm.state.value.sorted.map { it.id })
        vm.toggleSort(SortOrder.Seeders)
        assertEquals(listOf("low", "high", "none"), vm.state.value.sorted.map { it.id })
    }

    @Test
    fun `provider filter and categories reach the query`() = runTest {
        fake.script = { page("a") }
        val vm = viewModel()
        vm.setText("x")
        vm.setProviders(setOf("beta"))
        vm.toggleCategory(Category.Movies)
        vm.toggleCategory(Category.Tv)
        vm.toggleCategory(Category.Movies)
        vm.run()
        val query = fake.queries.single()
        assertEquals(listOf("beta"), query.providers)
        assertEquals(listOf(Category.Tv), query.categories)
        assertNotNull(vm.state.value.shown)
        assertEquals(ErrorKind.Timeout, ErrorKind.Timeout)
    }

    @Test
    fun `closing the details keeps the selection and does not reopen them`() = runTest {
        fake.script = { page("a", "b") }
        val vm = viewModel()
        vm.setText("x")
        vm.run()
        vm.select("b")
        assertTrue(vm.state.value.detailOpen)

        vm.closeDetail()
        // The row stays highlighted so the list can mark where the user was, but no detail is showing.
        assertEquals("b", vm.state.value.selectedId)
        assertFalse(vm.state.value.detailOpen)

        // Streaming more results, or a re-sort, must not open it either.
        vm.pickSort(SortOrder.Title)
        vm.toggleSort(SortOrder.Title)
        assertFalse(vm.state.value.detailOpen)
    }

    @Test
    fun `an open detail stays open while more results are added`() = runTest {
        fake.script = { q -> if (q.page == 1) page("a") else page("b") }
        val vm = viewModel()
        vm.setText("x")
        vm.run()
        vm.select("a")
        vm.loadMore()
        assertTrue(vm.state.value.detailOpen)
        assertEquals("a", vm.state.value.selectedId)
    }

    @Test
    fun `submitting another query keeps the filters and the sort`() = runTest {
        fake.script = { page("a") }
        val vm = viewModel()
        vm.setText("one")
        vm.toggleCategory(Category.Movies)
        vm.setProviders(setOf("beta"))
        vm.pickSort(SortOrder.Seeders)
        vm.run()
        vm.setText("two")
        vm.run()

        assertEquals(listOf("one", "two"), fake.queries.map { it.text })
        assertEquals(listOf(Category.Movies), fake.queries.last().categories)
        assertEquals(listOf("beta"), fake.queries.last().providers)
        assertEquals(SortOrder.Seeders, fake.queries.last().sort)
    }

    @Test
    fun `applying filters changes them together and does not search`() {
        val vm = viewModel()
        vm.applyFilters(setOf(Category.Music), setOf("alpha"), SortOrder.Size, SortDirection.Ascending)
        val state = vm.state.value
        assertEquals(setOf(Category.Music), state.categories)
        assertEquals(setOf("alpha"), state.providerIds)
        assertEquals(SortOrder.Size to SortDirection.Ascending, state.order to state.direction)
        assertTrue(fake.queries.isEmpty())
    }

    @Test
    fun `resetting filters clears categories and providers but keeps the sort`() {
        val vm = viewModel()
        vm.applyFilters(setOf(Category.Music), setOf("alpha"), SortOrder.Size, SortDirection.Ascending)
        vm.resetFilters()
        val state = vm.state.value
        assertTrue(state.categories.isEmpty())
        assertNull(state.providerIds)
        assertEquals(SortOrder.Size, state.order)
    }

    @Test
    fun `stopping a search is remembered until the next one`() = runTest {
        fake.stayOpen = true
        fake.script = { listOf(SearchEvent.ProviderStarted("alpha")) }
        val vm = viewModel()
        vm.setText("x")
        vm.run()
        vm.cancel()
        assertTrue(vm.state.value.cancelled)
        assertTrue((vm.state.value.progress.providers.getValue("alpha") as ProviderProgress.Failed).wasCancelled)

        fake.stayOpen = false
        fake.script = { page("a") }
        vm.run()
        assertFalse(vm.state.value.cancelled)
    }
}
