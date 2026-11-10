// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.settings

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.ui.WidthClass
import io.github.ashxtrem.hashlark.ui.rememberWindowShape
import java.time.LocalDateTime

/** Below this width the whole week doesn't fit legibly, so one day is edited at a time. */
private val GRID_MIN_WIDTH = 480.dp
private val DAY_LABEL = 44.dp
private val CELL_HEIGHT = 28.dp

private fun hh(hour: Int) = hour.toString().padStart(2, '0')

/**
 * The schedule as painted so far. Each paint builds on the previous paint, not on the last
 * recomposition, so two drag events in one frame both land. A [schedule] that comes back from
 * the parent is one of ours (kept as is) or a change from elsewhere (taken over).
 */
internal class WorkingSchedule(schedule: String?) {
    var value: String? = schedule
        private set

    /** What [change] sent and the parent hasn't passed back yet, oldest first. */
    private val sent = ArrayDeque<String?>()

    /** The parent passed [schedule]: an echo of a paint, or a change made elsewhere. */
    fun sync(schedule: String?) {
        val i = sent.indexOf(schedule)
        if (i >= 0) {
            repeat(i + 1) { sent.removeFirst() }
        } else if (schedule != value) {
            value = schedule
            sent.clear()
        }
    }

    fun change(next: String, onChange: (String) -> Unit) {
        value = next
        sent.addLast(next)
        // A parent that never passes the schedule back must not make this grow without end.
        if (sent.size > MAX_SENT) sent.removeFirst()
        onChange(next)
    }

    /** Sets one hour to [brush], unless it already is. */
    fun paint(day: Int, hour: Int, brush: ScheduleCell, onChange: (String) -> Unit) {
        if (ScheduleGrid.dayStrip(value, day)[hour] != brush) change(ScheduleGrid.paint(value, day, hour, brush), onChange)
    }

    private companion object {
        const val MAX_SENT = 64
    }
}

@Composable
private fun rememberWorkingSchedule(schedule: String?): WorkingSchedule {
    val working = remember { WorkingSchedule(schedule) }
    SideEffect { working.sync(schedule) }
    return working
}

/**
 * The weekly schedule. On a compact window (or a narrow pane): a day picker and that day's
 * 24-hour strip; on medium and expanded windows: the full 7×24 grid. Pick a mode from the
 * legend, then tap or drag over hours to paint them. [schedule] is the core's 168-letter string.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ScheduleGridEditor(schedule: String?, onChange: (String) -> Unit) {
    var brush by rememberSaveable { mutableStateOf(ScheduleCell.Alt) }
    val shape = rememberWindowShape()
    val now = LocalDateTime.now()
    val current = (now.dayOfWeek.value - 1) to now.hour

    Text("Pick a mode, then tap or drag over the hours it applies to.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        ScheduleCell.entries.forEach { cell ->
            FilterChip(
                selected = brush == cell,
                onClick = { brush = cell },
                label = { Text(cell.label) },
                leadingIcon = { Swatch(cell) },
            )
        }
    }
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val wide = shape.width >= WidthClass.Medium && maxWidth >= GRID_MIN_WIDTH
        if (wide) {
            WeekGrid(schedule, brush, current, maxWidth, onChange)
        } else {
            DayStrip(schedule, brush, current, onChange)
        }
    }
}

@Composable
private fun Swatch(cell: ScheduleCell) {
    Box(Modifier.size(16.dp).background(cellColor(cell), MaterialTheme.shapes.extraSmall).border(1.dp, MaterialTheme.colorScheme.outline, MaterialTheme.shapes.extraSmall))
}

@Composable
private fun cellColor(cell: ScheduleCell): Color = when (cell) {
    ScheduleCell.Normal -> MaterialTheme.colorScheme.surfaceContainerHighest
    ScheduleCell.Alt -> MaterialTheme.colorScheme.tertiaryContainer
    ScheduleCell.SeedOnly -> MaterialTheme.colorScheme.secondaryContainer
    ScheduleCell.Off -> MaterialTheme.colorScheme.errorContainer
}

@Composable
private fun cellContent(cell: ScheduleCell): Color = when (cell) {
    ScheduleCell.Normal -> MaterialTheme.colorScheme.onSurfaceVariant
    ScheduleCell.Alt -> MaterialTheme.colorScheme.onTertiaryContainer
    ScheduleCell.SeedOnly -> MaterialTheme.colorScheme.onSecondaryContainer
    ScheduleCell.Off -> MaterialTheme.colorScheme.onErrorContainer
}

/** One hour; the letter repeats the colour so the mode doesn't depend on colour alone. */
@Composable
private fun Cell(cell: ScheduleCell, label: String, isNow: Boolean, width: Dp, onPaint: () -> Unit) {
    Box(
        Modifier
            .width(width)
            .height(CELL_HEIGHT)
            .padding(1.dp)
            .background(cellColor(cell))
            .then(if (isNow) Modifier.border(BorderStroke(2.dp, MaterialTheme.colorScheme.primary)) else Modifier)
            .semantics {
                contentDescription = "$label, ${cell.label}" + if (isNow) ", now" else ""
                onClick(label = "Set to the picked mode") { onPaint(); true }
            },
        contentAlignment = Alignment.Center,
    ) {
        if (cell != ScheduleCell.Normal) Text(cell.short, style = MaterialTheme.typography.labelSmall, color = cellContent(cell))
    }
}

