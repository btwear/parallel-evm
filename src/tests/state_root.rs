use crate::parallel_manager::ParallelManager;
use crate::reward::Reward;
use crate::test_helpers;
use common_types::transaction::SignedTransaction;
use ethcore::ethereum;
use ethcore::factory::Factories;
use ethcore::open_state::{CleanupMode, State};
use ethcore::open_state_db::StateDB;
use ethereum_types::{H256, U256};
use std::collections::VecDeque;
use std::sync::Arc;

#[test]
fn reproduce_7840001_state_root_parallel() {
    let n = 1;
    let db_dir = "res/db_7840000";
    let block_dir = "res/blocks/7840001_7850000.bin";
    let reward_dir = "res/rewards/7840001_7850000.json";
    let last_hashes_dir = "res/lastHashes7840001";
    let state_root_7840000 =
        H256::from("0xa7ca2c04e692960dac04909b3212baf12df7666efac68afad4646b3205a32c91");

    let state_db = test_helpers::open_state_db(db_dir);
    let blocks = &test_helpers::read_blocks(block_dir, 1, n);
    let rewards = &Reward::from_file(reward_dir, 1, n);
    let mut last_hashes = VecDeque::from(test_helpers::load_last_hashes(last_hashes_dir));
    last_hashes.pop_front();
    last_hashes.resize(256, H256::zero());
    let machine = ethereum::new_constantinople_fix_test_machine();
    let factories = Factories::default();

    let mut state =
        State::from_existing(state_db, state_root_7840000, U256::from(0), factories).unwrap();

    let mut n_race = 0;
    for i in 0..n {
        let mut env_info = test_helpers::header_to_envinfo(&blocks[i].header);
        println!("Processing block #{}", env_info.number);
        last_hashes.push_front(blocks[i].header.parent_hash().clone());
        last_hashes.pop_back();
        env_info.last_hashes = Arc::new(last_hashes.clone().into());
        let mut parallel_manager = ParallelManager::new(state.clone());
        let mut txs = vec![];
        for utx in &blocks[i].transactions {
            txs.push(SignedTransaction::new(utx.clone()).unwrap());
        }
        parallel_manager.add_engines(4);
        parallel_manager.add_env_info(env_info.clone());
        parallel_manager.add_transactions(txs);
        parallel_manager.add_reward(&rewards[i]);
        parallel_manager.clone_to_secure();
        parallel_manager.consume();
        if parallel_manager.stop() {
            n_race += 1;
        }
        state = parallel_manager.drop();
        println!("{:?}", state.root());
    }

    state.commit().unwrap();
    println!("Data race count: {:?}", n_race);
    println!("{:?}", state.root());
}

#[test]
fn reproduce_7840001_state_root() {
    let db_dir = "res/db_7840000";
    let block_dir = "res/blocks/7840001_7850000.bin";
    let reward_dir = "res/rewards/7840001_7850000.json";
    let last_hashes_dir = "res/lastHashes7840001";
    let state_root_7840000 =
        H256::from("0xa7ca2c04e692960dac04909b3212baf12df7666efac68afad4646b3205a32c91");

    let state_db = test_helpers::open_state_db(db_dir);
    let block = &test_helpers::read_blocks(block_dir, 1, 1)[0];
    let reward = &Reward::from_file(reward_dir, 1, 1)[0];
    let machine = ethereum::new_constantinople_fix_test_machine();
    let mut env_info = test_helpers::header_to_envinfo(&block.header);
    env_info.last_hashes = Arc::new(test_helpers::load_last_hashes(last_hashes_dir));
    let factories = Factories::default();

    let mut state =
        State::from_existing(state_db, state_root_7840000, U256::from(0), factories).unwrap();

    // Execute transactions
    for utx in &block.transactions {
        let mut tx = SignedTransaction::new(utx.clone()).unwrap();
        let outcome = state.apply(&env_info, &machine, &tx, true).unwrap();
        env_info.gas_used = outcome.receipt.gas_used;
    }

    // Apply block rewards
    state.add_balance(
        &reward.miner.clone().into(),
        &reward.reward.into(),
        CleanupMode::NoEmpty,
    );

    state.commit().unwrap();
    println!("{:?}", state.root());
}

#[test]
fn playground() {}
