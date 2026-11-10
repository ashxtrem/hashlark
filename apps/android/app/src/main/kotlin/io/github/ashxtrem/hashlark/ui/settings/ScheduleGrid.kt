// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.settings

/** What one hour of the weekly schedule does; [code] is its letter in `torrents.schedule`. */
enum class ScheduleCell(val code: Char, val label: String, val short: String) {
    Normal('n', "Normal", "N"),
    Alt('a', "Alternative speed", "A"),
    SeedOnly('s', "Seed only", "S"),
    Off('o', "Off", "O"),
    ;

    companion object {
        fun of(code: Char): ScheduleCell? = entries.firstOrNull { it.code == code }
    }
}

/**
 * The weekly 7×24 schedule as the core stores it: 168 letters, Monday 00:00 first.
 * The compact editor shows one day as a 24-hour strip; these map that strip to and
 * from the whole grid.
 */
object ScheduleGrid {
    const val DAYS = 7
    const val HOURS = 24
    const val CELLS = DAYS * HOURS

    val DAY_NAMES = listOf("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")
    val DAY_SHORT = listOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")

    /** A schedule that is on but does nothing yet: every hour normal. */
    val ALL_NORMAL: String = ScheduleCell.Normal.code.toString().repeat(CELLS)

    fun index(day: Int, hour: Int): Int {
        require(day in 0 until DAYS && hour in 0 until HOURS) { "day $day, hour $hour" }
        return day * HOURS + hour
    }

    /** Whether [schedule] is a grid the core accepts. */
    fun isValid(schedule: String): Boolean = schedule.length == CELLS && schedule.all { ScheduleCell.of(it) != null }

    /** The 168 cells; no schedule, or a malformed one, is all normal. */
    fun cells(schedule: String?): List<ScheduleCell> {
        val text = schedule?.takeIf(::isValid) ?: ALL_NORMAL
        return text.map { ScheduleCell.of(it)!! }
    }

    fun encode(cells: List<ScheduleCell>): String {
        require(cells.size == CELLS) { "a schedule has $CELLS cells, not ${cells.size}" }
        return cells.joinToString("") { it.code.toString() }
    }

    /** The 24 hours of [day] (0 = Monday). */
    fun dayStrip(schedule: String?, day: Int): List<ScheduleCell> {
        val start = index(day, 0)
        return cells(schedule).subList(start, start + HOURS)
    }

    /** [schedule] with [day]'s 24 hours replaced by [strip]. */
    fun withDay(schedule: String?, day: Int, strip: List<ScheduleCell>): String {
        require(strip.size == HOURS) { "a day has $HOURS hours" }
        val all = cells(schedule).toMutableList()
        strip.forEachIndexed { hour, cell -> all[index(day, hour)] = cell }
        return encode(all)
    }

    /** [schedule] with one hour set to [cell]. */
    fun paint(schedule: String?, day: Int, hour: Int, cell: ScheduleCell): String {
        val all = cells(schedule).toMutableList()
        all[index(day, hour)] = cell
        return encode(all)
    }

    /** [schedule] with every hour of [day] copied to every other day. */
    fun copyDayToAll(schedule: String?, day: Int): String {
        val strip = dayStrip(schedule, day)
        return encode(List(DAYS) { strip }.flatten())
    }
}
