#[macro_use]
extern crate criterion;
extern crate parallel_evm;
use common_types::transaction::SignedTransaction;
use criterion::{Bencher, Criterion, Fun};
use ethcore::open_state::CleanupMode;
use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use ethereum_types::U256;
use parallel_evm::execution_engine::sequential_exec;
use parallel_evm::parallel_manager::ParallelManager;
use parallel_evm::test_helpers;
use std::fmt::{self, Debug, Formatter};

struct BenchInput {
    state: State<StateDB>,
    transactions: Vec<SignedTransaction>,
}

impl Debug for BenchInput {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "")
    }
}

fn bench_par_evm_1(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 1);
}
fn bench_par_evm_2(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 2);
}
fn bench_par_evm_4(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 4);
}
fn bench_par_evm_6(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 6);
}

fn bench_par_evm(b: &mut Bencher, input: &BenchInput, engines: usize) {
    b.iter(|| {
        let mut parallel_manager = ParallelManager::new(input.state.clone());
        parallel_manager.add_engines(engines);
        parallel_manager.add_transactions(input.transactions.clone());
        parallel_manager.consume();
        parallel_manager.stop();
    });
}

fn bench_seq_evm(b: &mut Bencher, input: &BenchInput) {
    b.iter(|| {
        let mut state = input.state.clone();
        sequential_exec(&mut state, &input.transactions);
        state.commit().unwrap();
    });
}

fn bench(c: &mut Criterion) {
    let tx_number = 10000;
    let seq_evm = Fun::new("Sequential", bench_seq_evm);
    let par_evm_1 = Fun::new("Parallel_1", bench_par_evm_1);
    let par_evm_2 = Fun::new("Parallel_2", bench_par_evm_2);
    let par_evm_4 = Fun::new("Parallel_4", bench_par_evm_4);
    let par_evm_6 = Fun::new("Parallel_6", bench_par_evm_6);
    let funs = vec![par_evm_1, par_evm_2, par_evm_4, par_evm_6, seq_evm];

    let senders = test_helpers::random_keypairs(tx_number);
    let to = test_helpers::random_addresses(tx_number);
    let transactions = test_helpers::transfer_txs(&senders, &to);
    let mut state = test_helpers::get_temp_state();
    for tx in &transactions {
        state
            .add_balance(&tx.sender(), &U256::from(1), CleanupMode::NoEmpty)
            .unwrap();
    }
    state.commit().unwrap();

    let input = BenchInput {
        state: state,
        transactions: transactions,
    };
    c.bench_functions("no_dependency_no_contract", funs, input);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(5);
    targets = bench
}
criterion_main!(benches);
