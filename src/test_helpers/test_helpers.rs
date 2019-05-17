use ethcore::open_state::State;
use ethcore::open_state_db::StateDB;
use ethcore::test_helpers::new_db;
use ethereum_types::U256;

/// Returns temp state db
pub fn get_temp_state_db() -> StateDB {
    let db = new_db();
    let journal_db = ::journaldb::new(
        db.key_value().clone(),
        ::journaldb::Algorithm::EarlyMerge,
        ::ethcore_db::COL_STATE,
    );
    StateDB::new(journal_db, 5 * 1024 * 1024)
}

/// Returns temp state
pub fn get_temp_state() -> State<StateDB> {
    let journal_db = get_temp_state_db();
    State::new(journal_db, U256::from(0), Default::default())
}

/*
/// TODO: remove following functions
/// Generate the state and transfer transactions by given address number and transaction number
use rand::thread_rng;
pub fn state_and_txs(address_n: usize, transaction_n: usize) -> (State<StateDB>, Vec<SignedTransaction>) {
// generate all address
let keypairs = random_keypairs(address_n);
// generate senders and receivers
let mut rng = thread_rng();
let mut senders = vec![];
let mut receivers = vec![];
for i in 0..transaction_n {
let mut result = keypairs.iter().choose_multiple(&mut rng, 2);
senders.push(result[0].clone());
receivers.push(result[1].address());
}
// generate transactions
let txs = random_transfer_txs(&senders, &receivers);

//StateDb and State
let mut state_db = test_helpers::get_temp_state_db();
let mut factories = Factories::default();
let vm_factory = Factory::new(VMType::Interpreter, 1024 * 32);
factories.vm = vm_factory.into();
let mut state = State::new(state_db, U256::from(0), factories);

// Add balance to senders
for sender in senders {
let address = sender.address();
state.add_balance(&address, &U256::from(50), CleanupMode::NoEmpty).unwrap();
}
state.commit();

(state, txs)
}

/// Generate the state and transfer transactions without dependency by given address number and transaction number
pub fn state_txs_nd(n: usize) -> (State<StateDB>, Vec<SignedTransaction>) {
// Set up transactions
let (senders, receivers) = random_senders_receivers(n);
// Generate transactions
let txs = random_transfer_txs(&senders, &receivers);
// StateDB and State
let mut state_db = test_helpers::get_temp_state_db();
let mut factories = Factories::default();
let vm_factory = Factory::new(VMType::Interpreter, 1024 * 32);
factories.vm = vm_factory.into();
let mut state = State::new(state_db, U256::from(0), factories);

// Add balance to senders
for sender in senders {
let address = sender.address();
state.add_balance(&address, &U256::from(50), CleanupMode::NoEmpty).unwrap();
}
state.commit();

(state, txs)
}
*/
