use criterion::{criterion_group, criterion_main, Criterion};
use deep_dive::analysis::comparer::Comparer;
use deep_dive::analysis::filetree::{FileInfo, FileTree, TarEntryType};
use deep_dive::image::Layer;

/// Build a synthetic image with `layer_count` layers.
///
/// Each layer adds a handful of small files so that stacking and comparing
/// requires real work, but the total data size stays tiny.
fn make_layers(layer_count: usize) -> Vec<Layer> {
    (0..layer_count)
        .map(|i| {
            let mut tree = FileTree::new();
            let path = format!("files/layer{}.txt", i);
            // Add a unique file for this layer.
            tree.add_path(
                &path,
                FileInfo {
                    entry_type: TarEntryType::Regular,
                    size: 64,
                    content_hash: i as u64,
                    content: vec![0u8; 64],
                    ..Default::default()
                },
            );
            // Re-add a common path in every layer to create shading/duplication.
            tree.add_path(
                "files/common.txt",
                FileInfo {
                    entry_type: TarEntryType::Regular,
                    size: 32,
                    content_hash: i as u64 + 1000,
                    content: vec![0u8; 32],
                    ..Default::default()
                },
            );
            Layer::new(i, format!("RUN echo {}", i), 96, tree)
        })
        .collect()
}

fn comparer_benchmark(c: &mut Criterion) {
    let layers = make_layers(250);

    c.bench_function("comparer_build_cache_250_layers", |b| {
        b.iter_with_setup(
            || Comparer::new(layers.clone()),
            |mut comparer| comparer.build_cache(),
        )
    });
}

criterion_group!(benches, comparer_benchmark);
criterion_main!(benches);
