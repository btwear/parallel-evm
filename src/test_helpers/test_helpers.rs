use common_types::block::Block;
use common_types::header::Header;
use ethcore::client::ClientConfig;
use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use ethcore::test_helpers::new_db;
use ethcore_blockchain::BlockChainDB;
use ethcore_db::NUM_COLUMNS;
use ethereum_types::{H256, U256};
use kvdb::KeyValueDB;
use kvdb_rocksdb::{CompactionProfile, Database, DatabaseConfig};
use rlp::{Decodable, PayloadInfo, Rlp};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use vm::EnvInfo;

/// Returns temp state db
pub fn get_temp_state_db() -> StateDB {
    let db = new_db();
    let journal_db = ::journaldb::new(
        db.key_value().clone(),
        ::journaldb::Algorithm::Archive,
        ::ethcore_db::COL_STATE,
    );
    StateDB::new(journal_db, 5 * 1024 * 1024)
}

/// Returns temp state
pub fn get_temp_state() -> State<StateDB> {
    let journal_db = get_temp_state_db();
    State::new(journal_db, U256::from(0), Default::default())
}

struct AppDB {
    key_value: Arc<dyn KeyValueDB>,
    blooms: blooms_db::Database,
    trace_blooms: blooms_db::Database,
}

impl BlockChainDB for AppDB {
    fn key_value(&self) -> &Arc<dyn KeyValueDB> {
        &self.key_value
    }

    fn blooms(&self) -> &blooms_db::Database {
        &self.blooms
    }

    fn trace_blooms(&self) -> &blooms_db::Database {
        &self.trace_blooms
    }
}

/// Return default database config
pub fn db_config() -> DatabaseConfig {
    let client_config = ClientConfig::default();
    let mut db_config = DatabaseConfig::with_columns(NUM_COLUMNS);
    db_config.memory_budget = client_config.db_cache_size;
    db_config.compaction = CompactionProfile::ssd();

    db_config
}

pub fn open_state_db(db_path: &str) -> StateDB {
    let db = open_database(&db_path);
    let journal_db = journaldb::new(
        db.key_value().clone(),
        ::journaldb::Algorithm::Archive,
        ::ethcore_db::COL_STATE,
    );
    let state_db = StateDB::new(journal_db, 5 * 1024 * 1024);

    state_db
}

pub fn open_database(db_path: &str) -> Arc<dyn BlockChainDB> {
    let config = db_config();
    let path = Path::new(db_path);

    let blooms_path = path.join("blooms");
    let trace_blooms_path = path.join("trace_blooms");
    fs::create_dir_all(&blooms_path).unwrap();
    fs::create_dir_all(&trace_blooms_path).unwrap();

    let db = AppDB {
        key_value: Arc::new(Database::open(&config, db_path).unwrap()),
        blooms: blooms_db::Database::open(blooms_path).unwrap(),
        trace_blooms: blooms_db::Database::open(trace_blooms_path).unwrap(),
    };

    Arc::new(db)
}

pub fn read_blocks(dir: &str, from: usize, to: usize) -> Vec<Block> {
    let mut instream = fs::File::open(&dir)
        .map_err(|_| format!("Cannot open given file: {}", dir))
        .unwrap();
    let first_bytes: Vec<u8> = vec![0; READAHEAD_BYTES];
    let mut first_read = 0;
    let mut blocks = vec![];
    const READAHEAD_BYTES: usize = 8;

    for i in 0..to + 1 {
        let mut bytes = if first_read > 0 {
            first_bytes.clone()
        } else {
            vec![0; READAHEAD_BYTES]
        };
        let n = if first_read > 0 {
            first_read
        } else {
            instream
                .read(&mut bytes)
                .map_err(|_| "Error reading from the file/stream.")
                .unwrap()
        };
        if n == 0 {
            break;
        }
        first_read = 0;
        let s = PayloadInfo::from(&bytes)
            .map_err(|e| format!("Invalid RLP in the file/stream: {:?}", e))
            .unwrap()
            .total();
        bytes.resize(s, 0);
        instream
            .read_exact(&mut bytes[n..])
            .map_err(|_| "Error reading from the file/stream.")
            .unwrap();

        if i >= from - 1 {
            let raw_block = Rlp::new(&bytes);
            let block = Block::decode(&raw_block).unwrap();
            blocks.push(block);
        }
    }
    blocks
}

pub fn update_envinfo_by_header(env_info: &mut EnvInfo, header: &Header) {
    env_info.number = header.number();
    env_info.author = header.author().clone();
    env_info.timestamp = header.timestamp();
    env_info.difficulty = header.difficulty().clone();
    env_info.gas_limit = header.gas_limit().clone();
    env_info.gas_used = U256::zero();
    let mut last_hashes = VecDeque::from(
        Arc::try_unwrap(std::mem::replace(
            &mut env_info.last_hashes,
            Arc::new(vec![]),
        ))
        .unwrap(),
    );
    last_hashes.push_front(header.parent_hash().clone());
    last_hashes.pop_back();
    env_info.last_hashes = Arc::new(last_hashes.into());
}

pub fn header_to_envinfo(header: &Header) -> EnvInfo {
    let mut last_hashes = vec![header.parent_hash().clone()];
    last_hashes.resize(256, H256::default());
    EnvInfo {
        number: header.number(),
        author: header.author().clone(),
        timestamp: header.timestamp(),
        difficulty: header.difficulty().clone(),
        gas_limit: header.gas_limit().clone(),
        last_hashes: Arc::new(last_hashes),
        gas_used: U256::zero(),
    }
}

pub fn load_last_hashes(dir: &str) -> Vec<H256> {
    let reader = BufReader::new(fs::File::open(dir).unwrap());
    let mut last_hashes = vec![];
    for hash in reader.lines() {
        last_hashes.push(H256::from_str(&hash.unwrap()[2..]).unwrap());
    }
    last_hashes
}
