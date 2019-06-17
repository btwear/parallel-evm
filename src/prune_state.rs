use crate::reward::Reward;
use crate::test_helpers;
use common_types::transaction::SignedTransaction;
use ethcore::ethereum;
use ethcore::factory::Factories;
use ethcore::open_state::{Account, Backend, CleanupMode, State};
use ethcore::open_state_db::StateDB;
use ethcore::test_helpers as eth_helpers;
use ethereum_types::{Address, H256, U256};
use kvdb::{DBOp, DBTransaction};
use std::collections::VecDeque;
use std::sync::Arc;
use vm::EnvInfo;

#[test]
fn save_account_to_db() {
    let n = 10000;

    let new_db_path = "/tmp/tmp_eth_db";
    if ::std::path::Path::new(new_db_path).exists() {
        println!("DB already exists");
        return;
    }
    let db_path = "res/db_7840000";
    let block_dir = "res/blocks/7840001_7850000.bin";
    let reward_dir = "res/rewards/7840001_7850000.json";
    let last_hashes_dir = "res/lastHashes7840001";
    let state_root_7840000 =
        H256::from("0xa7ca2c04e692960dac04909b3212baf12df7666efac68afad4646b3205a32c91");

    println!("Opening database...");
    let state_db = test_helpers::open_state_db(db_path);
    println!("Loading blocks...");
    let blocks = &test_helpers::read_blocks(block_dir, 1, n);
    println!("Loading rewards...");
    let rewards = &mut Reward::from_file(reward_dir, 1, n);
    let mut last_hashes = VecDeque::from(test_helpers::load_last_hashes(last_hashes_dir));
    last_hashes.pop_front();
    last_hashes.resize(256, H256::zero());

    let machine = ethereum::new_constantinople_fix_test_machine();
    let factories = Factories::default();

    let mut state = State::from_existing(
        state_db,
        state_root_7840000,
        U256::zero(),
        factories.clone(),
    )
    .unwrap();

    state.checkpoint();

    use std::time::{Duration, SystemTime};
    let mut time = SystemTime::now();

    for i in 0..n {
        let block = &blocks[i];
        let reward = &rewards[i];
        let mut env_info = test_helpers::header_to_envinfo(&block.header);
        env_info.last_hashes = Arc::new(test_helpers::load_last_hashes(last_hashes_dir));
        last_hashes.push_front(block.header.parent_hash().clone());
        last_hashes.pop_back();
        env_info.last_hashes = Arc::new(last_hashes.clone().into());
        for utx in &block.transactions {
            let tx = SignedTransaction::new(utx.clone()).unwrap();
            let outcome = state.apply(&env_info, &machine, &tx, true);
            match outcome {
                Err(err) => {
                    println!("[error] {}", err);
                    println!("in block #{}", 7840001 + i);
                    println!("{:?}", utx.hash());
                    panic!();
                }
                Ok(out) => env_info.gas_used = out.receipt.gas_used,
            }
        }

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
        if (i + 1) % 20 == 0 {
            println!("block #{} done", 7840001 + i);
            println!("time: {}", time.elapsed().unwrap().as_secs());
            println!("gas used {:?}", env_info.gas_used);
            time = SystemTime::now();
        }
    }

    state.revert_to_checkpoint();

    let new_root = {
        let db = test_helpers::open_database(new_db_path);
        let journal_db = ::journaldb::new(
            db.key_value().clone(),
            ::journaldb::Algorithm::EarlyMerge,
            ::ethcore_db::COL_STATE,
        );
        let mut new_state_db = StateDB::new(journal_db, 10 * 1024 * 1024);
        let mut new_state = State::new(new_state_db, U256::zero(), factories.clone());
        new_state.set_cache(state.drop_cache());
        new_state.clear_accounts_storage_root();
        new_state.commit();

        let (new_root, mut new_state_db) = new_state.drop();

        let mut batch = DBTransaction::new();
        new_state_db
            .journal_under(&mut batch, 0, &H256::random())
            .unwrap();
        db.key_value().write(batch).unwrap();
        new_root
    };

    let state_root_path = format!("{}/{}", new_db_path, "state_root.txt");
    let new_root_string = format!("{:?}", new_root);
    ::std::fs::write(state_root_path, new_root_string);
    println!("New state root: {:?}", new_root);

    let state_db = test_helpers::open_state_db(new_db_path);
    let mut state =
        State::from_existing(state_db, new_root, U256::zero(), Default::default()).unwrap();
    let mut last_hashes = VecDeque::from(test_helpers::load_last_hashes(last_hashes_dir));
    last_hashes.pop_front();
    last_hashes.resize(256, H256::zero());

    for i in 0..n {
        let block = &blocks[i];
        let reward = &rewards[i];
        let mut env_info = test_helpers::header_to_envinfo(&block.header);
        env_info.last_hashes = Arc::new(test_helpers::load_last_hashes(last_hashes_dir));
        last_hashes.push_front(block.header.parent_hash().clone());
        last_hashes.pop_back();
        env_info.last_hashes = Arc::new(last_hashes.clone().into());
        for utx in &block.transactions {
            let tx = SignedTransaction::new(utx.clone()).unwrap();
            let outcome = state.apply(&env_info, &machine, &tx, true).unwrap();
            env_info.gas_used = outcome.receipt.gas_used;
        }

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
        println!("gas used {:?}", env_info.gas_used);
    }
}

#[test]
fn revert_apply_get_cached() {
    let address = Address::from(1025534);
    let key = H256::from(3648519);
    let value = H256::from(255);
    let mut state = test_helpers::get_temp_state();
    state
        .set_storage(&address, key.clone(), value.clone())
        .unwrap();
    state.commit();
}

#[test]
fn load_single_account() {
    let db_path = "res/db_7840000";
    let state_db = test_helpers::open_state_db(db_path);
    let state_root =
        H256::from("0xa7ca2c04e692960dac04909b3212baf12df7666efac68afad4646b3205a32c91");
    let address = Address::from("0xD1CEeeeee83F8bCF3BEDad437202b6154E9F5405");

    let factories = Factories::default();
    let db = state_db.as_hash_db();
    // let trie_db = factories.trie.readonly(db, &state_root);
    // let from_rlp = |b: &[u8]| Account::from_rlp(b).expect("decoding db value failed");

    println!(
        "{:#?}",
        state_db.get_cached_account(&address).unwrap().unwrap()
    );
}

use std::io::{stdin, stdout, Read, Write};

fn pause() {
    let mut stdout = stdout();
    stdout.write(b"Press Enter to continue...").unwrap();
    stdout.flush().unwrap();
    stdin().read(&mut [0]).unwrap();
}
