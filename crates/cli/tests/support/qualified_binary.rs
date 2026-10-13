//! Optional test-only binding to an installed artifact. Selecting it requires
//! its full SHA-256; a missing/wrong candidate is a failed test, never a skip.
use std::sync::OnceLock;

pub fn thruline() -> &'static str {
    static BINARY: OnceLock<String> = OnceLock::new();
    BINARY.get_or_init(|| {
        let Some(path) = std::env::var_os("THRULINE_QUALIFIED_BINARY") else {
            return env!("CARGO_BIN_EXE_thruline").to_string();
        };
        let path = std::path::PathBuf::from(path)
            .canonicalize()
            .expect("qualified binary must exist");
        let expected = std::env::var("THRULINE_QUALIFIED_BINARY_SHA256")
            .expect("explicit candidate selection requires THRULINE_QUALIFIED_BINARY_SHA256");
        assert_eq!(
            expected.len(),
            64,
            "candidate digest must be a full SHA-256"
        );
        let file = std::fs::File::open(&path).expect("open qualified binary");
        use std::io::Read;
        let mut bytes = Vec::new();
        file.take(256 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .expect("read qualified binary");
        assert!(
            bytes.len() <= 256 * 1024 * 1024,
            "candidate exceeds 256 MiB"
        );
        assert_eq!(
            thruline_core::chain::sha256_hex(&bytes),
            expected,
            "qualified candidate differs from the explicitly selected SHA-256"
        );
        path.to_str()
            .expect("qualified binary path must be UTF-8")
            .to_owned()
    })
}
