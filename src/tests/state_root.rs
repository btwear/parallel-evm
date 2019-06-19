use crate::parallel_manager::ParallelManager;
use crate::test_helpers;
use crate::types::Reward;
use ethcore::factory::Factories;
use ethcore::open_state::State;
use ethereum_types::{H256, U256};
use std::collections::VecDeque;

#[test]
fn reproduce_7840001_state_root_parallel() {
    let n = 5;
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
    let factories = Factories::default();

    let state =
        State::from_existing(state_db, state_root_7840000, U256::from(0), factories).unwrap();

    let mut parallel_manager = ParallelManager::new(state, last_hashes.clone().into());
    parallel_manager.add_engines(2);
    for i in 0..n {
        let block = &blocks[i];
        let reward = &rewards[i];
        parallel_manager.push_block_and_reward(block.clone(), reward.clone());
        parallel_manager.step_one_block();
        println!("{:?}", parallel_manager.root());
    }

    parallel_manager.stop();
}
