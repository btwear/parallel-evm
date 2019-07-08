#[macro_use]
extern crate criterion;

mod benchmarks;

criterion_main! {
    benchmarks::no_dependency_big_block::benches,
    benchmarks::no_dependency_small_batch::benches,
    benchmarks::real_data::benches,
}
