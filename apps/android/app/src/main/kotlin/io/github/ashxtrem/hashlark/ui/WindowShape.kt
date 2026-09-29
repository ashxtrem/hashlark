// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfo
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class WidthClass { Compact, Medium, Expanded, Large }

enum class HeightClass { Compact, Medium, Expanded }

/** The physical arrangement of a foldable's window. */
enum class Posture {
    /** Flat, or a device that does not fold. */
    Flat,

    /** Half-open with a horizontal hinge: two halves stacked. */
    Tabletop,

    /** Half-open with a vertical hinge: two halves side by side. */
    Book,
}

/** How the five destinations are reached. */
enum class NavKind {
    /** No navigation of its own (tabletop posture: the halves of the screen are used for content). */
    None,

    /** Bottom bar: windows narrower than 600 dp. */
    Bar,

    /** Compact rail: 600 dp and wider. */
    Rail,

    /** Labelled sidebar: 1200 dp and wider. */
    Drawer,
}

/**
 * What a two-pane arrangement of the results workspace looks like, or that there is not enough
 * room for one. Decided by [WindowShape.planPanes] from the room that is really left.
 */
sealed interface PanePlan {
    /**
     * One pane. The insets are zero unless a separating hinge with a real gap forces the content onto
     * one side of it (nothing may sit under a hinge).
     */
    data class Single(val startInset: Dp = 0.dp, val endInset: Dp = 0.dp) : PanePlan

    /** The list [listWidth] wide, then [hingeGap] of hinge (0 on a flat display, where a divider is drawn), then the details. */
    data class Split(val listWidth: Dp, val hingeGap: Dp) : PanePlan
}

/**
 * Everything layouts decide on: the size of this window (not the device or
 * the screen: split screen and pop-up windows count), the space the system bars and cutouts take,
 * the text scale and the fold posture. Layouts never look at the device model, so what works for
 * the Galaxy Z Fold7 also works for other foldables, tablets and desktop windowing. Physical pixels and
 * physical density play no part: [widthDp] is the window in logical dp.
 */
@Immutable
data class WindowShape(
    val widthDp: Dp,
    val heightDp: Dp,
    val posture: Posture = Posture.Flat,
    /** Vertical position of the hinge in the window, as a fraction of its height (tabletop only). */
    val hingeFraction: Float = 0.5f,
    /** Where a vertical hinge starts and ends across the window's width, as fractions (book posture only). */
    val hingeStartFraction: Float = 0.5f,
    val hingeEndFraction: Float = 0.5f,
    /** The user's font scale; larger text needs wider panes, it never shrinks to fit. */
    val fontScale: Float = 1f,
    /** Horizontal room taken by system bars and display cutouts (landscape navigation bar, notch). */
    val startInset: Dp = 0.dp,
    val endInset: Dp = 0.dp,
) {
    val width: WidthClass = classifyWidth(widthDp)
    val height: HeightClass = classifyHeight(heightDp)

    val isTabletop: Boolean get() = posture == Posture.Tabletop
    val isBook: Boolean get() = posture == Posture.Book

    /** Little vertical room, e.g. the Fold7 cover screen in landscape. */
    val isShort: Boolean get() = height == HeightClass.Compact

    /** Which navigation the window gets. Decided before, and independent of, whether it is currently hidden. */
    val navigation: NavKind
        get() = when {
            isTabletop -> NavKind.None
            width == WidthClass.Large -> NavKind.Drawer
            isShort || width >= WidthClass.Medium -> NavKind.Rail
            else -> NavKind.Bar
        }

    val navigationWidth: Dp
        get() = when (navigation) {
            NavKind.Rail -> RAIL_WIDTH
            NavKind.Drawer -> DRAWER_WIDTH
            else -> 0.dp
        }

    /** The width left for content once navigation and the insets at the sides are taken away. */
    val contentWidth: Dp get() = (widthDp - navigationWidth - startInset - endInset).coerceAtLeast(0.dp)

    private val textScale: Float get() = fontScale.coerceIn(1f, 2f)

    /** Width a list pane needs (its content plus its own margins), at the current text scale. */
    val minListPane: Dp get() = LIST_MIN * textScale + PANE_MARGINS

    /** Width a details pane needs, at the current text scale. */
    val minDetailPane: Dp get() = DETAIL_MIN * textScale + PANE_MARGINS

    /**
     * Whether a list and its details fit side by side, and how. Never a question of the nominal width
     * alone: navigation, insets, margins, the divider and the hinge are subtracted first, and both panes must
     * still get their minimum (which grows with the text scale). The Fold7's inner display in portrait or
     * landscape (about 750 x 832 dp or 832 x 750 dp) therefore shows details full-screen.
     */
    fun planPanes(): PanePlan {
        if (isTabletop || isShort) return PanePlan.Single()
        val available = contentWidth
        if (isBook) {
            // Content coordinates: the origin is the left edge of the content area (after navigation and the inset).
            val origin = widthDp - available - endInset
            val hingeStart = widthDp * hingeStartFraction - origin
            val hingeEnd = widthDp * hingeEndFraction - origin
            val listWidth = hingeStart
            val detailWidth = available - hingeEnd
            val gap = (hingeEnd - hingeStart).coerceAtLeast(0.dp)
            if (listWidth >= minListPane && detailWidth >= minDetailPane) {
                return PanePlan.Split(listWidth, hingeGap = gap)
            }
            // One pane: keep off a hinge that has a gap, on the wider side of it.
            return when {
                gap <= 0.dp -> PanePlan.Single()
                listWidth >= detailWidth -> PanePlan.Single(endInset = (available - hingeStart).coerceAtLeast(0.dp))
                else -> PanePlan.Single(startInset = hingeEnd.coerceAtLeast(0.dp))
            }
        }
        if (width < WidthClass.Expanded) return PanePlan.Single()
        val divider = 1.dp
        if (available < minListPane + minDetailPane + divider) return PanePlan.Single()
        // The list takes the larger share while the details keep their minimum; a table never needs more than 560 dp.
        val listWidth = (available * 0.45f).coerceIn(minListPane, maxOf(minListPane, minOf(560.dp, available - minDetailPane - divider)))
        return PanePlan.Split(listWidth, hingeGap = 0.dp)
    }

    /** Both a list and details fit next to each other. */
    val hasTwoPanes: Boolean get() = planPanes() is PanePlan.Split

    companion object {
        val RAIL_WIDTH = 80.dp
        val DRAWER_WIDTH = 220.dp
        val LIST_MIN = 360.dp
        val DETAIL_MIN = 310.dp

        /** A pane's own side margins (16 dp each). */
        val PANE_MARGINS = 32.dp

        fun classifyWidth(width: Dp): WidthClass = when {
            width < 600.dp -> WidthClass.Compact
            width < 840.dp -> WidthClass.Medium
            width < 1200.dp -> WidthClass.Expanded
            else -> WidthClass.Large
        }

        fun classifyHeight(height: Dp): HeightClass = when {
            height < 480.dp -> HeightClass.Compact
            height < 900.dp -> HeightClass.Medium
            else -> HeightClass.Expanded
        }
    }
}

