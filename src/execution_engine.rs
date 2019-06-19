use crate::test_helpers;
use common_types::block::Block;
use common_types::transaction::SignedTransaction;
use crossbeam_channel::{self, unbounded, Receiver, Sender};
use ethcore::ethereum::new_constantinople_fix_test_machine as machine_generator;
use ethcore::open_state::{AccountEntry, State};
use ethcore::open_state_db::StateDB;
use ethcore::trace::trace::{Action, Res};
use ethereum_types::Address;
use parking_lot::RwLock;
use std::mem;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use vm::EnvInfo;

#[derive(Clone)]
pub enum ExecutionEvent {
    Stop,
    Transact(usize),
    BeginBlock(State<StateDB>, Arc<RwLock<Block>>),
    EndBlock,
    SendCache(Address, Sender<(Address, AccountEntry)>),
    WaitCache(Address),
}

pub struct ExecutionEngine {
    execution_channel_tx: Sender<ExecutionEvent>,
    cache_channel_tx: Sender<(Address, AccountEntry)>,
    end_block_channel_rx: Receiver<(State<StateDB>, Vec<Address>)>,
    handler: JoinHandle<()>,
}

impl ExecutionEngine {
    pub fn start(number: usize, mut env_info: EnvInfo) -> ExecutionEngine {
        let (execution_channel_tx, execution_channel_rx) = unbounded();
        let (cache_channel_tx, cache_channel_rx) = unbounded();
        let (end_block_channel_tx, end_block_channel_rx) = unbounded();
        let machine = machine_generator();
        let mut wrap_state: Option<State<StateDB>> = None;

        let handler = thread::Builder::new()
            .name(format!("engine{}", &number.to_string()))
            .spawn(move || {
                let mut cache_buffer = vec![];
                let mut internal_call_addr = vec![];
                let mut block: Arc<RwLock<Block>> = Default::default();
                loop {
                    match execution_channel_rx.recv().unwrap() {
                        ExecutionEvent::Stop => {
                            break;
                        }
                        ExecutionEvent::Transact(tx_index) => {
                            let block = &*block.read();
                            let tx = SignedTransaction::new(block.transactions[tx_index].clone())
                                .unwrap();
                            let outcome = wrap_state
                                .as_mut()
                                .unwrap()
                                .apply(&env_info, &machine, &tx, true)
                                .unwrap();
                            let trace = outcome.trace;
                            // TODO: check CALL
                            // the transaction has internal call
                            for sub_trace in &trace[1..] {
                                match &sub_trace.action {
                                    Action::Call(call) => {
                                        if !internal_call_addr.contains(&call.to) {
                                            internal_call_addr.push(call.to);
                                        }
                                    }
                                    Action::Create(_) => match &sub_trace.result {
                                        Res::Create(create) => {
                                            if !internal_call_addr.contains(&create.address) {
                                                internal_call_addr.push(create.address);
                                            }
                                        }
                                        _ => (),
                                    },
                                    _ => (),
                                }
                            }
                        }
                        ExecutionEvent::SendCache(addr, cache_channel_tx) => {
                            let account_entry = wrap_state.as_mut().unwrap().drop_account(&addr);
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
                                wrap_state
                                    .as_mut()
                                    .unwrap()
                                    .insert_cache(&_addr, account_entry);
                                if _addr == addr {
                                    cached = true;
                                } else {
                                    cache_buffer.push(_addr);
                                }
                            }
                        }
                        ExecutionEvent::BeginBlock(state, block_lock) => {
                            wrap_state = Some(state);
                            block = block_lock;
                            let block = &*block.read();
                            let header = &block.header;
                            test_helpers::update_envinfo_by_header(&mut env_info, &header);
                        }
                        ExecutionEvent::EndBlock => {
                            let call_addr = mem::replace(&mut internal_call_addr, vec![]);
                            end_block_channel_tx
                                .send((wrap_state.take().unwrap(), call_addr))
                                .unwrap();
                        }
                    }
                }
            })
            .unwrap();
        ExecutionEngine {
            execution_channel_tx: execution_channel_tx,
            cache_channel_tx: cache_channel_tx,
            end_block_channel_rx: end_block_channel_rx,
            handler: handler,
        }
    }

    pub fn transact(&self, tx_index: usize) {
        self.execution_channel_tx
            .send(ExecutionEvent::Transact(tx_index))
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

    pub fn stop(self) {
        self.execution_channel_tx
            .send(ExecutionEvent::Stop)
            .unwrap();
        self.handler.join().unwrap();
    }

    pub fn cache_channel_tx(&self) -> Sender<(Address, AccountEntry)> {
        self.cache_channel_tx.clone()
    }

    pub fn begin_block(&self, state: State<StateDB>, block_lock: Arc<RwLock<Block>>) {
        self.execution_channel_tx
            .send(ExecutionEvent::BeginBlock(state, block_lock))
            .unwrap();
    }

    pub fn end_block(&self) {
        self.execution_channel_tx
            .send(ExecutionEvent::EndBlock)
            .unwrap();
    }

    pub fn wait_state_and_call_addr(&self) -> (State<StateDB>, Vec<Address>) {
        let (state, call_addr) = self.end_block_channel_rx.recv().unwrap();
        return (state, call_addr);
    }
}
