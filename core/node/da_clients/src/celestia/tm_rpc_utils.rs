/*
    This file contains the utils for the Celestia Tendermint RPC.
    It is used to get the data root inclusion proof for a given height.
*/
#![allow(dead_code)]
use reqwest::{Client, Error as ReqwestError};
use std::{collections::HashMap, env, error::Error};

pub struct TendermintRPCClient {
    url: String,
}

impl Default for TendermintRPCClient {
    fn default() -> Self {
        TendermintRPCClient {
            url: env::var("TENDERMINT_RPC_URL").expect("TENDERMINT_RPC_URL not set"),
        }
    }
}

impl TendermintRPCClient {
    pub fn new(url: String) -> Self {
        TendermintRPCClient { url }
    }

    pub async fn get_data_root_inclusion_proof(&self, height: u64, start: u64, end: u64) -> Result<String, ReqwestError>{
        let client = Client::new();
        let url = format!("{}/data_root_inclusion_proof", self.url);
        client
            .get(url)
            .query(&[("height", height), ("start", start), ("end", end)])
            .send()
            .await
            .expect("Failed to send request")
            .text()
            .await
    }
}


