extern crate parallel_evm;
use common_types::block::Block;
use common_types::transaction::SignedTransaction;
use criterion::{Bencher, Criterion, Fun};
use ethcore::ethereum;
use ethcore::factory::Factories;
use ethcore::open_state::CleanupMode;
use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use ethereum_types::{H256, U256};
use parallel_evm::parallel_manager::ParallelManager;
use parallel_evm::test_helpers::{self, update_envinfo_by_header};
use parallel_evm::types::Reward;
use std::collections::VecDeque;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use vm::EnvInfo;

const DB_PATH: &str = "/tmp/tmp_eth_db";
const BLOCK_PATH: &str = "res/blocks/7840001_7850000.bin";
const REWARD_PATH: &str = "res/rewards/7840001_7850000.json";
const LAST_HASHES_PATH: &str = "res/lastHashes7840001";
const N: usize = 1;
const STATE_ROOT_STR: &str = "0xee45b8d18c5d1993cbd6b985cd2ed2f437f9a29ef89c75cd1dc24e352993a77c";

struct BenchInput {
    state: State<StateDB>,
    blocks: Vec<Block>,
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

fn bench_par_evm(b: &mut Bencher, input: &BenchInput, engines: usize) {
    b.iter(|| {
        let mut parallel_manager =
            ParallelManager::new(input.state.clone(), input.last_hashes.clone());
        for i in 0..N {
            parallel_manager
                .push_block_and_reward(input.blocks[i].clone(), input.rewards[i].clone());
        }
        parallel_manager.add_engines(engines);
        for _ in 0..N {
            parallel_manager.step_one_block();
        }
        parallel_manager.stop();
    });
}

fn bench_seq_evm(b: &mut Bencher, input: &BenchInput) {
    b.iter(|| {
        let mut state = input.state.clone();
        let machine = ethereum::new_constantinople_fix_test_machine();
        let mut env_info = EnvInfo::default();
        env_info.last_hashes = Arc::new(input.last_hashes.clone());
        for i in 0..N {
            update_envinfo_by_header(&mut env_info, &input.blocks[i].header);
            for utx in &input.blocks[i].transactions {
                let tx = SignedTransaction::new(utx.clone()).unwrap();
                let outcome = state.apply(&env_info, &machine, &tx, true).unwrap();
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
        }
    });
}

fn bench(c: &mut Criterion) {
    let seq_evm = Fun::new("Sequential", bench_seq_evm);
    let par_evm_1 = Fun::new("Parallel_1", bench_par_evm_1);
    let par_evm_2 = Fun::new("Parallel_2", bench_par_evm_2);
    let par_evm_3 = Fun::new("Parallel_3", bench_par_evm_3);
    let funs = vec![seq_evm, par_evm_1, par_evm_2, par_evm_3];

    let state_db = test_helpers::open_state_db(DB_PATH);
    let blocks = test_helpers::read_blocks(BLOCK_PATH, 1, N);
    let rewards = Reward::from_file(REWARD_PATH, 1, N);
    let mut last_hashes = VecDeque::from(test_helpers::load_last_hashes(LAST_HASHES_PATH));
    last_hashes.pop_front();
    last_hashes.resize(256, H256::zero());

    let factories = Factories::default();
    let root = H256::from(STATE_ROOT_STR);
    let state = State::from_existing(
        state_db.boxed_clone(),
        root.clone(),
        U256::zero(),
        factories.clone(),
    )
    .unwrap();

    let input = BenchInput {
        state: state,
        blocks: blocks,
        rewards: rewards,
        last_hashes: last_hashes.into(),
    };

    c.bench_functions("real_data", funs, input);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(5);
    targets = bench
}
