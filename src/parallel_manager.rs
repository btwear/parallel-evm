extern crate log;
use crate::execution_engine::ExecutionEngine;
use common_types::transaction::{Action, SignedTransaction};
use ethcore::factory::Factories;
use ethcore::open_state::{AccountEntry, State};
use ethcore::open_state_db::StateDB;
use ethereum_types::{Address, H256, U256};
use std::collections::HashMap;
use std::ops::Deref;

pub enum ParallelEvent {
    CacheBack(AccountEntry),
    DependencyCheck(Vec<Address>),
}

pub struct ParallelManager {
    // for state
    state_db: StateDB,
    state_root: H256,
    factories: Factories,

    // for parallel execution
    dependency_table: HashMap<Address, usize>,
    engines: Vec<ExecutionEngine>,
    best_thread: usize,
    threads: usize,
}

impl ParallelManager {
    pub fn new(state_db: StateDB, state_root: H256, factories: Factories) -> ParallelManager {
        ParallelManager {
            state_db: state_db,
            state_root: state_root,
            factories: factories,
            dependency_table: HashMap::new(),
            engines: vec![],
            best_thread: 0,
            threads: 0,
        }
    }

    pub fn add_engines(&mut self, number: usize) {
        let state = self.state();
        for i in 0..number {
            self.add_engine(state.clone(), i);
        }
    }

    pub fn add_engine(&mut self, state: State<StateDB>, id: usize) {
        self.engines.push(ExecutionEngine::start(state, id));
        self.threads = self.threads + 1;
    }

    fn state(&self) -> State<StateDB> {
        State::from_existing(
            self.state_db.boxed_clone_canon(&self.state_root()),
            self.state_root().clone(),
            U256::from(0),
            self.factories.clone(),
        )
        .unwrap()
    }

    pub fn assign_tx(&mut self, tx: &SignedTransaction) {
        // dependency level:
        //  0: no static dependency
        //  1: single dependency in dependency_tid[0]
        //  2: single dependency in dependency_tid[1]
        //  3: single or double dependency, depend on whether dependency_tid are different
        let mut dependency_level = 0;
        // dependency thread id.
        let mut dependency_tid = [0, 0];
        // address need to be insert to dependency table, possibly
        // ethereum address of transaction sender and receiver.
        let mut insert_addr = [Address::zero(); 2];
        insert_addr[0] = tx.sender();
        insert_addr[1] = match tx.deref().deref().action {
            Action::Create => Address::zero(),
            Action::Call(addr) => addr,
        };

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
            self.engines[drop_tid].send_cache(tx.sender(), cache_channel_tx);
            self.engines[exec_tid].wait_cache(tx.sender());
            insert_addr[0] = tx.sender();
        }

        // Update dependency table
        for i in 0..2 {
            if insert_addr[i] != Address::zero() {
                self.dependency_table.insert(insert_addr[i], exec_tid);
            }
        }

        if self.best_thread == exec_tid {
            self.best_thread = (self.best_thread + 1) % self.threads;
        }

        self.engines[exec_tid].push_transaction(tx.clone());
    }

    pub fn state_root(&self) -> &H256 {
        &self.state_root
    }

    pub fn set_root(&mut self, root: H256) {
        self.state_root = root;
    }

    pub fn stop(&mut self) {
        while let Some(engine) = self.engines.pop() {
            let mut state = engine.stop();
            state
                .commit_external(&mut self.state_db, &mut self.state_root)
                .unwrap();
        }
        self.threads = 0;
    }

    pub fn root(&self) -> H256 {
        self.state_root
    }
}

#[cfg(test)]
mod test {
    extern crate env_logger;
    use super::*;
    use crate::execution_engine::sequential_exec;
    use crate::test_helpers;
    use ethcore::open_state::CleanupMode;
    use std::io::Write;

    #[test]
    fn test_static_dependency() {
        init("SD");
        // Set up state db
        let mut state = test_helpers::get_temp_state();
        let transactions = test_helpers::static_dep_txs(50, 100, true);

        for tx in &transactions {
            state
                .add_balance(&tx.sender(), &U256::from(1), CleanupMode::NoEmpty)
                .unwrap();
        }
        state.commit().unwrap();
        let (root, state_db) = state.drop();

        // initiate PM
        let mut parallel_manager = ParallelManager::new(state_db, root, Factories::default());
        parallel_manager.add_engines(4);
        for tx in &transactions {
            parallel_manager.assign_tx(&tx);
        }
        parallel_manager.stop();

        // Sequential execution
        let mut state = test_helpers::get_temp_state();
        for tx in &transactions {
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
