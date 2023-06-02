//! Price service provides queries for onchain token data

use std::{ops::DerefMut, sync::Arc, time::Duration};

use ethabi_static::{BytesZcp, DecodeStatic};
use ethers::{
    prelude::abigen,
    types::{Address, BlockId, Bytes, U256},
    utils::serialize,
};
use ethers_providers::{Middleware, WsClientError};
use hex_literal::hex;
use log::{debug, warn};
use serde::Serialize;
use serde_json::{value::to_raw_value, Value};
use thingbuf::mpsc::{Receiver, Sender};

use fulcrum_ws_cli::FastWsClient;

use crate::{
    price_graph::{Edge, PriceGraph},
    types::Pair,
    uniswap_v2::UniswapV2Reserves,
    uniswap_v3::UniswapV3Slot0,
};

#[cfg(target_os = "linux")]
const QUERY_DEADLINE: Duration = Duration::from_millis(10); // prod
#[cfg(not(target_os = "linux"))]
const QUERY_DEADLINE: Duration = Duration::from_millis(500); // dev

/// Deployed Pool Viewer address
static VIEWER_ADDRESS: [u8; 20] = hex!("e8291c77c9ED8b929147784b8fC3843582E98EA8");

abigen!(
    UniswapPoolViewer,
    r#"[
        function getPoolData(bytes calldata v3Pools, bytes calldata v2Pools) public view returns (bytes memory v3PoolData, bytes memory v2PoolData)
    ]"#,
);

/// Provides queries and aggregations over multiple price sources
pub struct PriceService<M: Middleware + 'static> {
    /// Provider handle
    client: Arc<M>,
    /// Uniswap v3 pools
    uniswap_v3_pairs: Vec<Pair>,
    /// Uniswap v2 (style) pools
    uniswap_v2_pairs: Vec<Pair>,
    // prebuilt contract call params to avoid re-serialization in hot loop
    pool_data_call: Value,
}

impl<M> PriceService<M>
where
    M: Middleware<Provider = FastWsClient> + 'static,
    // <M as Middleware>::Provider: JsonRpcClient<Error = WsClientError>,
{
    /// Create a new `PriceService`
    pub fn new(
        client: Arc<M>,
        uniswap_v2_pairs: &[(Pair, Address)],
        uniswap_v3_pairs: &[(Pair, Address)],
    ) -> PriceService<M> {
        // Pre-build all the contract calls for re-use on the hot-path
        let pool_data_call = build_call(uniswap_v2_pairs, uniswap_v3_pairs, client.clone());

        Self {
            client,
            pool_data_call,
            uniswap_v2_pairs: uniswap_v2_pairs.iter().map(|x| x.0).collect(),
            uniswap_v3_pairs: uniswap_v3_pairs.iter().map(|x| x.0).collect(),
        }
    }
    /// Get the current block number of the price source
    pub async fn block_number(&self) -> u64 {
        self.client
            .get_block_number()
            .await
            .unwrap_or_default()
            .as_u64()
    }
    /// Starts the price service
    ///
    /// Returns a handle for issuing price sync requests
    pub async fn start(&self) -> (Sender<u64>, Receiver<Option<PriceGraph>>) {
        let (price_sync_tx, price_sync_rx) = thingbuf::mpsc::channel(5);
        let (price_queue_tx, price_queue_rx) = thingbuf::mpsc::channel(5);

        let mut buffers = Buffers::new();
        let client = Arc::clone(&self.client);
        let serialized_call_params = self.pool_data_call.clone();
        let v2_pairs = self.uniswap_v2_pairs.clone();
        let v3_pairs = self.uniswap_v3_pairs.clone();

        tokio::spawn({
            async move {
                while let Some(target_block) = price_sync_rx.recv().await {
                    buffers.reset();
                    if let Err(err) =
                        sync_prices(&client, target_block, &serialized_call_params, &mut buffers)
                            .await
                    {
                        warn!("price fetch (#{target_block}): {:?}", err);
                        let mut price_graph_ref =
                            price_queue_tx.send_ref().await.expect("capacity");
                        *price_graph_ref = Option::<PriceGraph>::None;
                    } else {
                        let mut price_graph_opt_ref =
                            price_queue_tx.send_ref().await.expect("capacity");
                        let price_graph_opt = DerefMut::deref_mut(&mut price_graph_opt_ref);
                        match price_graph_opt {
                            Some(p) => {
                                p.reset(target_block);
                                bootstrap_price_graph(
                                    p,
                                    v2_pairs.as_slice(),
                                    v3_pairs.as_slice(),
                                    &buffers.v2_reserves,
                                    &buffers.v3_slot0s,
                                );
                            }
                            None => {
                                let mut p = PriceGraph::empty();
                                bootstrap_price_graph(
                                    &mut p,
                                    v2_pairs.as_slice(),
                                    v3_pairs.as_slice(),
                                    &buffers.v2_reserves,
                                    &buffers.v3_slot0s,
                                );
                                *price_graph_opt_ref = Some(p);
                            }
                        }
                    }
                }
            }
        });

        (price_sync_tx, price_queue_rx)
    }
}

