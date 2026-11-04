//! Finding an item (ADR-005 C2): every word typed, in any order, without minding case, accents or width, in its text
//! or its files' names. Ported from the Mac's `Search.swift`.

use crate::history::{Item, Kind, file_name};
use unicode_normalization::UnicodeNormalization;

/// Lowercase, without accents (the combining marks U+0300–U+036F, once letters are taken apart), and full-width
/// letters as ordinary ones. Thai's own marks are letters of its words, outside that block, and are kept.
pub fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match u32::from(c) {
            // Full-width ASCII (Ａ … ｚ, ０ … ９ and their punctuation).
            0xFF01..=0xFF5E => char::from_u32(u32::from(c) - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .collect()
}

/// Whether `item` has every word of `query`.
pub fn matches(item: &Item, query: &str) -> bool {
    let query = fold(query);
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return true;
    }
    let haystack = if item.kind == Kind::Files {
        let names: Vec<&str> = item.files.iter().map(|f| file_name(f)).collect();
        fold(&format!("{} {}", names.join(" "), item.text))
    } else {
        fold(&item.text)
    };
    words.iter().all(|w| haystack.contains(w))
}

/// The items with every word of `query`, in the history's order.
pub fn filter<'a>(items: &'a [Item], query: &str) -> Vec<&'a Item> {
    items.iter().filter(|i| matches(i, query)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Capture, History};
    use chrono::Utc;

    #[test]
    fn words_are_found_in_any_order_without_minding_case_or_accents() {
        let mut h = History::default();
        let now = Utc::now();
        h.add(Capture::text("Crème brûlée recipe", None), now);
        h.add(Capture::text("ส้มตำไทย ใส่ปู", None), now);
        h.add(Capture::text("ＦＵＬＬ width", None), now);
        let found =
            |q: &str| -> Vec<String> { filter(h.items(), q).iter().map(|i| i.title()).collect() };
        assert_eq!(found("RECIPE creme"), ["Crème brûlée recipe"]);
        assert_eq!(found("ใส่ปู"), ["ส้มตำไทย ใส่ปู"]);
        assert_eq!(found("full"), ["ＦＵＬＬ width"]);
        assert_eq!(found("   ").len(), 3);
        assert!(found("nothing").is_empty());
    }
}
