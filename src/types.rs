use ethjson::hash::Address;
use ethjson::uint::Uint;
use serde_json;
use std::fs;
use std::io::{BufRead, BufReader};

#[derive(Clone, Debug, Deserialize)]
pub struct Reward {
    #[serde(rename = "blockNumber")]
    pub block_number: Uint,
    #[serde(rename = "blockMiner")]
    pub miner: Address,
    #[serde(rename = "blockReward")]
    pub reward: Uint,
    pub uncles: Vec<Uncle>,
    #[serde(rename = "uncleInclusionReward")]
    pub uncle_inclusion_reward: Uint,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Uncle {
    pub miner: Address,
    #[serde(rename = "unclePosition")]
    pub position: Uint,
    #[serde(rename = "blockreward")]
    pub reward: Uint,
}

impl Reward {
    pub fn drop(self) -> (Address, Uint, Vec<Uncle>) {
        (self.miner, self.reward, self.uncles)
    }

    pub fn from_json_str(json_str: &str) -> Reward {
        serde_json::from_str(json_str).unwrap()
    }

    pub fn from_file(dir: &str, from: usize, to: usize) -> Vec<Reward> {
        let f = fs::File::open(dir).unwrap();
        let reader = BufReader::new(f);
        let mut rewards = vec![];
        for (i, line) in reader.lines().enumerate() {
            if i >= to {
                break;
            } else if i >= from - 1 {
                rewards.push(Reward::from_json_str(&line.unwrap()[..]));
            }
        }
        rewards
    }
}

impl Uncle {
    pub fn drop(self) -> (Address, Uint) {
        (self.miner, self.reward)
    }
}
