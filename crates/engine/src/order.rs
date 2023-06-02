//! Order execution service
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use ethers::{
    contract::FunctionCall,
    prelude::abigen,
    types::{BlockNumber, Bytes, Chain, TxHash, U256},
};
use ethers_providers::{Middleware, PendingTransaction};
use ethers_signers::{LocalWallet, Signer};
use futures::{
    future::{select_all, select_ok},
    AsyncReadExt,
};
use log::{debug, error, info, trace};
use thingbuf::mpsc::{channel, Sender};
use tokio::select;

use crate::price_graph::CompositeTrade;
use fulcrum_ws_cli::{serialize_hex, HttpClient, Response, SendRawTxResponse};

/// Official sequencer rpc endpoint
const ARB_SEQUENCER_HTTPS: &str = "https://arb1-sequencer.arbitrum.io/rpc";
/// Arbitrum public rpc endpoint
const ARB_FULL_HTTPS: &str = "https://arb1.arbitrum.io/rpc";
/// Duration to keep alive tx submission connections
const HTTP_KEEP_ALIVE_S: Duration = Duration::from_secs(10);
/// Base fee per gas to use by default for order txs
const DEFAULT_BASE_FEE_PER_GAS: u64 = 200_000_000_u64;

abigen!(
    FulcrumExecutor,
    r#"[
        function swap(uint128 amountIn, uint128 payload) external
        function flashSwap(uint128 amountIn, uint128 payload) external
    ]"#,
);

#[derive(Debug, PartialEq)]
pub enum OrderError {
    /// Error while generating tx signature
    TxSigning,
    /// Error while sending transaction to the network
    TxSubmit,
    /// Error while decoding send tx response
    TxSubmitResponse,
    /// Error while waiting for tx to be included in the chain
    TxInclusion,
    /// Another tx is pending
    Busy,
}

/// Status of an order tx
#[derive(Copy, Clone)]
pub enum OrderTxStatus {
    // Order submitted to the network
    Submitted(Instant),
    // Order submitted to the network and response received
    Received(TxHash),
}

/// Provides trade order execution service
pub struct OrderService<M: Middleware + 'static> {
    /// Ethereum JSON-RPC client (ws)
    client: Arc<M>,
    /// Tx signer
    wallet: LocalWallet,
    /// Contract entrypoint for executing orders
    contract: FulcrumExecutor<M>,
    /// Latest known 'max fee per gas'
    max_fee_per_gas: U256,
    /// Http conn to sequencer RPC
    sequencer_client: HttpClient,
}

