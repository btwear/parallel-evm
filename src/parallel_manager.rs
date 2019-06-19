use crate::execution_engine::{ExecutionEngine, ExecutionEvent, SecureEngine};
use crate::reward::Reward;
use common_types::transaction::{Action, SignedTransaction};
use ethcore::factory::Factories;
use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use ethereum_types::{Address, H256, U256};
use hashbrown::HashMap;
use std::clone::Clone;
use std::ops::Deref;
use vm::EnvInfo;

pub struct ParallelManager {
    // transactions
    events: Vec<ExecutionEvent>,

    // for state
    state_db: StateDB,
    state_root: H256,
    factories: Factories,

    // for parallel execution
    dependency_table: HashMap<Address, usize>,
    engines: Vec<ExecutionEngine>,
    best_thread: usize,
    threads: usize,
    engine_states: Vec<State<StateDB>>,

    // secure thread
    secure_engine: SecureEngine,
}

impl Clone for ParallelManager {
    fn clone(&self) -> Self {
        let state = self.state();
        let secure_engine = SecureEngine::new(state);
        ParallelManager {
            events: self.events.clone(),
            state_db: self.state_db.boxed_clone(),
            state_root: self.state_root.clone(),
            factories: self.factories.clone(),
            dependency_table: HashMap::new(),
            engines: vec![],
            engine_states: vec![],
            best_thread: 0,
            threads: 0,
            secure_engine: secure_engine,
        }
    }
}

impl ParallelManager {
    pub fn new(state: State<StateDB>) -> ParallelManager {
        let (root, state_db) = state.clone().drop();
        ParallelManager {
            events: vec![],
            state_db: state_db,
            state_root: root,
            factories: Factories::default(),
            dependency_table: HashMap::new(),
            engines: vec![],
            engine_states: vec![],
            best_thread: 0,
            threads: 0,
            secure_engine: SecureEngine::new(state),
        }
    }

    pub fn set_state(&mut self, state: State<StateDB>) {
        let (root, state_db) = state.drop();
        self.state_root = root;
        self.state_db = state_db;
    }

    pub fn add_transactions(&mut self, mut txs: Vec<SignedTransaction>) {
        while !txs.is_empty() {
            self.events.push(ExecutionEvent::Transact(txs.remove(0)));
        }
    }

    pub fn add_reward(&mut self, reward: &Reward) {
        self.events.push(ExecutionEvent::AddBalance(
            reward.miner.clone().into(),
            reward.reward.clone().into(),
        ));
        for uncle in &reward.uncles {
            self.events.push(ExecutionEvent::AddBalance(
                uncle.miner.clone().into(),
                uncle.reward.clone().into(),
            ));
        }
    }

    pub fn add_env_info(&mut self, env_info: EnvInfo) {
        self.events.push(ExecutionEvent::ChangeEnv(env_info));
    }

    pub fn clone_to_secure(&mut self) {
        self.secure_engine.get_events(self.events.clone());
    }

    pub fn add_engines(&mut self, number: usize) {
        for i in 0..number {
            self.engines.push(ExecutionEngine::start(self.state(), i));
        }
    }

    pub fn state(&self) -> State<StateDB> {
        State::from_existing(
            self.state_db.boxed_clone_canon(&self.state_root()),
            self.state_root().clone(),
            U256::from(0),
            self.factories.clone(),
        )
        .unwrap()
    }

    pub fn consume(&mut self) {
        self.secure_engine.run();
        if self.engines.is_empty() {
            return;
        }
        for event in self.events.clone() {
            match event {
                ExecutionEvent::Transact(tx) => {
                    let to = match tx.deref().deref().action {
                        Action::Create => Address::zero(),
                        Action::Call(addr) => addr,
                    };
                    let exec_tid = self.get_exec_tid(&tx.sender(), &to);
                    self.engines[exec_tid].push_transaction(tx.clone());
                }
                ExecutionEvent::AddBalance(addr, amount) => {
                    let exec_tid = self.get_exec_tid(&addr, &Address::zero());
                    self.engines[exec_tid].push_add_balance(addr, amount);
                }
                ExecutionEvent::ChangeEnv(env_info) => {
                    for engine in &self.engines {
                        engine.push_env(env_info.clone());
                    }
                }
                _ => (),
            }
        }
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

    pub fn set_root(&mut self, root: H256) {
        self.state_root = root;
    }

    pub fn stop(&mut self) -> bool {
        let mut data_races = self.engines.is_empty();
        while let Some(engine) = self.engines.pop() {
            let engine_number = self.engines.len();
            let (state, internal_address) = engine.stop();
            if data_races {
                continue;
            }
            for addr in internal_address {
                if let Some(id) = self.dependency_table.get(&addr) {
                    if id != &engine_number {
                        data_races = true;
                        self.engine_states = vec![];
                        break;
                    }
                } else {
                    self.dependency_table.insert(addr, engine_number);
                }
            }
            self.engine_states.push(state);
        }

        data_races
    }

    pub fn apply_engines(&mut self) {
        self.secure_engine.terminate();
        while let Some(mut state) = self.engine_states.pop() {
            state
                .commit_external(&mut self.state_db, &mut self.state_root, true)
                .unwrap();
        }
    }

    pub fn apply_secure(&mut self) {
        let mut state = self.secure_engine.join();
        state
            .commit_external(&mut self.state_db, &mut self.state_root, true)
            .unwrap();
        self.engine_states = vec![];
    }

    pub fn drop(self) -> State<StateDB> {
        self.state()
    }

    pub fn root(&self) -> H256 {
        self.state_root
    }
}

#[cfg(test)]
mod tests {
    extern crate env_logger;
    use super::*;
    use crate::execution_engine::sequential_exec;
    use crate::test_helpers;
    use ethcore::open_state::CleanupMode;
    use std::io::Write;

    #[test]
    fn test_static_dependency_100_4() {
        let transactions = test_helpers::static_dep_txs(50, 100, true);
        test_static_dependency(&transactions, 4);
    }

    fn test_static_dependency(transactions: &Vec<SignedTransaction>, engines: usize) {
        init("SD");
        // Set up state db
        let mut state = test_helpers::get_temp_state();

        for tx in transactions {
            state
                .add_balance(&tx.sender(), &U256::from(1), CleanupMode::NoEmpty)
                .unwrap();
        }
        state.commit().unwrap();

        // initiate PM
        let mut parallel_manager = ParallelManager::new(state);
        parallel_manager.add_engines(engines);
        parallel_manager.add_transactions(transactions.clone());
        parallel_manager.clone_to_secure();
        parallel_manager.consume();
        parallel_manager.stop();

        // Sequential execution
        let mut state = test_helpers::get_temp_state();
        for tx in transactions {
            state
                .add_balance(&tx.sender(), &U256::from(1), CleanupMode::NoEmpty)
                .unwrap();
        }

        sequential_exec(&mut state, &transactions);
        state.commit().unwrap();

        assert_eq!(state.root(), parallel_manager.state_root());
    }

    fn init(test_name: &'static str) {
        env_logger::builder()
            .default_format_timestamp(false)
            .default_format_module_path(false)
            .format(move |buf, record| writeln!(buf, "[{}] {}", test_name, record.args()))
            .init();
    }
}