/** Compact: a day picker and that day's 24 hours in two rows of twelve. */
@Composable
private fun DayStrip(schedule: String?, brush: ScheduleCell, current: Pair<Int, Int>, onChange: (String) -> Unit) {
    var day by rememberSaveable { mutableIntStateOf(current.first) }
    val working = rememberWorkingSchedule(schedule)
    val strip = ScheduleGrid.dayStrip(schedule, day)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ScheduleGrid.DAY_SHORT.forEachIndexed { i, name ->
                FilterChip(selected = day == i, onClick = { day = i }, label = { Text(name) })
            }
        }
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val perRow = ScheduleGrid.HOURS / 2
            val cellWidth = (maxWidth - 32.dp) / perRow
            fun paint(hour: Int) = working.paint(day, hour, brush, onChange)
            Column(
                Modifier.pointerInput(day, brush, cellWidth) {
                    val hit = { pos: Offset ->
                        val col = ((pos.x - 32.dp.toPx()) / cellWidth.toPx()).toInt()
                        val row = (pos.y / (CELL_HEIGHT.toPx() + 16.dp.toPx())).toInt()
                        if (pos.x >= 32.dp.toPx() && col in 0 until perRow && row in 0..1) paint(row * perRow + col)
                    }
                    detectTapGestures(onTap = hit)
                }.pointerInput(day, brush, cellWidth) {
                    val hit = { pos: Offset ->
                        val col = ((pos.x - 32.dp.toPx()) / cellWidth.toPx()).toInt()
                        val row = (pos.y / (CELL_HEIGHT.toPx() + 16.dp.toPx())).toInt()
                        if (pos.x >= 32.dp.toPx() && col in 0 until perRow && row in 0..1) paint(row * perRow + col)
                    }
                    detectDragGestures(onDragStart = hit) { change, _ -> hit(change.position) }
                },
            ) {
                for (row in 0..1) {
                    Row(Modifier.height(16.dp)) {
                        Box(Modifier.width(32.dp))
                        for (col in 0 until perRow step 3) {
                            Text(
                                hh(row * perRow + col),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.width(cellWidth * 3),
                            )
                        }
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(if (row == 0) "AM" else "PM", style = MaterialTheme.typography.labelSmall, modifier = Modifier.width(32.dp))
                        for (col in 0 until perRow) {
                            val hour = row * perRow + col
                            Cell(strip[hour], "${ScheduleGrid.DAY_NAMES[day]} ${hh(hour)}:00", current == day to hour, cellWidth) { paint(hour) }
                        }
                    }
                }
            }
        }
        OutlinedButton(onClick = { working.change(ScheduleGrid.copyDayToAll(working.value, day), onChange) }) {
            Text("Copy ${ScheduleGrid.DAY_NAMES[day]} to every day")
        }
    }
}

/** Medium and expanded: the whole week, a row per day and a column per hour. */
@Composable
private fun WeekGrid(schedule: String?, brush: ScheduleCell, current: Pair<Int, Int>, width: Dp, onChange: (String) -> Unit) {
    val cells = ScheduleGrid.cells(schedule)
    val working = rememberWorkingSchedule(schedule)
    val cellWidth = (width - DAY_LABEL) / ScheduleGrid.HOURS
    fun paint(day: Int, hour: Int) = working.paint(day, hour, brush, onChange)
    Column {
        Row {
            Box(Modifier.width(DAY_LABEL))
            for (hour in 0 until ScheduleGrid.HOURS step 3) {
                Text(
                    hh(hour),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.width(cellWidth * 3),
                )
            }
        }
        Column(
            Modifier.pointerInput(brush, cellWidth) {
                val hit = { pos: Offset ->
                    val hour = ((pos.x - DAY_LABEL.toPx()) / cellWidth.toPx()).toInt()
                    val day = (pos.y / CELL_HEIGHT.toPx()).toInt()
                    if (pos.x >= DAY_LABEL.toPx() && hour in 0 until ScheduleGrid.HOURS && day in 0 until ScheduleGrid.DAYS) paint(day, hour)
                }
                detectDragGestures(onDragStart = hit) { change, _ -> hit(change.position) }
            }.pointerInput(brush, cellWidth) {
                detectTapGestures { pos ->
                    val hour = ((pos.x - DAY_LABEL.toPx()) / cellWidth.toPx()).toInt()
                    val day = (pos.y / CELL_HEIGHT.toPx()).toInt()
                    if (pos.x >= DAY_LABEL.toPx() && hour in 0 until ScheduleGrid.HOURS && day in 0 until ScheduleGrid.DAYS) paint(day, hour)
                }
            },
        ) {
            for (day in 0 until ScheduleGrid.DAYS) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(ScheduleGrid.DAY_SHORT[day], style = MaterialTheme.typography.labelMedium, textAlign = TextAlign.Start, modifier = Modifier.width(DAY_LABEL))
                    for (hour in 0 until ScheduleGrid.HOURS) {
                        Cell(cells[ScheduleGrid.index(day, hour)], "${ScheduleGrid.DAY_NAMES[day]} ${hh(hour)}:00", current == day to hour, cellWidth) {
                            paint(day, hour)
                        }
                    }
                }
            }
        }
    }
}
