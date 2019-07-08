use crate::execution_engine::ExecutionEngine;
use crate::secure_engine::SecureEngine;
use crate::test_helpers;
use crate::types::Reward;
use common_types::block::Block;
use common_types::transaction::Action;
use ethcore::open_state::{CleanupMode, State};
use ethcore::open_state_db::StateDB;
use ethereum_types::{Address, H256, U256};
use ethkey::public_to_address;
use hashbrown::HashMap;
use parking_lot::RwLock;
use std::ops::Deref;
use std::sync::Arc;
use vm::EnvInfo;

pub struct ParallelManager {
    // transactions
    blocks: Vec<Arc<RwLock<Block>>>,
    rewards: Vec<Reward>,
    current_env_info: Arc<RwLock<EnvInfo>>,

    // for state
    state_db: StateDB,
    state_root: H256,

    // for parallel execution
    dependency_table: HashMap<Address, usize>,
    engines: Vec<ExecutionEngine>,
    best_thread: usize,
    threads: usize,

    // secure thread
    secure_engine: SecureEngine,
    race: usize,
    tx_delay: usize,
}

impl ParallelManager {
    pub fn new(state: State<StateDB>, last_hashes: Vec<H256>, tx_delay: usize) -> Self {
        let (root, state_db) = state.clone().drop();
        let env_info: Arc<RwLock<EnvInfo>> = Default::default();
        {
            let mut env_info = env_info.write();
            env_info.last_hashes = Arc::new(last_hashes.clone());
        }
        ParallelManager {
            blocks: vec![],
            rewards: vec![],
            current_env_info: env_info.clone(),
            state_db: state_db,
            state_root: root,
            dependency_table: HashMap::new(),
            engines: vec![],
            best_thread: 0,
            threads: 0,
            secure_engine: SecureEngine::start(env_info, tx_delay),
            race: 0,
            tx_delay: tx_delay,
        }
    }

    pub fn push_block(&mut self, block: Block) {
        self.blocks.push(Arc::new(RwLock::new(block)));
    }

    pub fn push_block_arc(&mut self, block: Arc<RwLock<Block>>) {
        self.blocks.push(block);
    }

    pub fn push_block_and_reward_arc(&mut self, block: Arc<RwLock<Block>>, reward: Reward) {
        self.blocks.push(block);
        self.rewards.push(reward);
    }

    pub fn assign_block_and_reward_arc(
        &mut self,
        blocks: Vec<Arc<RwLock<Block>>>,
        rewards: Vec<Reward>,
    ) {
        self.blocks = blocks;
        self.rewards = rewards;
    }

    pub fn push_block_and_reward(&mut self, block: Block, reward: Reward) {
        self.blocks.push(Arc::new(RwLock::new(block)));
        self.rewards.push(reward);
    }

    pub fn add_engines(&mut self, engines: usize) {
        for _ in 0..engines {
            self.engines.push(ExecutionEngine::start(
                self.threads,
                self.current_env_info.clone(),
                self.tx_delay,
            ));
            self.threads += 1;
        }
    }

    fn apply_reward(&mut self) {
        if self.rewards.is_empty() {
            return;
        }

        let (miner, reward, uncles) = self.rewards.remove(0).drop();
        let mut state = self.state();
        state
            .add_balance(&miner.into(), &reward.into(), CleanupMode::NoEmpty)
            .unwrap();

        for uncle in uncles {
            let (miner, reward) = uncle.drop();
            state
                .add_balance(&miner.into(), &reward.into(), CleanupMode::NoEmpty)
                .unwrap();
        }
        state.commit().unwrap();
        let (root, state_db) = state.drop();
        self.state_root = root;
        self.state_db = state_db;
    }

    fn state(&self) -> State<StateDB> {
        State::from_existing(
            self.state_db.boxed_clone(),
            self.state_root.clone(),
            U256::zero(),
            Default::default(),
        )
        .unwrap()
    }

