//! Isolated allocation profile. Run with an explicit output path outside the
//! source inputs. DHAT describes heap allocation, not process RSS.
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("provide the DHAT output path")?;
    let _profiler = dhat::Profiler::builder().file_name(path).build();
    let bytes=serde_json::to_vec(&(0..1000).map(|i|serde_json::json!({"ruleId":"no-eval","file":format!("src/{i}.js"),"message":"Dynamic code execution","range":{"byteOffset":{"start":0,"end":8},"start":{"line":0}}})).collect::<Vec<_>>())?;
    for _ in 0..20 {
        let r = thruline_verify::integrations::decode("ast-grep-json-v1", &bytes)?;
        assert_eq!(r.observations.len(), 1000);
    }
    Ok(())
}
