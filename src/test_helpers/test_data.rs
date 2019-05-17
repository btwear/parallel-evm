extern crate bincode;
extern crate rand;
extern crate rustc_serialize;
use common_types::transaction::{Action, SignedTransaction, Transaction, UnverifiedTransaction};
use ethereum_types::{Address, H160, U256};
use ethstore::ethkey::{Generator, KeyPair, Random};
use rand::prelude::IteratorRandom;
use rand::thread_rng;
use rlp::{Decodable, Encodable, Rlp};
use rustc_hex::FromHex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

pub fn static_dep_txs(
    addr_number: usize,
    tx_number: usize,
    auto_load: bool,
) -> Vec<SignedTransaction> {
    let path = format!("/tmp/static_dep_txs_{}_{}.bin", addr_number, tx_number);
    let keypairs = random_keypairs(addr_number);
    let transactions = {
        if auto_load && Path::new(&path).exists() {
            load_transactions(&path)
        } else {
            let mut senders = Vec::new();
            let mut receivers = Vec::new();
            let mut rng = thread_rng();
            for _ in 0..tx_number {
                let result = keypairs.iter().choose_multiple(&mut rng, 2);
                senders.push(result[0].clone());
                receivers.push(result[1].address());
            }
            let txs = transfer_txs(&senders, &receivers);
            save_transactions(&txs, &path);
            txs
        }
    };

    transactions
}

pub fn random_keypairs(n: usize) -> Vec<KeyPair> {
    let mut keypair_vec = Vec::new();
    for _ in 0..n {
        keypair_vec.push(Random.generate().unwrap());
    }
    keypair_vec
}

pub fn random_addresses(n: usize) -> Vec<Address> {
    let mut address_vec = Vec::new();
    for _ in 0..n {
        address_vec.push(H160::random());
    }
    address_vec
}

/// Generate transfer transaction by given sender keypairs and receiver addresses
pub fn transfer_txs(
    sender_keypairs: &Vec<KeyPair>,
    to_addresses: &Vec<Address>,
) -> Vec<SignedTransaction> {
    assert_eq!(sender_keypairs.len(), to_addresses.len());
    let n = sender_keypairs.len();
    let mut result = Vec::new();
    let mut nonce_table = HashMap::new();
    for i in 0..n {
        let sender = &sender_keypairs[i];
        let to = &to_addresses[i];
        let t = Transaction {
            action: Action::Call(*to),
            value: U256::from(1),
            data: "".from_hex().unwrap(),
            gas: U256::from(100_000),
            gas_price: U256::zero(),
            nonce: {
                let nonce = nonce_table
                    .get(&sender.address())
                    .cloned()
                    .unwrap_or(U256::zero());
                nonce_table.insert(sender.address(), nonce + 1);
                nonce
            },
        }
        .sign(sender.secret(), None);
        result.push(t)
    }
    result
}

pub fn save_transactions(transactions: &Vec<SignedTransaction>, path: &str) {
    let mut writer = BufWriter::new(File::create(path).unwrap());
    let mut rlp_transactions = vec![];

    for tx in transactions {
        rlp_transactions.push(tx.rlp_bytes());
    }

    bincode::serialize_into(&mut writer, &rlp_transactions).unwrap();
}

pub fn load_transactions(path: &str) -> Vec<SignedTransaction> {
    let mut reader = BufReader::new(File::open(path).unwrap());
    let decoded: Vec<Vec<u8>> = bincode::deserialize_from(&mut reader).unwrap();
    let mut transactions = vec![];

    for rlp_tx in decoded {
        let unverified_tx = UnverifiedTransaction::decode(&Rlp::new(&rlp_tx)).unwrap();
        let tx = SignedTransaction::new(unverified_tx).unwrap();
        transactions.push(tx);
    }

    transactions
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_helpers::transfer_txs;

    #[test]
    fn test_save_load_txs() {
        let senders = random_keypairs(5);
        let receivers = random_addresses(5);
        let transactions = transfer_txs(&senders, &receivers);

        let tmp_path = "/tmp/txs.bin";
        save_transactions(&transactions, tmp_path);
        let load_txs = load_transactions(tmp_path);
        let result = transactions == load_txs;
        assert!(result);
    }
}
