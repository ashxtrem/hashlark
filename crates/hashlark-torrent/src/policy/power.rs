// SPDX-License-Identifier: GPL-3.0-or-later

//! What running on battery does (plan §3.3, Power).

use hashlark_core::settings::OnBattery;

use super::Desired;

/// The effect of the battery state on the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatteryEffect {
    None,
    /// Use the alternative speed limits.
    Alt,
    /// Pause everything.
    Pause,
}

/// `on_battery` is the reported state (a laptop unplugged, or Android in
/// battery saver or below 15% and not charging); `setting` is the user's
/// choice.
pub(crate) fn battery_effect(on_battery: bool, setting: OnBattery) -> BatteryEffect {
    if !on_battery {
        return BatteryEffect::None;
    }
    match setting {
        OnBattery::Nothing => BatteryEffect::None,
        OnBattery::AltSpeed => BatteryEffect::Alt,
        OnBattery::Pause => BatteryEffect::Pause,
    }
}

/// Holds every torrent while the battery pause is on.
pub(crate) fn gate(on_battery_pause: bool) -> Option<Desired> {
    on_battery_pause.then_some(Desired::OnBattery)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_of_each_setting() {
        for setting in [OnBattery::Nothing, OnBattery::AltSpeed, OnBattery::Pause] {
            assert_eq!(battery_effect(false, setting), BatteryEffect::None);
        }
        assert_eq!(
            battery_effect(true, OnBattery::Nothing),
            BatteryEffect::None
        );
        assert_eq!(
            battery_effect(true, OnBattery::AltSpeed),
            BatteryEffect::Alt
        );
        assert_eq!(battery_effect(true, OnBattery::Pause), BatteryEffect::Pause);
    }

    #[test]
    fn gate_holds_on_pause() {
        assert_eq!(gate(true), Some(Desired::OnBattery));
        assert_eq!(gate(false), None);
    }
}
