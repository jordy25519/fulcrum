//! A stripped down Ethereum JSON-RPC WS client based on ethers-providers `WsClient`
use std::{fmt, sync::Arc, time::Instant};

use async_trait::async_trait;
use compact_str::CompactString;
use ethers_providers::{ConnectionDetails, JsonRpcClient, WsClientError};
use log::error;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::value::{to_raw_value, RawValue};

use crate::{manager::RequestManager, types::PreserializedCallRequest};

pub const ETH_CALL: &'static str = "eth_call";
pub const ETH_BLOCK_NUMBER: &'static str = "eth_blockNumber";

#[derive(Clone)]
pub struct FastWsClient {
    // Used to send requests to the `RequestManager`
    pub(crate) requests: tokio::sync::mpsc::UnboundedSender<PreserializedCallRequest>,
}

impl FastWsClient {
    /// Crude report on the latency of the ws connection
    pub async fn report_latency(&self) -> f64 {
        let mut avg_latency = 0_u128;
        for _ in 0..10 {
            let t0 = Instant::now();
            let _: Result<String, WsClientError> = self.request("net_version", [""]).await;
            avg_latency += (Instant::now() - t0).as_millis();
        }
        avg_latency as f64 / 10f64
    }
    /// Establishes a new websocket connection
    pub async fn connect(conn: impl Into<ConnectionDetails>) -> Result<Self, WsClientError> {
        let (man, this) = RequestManager::connect(conn.into()).await?;
        man.spawn();
        Ok(this)
    }

    pub async fn eth_block_number<'a>(&self) -> Result<u64, WsClientError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let call = PreserializedCallRequest {
            method: CompactString::new(ETH_BLOCK_NUMBER),
            params: Default::default(),
            sender: tx,
        };

        self.requests
            .send(call)
            .map_err(|_| WsClientError::DeadChannel)?;

        match rx.await {
            Ok(Ok(res)) => {
                let s = res.get();
                let mut n = [0_u8; 8];
                // "0x" <- strip these chars, the output is valid hex
                faster_hex::hex_decode_unchecked(
                    unsafe { s.get_unchecked(3..s.len() - 1) }.as_bytes(),
                    &mut n,
                );
                Ok(u64::from_le_bytes(n))
            }
            Ok(Err(err)) => {
                error!("eth_blockNumber rpc: {:?}", err);
                Err(WsClientError::UnexpectedClose)
            }
            Err(err) => {
                error!("eth_blockNumber channel dropped: {:?}", err);
                Err(WsClientError::UnexpectedClose)
            }
        }
    }

    /// Issue an Ethereum JSON-RPC 'eth_call' request with pre-serialized `params`
    /// - `params` pre-serialized (hexified RLP) payload
    pub async fn eth_call<'a>(
        &self,
        params: &Arc<Box<RawValue>>,
        buffer: &mut Vec<u8>,
    ) -> Result<(), WsClientError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let call = PreserializedCallRequest {
            method: CompactString::new(ETH_CALL),
            params: Arc::clone(params),
            sender: tx,
        };

        // TODO: its simpler to call await on the ws backend directly
        // its like this to map responses to requests by id in proper async setup
        // in this implementation we know that requests and responses come sequentially
        self.requests
            .send(call)
            .map_err(|_| WsClientError::DeadChannel)?;

        match rx.await {
            // TODO: dropping the Box<> here is costly
            // - de-alloc in another thread or avoid the alloc, larger refactor
            Ok(Ok(res)) => {
                let s = res.get();
                buffer.resize((s.len() - 4) / 2, 0); // "0x" <- strip these chars
                                                     // the output is valid hex
                faster_hex::hex_decode_unchecked(
                    unsafe { s.get_unchecked(3..s.len() - 1) }.as_bytes(),
                    buffer,
                );

                Ok(())
            }
            Ok(Err(err)) => Err(err.into()),
            Err(err) => {
                error!("eth_call channel dropped: {:?}", err);
                Err(WsClientError::UnexpectedClose)
            }
        }
    }

    // this is taken verbatim from ethers_providers::WsClient for compatibility
    async fn make_request<R>(&self, method: &str, params: Box<RawValue>) -> Result<R, WsClientError>
    where
        R: DeserializeOwned,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let instruction = PreserializedCallRequest {
            method: CompactString::new(method),
            params: Arc::new(params),
            sender: tx,
        };
        self.requests
            .send(instruction)
            .map_err(|_| WsClientError::DeadChannel)?;

        let res = rx.await.map_err(|_| WsClientError::UnexpectedClose)??;
        let resp = serde_json::from_str(res.get())?;
        Ok(resp)
    }
}

#[async_trait]
impl JsonRpcClient for FastWsClient {
    type Error = WsClientError;
    // this is taken verbatim from ethers_providers::WsClient for compatibility
    async fn request<T, R>(&self, method: &str, params: T) -> Result<R, WsClientError>
    where
        T: Serialize + Send + Sync,
        R: DeserializeOwned,
    {
        let params = to_raw_value(&params)?;
        let res = self.make_request(method, params).await?;

        Ok(res)
    }
}

impl fmt::Debug for FastWsClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FastWsClient").finish_non_exhaustive()
    }
}
