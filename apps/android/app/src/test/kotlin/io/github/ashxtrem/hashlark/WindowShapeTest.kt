// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.SyncReport
import io.github.ashxtrem.hashlark.ui.HeightClass
import io.github.ashxtrem.hashlark.ui.NavKind
import io.github.ashxtrem.hashlark.ui.PanePlan
import io.github.ashxtrem.hashlark.ui.Posture
import io.github.ashxtrem.hashlark.ui.WidthClass
import io.github.ashxtrem.hashlark.ui.WindowShape
import io.github.ashxtrem.hashlark.ui.providers.ReposViewModel
import io.github.ashxtrem.hashlark.ui.search.RowDensity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which layout each window gets. The Galaxy Z Fold7 shapes are the ones computed from its
 * specifications at 420 dpi (inner 8.0" 1968 x 2184 px is about 750 x 832 dp, cover 6.5" 1080 x 2520 px is about
 * 411 x 960 dp before system bars); none of this reads the device, only the window in dp.
 */
class WindowShapeTest {
    private fun shape(
        w: Int,
        h: Int,
        posture: Posture = Posture.Flat,
        fontScale: Float = 1f,
        inset: Int = 0,
        hingeStart: Float = 0.5f,
        hingeEnd: Float = 0.5f,
    ) = WindowShape(
        w.dp, h.dp, posture,
        hingeStartFraction = hingeStart, hingeEndFraction = hingeEnd,
        fontScale = fontScale, startInset = inset.dp, endInset = inset.dp,
    )

    private fun twoPanes(s: WindowShape) = s.planPanes() is PanePlan.Split

    @Test
    fun `cover screen portrait is compact and tall, with a bottom bar and one pane`() {
        val s = shape(411, 960)
        assertEquals(WidthClass.Compact, s.width)
        assertEquals(HeightClass.Expanded, s.height)
        assertEquals(NavKind.Bar, s.navigation)
        assertFalse(twoPanes(s))
        assertFalse(s.isShort)
    }

    @Test
    fun `narrow phones use a bottom bar and a single pane`() {
        for (w in listOf(320, 360, 411, 599)) {
            val s = shape(w, 800)
            assertEquals("width $w", NavKind.Bar, s.navigation)
            assertFalse("width $w", twoPanes(s))
        }
    }

    @Test
    fun `cover screen landscape is short, so one pane whatever its width`() {
        val s = shape(960, 411)
        assertTrue(s.isShort)
        assertEquals(NavKind.Rail, s.navigation)
        assertFalse(twoPanes(s))
    }

    @Test
    fun `the Fold7 inner display in portrait and landscape shows details full-screen`() {
        val portrait = shape(750, 832)
        assertEquals(WidthClass.Medium, portrait.width)
        assertEquals(NavKind.Rail, portrait.navigation)
        assertFalse(twoPanes(portrait))

        val landscape = shape(832, 750)
        assertEquals(WidthClass.Medium, landscape.width)
        assertFalse(twoPanes(landscape))
        // With the status and gesture bars the usable window is even smaller.
        assertFalse(twoPanes(shape(750, 780)))
        assertFalse(twoPanes(shape(832, 698)))
    }

    @Test
    fun `the 600 dp rule is gone - nothing between 600 and 839 splits`() {
        for (w in listOf(600, 640, 700, 768, 839)) assertFalse("width $w", twoPanes(shape(w, 900)))
    }

    @Test
    fun `two panes start at 840 dp only when navigation, insets and margins leave room`() {
        assertFalse(twoPanes(shape(839, 800)))
        val at840 = shape(840, 800)
        assertTrue(twoPanes(at840))
        val plan = at840.planPanes() as PanePlan.Split
        // Both panes get at least their minimum: 360 dp of content plus margins for the list, 310 for details.
        assertTrue(plan.listWidth >= at840.minListPane)
        assertTrue(at840.contentWidth - plan.listWidth - 1.dp >= at840.minDetailPane)
        assertEquals(0.dp, plan.hingeGap)

        // The same width with a wide cutout or side bar on both edges no longer fits.
        assertFalse(twoPanes(shape(840, 800, inset = 32)))
    }

    @Test
    fun `wide windows split and get the expanded navigation from 1200 dp`() {
        val w1024 = shape(1024, 768)
        assertEquals(NavKind.Rail, w1024.navigation)
        assertTrue(twoPanes(w1024))
        val w1199 = shape(1199, 800)
        assertEquals(NavKind.Rail, w1199.navigation)
        val w1280 = shape(1280, 800)
        assertEquals(NavKind.Drawer, w1280.navigation)
        assertEquals(WindowShape.DRAWER_WIDTH, w1280.navigationWidth)
        assertTrue(twoPanes(w1280))
        val plan = w1280.planPanes() as PanePlan.Split
        assertTrue(plan.listWidth <= 560.dp)
    }

