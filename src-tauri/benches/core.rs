use criterion::{Criterion, criterion_group, criterion_main};
use knightframe_lib::{agent_loop, project, provider, tools};
use serde_json::json;

fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("benchmark directory");
    std::fs::create_dir_all(directory.path().join("src/features")).expect("fixture tree");
    for index in 0..240 {
        let dependency = (index + 1) % 240;
        std::fs::write(
            directory
                .path()
                .join(format!("src/features/module_{index}.ts")),
            format!(
                "import {{ value }} from './module_{dependency}';\nexport const value = {index};\n"
            ),
        )
        .expect("fixture file");
    }
    directory
}

fn core_benchmarks(criterion: &mut Criterion) {
    let directory = fixture();
    criterion.bench_function("manifest_240_files", |bencher| {
        bencher.iter(|| project::build_manifest(std::hint::black_box(directory.path())))
    });
    let indexed = project::build_manifest(directory.path()).expect("manifest");
    criterion.bench_function("graph_240_files", |bencher| {
        bencher.iter(|| project::graph_snapshot(std::hint::black_box(&indexed)))
    });
    criterion.bench_function("warm_indexed_text_search_240_files", |bencher| {
        bencher.iter(|| {
            tools::search_indexed(
                std::hint::black_box(&indexed),
                std::hint::black_box("value = 42"),
            )
        })
    });
    let artifact = json!({"matches": (0..700).map(|index| format!("src/file_{index}.rs:{}", index + 1)).collect::<Vec<_>>()});
    criterion.bench_function("tool_projection_700_matches", |bencher| {
        bencher
            .iter(|| agent_loop::project_artifact("bench".into(), std::hint::black_box(&artifact)))
    });
    let event = r#"{"choices":[{"delta":{"content":"ok"},"finish_reason":null}],"usage":{"prompt_tokens":210,"prompt_tokens_details":{"cached_tokens":180},"completion_tokens":4}}"#;
    criterion.bench_function("provider_sse_delta", |bencher| {
        bencher.iter(|| provider::parse_sse_data(std::hint::black_box(event)))
    });
}

criterion_group!(benches, core_benchmarks);
criterion_main!(benches);
