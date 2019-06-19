#[macro_use]
extern crate criterion;

mod benchmarks;

criterion_main! {
    benchmarks::no_dependency::benches,
    benchmarks::no_dependency_small_batch::benches,
    benchmarks::real_data::benches,
}
