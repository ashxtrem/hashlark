// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.ui.HeightClass
import io.github.ashxtrem.hashlark.ui.Posture
import io.github.ashxtrem.hashlark.ui.WidthClass
import io.github.ashxtrem.hashlark.ui.WindowShape
import io.github.ashxtrem.hashlark.ui.providers.ReposViewModel
import io.github.ashxtrem.hashlark.core.SyncReport
import io.github.ashxtrem.hashlark.ui.search.RowDensity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The window shapes of a Galaxy Z Fold7 (and what a layout decides for each). */
class WindowShapeTest {
    private fun shape(w: Int, h: Int, posture: Posture = Posture.Flat) = WindowShape(w.dp, h.dp, posture)

    @Test
    fun `cover screen portrait is compact and tall`() {
        val s = shape(411, 960)
        assertEquals(WidthClass.Compact, s.width)
        assertEquals(HeightClass.Expanded, s.height)
        assertFalse(s.hasTwoPanes)
        assertFalse(s.isShort)
    }

    @Test
    fun `cover screen landscape is wide but short`() {
        val s = shape(960, 411)
        assertEquals(WidthClass.Expanded, s.width)
        assertTrue(s.isShort)
        assertTrue(s.hasTwoPanes)
    }

    @Test
    fun `inner screen gets two panes, and three only on large windows`() {
        val portrait = shape(750, 832)
        assertEquals(WidthClass.Medium, portrait.width)
        assertTrue(portrait.hasTwoPanes)
        assertFalse(portrait.hasThreePanes)

        val wide = shape(1000, 800)
        assertEquals(WidthClass.Expanded, wide.width)
        assertTrue(wide.hasTwoPanes)
        assertFalse(wide.hasThreePanes)
        assertEquals(WidthClass.Large, shape(1400, 900).width)
        assertTrue(shape(1400, 900).hasThreePanes)
    }

    @Test
    fun `postures change the layout, not the size class`() {
        val tabletop = shape(750, 832, Posture.Tabletop)
        assertTrue(tabletop.isTabletop)
        assertFalse(tabletop.hasThreePanes)

        val book = shape(832, 750, Posture.Book)
        assertTrue(book.isBook)
        assertTrue(book.hasTwoPanes)
        assertFalse(book.hasThreePanes)
    }

    @Test
    fun `split screen and pop-up sizes fall back to the narrow layout`() {
        assertEquals(WidthClass.Compact, shape(375, 832).width)
        assertEquals(WidthClass.Compact, shape(250, 832).width)
        assertEquals(HeightClass.Compact, shape(750, 300).height)
    }

    @Test
    fun `rows pick their density from the width they get`() {
        assertEquals(RowDensity.Narrow, RowDensity.forWidth(360f))
        assertEquals(RowDensity.Medium, RowDensity.forWidth(600f))
        assertEquals(RowDensity.Wide, RowDensity.forWidth(900f))
    }

    @Test
    fun `sync summaries say what changed`() {
        assertEquals("no changes", ReposViewModel.summary(SyncReport()))
        assertEquals(
            "2 new (disabled until you enable them), 1 updated, 1 skipped",
            ReposViewModel.summary(SyncReport(added = listOf("a", "b"), updated = listOf("c"), errors = listOf("x"))),
        )
    }
}