/// Fetch latest available prices/metadata from all sources
/// Compute heuristics for best prices to update the given price graph
async fn sync_prices<M>(
    client: &Arc<M>,
    at: u64,
    serialized_call_params: &Value,
    buffers: &mut Buffers,
) -> Result<(), WsClientError>
where
    M: Middleware<Provider = FastWsClient> + 'static,
{
    let target_block = serialize(&BlockId::Number(at.into()));
    let serialized_call_params_with_block =
        Arc::new(to_raw_value(&[serialized_call_params, &target_block]).unwrap());
    // Execute an eth_call to the chain receiving price info
    // returns the Ethereum RLP encoded bytes (de-hexed)
    // allow 2 attempts

    // TODO: this is racey and can fail
    // - ideas: query multiple sources
    // - use subscription/push approach (needs fast local node)
    for _attempt in 1..=2_u32 {
        let result = client
            .provider()
            .as_ref()
            .eth_call(&serialized_call_params_with_block, &mut buffers.return_data)
            .await;
        match result {
            Ok(_) => break,
            Err(WsClientError::JsonRpcError(json_rpc_err)) => {
                if json_rpc_err.code == -32_000_i64 {
                    // try syncing again
                    debug!("remote header #{at} not ready: {:?}", json_rpc_err);
                    tokio::time::sleep(QUERY_DEADLINE).await;
                } else {
                    warn!("remote header #{at}: {:?}", json_rpc_err);
                }
            }
            Err(err) => return Err(err),
        }
    }
    if buffers.return_data.is_empty() {
        return Err(WsClientError::TooManyReconnects); // TODO: proper error
    }

    decode_pools_data(
        buffers.return_data.as_slice(),
        &mut buffers.v3_slot0s,
        &mut buffers.v2_reserves,
    );

    Ok(())
}
/// bootstrap a price graph instance using the given price information
fn bootstrap_price_graph(
    price_graph: &mut PriceGraph,
    v2_pairs: &[Pair],
    v3_pairs: &[Pair],
    v2_reserves: &[UniswapV2Reserves],
    v3_slots: &[UniswapV3Slot0],
) {
    // calculate price heuristics for all v2 sources (query onchain reserves and calculate offline)
    for (
        Pair {
            token0,
            token1,
            fee,
            exchange_id,
        },
        UniswapV2Reserves {
            reserve_0,
            reserve_1,
        },
    ) in v2_pairs.iter().zip(v2_reserves.iter())
    {
        let edge = Edge::new_v2(*reserve_0, *reserve_1, *fee, *exchange_id);
        price_graph.add_edge(*token0, *token1, edge);
    }

    // calculate price heuristics for uniswap v3 pairs
    for (
        Pair {
            token0,
            token1,
            fee,
            ..
        },
        UniswapV3Slot0 {
            sqrt_p_x96,
            liquidity,
        },
    ) in v3_pairs.iter().zip(v3_slots.iter())
    {
        let edge = Edge::new_v3(*sqrt_p_x96, (*liquidity).into(), *fee, true);
        price_graph.add_edge(*token0, *token1, edge);
    }
}

