//! Finding the base type inside a magic item's name.
use crate::data::ItemDb;

#[must_use]
pub fn magic_basetype(items: &ItemDb, name: &str) -> Option<String> {
    let words: Vec<&str> = name.split(' ').collect();

    // every run of consecutive words
    let mut candidates: Vec<String> = Vec::new();
    for start in 0..words.len() {
        for end in start..words.len() {
            if let Some(run) = words.get(start..=end) {
                candidates.push(run.join(" "));
            }
        }
    }

    let mut found: Vec<String> = candidates
        .into_iter()
        .filter(|n| {
            items
                .by_translated("ITEM", n)
                .is_some_and(|r| r.first().is_some_and(|i| i.craftable.is_some()))
        })
        .collect();
    // longest first; the sort is stable
    found.sort_by_key(|n| std::cmp::Reverse(n.encode_utf16().count()));
    found.into_iter().next()
}