    pub fn step_one_block(&mut self) {
        if self.blocks.is_empty() {
            return;
        }

        let block_lock = self.blocks.remove(0);
        let real_block = &*block_lock.read();
        {
            let mut env_info = self.current_env_info.write();
            test_helpers::update_envinfo_by_header(&mut env_info, &real_block.header);
        }
        // Give block lock and state to all engines
        for engine in &self.engines {
            engine.begin_block(self.state(), block_lock.clone());
        }
        self.secure_engine
            .begin_block(self.state(), block_lock.clone());

        // Process transactions
        let transactions = &real_block.transactions;
        for i in 0..transactions.len() {
            let utx = &transactions[i];
            let sender = public_to_address(&utx.recover_public().unwrap());
            let to = match utx.deref().action {
                Action::Create => None,
                Action::Call(addr) => Some(addr),
            };
            let exec_tid = self.get_exec_tid(&sender, &to);
            self.engines[exec_tid].transact(i);
        }

        for engine in &self.engines {
            engine.end_block();
        }

        let mut engine_states = vec![];
        let mut data_races = self.engines.is_empty();
        for (engine_number, engine) in self.engines.iter().enumerate() {
            let (state, call_addr) = engine.wait_state_and_call_addr();
            if data_races {
                continue;
            }
            for addr in call_addr {
                if let Some(id) = self.dependency_table.get(&addr) {
                    if id != &engine_number {
                        data_races = true;
                        break;
                    }
                } else {
                    self.dependency_table.insert(addr, engine_number);
                }
            }
            engine_states.push(state);
        }
        self.dependency_table.clear();

        if data_races {
            self.apply_secure();
            self.race += 1;
        } else {
            self.secure_engine.terminate();
            self.apply_states(engine_states);
        }

        self.apply_reward();
    }

    pub fn state_root(&self) -> &H256 {
        &self.state_root
    }

    pub fn apply_states(&mut self, mut states: Vec<State<StateDB>>) {
        while let Some(mut state) = states.pop() {
            state
                .commit_external(&mut self.state_db, &mut self.state_root, true)
                .unwrap();
        }
    }

    pub fn apply_secure(&mut self) {
        let mut state = self.secure_engine.end_block();
        state
            .commit_external(&mut self.state_db, &mut self.state_root, true)
            .unwrap();
    }

    pub fn stop(mut self) -> (H256, StateDB) {
        while let Some(engine) = self.engines.pop() {
            engine.stop();
        }
        self.secure_engine.stop();
        (self.state_root, self.state_db)
    }

    #[inline(always)]
    fn get_exec_tid(&mut self, sender: &Address, to: &Option<Address>) -> usize {
        let mut dependency_level = 0;
        let mut dependency_tid = [0, 0];
        // address need to be insert to dependency table, possibly
        // ethereum address of transaction sender and receiver.
        let mut insert_addr = [Some(sender.clone()), to.clone()];
        // Find static dependency between threads, and count the
        // dependency level.
        for i in 0..2 {
            if let Some(addr) = insert_addr[i].as_ref() {
                match self.dependency_table.get(addr) {
                    Some(tid) => {
                        dependency_tid[i] = *tid;
                        dependency_level = dependency_level + i + 1;
                        insert_addr[i] = None;
                    }
                    None => (),
                }
            }
        }
        let mut exec_tid = self.best_thread;
        if dependency_level == 1
            || dependency_level == 2
            || (dependency_level == 3 && dependency_tid[0] == dependency_tid[1])
        {
            // If single dependency
            if dependency_level == 3 {
                dependency_level = 2;
            }
            exec_tid = dependency_tid[dependency_level - 1];
        } else if dependency_level == 3 {
            // 1. If double dependency, send DropAddress signal to sender_tid
            // 2. Wait for the address cache from sender_tid, and transfer address cache to to_tid
            let drop_tid;
            drop_tid = dependency_tid[0];
            exec_tid = dependency_tid[1];

            let cache_channel_tx = self.engines[exec_tid].cache_channel_tx();
            self.engines[drop_tid].send_cache(sender.clone(), cache_channel_tx);
            self.engines[exec_tid].wait_cache(sender.clone());
            insert_addr[0] = Some(sender.clone());
        }
        // Update dependency table
        for i in 0..2 {
            if let Some(addr) = insert_addr[i] {
                self.dependency_table.insert(addr, exec_tid);
            }
        }

        if self.best_thread == exec_tid {
            self.best_thread = (self.best_thread + 1) % self.engines.len();
        }

        exec_tid
    }
}
