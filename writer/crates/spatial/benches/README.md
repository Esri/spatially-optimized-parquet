# Writer benchmarks

The benchmark target runs optimized single-file and eight-file partitioned output through the
complete public pipeline, including output validation, using 512 MiB point and polygon fixtures.

Fixtures are generated before Criterion starts measuring. Outputs are removed after each timed run,
and throughput uses the actual input Parquet file size.

Each fixture has 52 root columns: `id`, `geometry`, 45 non-nullable `Float64` columns named
`value_00` through `value_44`, and five UTF-8 columns named `name`, `description`, `source_url`,
`owner`, and `external_ref`. Numeric and string values use fixed-seed deterministic generators.
The string columns cover short names, long descriptions, URL-shaped values, owner labels, and
identifier-shaped references with varying lengths.

Run all cases:

```sh
cargo bench -p spatial --bench writer
```

Filter to one case when measuring a specific path:

```sh
cargo bench -p spatial --bench writer -- polygon/optimized_single
cargo bench -p spatial --bench writer -- optimized_partitioned
```
