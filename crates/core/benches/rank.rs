use criterion::{Criterion, black_box, criterion_group, criterion_main};

use pirsa_core::search::rank_with_usage;
use pirsa_core::usage::Usage;
use pirsa_core::{AppEntry, LaunchTarget};

/// A Start Menu-sized index, matching what the launcher handles on a typical
/// development machine.
fn apps() -> Vec<AppEntry> {
    (0..200)
        .map(|index| {
            AppEntry::new(
                format!("Application {index}"),
                "Start Menu".to_owned(),
                LaunchTarget::Shell(format!("test:app-{index}")),
            )
        })
        .collect()
}

fn ranking(c: &mut Criterion) {
    let apps = apps();
    let usage = Usage::default();

    c.bench_function("rank 200 apps", |bencher| {
        bencher.iter(|| rank_with_usage(black_box(&apps), black_box("app 12"), &usage));
    });
}

criterion_group!(benches, ranking);
criterion_main!(benches);
