use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use thruline_verify::integrations::decode;
fn benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("integration-report-decode");
    for n in [1usize, 100, 1000] {
        let rows:Vec<_>=(0..n).map(|i|serde_json::json!({"ruleId":"no-eval","file":format!("src/{i}.js"),"message":"Dynamic code execution","range":{"byteOffset":{"start":0,"end":8},"start":{"line":0}}})).collect();
        let bytes = serde_json::to_vec(&rows).expect("fixture serializes");
        assert_eq!(
            decode("ast-grep-json-v1", &bytes)
                .expect("fixture decodes")
                .observations
                .len(),
            n
        );
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(format!("ast-grep-{n}"), |b| {
            b.iter(|| decode("ast-grep-json-v1", black_box(&bytes)))
        });
    }
    group.finish();
}
criterion_group! {name=benches;config=Criterion::default().sample_size(20).warm_up_time(std::time::Duration::from_secs(1)).measurement_time(std::time::Duration::from_secs(2));targets=benchmarks}
criterion_main!(benches);
