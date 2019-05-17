use common_types::transaction::{Action, SignedTransaction};
use ethcore::ethereum::new_frontier_test_machine;
use ethcore::machine::EthereumMachine as Machine;
use ethcore::open_executive::{Executive, TransactOptions};
use ethcore::open_state::{AccountEntry, State};
use ethcore::open_state_db::StateDB;
use ethereum_types::{Address, U256};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::mpsc;
use std::thread;
use vm::EnvInfo;

pub fn sequential_exec(
    mut state: State<StateDB>,
    env_info: &EnvInfo,
    machine: &Machine,
    txs: Vec<SignedTransaction>,
) -> State<StateDB> {
    // Execute transactions
    for tx in txs {
        let outcome = state.apply(&env_info, machine, &tx, false);
    }
    state
}

pub enum ExecutionEvent {
    Stop,
    Transact(SignedTransaction),
    DropAddress(Address),
    InsertCache(Address, AccountEntry),
    ChangeEnv(EnvInfo),
}

/// in progress
pub fn parallel_exec(
    mut state: State<StateDB>,
    env_info: &EnvInfo,
    machine: &Machine,
    txs: Vec<SignedTransaction>,
    threads: usize,
) -> State<StateDB> {
    // Spawn execution threads
    // d_channel for drop cache, it send cache entry back to thread manager
    let (d_channel_tx, d_channel_rx) = mpsc::channel();
    let mut channel_txs = Vec::new();
    let mut thread_handlers = Vec::new();
    for _ in 0..threads {
        let (channel_tx, channel_rx) = mpsc::channel();
        channel_txs.push(channel_tx);
        // Generate execution background
        let mut sub_state = state.clone();
        let mut info = EnvInfo::default();
        info.gas_limit = U256::from(100_000_000);
        let machine = new_frontier_test_machine();
        let schedule = machine.schedule(info.number);
        let d_channel_tx = d_channel_tx.clone();
        let exec_inner = move || {
            loop {
                let received = channel_rx.recv().unwrap();
                match received {
                    ExecutionEvent::Stop => {
                        return sub_state;
                    }
                    ExecutionEvent::Transact(tx) => {
                        let mut exec =
                            Executive::new(&mut sub_state, &mut info, &machine, &schedule);
                        let _ = {
                            let opts = TransactOptions::with_no_tracing();
                            exec.transact(&tx, opts).unwrap()
                        };
                    }
                    ExecutionEvent::DropAddress(addr) => {
                        let cache_entry = sub_state.drop_account(&addr);
                        d_channel_tx.send(cache_entry).unwrap();
                    }
                    ExecutionEvent::InsertCache(addr, entry) => {
                        sub_state.insert_cache(&addr, entry);
                    }
                    ExecutionEvent::ChangeEnv(_env_info) => {
                        // TODO: implement environment switching
                    }
                }
            }
        };
        let handle = thread::spawn(exec_inner);
        thread_handlers.push(handle);
    }

    // Dependency hashmap
    let mut dependency_table = HashMap::new();

    // Send transactions to execution threads
    let mut thread_id = 0;
    for tx in txs {
        let mut d_flag = [false, false]; //dependency flag, [0]: sender dependency, [1]to dependency
        let mut d_tid = [0, 0]; //dependency thread id
        let mut d_addr = [Address::zero(); 2];
        d_addr[0] = tx.sender();
        d_addr[1] = match tx.deref().deref().action {
            Action::Create => Address::zero(),
            Action::Call(addr) => addr,
        };
        for i in 0..2 {
            match dependency_table.get(&d_addr[i]) {
                Some(tid) => {
                    d_flag[i] = true;
                    d_tid[i] = *tid;
                }
                None => (),
            }
        }

        let mut exec_tid = thread_id;
        if !(d_flag[0] || d_flag[1]) {
            // If no dependency, insert sender and target address to dependency table then continue round robin
            dependency_table.insert(d_addr[0], exec_tid);
            if d_addr[1] != Address::zero() {
                dependency_table.insert(d_addr[1], exec_tid);
            }
        } else if !d_flag[0] && d_flag[1] {
            // If single dependency
            exec_tid = d_tid[1];
            dependency_table.insert(d_addr[0], exec_tid);
        } else if d_flag[0] && !d_flag[1] {
            exec_tid = d_tid[0];
            dependency_table.insert(d_addr[1], exec_tid);
        } else if d_tid[0] == d_tid[1] {
            exec_tid = d_tid[0];
        } else {
            // 1. If double dependency, send DropAddress signal to sender_tid
            // 2. Wait for the address cache from sender_tid, and transfer address cache to to_tid
            exec_tid = d_tid[1];
            channel_txs[d_tid[0]]
                .send(ExecutionEvent::DropAddress(d_addr[0]))
                .unwrap();
            let address_cache = d_channel_rx.recv().unwrap();
            channel_txs[exec_tid]
                .send(ExecutionEvent::InsertCache(d_addr[0], address_cache))
                .unwrap();
            dependency_table.insert(d_addr[0], exec_tid);
        }
        if thread_id == exec_tid {
            thread_id = (thread_id + 1) % threads;
        }
        channel_txs[exec_tid]
            .send(ExecutionEvent::Transact(tx))
            .unwrap();
    }

    // Send stop signal to execution threads
    for channel_tx in channel_txs {
        channel_tx.send(ExecutionEvent::Stop).unwrap();
    }

    // TODO: aggregate execution results
    // Join all threads, and commit every executed result to global state_db
    /*
    let (mut current_root, mut global_db) = state.drop();
    for handle in thread_handlers {
        let mut sub_state = handle.join().unwrap();
        sub_state.set_root(current_root);
        sub_state.commit_to_external_db(&mut global_db).unwrap();
        current_root = *sub_state.root();
    }

    let mut state = State::from_existing(global_db, current_root, U256::from(0), Default::default()).unwrap();
    assert_eq!(*state.root(), current_root);

    current_root
    */
    state
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_helpers;
    #[test]
    fn test_root_no_dependencies() {
        // Test with no dependencies
        let n = 10;
        let (mut state, txs) = test_helpers::test_state_txs_nd(n);

        let seq_state = state.clone();
        let seq_txs = txs.clone();
        seq_state = sequential_exec(seq_state, seq_txs);
        state = parallel_exec(state, txs, 4);
        assert_eq!(root_seq.root(), root_para.root());
    }
}
