//! Preserve the app's Unicode 17 rules with the v5 compiler's Unicode 16 stdlib.
//! Only the changed lowercase mappings and final-sigma properties are supplied;
//! ordinary mappings continue using Rust's standard character conversion.
extern crate alloc;
use alloc::string::String;
mod data {
    include!("names_data.rs");
}

// A compiler/table change must be reviewed instead of changing skill uniqueness.
const _: () = assert!(matches!(char::UNICODE_VERSION, (16 | 17, 0, 0)));

fn property(ranges: &[(u32, u32)], c: char) -> bool {
    let n = u32::from(c);
    let index = ranges.partition_point(|(_, end)| *end < n);
    ranges.get(index).is_some_and(|(start, _)| *start <= n)
}
fn next_cased(mut chars: impl Iterator<Item = char>) -> bool {
    chars
        .find(|c| !property(data::CASE_IGNORABLE, *c))
        .is_some_and(|c| property(data::CASED, c))
}
pub fn lowercase(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for (index, c) in text.char_indices() {
        if c == 'Σ' {
            let final_sigma = next_cased(text[..index].chars().rev())
                && !next_cased(
                    text[index
                        .checked_add(c.len_utf8())
                        .expect("char_indices stays within the string")..]
                        .chars(),
                );
            result.push(if final_sigma { 'ς' } else { 'σ' });
        } else if let Ok(index) =
            data::LOWER_CHANGES.binary_search_by_key(&u32::from(c), |(upper, _)| *upper)
        {
            // The generator validates scalar mappings; no unsafe conversion.
            result
                .push(char::from_u32(data::LOWER_CHANGES[index].1).expect("valid Unicode mapping"));
        } else {
            result.extend(c.to_lowercase());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_scalar_and_sigma_context_matches_app_unicode17() {
        assert_eq!(
            char::UNICODE_VERSION,
            (17, 0, 0),
            "Run semantic tests with the app's Rust 1.96.1 toolchain"
        );
        for c in (0..=0x10ffff).filter_map(char::from_u32) {
            let scalar = alloc::format!("{c}");
            let after_sigma = alloc::format!("AΣ{c}");
            let before_sigma = alloc::format!("{c}Σ");
            for text in [scalar, after_sigma, before_sigma] {
                assert_eq!(lowercase(&text), text.to_lowercase(), "{c:?}");
            }
        }
    }
}