/// Deserialize packed pools data into the given buffers
/// Uses a custom packed serialization not RLP/ethabi
fn decode_pools_data<'a>(
    raw_pool_data: &'a [u8],
    v3_slots: &mut Vec<UniswapV3Slot0>,
    v2_reserves: &mut Vec<UniswapV2Reserves>,
) {
    #[derive(DecodeStatic)]
    struct PoolData<'a> {
        v3_slots_data: BytesZcp<'a>,
        v2_reserves_data: BytesZcp<'a>,
    }
    let pool_data = PoolData::decode(raw_pool_data).expect("bytes 2-tuple");

    // decode v3 reserves
    let v3_slots_data = pool_data.v3_slots_data.as_ref();
    let pool_count = v3_slots_data.len() / 36; // 36 bytes == the size of each packed pool datum (160bit + 128bit)
    for idx in 0..pool_count {
        let offset = idx * 36;
        let sqrt_p_x96 = U256::from_big_endian(&v3_slots_data[offset..offset + 20]);
        let liquidity = u128::from_be_bytes(unsafe {
            *(v3_slots_data.get_unchecked(offset + 20..offset + 36) as *const [u8]
                as *const [u8; 16])
        });
        v3_slots.push(UniswapV3Slot0 {
            liquidity,
            sqrt_p_x96,
        });
    }

    // decode v2 reserves
    let v2_reserves_data = pool_data.v2_reserves_data.as_ref();
    let pool_count = v2_reserves_data.len() / 32; // 32 bytes == the size of each packed pool datum (128bit + 128bit)
    for idx in 0..pool_count {
        let offset = idx * 32;
        let reserve_0 = u128::from_be_bytes(unsafe {
            *(v2_reserves_data.get_unchecked(offset..offset + 16) as *const [u8] as *const [u8; 16])
        });
        let reserve_1 = u128::from_be_bytes(unsafe {
            *(v2_reserves_data.get_unchecked(offset + 16..offset + 32) as *const [u8]
                as *const [u8; 16])
        });
        v2_reserves.push(UniswapV2Reserves {
            reserve_0,
            reserve_1,
        });
    }
}

/// Return the prebuilt contract call i.e for an Eth-JSON RPC eth_call request
fn build_call<M: Middleware + 'static>(
    v2_pairs: &[(Pair, Address)],
    v3_pairs: &[(Pair, Address)],
    client: Arc<M>,
) -> Value {
    #[derive(Serialize)]
    struct CallRequestParams {
        pub data: Bytes,
        pub to: Address,
    }
    let pool_viewer = UniswapPoolViewer::new(VIEWER_ADDRESS, client);
    let mut v3_addresses = Vec::with_capacity(v3_pairs.len() * 20);
    for (_, pool_address) in v3_pairs.iter() {
        v3_addresses.extend_from_slice(&pool_address.0);
    }

    let mut v2_addresses = Vec::with_capacity(v2_pairs.len() * 20);
    for (_, pool_address) in v2_pairs.iter() {
        v2_addresses.extend_from_slice(&pool_address.0);
    }

    let pools_call =
        pool_viewer.get_pool_data(Bytes(v3_addresses.into()), Bytes(v2_addresses.into()));

    // removes extraneous fields
    let call_params = CallRequestParams {
        data: pools_call.tx.data().unwrap().clone(),
        to: *pools_call.tx.to().unwrap().as_address().unwrap(),
    };
    // let latest_block = serde_json::Value::String("latest".to_string());
    // let serialized_call_params = to_raw_value(&[&serialize(&call_params), &latest_block]).unwrap();
    serialize(&call_params)
}

/// Re-usable buffer for price queries
struct Buffers {
    return_data: Vec<u8>,
    v2_reserves: Vec<UniswapV2Reserves>,
    v3_slot0s: Vec<UniswapV3Slot0>,
}