    @Test
    fun `short windows keep to one pane`() {
        assertFalse(twoPanes(shape(1024, 400)))
        assertFalse(twoPanes(shape(1280, 479)))
        assertTrue(twoPanes(shape(1280, 480)))
    }

    @Test
    fun `larger text needs more room instead of smaller text`() {
        // 1024 dp: fits at normal size and at 1.3x, but not once insets eat the little that is left.
        assertTrue(twoPanes(shape(1024, 800, fontScale = 1.0f)))
        assertTrue(twoPanes(shape(1024, 800, fontScale = 1.3f)))
        assertFalse(twoPanes(shape(1024, 800, fontScale = 1.3f, inset = 16)))
        // At 2.0x even 1280 dp cannot hold two comfortable panes.
        assertFalse(twoPanes(shape(1280, 900, fontScale = 2.0f)))
        // A larger scale never asks for less room.
        assertTrue(shape(1000, 800, fontScale = 2f).minListPane > shape(1000, 800, fontScale = 1f).minListPane)
        // Below 1x is treated as 1x: small text does not squeeze the panes.
        assertEquals(shape(1000, 800, fontScale = 1f).minListPane, shape(1000, 800, fontScale = 0.85f).minListPane)
    }

    @Test
    fun `book posture splits at the hinge only when both halves fit`() {
        // A flexible display's fold has no gap: the halves meet at the hinge.
        val wide = shape(1100, 800, Posture.Book)
        val plan = wide.planPanes()
        assertTrue(plan is PanePlan.Split)
        assertEquals(0.dp, (plan as PanePlan.Split).hingeGap)
        // Each Fold7 half is about 416 dp before the rail: not enough for a 360 dp list plus margins.
        assertFalse(twoPanes(shape(832, 750, Posture.Book)))
    }

    @Test
    fun `a hinge with a gap never has content under it`() {
        // A gap of 140 dp in the middle of a window too narrow for two panes: one side is used, not both.
        val s = shape(700, 800, Posture.Book, hingeStart = 0.4f, hingeEnd = 0.6f)
        val plan = s.planPanes() as PanePlan.Single
        assertTrue(plan.startInset > 0.dp || plan.endInset > 0.dp)
        // Two panes fit around a gap when both sides are big enough, and the gap is kept.
        val roomy = shape(1400, 900, Posture.Book, hingeStart = 0.47f, hingeEnd = 0.53f)
        val split = roomy.planPanes() as PanePlan.Split
        assertTrue(split.hingeGap > 0.dp)
    }

    @Test
    fun `tabletop posture has no navigation and no side by side panes`() {
        val tabletop = shape(750, 832, Posture.Tabletop)
        assertTrue(tabletop.isTabletop)
        assertEquals(NavKind.None, tabletop.navigation)
        assertFalse(twoPanes(tabletop))
    }

    @Test
    fun `split screen and pop-up sizes fall back to the narrow layout`() {
        assertEquals(WidthClass.Compact, shape(375, 832).width)
        assertEquals(WidthClass.Compact, shape(250, 832).width)
        assertEquals(HeightClass.Compact, shape(750, 300).height)
        assertFalse(twoPanes(shape(375, 832)))
        // Half of a wide tablet window is a phone-sized window.
        assertFalse(twoPanes(shape(640, 800)))
    }

    @Test
    fun `physical resolution never enters the decision`() {
        // 1968 x 2184 px at 420 dpi and at a display zoom of 480 dpi are different windows in dp.
        val defaultZoom = shape((1968 * 160 / 420f).toInt(), (2184 * 160 / 420f).toInt())
        val zoomedIn = shape((1968 * 160 / 480f).toInt(), (2184 * 160 / 480f).toInt())
        assertEquals(749, defaultZoom.widthDp.value.toInt())
        assertEquals(656, zoomedIn.widthDp.value.toInt())
        assertFalse(twoPanes(defaultZoom))
        assertFalse(twoPanes(zoomedIn))
        // Display zoom that makes the window larger in dp can bring two panes back.
        val smaller = shape((1968 * 160 / 360f).toInt(), (2184 * 160 / 360f).toInt())
        assertTrue(smaller.widthDp.value >= 840)
        assertTrue(twoPanes(smaller))
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
