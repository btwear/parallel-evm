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
use parallel_evm::execution_engine::sequential_exec;
use parallel_evm::parallel_manager::ParallelManager;
use parallel_evm::reward::Reward;
use parallel_evm::test_helpers::{self, update_envinfo_by_header};
use std::collections::VecDeque;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use vm::EnvInfo;

const DB_PATH: &str = "/tmp/tmp_eth_db";
const BLOCK_PATH: &str = "res/blocks/7840001_7850000.bin";
const REWARD_PATH: &str = "res/rewards/7840001_7850000.json";
const LAST_HASHES_PATH: &str = "res/lastHashes7840001";
const N: usize = 1;

struct BenchInput {
    state: State<StateDB>,
    blocks: Vec<Block>,
    rewards: Vec<Reward>,
    last_hashes: Vec<H256>,
    parallel_managers: Vec<ParallelManager>,
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
    let mut parallel_managers = input.parallel_managers.clone();
    for pm in &mut parallel_managers {
        pm.clone_to_secure();
    }
    b.iter(|| {
        let mut state = input.state.clone();
        for parallel_manager in &mut parallel_managers {
            parallel_manager.set_state(state.clone());
            parallel_manager.add_engines(engines);
            parallel_manager.consume();
            let race = parallel_manager.stop();
            if race {
                parallel_manager.apply_secure();
            } else {
                parallel_manager.apply_engines();
                println!("no races");
            }
            state = parallel_manager.state();
        }
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
            state.commit();
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
    let root = H256::from("0xee45b8d18c5d1993cbd6b985cd2ed2f437f9a29ef89c75cd1dc24e352993a77c");
    let mut state = State::from_existing(
        state_db.boxed_clone(),
        root.clone(),
        U256::zero(),
        factories.clone(),
    )
    .unwrap();

    let mut env_info = EnvInfo::default();
    env_info.last_hashes = Arc::new(last_hashes.clone().into());
    let machine = ethereum::new_constantinople_fix_test_machine();
    let mut parallel_managers = vec![];
    for i in 0..N {
        let block = &blocks[i];
        let reward = &rewards[i];
        let mut parallel_manager = ParallelManager::new(state.clone());
        update_envinfo_by_header(&mut env_info, &block.header);
        let mut txs = vec![];
        for utx in &block.transactions {
            txs.push(SignedTransaction::new(utx.clone()).unwrap());
        }
        parallel_manager.add_env_info(env_info.clone());
        parallel_manager.add_transactions(txs.clone());
        parallel_manager.add_reward(reward.clone());
        parallel_manager.clone_to_secure();

        parallel_managers.push(parallel_manager);
    }

    let input = BenchInput {
        state: state,
        blocks: blocks,
        rewards: rewards,
        last_hashes: last_hashes.into(),
        parallel_managers: parallel_managers,
    };
    c.bench_functions("real_data", funs, input);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(5);
    targets = bench
}