impl Buffers {
    fn new() -> Self {
        Self {
            return_data: Vec::with_capacity(2048),
            v2_reserves: Vec::with_capacity(18),
            v3_slot0s: Vec::with_capacity(18),
        }
    }
    /// Reset the buffers
    fn reset(&mut self) {
        unsafe {
            self.return_data.set_len(0);
            self.v3_slot0s.set_len(0);
            self.v2_reserves.set_len(0);
        }
    }
}

#[cfg(test)]
mod test {
    use hex_literal::hex;

    use super::*;

    #[test]
    fn decode_v3_pool_data() {
        let mut v2_pool_data = Vec::<UniswapV2Reserves>::with_capacity(10);
        let mut v3_pool_data = Vec::<UniswapV3Slot0>::with_capacity(10);

        let buf = hex!("0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000000fc00000000000000000002cd2ebc00d3d87647d074000000000000000142e186bff48725c500000000000000000002cdd49150b8853d1518b800000000000000000c22f81dc383d7a700000000000000000000121437095d8fafca250700000000000000019164300c5bbc76c20000000000000027ab0a341aa02ea5f3f1f28dab0000000000014353db7630f26bb1d7e40000000000000027b66bdd1c8206e7c05f60f5fc0000000000018dd9dc9c7d1cc155985a00000000000000000002cd01f5b1925fe9e29afa0000000000000000451466246a5c602200000000000000010004ed64338acdd2e1e63a6d0000000000000000008ba6451fd0be080000000000000000000000000000000000000000000000000000000000000000000000c00000000000000090a985271d9311fb5900000000000000000000046d30a327e3000000000000006f999835a0a52e29a0000000000002aee774c2d30a625791f00000000000000160d83aeaa137ebc697000000000000000000000ad2e96b0759000000000000006e1bdc2aca5329f3180000000000000000000003610c8e90b8000000000000007ed070773c5750d9fd0000000000030caf4f30fa5b2e06b36c000000000000005641b7828c5b0cc2980000000000000000000002a54a96943b");
        decode_pools_data(&buf, &mut v3_pool_data, &mut v2_pool_data);

        println!("{:?}", v2_pool_data);
        println!("{:?}", v3_pool_data);

        assert_eq!(
            v2_pool_data.as_slice(),
            &[
                UniswapV2Reserves {
                    reserve_0: 2668546359186462735193,
                    reserve_1: 4867013945315
                },
                UniswapV2Reserves {
                    reserve_0: 2058656247230105528736,
                    reserve_1: 3243813018648698957566448
                },
                UniswapV2Reserves {
                    reserve_0: 6508834937784752653975,
                    reserve_1: 11900975515481
                },
                UniswapV2Reserves {
                    reserve_0: 2031149374690418094872,
                    reserve_1: 3715357380792
                },
                UniswapV2Reserves {
                    reserve_0: 2339309389145730767357,
                    reserve_1: 3686679743187219837793132
                },
                UniswapV2Reserves {
                    reserve_0: 1591155387411559400088,
                    reserve_1: 2908944241723
                }
            ]
        );

        assert_eq!(
            v3_pool_data.as_slice(),
            &[
                UniswapV3Slot0 {
                    sqrt_p_x96: 3386798865505532038860916_u128.into(),
                    liquidity: 23266025308972066245
                },
                UniswapV3Slot0 {
                    sqrt_p_x96: 3389857949033178074519736_u128.into(),
                    liquidity: 874534084381235111
                },
                UniswapV3Slot0 {
                    sqrt_p_x96: 85375497376946392278279_u128.into(),
                    liquidity: 28923295536516986562
                },
                UniswapV3Slot0 {
                    sqrt_p_x96: 3142832610048170119692050140587_u128.into(),
                    liquidity: 1526871267605972601919460
                },
                UniswapV3Slot0 {
                    sqrt_p_x96: 3146355009075363713121488270844_u128.into(),
                    liquidity: 1878798333881591289714778
                },
                UniswapV3Slot0 {
                    sqrt_p_x96: 3385972919054160141392634_u128.into(),
                    liquidity: 4977715794740535330
                },
                UniswapV3Slot0 {
                    sqrt_p_x96: 79234119266787650735450765933_u128.into(),
                    liquidity: 39307837579509256
                }
            ]
        );
    }
}
