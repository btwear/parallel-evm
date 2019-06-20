use crate::execution_engine::ExecutionEngine;
use crate::secure_engine::SecureEngine;
use crate::types::Reward;
use common_types::block::Block;
use common_types::transaction::Action;
use ethcore::open_state::{CleanupMode, State};
use ethcore::open_state_db::StateDB;
use ethereum_types::{Address, H256, U256};
use ethkey::public_to_address;
use hashbrown::HashMap;
use parking_lot::RwLock;
use std::clone::Clone;
use std::ops::Deref;
use std::sync::Arc;
use vm::EnvInfo;

pub struct ParallelManager {
    // transactions
    blocks: Vec<Arc<RwLock<Block>>>,
    rewards: Vec<Reward>,
    initial_last_hashes: Vec<H256>,

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
}

impl ParallelManager {
    pub fn new(state: State<StateDB>, last_hashes: Vec<H256>) -> Self {
        let (root, state_db) = state.clone().drop();
        let mut initial_env_info: EnvInfo = Default::default();
        initial_env_info.last_hashes = Arc::new(last_hashes.clone());
        ParallelManager {
            blocks: vec![],
            rewards: vec![],
            initial_last_hashes: last_hashes,
            state_db: state_db,
            state_root: root,
            dependency_table: HashMap::new(),
            engines: vec![],
            best_thread: 0,
            threads: 0,
            secure_engine: SecureEngine::start(initial_env_info),
        }
    }

    pub fn push_block(&mut self, block: Block) {
        self.blocks.push(Arc::new(RwLock::new(block)));
    }

    pub fn push_block_and_reward(&mut self, block: Block, reward: Reward) {
        self.blocks.push(Arc::new(RwLock::new(block)));
        self.rewards.push(reward);
    }

    pub fn add_engines(&mut self, engines: usize) {
        for _ in 0..engines {
            let mut env_info: EnvInfo = Default::default();
            env_info.last_hashes = Arc::new(self.initial_last_hashes.clone());
            self.engines
                .push(ExecutionEngine::start(self.threads, env_info));
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
        state
            .commit_external(&mut self.state_db, &mut self.state_root, true)
            .unwrap();
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

        let block = self.blocks.remove(0);
        // Give block lock and state to all engines
        for engine in &self.engines {
            engine.begin_block(self.state(), block.clone());
        }
        self.secure_engine.begin_block(self.state(), block.clone());

        // Process transactions
        let block = &*block.read();
        let transactions = &block.transactions;
        for i in 0..transactions.len() {
            let utx = &transactions[i];
            let sender = public_to_address(&utx.recover_public().unwrap());
            let to = match utx.deref().action {
                Action::Create => Address::zero(),
                Action::Call(addr) => addr,
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

        if data_races {
            self.apply_secure();
        } else {
            self.apply_states(engine_states);
        }

        self.apply_reward();
        self.dependency_table.clear();
    }

    fn get_exec_tid(&mut self, sender: &Address, to: &Address) -> usize {
        let mut dependency_level = 0;
        // dependency thread id.
        let mut dependency_tid = [0, 0];
        // address need to be insert to dependency table, possibly
        // ethereum address of transaction sender and receiver.
        let mut insert_addr = [sender.clone(), to.clone()];

        // Find static dependency between threads, and count the
        // dependency level.
        for i in 0..2 {
            match self.dependency_table.get(&insert_addr[i]) {
                Some(tid) => {
                    dependency_tid[i] = *tid;
                    dependency_level = dependency_level + i + 1;
                    insert_addr[i] = Address::zero();
                }
                None => (),
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
            insert_addr[0] = sender.clone();
        }

        // Update dependency table
        for i in 0..2 {
            if insert_addr[i] != Address::zero() {
                self.dependency_table.insert(insert_addr[i], exec_tid);
            }
        }

        if self.best_thread == exec_tid {
            self.best_thread = (self.best_thread + 1) % self.engines.len();
        }

        exec_tid
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
        self.secure_engine.end_block();
        let mut state = self.secure_engine.wait_state();
        state
            .commit_external(&mut self.state_db, &mut self.state_root, true)
            .unwrap();
    }

    pub fn root(&self) -> H256 {
        self.state_root
    }

    pub fn stop(mut self) {
        while let Some(engine) = self.engines.pop() {
            engine.stop();
        }
        self.secure_engine.stop();
    }
}
