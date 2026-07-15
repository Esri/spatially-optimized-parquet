# Writer benchmarks

The benchmark target runs optimized single-file and eight-file partitioned output through the
complete public pipeline, including output validation, using approximately 512 MiB fixtures.

Fixtures are generated before Criterion starts measuring. Outputs are removed after each timed run,
and throughput uses the actual input Parquet file size.

Each fixture has 52 root columns: `id`, `geometry`, 45 non-nullable `Float64` columns named
`value_00` through `value_44`, and five UTF-8 columns named `name`, `description`, `source_url`,
`owner`, and `external_ref`. Numeric and string values use fixed-seed deterministic generators.
The string columns cover short names, long descriptions, URL-shaped values, owner labels, and
identifier-shaped references with varying lengths.

The `polygon_wide_4x` fixture has exactly 208 root columns: `id`, `geometry`, 185 `Float64`
columns, and 21 UTF-8 columns. It runs only the optimized single-file case to isolate how the
single-file plan scales with four times the standard column count.

Run all cases:

```sh
cargo bench -p spatial --bench writer
```

Filter to one case when measuring a specific path:

```sh
cargo bench -p spatial --bench writer -- polygon/optimized_single
cargo bench -p spatial --bench writer -- polygon_wide_4x/optimized_single
cargo bench -p spatial --bench writer -- optimized_partitioned
```

Print the independent DataFusion physical plans for one case without collecting Criterion samples:

```sh
cargo bench -p spatial --bench writer --features print-plan -- \
  polygon/optimized_partitioned_8 --test
```

Partitioned output prints the cluster-boundary aggregate and final sink plans. Extent aggregation
also prints when reprojection, row selection, or missing source metadata disables the metadata
fast path.
