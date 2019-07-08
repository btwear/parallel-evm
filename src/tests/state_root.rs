use crate::parallel_manager::ParallelManager;
use crate::test_helpers;
use crate::types::Reward;
use ethcore::factory::Factories;
use ethcore::open_state::State;
use ethereum_types::{H256, U256};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn reproduce_7840001_state_root_parallel() {
    let n = 10000;
    let db_dir = "/tmp/tmp_eth_db";
    let block_dir = "/tmp/res/blocks/7840001_7850000.bin";
    let reward_dir = "/tmp/res/rewards/7840001_7850000.json";
    let last_hashes_dir = "/tmp/res/lastHashes7840001";
    let state_root_7840000 =
        H256::from("0xee45b8d18c5d1993cbd6b985cd2ed2f437f9a29ef89c75cd1dc24e352993a77c");

    for threads in 16..17 {
        let state_db = test_helpers::open_state_db(db_dir);
        let blocks = test_helpers::read_blocks(block_dir, 1, n)
            .into_iter()
            .map(|b| Arc::new(RwLock::new(b)))
            .collect();
        let rewards = Reward::from_file(reward_dir, 1, n);
        let mut last_hashes = VecDeque::from(test_helpers::load_last_hashes(last_hashes_dir));
        last_hashes.pop_front();
        last_hashes.resize(256, H256::zero());
        let factories = Factories::default();

        let state =
            State::from_existing(state_db, state_root_7840000, U256::from(0), factories).unwrap();

        let now = Instant::now();
        let mut parallel_manager = ParallelManager::new(state, last_hashes.clone().into(), 0);
        parallel_manager.add_engines(threads);
        parallel_manager.assign_block_and_reward_arc(blocks, rewards);
        for i in 0..n {
            parallel_manager.step_one_block();
            if (i + 1) % 1000 == 0 {
                println!("#{}", i + 1);
            }
        }

        parallel_manager.stop();
        println!("Elapsed time: {}", now.elapsed().as_secs());
        break;
    }
}
