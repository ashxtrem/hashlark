// SPDX-License-Identifier: GPL-3.0-or-later

//! Small helpers shared across the core.

/// FNV-1a: a tiny hash that is stable across Rust versions, unlike
/// `DefaultHasher`, so result ids stay the same between releases.
pub(crate) fn fnv1a64(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64("a"), 0xaf63_dc4c_8601_ec8c);
    }
}
