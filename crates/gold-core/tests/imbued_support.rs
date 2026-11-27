//! A gem with a built-in support ("Supported by Level 1 <zero-width space>Diminishment").
#![expect(clippy::unwrap_used, reason = "tests")]
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use std::path::Path;

#[test]
fn imbued_support_with_zero_width_space() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = GameData::load(&root.join("../../assets")).unwrap();
    let text = std::fs::read_to_string(root.join("tests/imbued_support.txt")).unwrap();
    assert!(text.contains('\u{200b}'));
    let item = parse_clipboard(&data, &text).unwrap();
    assert!(item.gem.imbued);
}
