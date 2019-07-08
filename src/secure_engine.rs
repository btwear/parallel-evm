use common_types::block::Block;
use common_types::transaction::SignedTransaction;
use crossbeam_channel::{self as channel, Receiver, Sender};
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
    pub fn start(env_info: Arc<RwLock<EnvInfo>>, tx_delay: usize) -> SecureEngine {
        let (execution_channel_tx, execution_channel_rx) = channel::bounded(4);
        let (end_block_channel_tx, end_block_channel_rx) = channel::bounded(4);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handler = thread::Builder::new()
            .name(format!("secure_engine"))
            .spawn(move || {
                let machine = machine_generator();
                loop {
                    match execution_channel_rx.recv().unwrap() {
                        SecureEvent::Stop => {
                            break;
                        }
                        SecureEvent::BeginBlock(mut state, block_lock) => {
                            let block = &*block_lock.read();
                            let env_info = env_info.read();
                            for utx in &block.transactions {
                                if running.load(Ordering::Relaxed) {
                                    let tx = SignedTransaction::new(utx.clone()).unwrap();
                                    match state
                                        .apply_with_delay(&env_info, &machine, &tx, false, tx_delay)
                                    {
                                        Err(_) => break,
                                        _ => (),
                                    }
                                } else {
                                    break;
                                }
                            }
                            match execution_channel_rx.recv().unwrap() {
                                SecureEvent::EndBlock => {
                                    end_block_channel_tx.send(state).unwrap();
                                }
                                _ => (),
                            }
                        }
                        _ => (),
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
        self.execution_channel_tx.send(SecureEvent::Stop).unwrap();
    }

    pub fn begin_block(&self, state: State<StateDB>, block_lock: Arc<RwLock<Block>>) {
        (*self.running).store(true, Ordering::Relaxed);
        self.execution_channel_tx
            .send(SecureEvent::BeginBlock(state, block_lock))
            .unwrap();
    }

    pub fn end_block(&self) -> State<StateDB> {
        self.execution_channel_tx
            .send(SecureEvent::EndBlock)
            .unwrap();
        self.end_block_channel_rx.recv().unwrap()
    }
}
