extern crate parallel_evm;
use common_types::block::Block;
use common_types::transaction::SignedTransaction;
use criterion::{Bencher, Criterion, Fun};
use ethcore::ethereum;
use ethcore::open_state::CleanupMode;
use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use ethcore_blockchain::BlockChainDB;
use ethereum_types::{H256, U256};
use parallel_evm::parallel_manager::ParallelManager;
use parallel_evm::test_helpers::{self, update_envinfo_by_header};
use parallel_evm::types::Reward;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use vm::EnvInfo;

const TX_DELAY: usize = 0;
const DB_PATH: &str = "/tmp/tmp_eth_db";
const BLOCK_PATH: &str = "/tmp/res/blocks/7840001_7850000.bin";
const REWARD_PATH: &str = "/tmp/res/rewards/7840001_7850000.json";
const LAST_HASHES_PATH: &str = "/tmp/res/lastHashes7840001";
const N: usize = 100;
const STATE_ROOT_STR: &str = "0xee45b8d18c5d1993cbd6b985cd2ed2f437f9a29ef89c75cd1dc24e352993a77c";

struct BenchInput {
    db: Arc<dyn BlockChainDB>,
    root: H256,
    blocks: Vec<Arc<RwLock<Block>>>,
    rewards: Vec<Reward>,
    last_hashes: Vec<H256>,
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
fn bench_par_evm_3(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 3);
}
fn bench_par_evm_4(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 4);
}
fn bench_par_evm_5(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 5);
}
fn bench_par_evm_6(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 6);
}
fn bench_par_evm_7(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 7);
}
fn bench_par_evm_8(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 8);
}
fn bench_par_evm_9(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 9);
}
fn bench_par_evm_10(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 10);
}
fn bench_par_evm_11(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 11);
}
fn bench_par_evm_12(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 12);
}
fn bench_par_evm_13(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 13);
}
fn bench_par_evm_14(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 14);
}
fn bench_par_evm_15(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 15);
}
fn bench_par_evm_16(b: &mut Bencher, input: &BenchInput) {
    bench_par_evm(b, input, 16);
}

fn bench_par_evm(b: &mut Bencher, input: &BenchInput, engines: usize) {
    b.iter(|| {
        let journal_db = journaldb::new(
            input.db.key_value().clone(),
            ::journaldb::Algorithm::Archive,
            ::ethcore_db::COL_STATE,
        );
        let state_db = StateDB::new(journal_db, 5 * 1024 * 1024);
        let state = State::from_existing(
            state_db,
            input.root.clone(),
            U256::zero(),
            Default::default(),
        )
        .unwrap();
        let mut parallel_manager = ParallelManager::new(state, input.last_hashes.clone(), TX_DELAY);
        parallel_manager.assign_block_and_reward_arc(input.blocks.clone(), input.rewards.clone());
        parallel_manager.add_engines(engines);
        for _ in 0..N {
            parallel_manager.step_one_block();
        }
        parallel_manager.stop();
    });
}

fn bench_seq_evm(b: &mut Bencher, input: &BenchInput) {
    b.iter(|| {
        let journal_db = journaldb::new(
            input.db.key_value().clone(),
            ::journaldb::Algorithm::Archive,
            ::ethcore_db::COL_STATE,
        );
        let mut state_db = StateDB::new(journal_db, 5 * 1024 * 1024);
        let mut root = input.root.clone();
        let machine = ethereum::new_constantinople_fix_test_machine();
        let mut env_info = EnvInfo::default();
        env_info.last_hashes = Arc::new(input.last_hashes.clone());
        for i in 0..N {
            let mut state =
                State::from_existing(state_db, root, U256::zero(), Default::default()).unwrap();
            update_envinfo_by_header(&mut env_info, &input.blocks[i].read().header);
            for utx in input.blocks[i].read().clone().transactions {
                let tx = SignedTransaction::new(utx.clone()).unwrap();
                let outcome = state
                    .apply_with_delay(&env_info, &machine, &tx, true, TX_DELAY)
                    .unwrap();
                env_info.gas_used = outcome.receipt.gas_used;
            }

            let reward = &input.rewards[i];
            state
                .add_balance(
                    &reward.miner.clone().into(),
                    &reward.reward.into(),
                    CleanupMode::NoEmpty,
                )
                .unwrap();
            for uncle in &reward.uncles {
                state
                    .add_balance(
                        &uncle.miner.clone().into(),
                        &uncle.reward.into(),
                        CleanupMode::NoEmpty,
                    )
                    .unwrap();
            }
            state.commit().unwrap();
            let (new_root, new_state_db) = state.drop();
            state_db = new_state_db;
            root = new_root;
        }
    });
}

fn bench(c: &mut Criterion) {
    let seq_evm = Fun::new("Sequential", bench_seq_evm);
    let par_evm_1 = Fun::new("Parallel_1", bench_par_evm_1);
    let par_evm_2 = Fun::new("Parallel_2", bench_par_evm_2);
    let par_evm_3 = Fun::new("Parallel_3", bench_par_evm_3);
    let par_evm_4 = Fun::new("Parallel_4", bench_par_evm_4);
    let par_evm_5 = Fun::new("Parallel_5", bench_par_evm_5);
    let par_evm_6 = Fun::new("Parallel_6", bench_par_evm_6);
    let par_evm_7 = Fun::new("Parallel_7", bench_par_evm_7);
    let par_evm_8 = Fun::new("Parallel_8", bench_par_evm_8);
    let par_evm_9 = Fun::new("Parallel_9", bench_par_evm_9);
    let par_evm_10 = Fun::new("Parallel_10", bench_par_evm_10);
    let par_evm_11 = Fun::new("Parallel_11", bench_par_evm_11);
    let par_evm_12 = Fun::new("Parallel_12", bench_par_evm_12);
    let par_evm_13 = Fun::new("Parallel_13", bench_par_evm_13);
    let par_evm_14 = Fun::new("Parallel_14", bench_par_evm_14);
    let par_evm_15 = Fun::new("Parallel_15", bench_par_evm_15);
    let par_evm_16 = Fun::new("Parallel_16", bench_par_evm_16);
    let funs = vec![
        par_evm_1, par_evm_2, par_evm_3, par_evm_4, par_evm_5, par_evm_6, par_evm_7, par_evm_8,
        par_evm_9, par_evm_10, par_evm_11, par_evm_12, par_evm_13, par_evm_14, par_evm_15,
        par_evm_16,
    ];

    let db = test_helpers::open_database(DB_PATH);
    let blocks = test_helpers::read_blocks(BLOCK_PATH, 1, N)
        .into_iter()
        .map(|b| Arc::new(RwLock::new(b)))
        .collect();
    let rewards = Reward::from_file(REWARD_PATH, 1, N);
    let mut last_hashes = VecDeque::from(test_helpers::load_last_hashes(LAST_HASHES_PATH));
    last_hashes.pop_front();
    last_hashes.resize(256, H256::zero());

    let input = BenchInput {
        db: db,
        root: H256::from(STATE_ROOT_STR),
        blocks: blocks,
        rewards: rewards,
        last_hashes: last_hashes.into(),
    };

    c.bench_functions("real_data", funs, input);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(2);
    targets = bench
}
