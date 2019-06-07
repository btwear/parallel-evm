use common_types::transaction::SignedTransaction;
use ethcore::ethereum::new_constantinople_fix_test_machine as machine_generator;
use ethcore::machine::EthereumMachine;
use ethcore::open_state::{AccountEntry, CleanupMode, State};
use ethcore::open_state_db::StateDB;
use ethcore::trace::trace::{Action, Res};
use ethereum_types::{Address, U256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use vm::EnvInfo;

#[derive(Clone)]
pub enum ExecutionEvent {
    Stop,
    Transact(SignedTransaction),
    ChangeEnv(EnvInfo),
    SendCache(Address, Sender<(Address, AccountEntry)>),
    WaitCache(Address),
    AddBalance(Address, U256),
}

pub struct ExecutionEngine {
    execution_channel_tx: Sender<ExecutionEvent>,
    cache_channel_tx: Sender<(Address, AccountEntry)>,
    handler: JoinHandle<(State<StateDB>, Vec<Address>)>,
}

pub struct SecureEngine {
    state: State<StateDB>,
    handler: Option<JoinHandle<State<StateDB>>>,
    running: Option<Weak<AtomicBool>>,
    execution_events: Option<Vec<ExecutionEvent>>,
}

impl ExecutionEngine {
    pub fn start(mut state: State<StateDB>, number: usize) -> ExecutionEngine {
        let (execution_channel_tx, execution_channel_rx) = mpsc::channel();
        let (cache_channel_tx, cache_channel_rx) = mpsc::channel();
        let mut env_info = EnvInfo::default();
        let machine = machine_generator();
        env_info.gas_limit = U256::from(100_000_000);

        let handler = thread::Builder::new()
            .name(format!("{}{}", "engine".to_string(), &number.to_string()))
            .spawn(move || {
                let mut cache_buffer = vec![];
                let mut internal_call_addr = vec![];
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
                            for sub_trace in &trace[1..] {
                                match &sub_trace.action {
                                    Action::Call(call) => internal_call_addr.push(call.to),
                                    Action::Create(_) => match &sub_trace.result {
                                        Res::Create(create) => {
                                            internal_call_addr.push(create.address)
                                        }
                                        _ => (),
                                    },
                                    _ => (),
                                }
                            }
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
                        ExecutionEvent::AddBalance(addr, amount) => {
                            state
                                .add_balance(&addr, &amount, CleanupMode::NoEmpty)
                                .unwrap();
                        }
                    }
                }
                (state, internal_call_addr)
            })
            .unwrap();
        let execution_engine = ExecutionEngine {
            execution_channel_tx: execution_channel_tx,
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

    pub fn push_add_balance(&self, addr: Address, amount: U256) {
        self.execution_channel_tx
            .send(ExecutionEvent::AddBalance(addr, amount))
            .unwrap();
    }

    pub fn push_env(&self, env_info: EnvInfo) {
        self.execution_channel_tx
            .send(ExecutionEvent::ChangeEnv(env_info))
            .unwrap();
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

    pub fn stop(self) -> (State<StateDB>, Vec<Address>) {
        self.execution_channel_tx
            .send(ExecutionEvent::Stop)
            .unwrap();
        self.handler.join().unwrap()
    }

    pub fn cache_channel_tx(&self) -> Sender<(Address, AccountEntry)> {
        self.cache_channel_tx.clone()
    }
}

impl SecureEngine {
    pub fn new(state: State<StateDB>) -> SecureEngine {
        SecureEngine {
            state: state,
            handler: None,
            running: None,
            execution_events: None,
        }
    }

    pub fn run(&mut self) {
        if let Some(events) = self.execution_events.take() {
            let mut env_info = EnvInfo::default();
            env_info.gas_limit = U256::from(100_000_000);
            let running = Arc::new(AtomicBool::new(true));
            let mut state = self.state.clone();
            let machine = machine_generator();
            self.running = Some(Arc::downgrade(&running));
            self.handler = Some(
                thread::Builder::new()
                    .name("secure_engine".to_string())
                    .spawn(move || {
                        for event in events {
                            if running.load(Ordering::Relaxed) {
                                match event {
                                    ExecutionEvent::Transact(tx) => {
                                        state.apply(&env_info, &machine, &tx, false).unwrap();
                                    }
                                    ExecutionEvent::ChangeEnv(env) => env_info = env,
                                    ExecutionEvent::AddBalance(addr, amount) => {
                                        state
                                            .add_balance(&addr, &amount, CleanupMode::NoEmpty)
                                            .unwrap();
                                    }
                                    _ => (),
                                }
                            }
                        }
                        state
                    })
                    .unwrap(),
            );
        }
    }

    // TODO:
    pub fn get_events(&mut self, events: Vec<ExecutionEvent>) {
        self.execution_events = Some(events);
    }

    pub fn add_transactions(&mut self, mut txs: Vec<SignedTransaction>) {
        if let Some(mut events) = self.execution_events.take() {
            while !txs.is_empty() {
                events.push(ExecutionEvent::Transact(txs.remove(0)));
            }
            self.execution_events = Some(events)
        } else {
            panic!("No enviroment information");
        }
    }

    pub fn change_env(&mut self, mut env_info: EnvInfo) {
        if let Some(mut events) = self.execution_events.take() {
            events.push(ExecutionEvent::ChangeEnv(env_info));
        } else {
            self.execution_events = Some(vec![ExecutionEvent::ChangeEnv(env_info)]);
        }
    }

    pub fn join(&mut self) -> State<StateDB> {
        self.handler.take().unwrap().join().unwrap()
    }

    pub fn terminate(&mut self) {
        if let Some(running) = self.running.take() {
            match running.upgrade() {
                Some(running) => {
                    (*running).store(false, Ordering::Relaxed);
                    self.handler.take().unwrap().join().unwrap();
                }
                None => (),
            }
        }
    }
}

pub fn sequential_exec(state: &mut State<StateDB>, txs: &Vec<SignedTransaction>) {
    let mut env_info = EnvInfo::default();
    env_info.gas_limit = U256::from(100_000_000);
    let machine = machine_generator();

    for tx in txs {
        state.apply(&env_info, &machine, &tx, false).unwrap();
    }
}
