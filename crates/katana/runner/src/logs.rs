use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Duration;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::KatanaRunner;

#[derive(Serialize, Deserialize, Debug)]
pub struct TimedLog<T> {
    timestamp: String,
    level: String,
    fields: T,
    target: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MinedMessage {
    message: String,
    block_number: String,
    tx_count: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UsageMessage {
    message: String,
    usage: String,
}

pub type MinedLog = TimedLog<MinedMessage>;
pub type UsageLog = TimedLog<UsageMessage>;

impl KatanaRunner {
    pub fn blocks(&self) -> Vec<MinedLog> {
        BufReader::new(File::open(&self.log_file_path).unwrap())
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| match serde_json::from_str(&line) {
                Ok(log) => Some(log),
                Err(_) => None,
            })
            .filter_map(|log: MinedLog| match log.fields.message.contains("Block mined.") {
                true => Some(log),
                false => None,
            })
            .collect()
    }

    pub async fn blocks_until_empty(&self) -> Vec<MinedLog> {
        let mut blocks = self.blocks();
        loop {
            if let Some(block) = blocks.last() {
                if block.fields.tx_count == "0" {
                    break;
                }
            }

            let len_at_call = blocks.len();
            while len_at_call == blocks.len() {
                sleep(Duration::from_secs(1)).await;
                blocks = self.blocks();
            }
        }
        blocks
    }

    pub async fn block_sizes(&self) -> Vec<u32> {
        self.blocks_until_empty()
            .await
            .into_iter()
            .map(|block| {
                block
                    .fields
                    .tx_count
                    .parse::<u32>()
                    .expect("Failed to parse number of transactions")
            })
            .collect()
    }

    pub async fn block_times(&self) -> Vec<i64> {
        let mut v = self
            .blocks_until_empty()
            .await
            .into_iter()
            .map(|block| block.timestamp.parse().expect("Failed to parse time"))
            .collect::<Vec<DateTime<Utc>>>()
            .windows(2)
            .map(|w| (w[1] - w[0]).num_milliseconds())
            .collect::<Vec<_>>();

        // First block has no previous one, so always has a time of 0
        v.insert(0, 0);
        v
    }

    pub async fn steps(&self) -> Vec<u64> {
        let matching = "Steps: ";
        BufReader::new(File::open(&self.log_file_path).unwrap())
            .lines()
            .filter_map(|line| serde_json::from_str::<UsageLog>(&line.unwrap()).ok())
            .filter_map(|log| {
                let line = log.fields.usage;
                if let Some(start) = line.find(matching) {
                    let end = line.find(" | ");
                    let steps = line[start + matching.len()..end.unwrap()].to_string();

                    Some(steps.parse::<u64>().unwrap())
                } else {
                    None
                }
            })
            .collect()
    }
}

#[test]
fn test_parse_katana_logs() {
    let log = r#"{"timestamp":"2024-06-18T16:51:49.139195Z","level":"INFO","fields":{"message":"Block mined.","block_number":"1","tx_count":"1"},"target":"katana::core::backend"}"#;
    let log: MinedLog = serde_json::from_str(log).unwrap();
    assert_eq!(log.fields.message, "Block mined.");
    assert_eq!(log.fields.tx_count, "1");
    assert_eq!(log.fields.block_number, "1");
}

#[test]
fn test_parse_katana_usage_logs() {
    let log = r#"{"timestamp":"2024-06-19T09:11:56.406990Z","level":"TRACE","fields":{"message":"Transaction resource usage.","usage":"Steps: 3513 | Ec Op Builtin: 3 | L 1 Blob Gas Usage: 0 | L1 Gas: 8063 | Pedersen: 16 | Range Checks: 75"},"target":"executor"}"#;
    let log: UsageLog = serde_json::from_str(log).unwrap();
    assert_eq!(log.fields.message, "Transaction resource usage.");
    assert_eq!(
        log.fields.usage,
        "Steps: 3513 | Ec Op Builtin: 3 | L 1 Blob Gas Usage: 0 | L1 Gas: 8063 | Pedersen: 16 | \
         Range Checks: 75"
    );
}
