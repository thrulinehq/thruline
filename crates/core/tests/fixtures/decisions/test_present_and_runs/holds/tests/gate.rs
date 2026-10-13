#[path = "../src/feature.rs"]
mod feature;

#[test]
fn approval_requires_pro() {
    ::std::assert!(!feature::gate_open(false));
    ::std::assert!(feature::gate_open(true));
}
