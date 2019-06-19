use crate::test_helpers;
use common_types::block::Block;
use common_types::transaction::SignedTransaction;
use crossbeam_channel::{self, unbounded, Receiver, Sender};
use ethcore::ethereum::new_constantinople_fix_test_machine as machine_generator;
use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use vm::EnvInfo;

#[derive(Clone)]
pub enum SecureEvent {
    Stop,
    BeginBlock(State<StateDB>, Arc<RwLock<Block>>),
    EndBlock,
}

pub struct SecureEngine {
    execution_channel_tx: Sender<SecureEvent>,
    end_block_channel_rx: Receiver<State<StateDB>>,
    running: Arc<AtomicBool>,
    handler: JoinHandle<()>,
}

impl SecureEngine {
    pub fn start(mut env_info: EnvInfo) -> SecureEngine {
        let (execution_channel_tx, execution_channel_rx) = unbounded();
        let (end_block_channel_tx, end_block_channel_rx) = unbounded();
        let machine = machine_generator();
        let mut wrap_state: Option<State<StateDB>> = None;
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handler = thread::Builder::new()
            .name(format!("secure_engine"))
            .spawn(move || {
                let mut block: Arc<RwLock<Block>>;
                loop {
                    match execution_channel_rx.recv().unwrap() {
                        SecureEvent::Stop => {
                            break;
                        }
                        SecureEvent::BeginBlock(state, block_lock) => {
                            wrap_state = Some(state);
                            block = block_lock;
                            let block = &*block.read();
                            let header = &block.header;
                            test_helpers::update_envinfo_by_header(&mut env_info, &header);
                            for utx in &block.transactions {
                                if running.load(Ordering::Relaxed) {
                                    let tx = SignedTransaction::new(utx.clone()).unwrap();
                                    wrap_state
                                        .as_mut()
                                        .unwrap()
                                        .apply(&env_info, &machine, &tx, false)
                                        .unwrap();
                                } else {
                                    break;
                                }
                            }
                        }
                        SecureEvent::EndBlock => {
                            end_block_channel_tx
                                .send(wrap_state.take().unwrap())
                                .unwrap();
                        }
                    }
                }
            })
            .unwrap();
        SecureEngine {
            execution_channel_tx: execution_channel_tx,
            end_block_channel_rx: end_block_channel_rx,
            running: running_clone,
            handler: handler,
        }
    }

    pub fn stop(self) {
        self.execution_channel_tx.send(SecureEvent::Stop).unwrap();
        self.handler.join().unwrap();
    }

    pub fn terminate(&self) {
        (*self.running).store(false, Ordering::Relaxed);
    }

    pub fn begin_block(&self, state: State<StateDB>, block_lock: Arc<RwLock<Block>>) {
        self.execution_channel_tx
            .send(SecureEvent::BeginBlock(state, block_lock))
            .unwrap();
    }

    pub fn end_block(&self) {
        self.execution_channel_tx
            .send(SecureEvent::EndBlock)
            .unwrap();
    }

    pub fn wait_state(&self) -> (State<StateDB>) {
        let state = self.end_block_channel_rx.recv().unwrap();
        return state;
    }
}
