use common_types::transaction::SignedTransaction;
use ethcore::ethereum::new_frontier_test_machine;
use ethcore::open_state::{AccountEntry, State};
use ethcore::open_state_db::StateDB;
use ethcore::trace::trace::Action;
use ethereum_types::{Address, U256};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use vm::EnvInfo;

enum ExecutionEvent {
    Stop,
    Transact(SignedTransaction),
    DropCache(Address),
    InsertCache(Address, AccountEntry),
    ChangeEnv(EnvInfo),
    SendCache(Address, Sender<(Address, AccountEntry)>),
    WaitCache(Address),
}

pub struct ExecutionEngine {
    execution_channel_tx: Sender<ExecutionEvent>,
    parallel_channel_rx: Receiver<AccountEntry>,
    cache_channel_tx: Sender<(Address, AccountEntry)>,
    handler: JoinHandle<State<StateDB>>,
}

impl ExecutionEngine {
    pub fn start(mut state: State<StateDB>, number: usize) -> ExecutionEngine {
        let (execution_channel_tx, execution_channel_rx) = mpsc::channel();
        let (parallel_channel_tx, parallel_channel_rx) = mpsc::channel();
        let (cache_channel_tx, cache_channel_rx) = mpsc::channel();
        let mut env_info = EnvInfo::default();
        env_info.gas_limit = U256::from(100_000_000);
        let machine = new_frontier_test_machine();

        let handler = thread::Builder::new()
            .name(format!("{}{}", "engine".to_string(), &number.to_string()))
            .spawn(move || {
                let mut cache_buffer = vec![];
                loop {
                    match execution_channel_rx.recv().unwrap() {
                        ExecutionEvent::Stop => {
                            break;
                        }
                        ExecutionEvent::Transact(tx) => {
                            let outcome = state.apply(&env_info, &machine, &tx, true).unwrap();
                            let trace = outcome.trace;
                            // TODO: check CALL
                            // the transaction has internal call
                            if trace.len() > 1 {}
                        }
                        ExecutionEvent::DropCache(addr) => {
                            let account_entry = state.drop_account(&addr);
                            parallel_channel_tx.send(account_entry).unwrap();
                        }
                        ExecutionEvent::InsertCache(addr, account_entry) => {
                            state.insert_cache(&addr, account_entry);
                        }
                        ExecutionEvent::SendCache(addr, cache_channel_tx) => {
                            let account_entry = state.drop_account(&addr);
                            cache_channel_tx.send((addr, account_entry)).unwrap();
                        }
                        ExecutionEvent::WaitCache(addr) => {
                            let mut cached = false;
                            let len = cache_buffer.len();
                            for i in 0..len {
                                if cache_buffer[i] == addr {
                                    cache_buffer.remove(i);
                                    cached = true;
                                    break;
                                }
                            }
                            while !cached {
                                let (_addr, account_entry) = cache_channel_rx.recv().unwrap();
                                state.insert_cache(&_addr, account_entry);
                                if _addr == addr {
                                    cached = true;
                                } else {
                                    cache_buffer.push(_addr);
                                }
                            }
                        }
                        ExecutionEvent::ChangeEnv(new_env_info) => {
                            env_info = new_env_info;
                        }
                    }
                }
                state
            })
            .unwrap();
        let execution_engine = ExecutionEngine {
            execution_channel_tx: execution_channel_tx,
            parallel_channel_rx: parallel_channel_rx,
            cache_channel_tx: cache_channel_tx,
            handler: handler,
        };

        return execution_engine;
    }

    pub fn push_transaction(&self, tx: SignedTransaction) {
        self.execution_channel_tx
            .send(ExecutionEvent::Transact(tx))
            .unwrap();
    }

    pub fn drop_cache(&self, addr: Address) -> AccountEntry {
        self.execution_channel_tx
            .send(ExecutionEvent::DropCache(addr))
            .unwrap();
        self.parallel_channel_rx.recv().unwrap()
    }

    pub fn send_cache(&self, addr: Address, channel_tx: Sender<(Address, AccountEntry)>) {
        self.execution_channel_tx
            .send(ExecutionEvent::SendCache(addr, channel_tx))
            .unwrap();
    }

    pub fn wait_cache(&self, addr: Address) {
        self.execution_channel_tx
            .send(ExecutionEvent::WaitCache(addr))
            .unwrap();
    }

    pub fn insert_cache(&self, addr: Address, cache: AccountEntry) {
        self.execution_channel_tx
            .send(ExecutionEvent::InsertCache(addr, cache))
            .unwrap();
    }

    pub fn stop(self) -> State<StateDB> {
        self.execution_channel_tx
            .send(ExecutionEvent::Stop)
            .unwrap();
        self.handler.join().unwrap()
    }

    pub fn cache_channel_tx(&self) -> Sender<(Address, AccountEntry)> {
        self.cache_channel_tx.clone()
    }
}

pub fn sequential_exec(state: &mut State<StateDB>, txs: &Vec<SignedTransaction>) {
    let mut env_info = EnvInfo::default();
    env_info.gas_limit = U256::from(100_000_000);
    let machine = new_frontier_test_machine();

    for tx in txs {
        state.apply(&env_info, &machine, &tx, false).unwrap();
    }
}
