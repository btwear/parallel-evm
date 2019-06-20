extern crate parallel_evm;
use common_types::block::Block;
use common_types::transaction::SignedTransaction;
use criterion::{Bencher, Criterion, Fun};
use ethcore::ethereum;
use ethcore::open_state::CleanupMode;
use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use ethereum_types::U256;
use parallel_evm::parallel_manager::ParallelManager;
use parallel_evm::test_helpers;
use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use vm::EnvInfo;

const TX_COUNT: usize = 100000;
const CHUNK_SIZE: usize = 2000;

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
    let mut blocks = vec![];
    for txs in input.transactions.chunks(CHUNK_SIZE) {
        blocks.push(Block {
            header: Default::default(),
            transactions: txs.into_iter().map(|stx| stx.deref().clone()).collect(),
            uncles: vec![],
        });
    }
    b.iter(|| {
        let mut parallel_manager = ParallelManager::new(input.state.clone(), vec![], false);
        parallel_manager.add_engines(engines);
        for i in 0..TX_COUNT / CHUNK_SIZE {
            parallel_manager.push_block(blocks[i].clone());
            parallel_manager.step_one_block();
        }
        parallel_manager.stop();
    });
}

fn bench_seq_evm(b: &mut Bencher, input: &BenchInput) {
    b.iter(|| {
        let mut state = input.state.clone();
        let machine = ethereum::new_constantinople_fix_test_machine();
        let mut env_info: EnvInfo = Default::default();
        env_info.gas_limit = U256::from(100_000_000);
        for tx in &input.transactions {
            state.apply(&env_info, &machine, &tx, false).unwrap();
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

    let senders = test_helpers::random_keypairs(TX_COUNT);
    let to = test_helpers::random_addresses(TX_COUNT);
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
    c.bench_functions("no_dependency_small_batch", funs, input);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(5);
    targets = bench
}
