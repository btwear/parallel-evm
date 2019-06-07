mod test_data;
mod test_helpers;

pub use self::test_data::*;
pub use self::test_helpers::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ethcore::factory::Factories;
    use ethcore::open_state::State;
    use ethereum_types::{Address, H256, U256};

    #[test]
    fn test_state_from_db() {
        let db_path = "res/db_7840000";
        let state_root =
            H256::from("0xa7ca2c04e692960dac04909b3212baf12df7666efac68afad4646b3205a32c91");

        let state_db = open_state_db(db_path);

        let state =
            State::from_existing(state_db, state_root, U256::zero(), Factories::default()).unwrap();

        println!(
            "{:?}",
            state.balance(&Address::from("0x21b3e76134AAa7EA56B5250CfA782753141b1eca"))
        );
    }

    #[test]
    fn test_read_block() {
        let block_dir = "res/blocks/7840001_7850000.bin";
        let blocks = read_blocks(block_dir, 9900, 10000);
        println!("{:?}", ::std::mem::size_of_val(&blocks));
        println!("{:?}", blocks.len());
    }
}