/** Two halves of at least 300 dp each, or the window is not split at the hinge. */
private const val MIN_SPLIT_DP = 600

/** The shape of the current window; recomposes when it is resized, folded or rotated. */
@Composable
fun rememberWindowShape(): WindowShape {
    val size = LocalWindowInfo.current.containerSize
    val density = LocalDensity.current
    val layoutDirection = LocalLayoutDirection.current
    val insets = WindowInsets.safeDrawing
    val startInset = with(density) { insets.getLeft(this, layoutDirection).toDp() }
    val endInset = with(density) { insets.getRight(this, layoutDirection).toDp() }
    val posture = currentWindowAdaptiveInfo().windowPosture
    val hinge = posture.hingeList.firstOrNull { it.isSeparating || !it.isFlat }
    val widthDp = with(density) { size.width.toDp() }
    val heightDp = with(density) { size.height.toDp() }
    // A fold only splits the window into two panes when each half is big enough to use.
    // (A window that is much smaller than the unfolded screen, like the cover screen,
    // must never be split, even if the system still reports the hinge.)
    val tabletop = posture.isTabletop && heightDp >= MIN_SPLIT_DP.dp
    val book = !posture.isTabletop && hinge != null && hinge.isVertical && !hinge.isFlat && widthDp >= MIN_SPLIT_DP.dp
    val hingeTop = hinge?.bounds?.top
    val hingeLeft = hinge?.bounds?.left
    val hingeRight = hinge?.bounds?.right
    return remember(size, density.density, density.fontScale, startInset, endInset, tabletop, book, hingeTop, hingeLeft, hingeRight) {
        WindowShape(
            widthDp = widthDp,
            heightDp = heightDp,
            fontScale = density.fontScale,
            startInset = startInset,
            endInset = endInset,
            posture = when {
                tabletop -> Posture.Tabletop
                book -> Posture.Book
                else -> Posture.Flat
            },
            hingeFraction = if (tabletop && hingeTop != null && size.height > 0) {
                (hingeTop / size.height).coerceIn(0.2f, 0.8f)
            } else {
                0.5f
            },
            // Not clamped: the hinge is wherever the hardware puts it, and panes are sized from it.
            hingeStartFraction = if (book && hingeLeft != null && size.width > 0) (hingeLeft / size.width).coerceIn(0f, 1f) else 0.5f,
            hingeEndFraction = if (book && hingeRight != null && size.width > 0) (hingeRight / size.width).coerceIn(0f, 1f) else 0.5f,
        )
    }
}
