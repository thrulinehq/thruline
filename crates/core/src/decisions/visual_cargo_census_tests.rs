use super::*;

#[test]
fn native_metadata_census_refuses_empty_unknown_or_incomplete_rows() {
    let script = include_str!("visual_process_census.py");
    let program = format!(
        "namespace = {{'__name__': 'census_test'}}\nexec({}, namespace)\n{}",
        serde_json::to_string(script).unwrap(),
        r#"import os
Refusal, census = namespace['Refusal'], namespace['census']
class Fake:
    def __init__(self, ids, unknown=False): self.ids, self.unknown = ids, unknown
    def pids(self): return self.ids
    def row(self, pid):
        if self.unknown: raise Refusal('synthetic unknown')
        return 0, 'cargo'
for native in (Fake(set()), Fake({1}), Fake({os.getpid()}, True)):
    try: census(native)
    except Refusal: pass
    else: raise AssertionError('unknown/empty metadata became a census')
assert census(Fake({os.getpid()})) == f'{os.getpid()} 0 cargo\n'
try: census(Fake({os.getpid()}), clock=iter([0, 3]).__next__)
except Refusal: pass
else: raise AssertionError('expired census returned metadata')
"#
    );
    let output = Command::new("python3")
        .args(["-I", "-c", &program])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "C50: visual cargo census dropped for v0.2 (C44 §8)"]
fn native_census_inside_the_capture_read_denial_sandbox() {
    const CHILD: &str = "THRULINE_CENSUS_TEST_CHILD";
    const PROTECTED: &str = "THRULINE_CENSUS_TEST_PROTECTED";
    if std::env::var_os(CHILD).is_some() {
        let protected = std::env::var_os(PROTECTED).unwrap();
        assert_eq!(
            std::fs::read(protected).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        // Cargo is a real visible ancestor of this focused test. The sandbox
        // must preserve the compiler census while denying protected files.
        assert!(active_compilers().unwrap() >= 1);
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let protected = temp.path().join("denied-fixture");
    std::fs::write(&protected, "synthetic fixture only").unwrap();
    let supplied = std::env::var_os("THRULINE_CENSUS_TEST_PROFILE");
    let mut profile = supplied.map_or_else(
        || "(version 1) (allow default)".to_owned(),
        |path| std::fs::read_to_string(path).unwrap(),
    );
    profile.push_str(&format!(
        " (deny file-read* file-write* (literal {}))",
        serde_json::to_string(&protected).unwrap()
    ));
    let profile_path = temp.path().join("capture.sb");
    std::fs::write(&profile_path, profile).unwrap();
    let current = std::thread::current();
    let output = Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(&profile_path)
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", current.name().unwrap(), "--nocapture"])
        .env(CHILD, "1")
        .env(PROTECTED, &protected)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "sandbox compiler census failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
