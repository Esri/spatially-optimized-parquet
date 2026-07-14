mod support;

use criterion::{Criterion, criterion_group, criterion_main};

fn benchmark_writer(criterion: &mut Criterion) {
  support::suite::benchmark_writer(criterion, 512, "small");
}

criterion_group! {
  name = benches;
  config = support::suite::criterion_config();
  targets = benchmark_writer
}
criterion_main!(benches);
