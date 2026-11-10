// SPDX-License-Identifier: GPL-3.0-or-later

//! The weekly schedule (plan §3.3): which mode applies in an hour, and the
//! speed limits in effect.

use hashlark_core::settings::{SpeedLimits, TorrentSettings, WeekGrid};

use super::Desired;
use super::power::BatteryEffect;
use crate::model::ScheduleMode;

/// A local wall-clock hour of the week. A DST change skips or repeats an
/// hour of the grid, as the wall clock does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// 0 = Monday ... 6 = Sunday.
    pub weekday: u8,
    /// 0-23.
    pub hour: u8,
}

impl LocalTime {
    pub fn new(weekday: u8, hour: u8) -> Self {
        Self { weekday, hour }
    }

    /// The cell of the week, 0 = Monday 00:00.
    fn index(self) -> usize {
        usize::from(self.weekday) * 24 + usize::from(self.hour)
    }
}

/// The mode the grid sets at `now`. No grid, a malformed one, or a time
/// outside the week mean `Normal`.
pub(crate) fn mode_at(grid: Option<&WeekGrid>, now: LocalTime) -> ScheduleMode {
    let Some(grid) = grid.filter(|g| g.is_valid()) else {
        return ScheduleMode::Normal;
    };
    match grid.cell(now.index()) {
        Some('a') => ScheduleMode::Alt,
        Some('s') => ScheduleMode::SeedOnly,
        Some('o') => ScheduleMode::Off,
        _ => ScheduleMode::Normal,
    }
}

/// Holds torrents the schedule doesn't allow: everything when Off, and
/// downloads when Seed only.
pub(crate) fn gate(mode: ScheduleMode, is_download: bool) -> Option<Desired> {
    match mode {
        ScheduleMode::Off => Some(Desired::Scheduled),
        ScheduleMode::SeedOnly if is_download => Some(Desired::Scheduled),
        _ => None,
    }
}

/// The session speed limits in effect: the alternative ones when turtle is
/// on, the schedule says `Alt`, or the battery policy says so; otherwise
/// the normal ones.
pub(crate) fn effective_limits(
    t: &TorrentSettings,
    mode: ScheduleMode,
    battery: BatteryEffect,
) -> SpeedLimits {
    if t.turtle || mode == ScheduleMode::Alt || battery == BatteryEffect::Alt {
        t.alt_limits
    } else {
        t.limits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of `fill` with some cells set.
    fn grid(fill: char, cells: &[(usize, char)]) -> WeekGrid {
        let mut v: Vec<char> = std::iter::repeat_n(fill, WeekGrid::LEN).collect();
        for &(i, c) in cells {
            v[i] = c;
        }
        WeekGrid(v.into_iter().collect())
    }

    #[test]
    fn each_mode() {
        let g = grid('n', &[(10, 'a'), (11, 's'), (12, 'o')]);
        let at = |h| mode_at(Some(&g), LocalTime::new(0, h));
        assert_eq!(at(9), ScheduleMode::Normal);
        assert_eq!(at(10), ScheduleMode::Alt);
        assert_eq!(at(11), ScheduleMode::SeedOnly);
        assert_eq!(at(12), ScheduleMode::Off);
    }

    #[test]
    fn week_edges() {
        let g = grid('n', &[(0, 'o'), (167, 's')]);
        assert_eq!(mode_at(Some(&g), LocalTime::new(0, 0)), ScheduleMode::Off);
        assert_eq!(
            mode_at(Some(&g), LocalTime::new(0, 1)),
            ScheduleMode::Normal
        );
        assert_eq!(
            mode_at(Some(&g), LocalTime::new(6, 23)),
            ScheduleMode::SeedOnly
        );
        assert_eq!(
            mode_at(Some(&g), LocalTime::new(6, 22)),
            ScheduleMode::Normal
        );
        // Tuesday 03:00 is cell 27.
        let g = grid('n', &[(27, 'a')]);
        assert_eq!(mode_at(Some(&g), LocalTime::new(1, 3)), ScheduleMode::Alt);
    }

    #[test]
    fn no_or_malformed_grid_is_normal() {
        assert_eq!(mode_at(None, LocalTime::new(0, 0)), ScheduleMode::Normal);
        let short = WeekGrid("o".repeat(167));
        assert_eq!(
            mode_at(Some(&short), LocalTime::new(0, 0)),
            ScheduleMode::Normal
        );
        let bad = WeekGrid("x".repeat(168));
        assert_eq!(
            mode_at(Some(&bad), LocalTime::new(0, 0)),
            ScheduleMode::Normal
        );
        // Out of the week.
        let all_off = grid('o', &[]);
        assert_eq!(
            mode_at(Some(&all_off), LocalTime::new(7, 0)),
            ScheduleMode::Normal
        );
    }

    #[test]
    fn malformed_grid_fails_validation() {
        let mut settings = hashlark_core::settings::Settings::default();
        settings.torrents.schedule = Some(WeekGrid("n".repeat(167)));
        assert!(settings.validate().is_err());
        settings.torrents.schedule = Some(grid('n', &[]));
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn gate_by_mode() {
        assert_eq!(gate(ScheduleMode::Off, false), Some(Desired::Scheduled));
        assert_eq!(gate(ScheduleMode::Off, true), Some(Desired::Scheduled));
        assert_eq!(gate(ScheduleMode::SeedOnly, true), Some(Desired::Scheduled));
        assert_eq!(gate(ScheduleMode::SeedOnly, false), None);
        assert_eq!(gate(ScheduleMode::Alt, true), None);
        assert_eq!(gate(ScheduleMode::Normal, true), None);
    }

    #[test]
    fn alt_limits_for_turtle_schedule_and_battery() {
        let mut t = TorrentSettings {
            limits: SpeedLimits {
                down_kib: Some(5000),
                up_kib: None,
            },
            ..TorrentSettings::default()
        };
        let normal = |t: &TorrentSettings, m, b| effective_limits(t, m, b) == t.limits;
        assert!(normal(&t, ScheduleMode::Normal, BatteryEffect::None));
        assert!(normal(&t, ScheduleMode::SeedOnly, BatteryEffect::Pause));
        assert!(normal(&t, ScheduleMode::Off, BatteryEffect::None));
        let alt = |t: &TorrentSettings, m, b| effective_limits(t, m, b) == t.alt_limits;
        assert!(alt(&t, ScheduleMode::Alt, BatteryEffect::None));
        assert!(alt(&t, ScheduleMode::Normal, BatteryEffect::Alt));
        t.turtle = true;
        assert!(alt(&t, ScheduleMode::Normal, BatteryEffect::None));
        assert_ne!(t.limits, t.alt_limits);
    }
}
