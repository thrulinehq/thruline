//! T9 — the Technical line of the one applicability rule is the same words in
//! the engine (`voice::APPLICABILITY_TECHNICAL_LINES`) and in the browser
//! (`hq-copy.js` `change.coverage.carried`, rendered into
//! `hq-copy.golden.json` with `hq-copy.test.js`'s own sample fills).

use thruline_core::voice::APPLICABILITY_TECHNICAL_LINES;

/// `hq-copy.test.js` `SAMPLE`: the values the golden is rendered with.
const SAMPLE: [(&str, &str); 3] = [
    ("{path}", "src/app/main.rs"),
    ("{version}", "2"),
    ("{now}", "9f3d21e"),
];

#[test]
fn the_carried_forward_lines_match_the_browsers_copy_file_word_for_word() {
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/workspace/hq-copy.golden.json");
    let raw = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("{}: {e}", golden_path.display()));
    let golden: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&raw).expect("the golden is JSON");

    for (slug, template) in APPLICABILITY_TECHNICAL_LINES {
        let mut rendered = (*template).to_string();
        for (slot, value) in SAMPLE {
            rendered = rendered.replace(slot, value);
        }
        assert!(!rendered.contains('{'), "an unfilled slot in {slug}");
        assert_eq!(
            golden
                .get(&format!("change.coverage.carried.{slug}"))
                .and_then(|v| v.as_str()),
            Some(rendered.as_str()),
            "the engine and the browser disagree about {slug}"
        );
    }
    let browser: Vec<&String> = golden
        .keys()
        .filter(|k| k.starts_with("change.coverage.carried."))
        .collect();
    assert_eq!(
        browser.len(),
        APPLICABILITY_TECHNICAL_LINES.len(),
        "every carried-forward line the browser shows has an engine copy: {browser:?}"
    );
}
