// SPDX-License-Identifier: GPL-3.0-or-later

//! Newznab category numbers, used by Torznab (Jackett, Prowlarr, Sonarr,
//! Radarr).

use crate::model::Category;

/// The main Newznab id for a Hashlark category.
pub fn id_of(category: Category) -> u32 {
    match category {
        Category::Games => 1000,
        Category::Movies => 2000,
        Category::Music => 3000,
        Category::Software => 4000,
        Category::Tv => 5000,
        Category::Anime => 5070,
        Category::Books => 7000,
        Category::Other => 8000,
    }
}

/// The Hashlark category of a Newznab id (including sub-categories).
pub fn category_of(id: u32) -> Option<Category> {
    Some(match id {
        5070 => Category::Anime,
        4050 => Category::Games,
        1000..=1999 => Category::Games,
        2000..=2999 => Category::Movies,
        3000..=3999 => Category::Music,
        4000..=4999 => Category::Software,
        5000..=5999 => Category::Tv,
        7000..=7999 => Category::Books,
        6000..=6999 | 8000..=8999 => Category::Other,
        // Private sites use ids above 100000 for their own categories.
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_subcategories() {
        for c in Category::ALL {
            assert_eq!(category_of(id_of(c)), Some(c), "{c}");
        }
        assert_eq!(category_of(2040), Some(Category::Movies));
        assert_eq!(category_of(5070), Some(Category::Anime));
        assert_eq!(category_of(4050), Some(Category::Games));
        assert_eq!(category_of(100_123), None);
    }
}
