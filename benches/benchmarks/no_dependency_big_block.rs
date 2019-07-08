extern crate parallel_evm;
use common_types::block::Block;
use common_types::transaction::SignedTransaction;
use criterion::{Bencher, Criterion, Fun};
use ethcore::ethereum;
use ethcore::open_state::CleanupMode;
use ethereum_types::U256;
use parallel_evm::parallel_manager::ParallelManager;
use parallel_evm::test_helpers;
use parking_lot::RwLock;
use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;
use vm::EnvInfo;

const TX_DELAY: usize = 0;
const TX_NUMBER: usize = 20000;

struct BenchInput {
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
    let mut block = Block {
        header: Default::default(),
        transactions: input
            .transactions
            .clone()
            .into_iter()
            .map(|stx| stx.deref().clone())
            .collect(),
        uncles: vec![],
    };
    block.header.set_gas_limit(U256::from(100000000));
    let block = Arc::new(RwLock::new(block));
    b.iter(|| {
        let state = test_helpers::get_temp_state();
        let mut parallel_manager = ParallelManager::new(state, vec![], TX_DELAY);
        parallel_manager.push_block_arc(block.clone());
        parallel_manager.add_engines(engines);
        parallel_manager.stop();
    });
}

fn bench_seq_evm(b: &mut Bencher, input: &BenchInput) {
    b.iter(|| {
        let mut state = test_helpers::get_temp_state();
        let machine = ethereum::new_constantinople_fix_test_machine();
        let mut env_info: EnvInfo = Default::default();
        env_info.gas_limit = U256::from(1000000000);
        for tx in &input.transactions {
            state
                .apply_with_delay(&env_info, &machine, &tx, false, TX_DELAY)
                .unwrap();
        }
    });
}

fn bench(c: &mut Criterion) {
    let seq_evm = Fun::new("Sequential", bench_seq_evm);
    let par_evm_1 = Fun::new("Parallel_1", bench_par_evm_1);
    let par_evm_2 = Fun::new("Parallel_2", bench_par_evm_2);
    let par_evm_4 = Fun::new("Parallel_4", bench_par_evm_4);
    let par_evm_6 = Fun::new("Parallel_6", bench_par_evm_6);
    let funs = vec![seq_evm, par_evm_1, par_evm_2, par_evm_4, par_evm_6];

    let senders = test_helpers::random_keypairs(TX_NUMBER);
    let to = test_helpers::random_addresses(TX_NUMBER);
    let transactions = test_helpers::transfer_txs(&senders, &to);
    let mut state = test_helpers::get_temp_state();
    for tx in &transactions {
        state
            .add_balance(&tx.sender(), &U256::from(1), CleanupMode::NoEmpty)
            .unwrap();
    }
    state.commit().unwrap();

    let input = BenchInput {
        transactions: transactions,
    };
    c.bench_functions("no_dependency_big_block", funs, input);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(2);
    targets = bench
}