impl<M> OrderService<M>
where
    M: Middleware + 'static,
{
    #[cfg(test)]
    /// Return the provider
    fn provider(&self) -> Arc<M> {
        self.client.clone()
    }
    /// Instantiate a new `OrderService`
    /// - `contract` where to send order txs (i.e smart contract)
    /// - `order_fee` the uniswap v3 pool fee tier for flash loans
    /// - `wallet` account to execute transactions, wrapped in ethers-signer implementation
    pub async fn new(
        client: Arc<M>,
        chain: Chain,
        contract: FulcrumExecutor<M>,
        wallet: LocalWallet,
    ) -> OrderService<M> {
        assert_eq!(chain as u64, wallet.chain_id(), "incompatible chain IDs");
        assert_eq!(
            wallet.address(),
            client.default_sender().expect("default sender configured"),
            "configure wallet & provider"
        );

        Self {
            sequencer_client: fulcrum_ws_cli::make_http_client(HTTP_KEEP_ALIVE_S),
            client,
            contract,
            wallet,
            max_fee_per_gas: DEFAULT_BASE_FEE_PER_GAS.into(),
        }
    }
    /// Start the order service
    /// `dry_run` - if true do not submit the built order txs
    pub async fn start(self, dry_run: bool) -> Sender<(u128, CompositeTrade)> {
        let mut nonce = self
            .client
            .get_transaction_count(self.wallet.address(), None)
            .await
            .expect("nonce fetched");
        info!(
            "config: order account: {:?}, nonce: {:?}",
            self.wallet.address(),
            nonce
        );

        let (tx, rx) = channel(5);
        let mut warm_interval = tokio::time::interval(HTTP_KEEP_ALIVE_S - Duration::from_secs(5)); // ensure slightly less than timeout
                                                                                                   // The ideal interval for base fee update (unused for now as simply over-estimating is fine i.e tx submitted, min fee charged)
        tokio::spawn({
            let mut inflight_guard = None;
            async move {
                loop {
                    select! {
                        biased;
                        trade_request = rx.recv() => {
                            if let Some((amount_in, ref trade)) = trade_request {
                                match self.flash_swap(nonce, amount_in, trade, &mut inflight_guard, dry_run).await {
                                    Err(OrderError::Busy) => info!("another tx is pending: #{:?}", nonce.as_u32()),
                                    _ => nonce += U256::one(),
                                }
                            }
                        }
                        _ = warm_interval.tick() => self.warm_connections(),
                    }
                }
            }
        });

        tx
    }
    /// Provide some local estimation of transaction `gas_limit`
    const fn calculate_gas() -> u64 {
        // from foundry gas reports + 100%
        (613_827_u64 + 50_124) * 2
    }
    /// Update gas price querying the configured chain
    pub async fn sync_base_fee(&mut self) {
        let t0 = Instant::now();
        let base_fee_per_gas = match self.client.get_block(BlockNumber::Latest).await {
            Ok(Some(block)) => block
                .base_fee_per_gas
                .map(|b| 2 * b.as_u64()) // 2x ensures base fee is suitable for upto 6 blocks
                .unwrap_or(DEFAULT_BASE_FEE_PER_GAS),
            _ => DEFAULT_BASE_FEE_PER_GAS,
        };
        // Arbitrum does not consider max_priority_fee
        self.max_fee_per_gas = base_fee_per_gas.into();
        debug!("update gas ⛽️: {:?}", Instant::now() - t0);
    }
    /// Keep the order submission connections warm
    pub fn warm_connections(&self) {
        tokio::spawn({
            let http_client = self.sequencer_client.clone();
            async move {
                let t0 = Instant::now();
                let warm_futs = [
                    http_client.post_async(
                        ARB_SEQUENCER_HTTPS,
                        r#"{"method":"eth_chainId","params":[]}"#,
                    ),
                    http_client
                        .post_async(ARB_FULL_HTTPS, r#"{"method":"eth_chainId","params":[]}"#),
                ];
                // mark trade as in flight
                let (res1, _, other) = select_all(warm_futs).await;
                if let Err(err) = res1 {
                    error!("warm seq conn(1): {:?}", err);
                }
                let (res2, _, _) = select_all(other).await;
                if let Err(err) = res2 {
                    error!("warm seq conn(2): {:?}", err);
                }
                debug!("warm conns 🔥: {:?}", Instant::now() - t0);
            }
        });
    }
    /// Returns current max fee per gas for the configured chain
    pub fn max_fee_per_gas(&self) -> u64 {
        self.max_fee_per_gas.as_u64()
    }
    /// Construct contract call for order execution given the trade `path`
    /// - `fee_tier` the fee tier for the initial loan pool denoted by `path[0]`
    fn build_call(&self, amount_in: u128, trade: &CompositeTrade) -> FunctionCall<Arc<M>, M, ()> {
        // somewhat pathological attempt at optimizing for encoding speed e.g vs using RLP crate and typical solidity ABI
        // pack the trade path as a u128, contract uses lookup tables with mirrored enums and addresses
        // used by this client
        // ~50 dead bits in `payload`
        //  32 unused bits + ~18 bits reclaimable if use some tighter assumptions about ranges

        let path = &trade.path;
        // dex/exchange Id 8 (bits)
        let mut payload = path[0].exchange_id as u128;
        payload |= (path[1].exchange_id as u128) << 8;
        payload |= (path[2].exchange_id as u128) << 16;

        // token path a,b,c (8 bits)
        payload |= (path[0].token_in as u128) << 24;
        payload |= (path[0].token_out as u128) << 32;
        if path[0].token_in != path[1].token_out {
            payload |= (path[1].token_out as u128) << 40;
        } else {
            // an unused number that will map to the 0 address
            payload |= 255_u128 << 40;
        }

        // pair fee tiers 16 bits each
        payload |= (path[0].fee_tier as u128) << 48;
        payload |= (path[1].fee_tier as u128) << 64;
        payload |= (path[2].fee_tier as u128) << 80;
        // 3 + 3 + 6 bytes = 24 hex chars, 32 bits unused
        trace!("payload: {:032x}", payload);

        /*
            let method = [235, 51, 224, 234];
            let data = encode_function_data(function, args)?;
            let tx = Eip1559TransactionRequest {
                to: Some(self.address.into()),
                data: Some(data),
                ..Default::default()
            };
            let tx: TypedTransaction = tx.into();
        }
        */
        // TODO: simplify to the above
        self.contract.flash_swap(amount_in, payload)
    }

    /// Execute a flash swap along `path` loaning `amount_in` from the uniswap v3 pool specified with `path[0]`
    async fn flash_swap(
        &self,
        nonce: U256,
        amount_in: u128,
        trade: &CompositeTrade,
        inflight: &mut Option<OrderTxStatus>,
        dry_run: bool,
    ) -> Result<(), OrderError> {
        let t0 = Instant::now();
        match inflight {
            None => {}
            Some(OrderTxStatus::Submitted(timestamp)) => {
                if t0.duration_since(*timestamp) < Duration::from_secs(2) {
                    return Err(OrderError::Busy);
                } else {
                    debug!("removing stale tx");
                    let _ = inflight.take();
                }
            }
            Some(OrderTxStatus::Received(_)) => {
                return Err(OrderError::Busy);
            }
        }

        // Build tx
        let mut flash_swap_call = self.build_call(amount_in, trade);
        let tx = flash_swap_call
            .tx
            .set_chain_id(self.wallet.chain_id())
            .set_nonce(nonce)
            .set_gas_price(self.max_fee_per_gas)
            .set_gas(Self::calculate_gas())
            .set_to((*self.contract).address());
        let signature = self
            .wallet
            // TODO(optimization):
            // EC math causing most of slowness need special hardware
            // some unnecessary copy and mem-move in here
            .sign_transaction_sync(tx)
            .map_err(|_| OrderError::TxSigning)?;
        // TODO(optimization):
        // rlp encodes the tx, allocs a string+vec each time
        let request = create_send_raw_tx_json(&tx.rlp_signed(&signature));
        let send_raw_tx_futs = [
            self.sequencer_client
                .post_async(ARB_SEQUENCER_HTTPS, request.as_str()),
            self.sequencer_client
                .post_async(ARB_FULL_HTTPS, request.as_str()),
        ];
        if dry_run {
            info!("built tx: {:?}", Instant::now() - t0);
            debug!("{request}");
            return Ok(());
        }

        // sending tx
        // mark trade as in flight
        *inflight = Some(OrderTxStatus::Submitted(t0));
        let result = select_ok(send_raw_tx_futs).await;
        info!("sent tx #{}: {:?}", nonce.as_u32(), Instant::now() - t0);

        // we are less performance critical after the order is submitted
        let tx_hash = match result {
            Ok((response, _)) => {
                // the tx sent ok, inc local nonce
                decode_send_raw_tx_response(response)
                    .await
                    .map_err(|_| OrderError::TxSubmitResponse)
            }
            Err(err) => {
                error!("tx submit #{}: {:?}", nonce.as_u32(), err);
                Err(OrderError::TxSubmit)
            }
        }?;
        // mark trade as received
        *inflight = Some(OrderTxStatus::Received(tx_hash));
        debug!("watching tx: {:?}", tx_hash);
        // on error we could await the other future
        let receipt = PendingTransaction::new(tx_hash, self.client.provider())
            .await
            .map_err(|err| {
                error!("tx inclusion: {:?}", err);
                OrderError::TxInclusion
            })?;
        debug!("tx execution\n{:?}", receipt);

        *inflight = None;
        Ok(())
    }
}

/// Decode an Ethereum JSON-RPC 'eth_sendRawTransaction' response payload, returning the tx hash
async fn decode_send_raw_tx_response(response: Response) -> Result<TxHash, ()> {
    // TODO: fix this
    let mut body = response.into_body();
    let mut buf = Vec::with_capacity(128);
    if let Err(err) = body.read_to_end(&mut buf).await {
        error!("tx response: {:?}", err);
        return Err(());
    }

    match serde_json::from_slice(buf.as_ref()) {
        Ok(SendRawTxResponse { result, .. }) => Ok(result),
        Err(err) => {
            error!("tx response: {:?} for {:?}", err, unsafe {
                core::str::from_utf8_unchecked(buf.as_slice())
            });
            Err(())
        }
    }
}

/// Encode an Ethereum JSON-RPC 'eth_sendRawTransaction' payload
fn create_send_raw_tx_json(signed_tx: &Bytes) -> String {
    let hexed_tx = serialize_hex(signed_tx);
    format!(
        r#"{{"id":1337,"jsonrpc":"2.0","method":"eth_sendRawTransaction","params":["0x{}"]}}"#,
        hexed_tx
    )
}

#[cfg(test)]
mod test {
    use std::{str::FromStr, sync::Arc};

    use ethers::{
        types::{
            transaction::eip2718::TypedTransaction, Address, Bytes, Chain, NameOrAddress,
            Transaction, TxHash, U256,
        },
        utils::rlp::Rlp,
    };
    use ethers_providers::{MockProvider, Provider};
    use ethers_signers::{LocalWallet, Signer};
    use hex_literal::hex;

    use fulcrum_ws_cli::AsyncBody;

    use crate::price_graph::{CompositeTrade, Trade};

    use super::*;

    /// Instantiate a new `OrderService` ready for test
    async fn make_service() -> OrderService<Provider<MockProvider>> {
        let wallet = "0000000000000000000000000000000000000000000000000000000000000001"
            .parse::<LocalWallet>()
            .unwrap()
            .with_chain_id(Chain::Arbitrum);

        let provider =
            Provider::<MockProvider>::new(MockProvider::new()).with_sender(wallet.address());
        let provider = Arc::new(provider);

        (*(provider.clone()))
            .as_ref()
            .push(U256::from(5))
            .expect("response mocked");

        let contract = FulcrumExecutor::new(Address::from_low_u64_be(u64::MAX), provider.clone());
        let service = OrderService::new(provider.clone(), Chain::Arbitrum, contract, wallet).await;

        return service;
    }

    #[test]
    fn encode_send_raw_tx_json() {
        assert_eq!(
            create_send_raw_tx_json(&Bytes::from_static(b"10334551124512451245012343241234")),
            r#"{"id":1337,"jsonrpc":"2.0","method":"eth_sendRawTransaction","params":["0x3130333334353531313234353132343531323435303132333433323431323334"]}"#,
        );
    }

    #[tokio::test]
    async fn decode_send_raw_tx_response_to_tx_hash() {
        let body = AsyncBody::from(
            serde_json::json!({
                    "id": 1,
                    "jsonrpc": "2.0",
                    "result": "0xf4309c18697d471c4569af9221b3cf6f0a3926ebc5bc27db2087f28c6af5d8af",
            })
            .to_string(),
        );
        let response = Response::new(body);

        assert_eq!(
            decode_send_raw_tx_response(response).await.unwrap(),
            TxHash::from_str("f4309c18697d471c4569af9221b3cf6f0a3926ebc5bc27db2087f28c6af5d8af")
                .expect("valid tx hash"),
        );
    }

    #[tokio::test]
    async fn build_call_works() {
        let service = make_service().await;

        let path = CompositeTrade::new([
            Trade::new(1, 2, 500, 1),
            Trade::new(2, 1, 3000, 1),
            Trade::default(),
        ]);
        let call = service.build_call(10_000000_u128, &path);

        assert_eq!(call.tx.rlp(), Bytes::from_static(
            hex!("02f862808080808094000000000000000000000000ffffffffffffffff80b844eb33e0ea0000000000000000000000000000000000000000000000000000000000989680000000000000000000000000000000000000000000000bb801f4ff0201000101c0").as_slice()
        ));

        let path2 = CompositeTrade::new([
            Trade::new(3, 2, 3_000, 0),
            Trade::new(2, 1, 500, 1),
            Trade::new(1, 3, 0, 1),
        ]);
        let call2 = service.build_call(10_000000_u128, &path2);

        assert_eq!(call2.tx.rlp(), Bytes::from_static(
            hex!("02f862808080808094000000000000000000000000ffffffffffffffff80b844eb33e0ea00000000000000000000000000000000000000000000000000000000009896800000000000000000000000000000000000000000000001f40bb8010203010100c0").as_slice()
        ));
    }

    #[tokio::test]
    async fn sync_base_fee_works() {
        let mut service = make_service().await;
        (*service.provider())
            .as_ref()
            .push(U256::from(3_000_000_000_u64))
            .expect("response mocked");

        service.sync_base_fee().await;
        assert_eq!(service.max_fee_per_gas(), 3_000_000_000_u64 * 2);
    }

    #[tokio::test]
    async fn bench_flash_swap_presend() {
        // try rust-secpk256k1 (btc core bindings) or needs some AVX hardware
        // ~55-75µs
        let service = make_service().await;
        let trade = CompositeTrade::new([
            Trade::new(3, 2, 3_000, 0),
            Trade::new(2, 1, 500, 1),
            Trade::new(1, 3, 0, 1),
        ]);

        let mut total = Duration::ZERO;
        let mut inflight_status = None;
        for i in 0..100 {
            let start = Instant::now();
            let result = service
                .flash_swap(
                    U256::one(),
                    100_000000_u128,
                    &trade,
                    &mut inflight_status,
                    true,
                )
                .await;
            assert_eq!(result, Ok(()));
            total += Instant::now().duration_since(start);
        }
        println!("mean: {:?}", total.as_micros() as f64 / 100_f64);
    }

    // TODO: setup mocking for http client
    // #[ignore]
    // #[tokio::test]
    // async fn flash_swap_works() {
    //     let service = make_service().await;
    //     let provider = service.provider();
    //     assert_eq!(service.nonce, U256::from(5));

    //     let trade = CompositeTrade::new([
    //         Trade::new(3, 2, 3_000, 0),
    //         Trade::new(2, 1, 500, 1),
    //         Trade::new(1, 3, 0, 1),
    //     ]);
    //     // push eth_sendRawTransaction response
    //     let fake_tx_hash = TxHash(hex!(
    //         "d5ac65792636f33afecfb829a42497c7062ee846b4e9bb16da7ddd67a8035b41"
    //     ));
    //     (*provider)
    //         .as_ref()
    //         .push(fake_tx_hash)
    //         .expect("response mocked");

    //     // push eth_getTransactionByHash response
    //     let tx_response = Transaction {
    //         hash: fake_tx_hash,
    //         nonce: service.nonce.get(),
    //         gas: OrderService::<Provider<MockProvider>>::calculate_gas().into(),
    //         gas_price: Some(service.max_fee_per_gas().into()),
    //         from: service.wallet.address(),
    //         to: Some(service.contract.address()),
    //         block_number: Some(123_u64.into()),
    //         chain_id: Some((service.wallet.chain_id()).into()),
    //         ..Default::default()
    //     };
    //     (*provider)
    //         .as_ref()
    //         .push::<Bytes, _>(tx_response.rlp())
    //         .expect("response mocked");

    //     // Test
    //     let result = service.flash_swap(100_000000_u128, &trade, false).await;
    //     assert_eq!(result, Ok(()));

    //     assert!((*provider)
    //         .as_ref()
    //         .assert_request(
    //             "eth_getTransactionCount",
    //             [
    //                 "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf".to_string(),
    //                 "latest".to_string()
    //             ]
    //         )
    //         .is_ok());

    //     let expected_tx_bytes = hex!("02f8b282a4b105840bebc200840bebc200830b24ea94000000000000000000000000ffffffffffffffff80b844f3bfa1f30000000000000000000000000000000000000000000000000000000005f5e1000000000000000000000000000000000000000000000001f40bb8010203010100c080a0d4bcf9f6dd6161d7783a9d1fa36c791c3cc034e024da850d32caa0899e1a4d6aa068967bdd28cee82bfde6dcc0b70ed2231837f601c17a57d4695f1fad49eefb65").as_slice();
    //     let expected_tx_hex = format!("0x{}", hex::encode(expected_tx_bytes));
    //     assert!((*provider)
    //         .as_ref()
    //         .assert_request("eth_sendRawTransaction", [expected_tx_hex])
    //         .is_ok());

    //     let tx_rlp = Rlp::new(expected_tx_bytes.as_ref());
    //     let (tx, _sig) = TypedTransaction::decode_signed(&tx_rlp).unwrap();
    //     assert_eq!(*tx.from().expect("from set"), service.wallet.address());
    //     assert_eq!(
    //         *tx.to().expect("to set"),
    //         NameOrAddress::Address(service.contract.address())
    //     );
    //     assert_eq!(*tx.nonce().expect("nonce set"), service.nonce.get());
    //     assert_eq!(
    //         tx.gas_price().expect("gas price set").as_u64(),
    //         service.max_fee_per_gas()
    //     );
    //     assert_eq!(
    //         tx.gas().expect("gas set").as_u64(),
    //         OrderService::<Provider<MockProvider>>::calculate_gas(),
    //     );

    //     assert_eq!(service.nonce.get(), U256::from(6));
    // }
}
