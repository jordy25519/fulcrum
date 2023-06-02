//! Trade simulator

use ethabi_static::{AddressZcp, DecodeStatic, Tuple};
use ethers::types::U256;
use fulcrum_sequencer_feed::TransactionInfo;
use log::{debug, info, warn};

use crate::{
    constant::arbitrum::{CAMELOT_ROUTER, SUSHI_ROUTER},
    price_graph::Edge,
    trade_router::*,
    types::{ExchangeId, RouterId, Token},
    uniswap_v3::fee_from_path_bytes,
    zero_ex, PriceGraph,
};

/// Simulates trades locally against a price graph
pub struct TradeSimulator<'a> {
    /// The price graph to simulate trades onto
    graph: &'a mut PriceGraph,
    /// True if any essential trades were unable to be simulated
    skip: bool,
}

impl<'a> TradeSimulator<'a> {
    pub fn new(graph: &'a mut PriceGraph) -> Self {
        TradeSimulator { graph, skip: false }
    }
    /// True if any trades were skipped
    /// i.e this round of trading does not have accurate local prices
    pub fn skipped(&self) -> bool {
        self.skip
    }
    /// Apply the trade if possible
    /// - `exact_in` true if `trade` is adding exact amount of tokens to the pool
    fn try_run_trade<const D: bool>(&mut self, trade: &TradeInfo) {
        // TODO: could be clever here and simulate only trades that are dependent on prices we care about
        // its not clear how useful this would be, effort required for the dependency graph implementation, or performance gain/loss
        if trade.path.is_empty() {
            // not a trade we're monitoring
            debug!("trade on unknown paths");
            return;
        }
        // trade had a component we aren't monitoring
        if !trade.unknown.is_empty() {
            for (token_in, token_out, fee) in trade.unknown.iter() {
                // TODO: the 1inch output here is garbage
                warn!("needed 🏊‍♂️: {:x}/{:x} ({fee})", token_in, token_out);
            }
            self.skip = true;
            return;
        }

        // TODO: monomorphic
        if D {
            // apply the trade
            let mut amount_in = trade.amount.as_u128();
            for (token_in, token_out, fee) in trade.path.iter() {
                // if we fail here there is a pool we aren't monitoring explicitly e.g different fee tier or token combination
                debug!("update edge: {:?}/{:?}/{fee}", token_in, token_out);
                // all v3 edges are stored with zero for one value
                let edge_id = Edge::hash(
                    *token_in as u8,
                    *token_out as u8,
                    trade.exchange_id as u8,
                    (*fee) as u16,
                );
                // outputs the next amount in for the subsequent trade
                debug!("selling: {:?}{:?}", amount_in, token_in);
                if let Ok(amount_out) = self
                    .graph
                    .update_edge_in(*token_in, *token_out, edge_id, amount_in)
                {
                    amount_in = amount_out;
                    debug!("received: {:?}{:?}", amount_in, token_out);
                } else {
                    // usually a missing edge is a fee tier we aren't interested in
                    info!(
                        "missing pool: {:?}/{:?}/{fee} {:?}",
                        token_in, token_out, trade.exchange_id
                    );
                    return;
                }
            }
        } else {
            // apply the trade
            let mut amount_out = trade.amount.as_u128();
            for (token_out, token_in, fee) in trade.path.iter() {
                // if we fail here there is a pool we aren't monitoring explicitly e.g different fee tier or token combination
                debug!("update edge: {:?}/{:?}/{fee}", token_in, token_out);
                // all v3 edges are stored with zero for one value
                let edge_id = Edge::hash(
                    *token_in as u8,
                    *token_out as u8,
                    trade.exchange_id as u8,
                    (*fee) as u16,
                );
                // outputs the next amount out for the subsequent trade
                debug!("requesting: {:?}{:?}", amount_out, token_out);
                if let Ok(amount_in) = self
                    .graph
                    .update_edge_out(*token_out, *token_in, edge_id, amount_out)
                {
                    amount_out = amount_in;
                    debug!("owed: {:?}{:?}", amount_out, token_in);
                } else {
                    // usually a missing edge is a fee tier we aren't interested in
                    info!(
                        "missing pool: {:?}/{:?}/{fee} {:?}",
                        token_in, token_out, trade.exchange_id
                    );
                    return;
                }
            }
        }
    }
    /// Extract trade information from raw transactions and apply locally if possible
    ///
    /// Note: there will always be some transactions with trades we cannot simulate e.g. routed through some custom contract
    /// this is a best effort, accuracy for speed tradeoff
    /// this could be refactored but we are interested in performance (less branching)
    pub fn wrangle_transaction(&mut self, tx: &TransactionInfo) {
        // need atleast 4 bytes of input to call a contract method
        if tx.input.len() < 5 {
            return;
        }

        // TODO: this needs some clean up e.g. visitor pattern
        if let Some(router_id) = ROUTERS.get(&tx.to.0) {
            let selector: [u8; 4] = unsafe { tx.input.get_unchecked(0..4) }.try_into().unwrap(); // length asserted prior
            let buf = &tx.input[4..];

            // we expect inputs to be well-formed, this is brittle but most inputs should be well formed anyway
            // i.e. we're  willing to tolerate the occasional panic and restart for improved normal case
            match router_id {
                RouterId::UniswapV3RouterV1 => {
                    if selector == UNISWAP_V3_V1_EXACT_INPUT {
                        debug!("🦄1 exact input");
                        let swap = UniswapV3ExactInputParamsV1::decode(buf).unwrap();
                        self.v3_path_to_trade_info::<true>(swap.path.as_ref(), swap.amount_in);
                    } else if selector == UNISWAP_V3_V1_EXACT_OUTPUT {
                        debug!("🦄1 exact output");
                        let swap = UniswapV3ExactOutputParamsV1::decode(buf).unwrap();
                        self.v3_path_to_trade_info::<false>(swap.path.as_ref(), swap.amount_out);
                    } else if selector == UNISWAP_V3_V1_EXACT_INPUT_SINGLE {
                        debug!("🦄1 exact input single");
                        let UniswapV3ExactInputSingleParamsV1 {
                            amount_in,
                            token_in,
                            token_out,
                            fee,
                            ..
                        } = UniswapV3ExactInputSingleParamsV1::decode(buf).unwrap();
                        self.try_run_trade::<true>(&exact_single_to_trade_info(
                            token_in.as_ref(),
                            token_out.as_ref(),
                            amount_in,
                            fee,
                        ));
                    } else if selector == UNISWAP_V3_V1_EXACT_OUTPUT_SINGLE {
                        debug!("🦄1 exact output single");
                        let UniswapV3ExactOutputSingleParamsV1 {
                            token_in,
                            token_out,
                            amount_out,
                            fee,
                            ..
                        } = UniswapV3ExactOutputSingleParamsV1::decode(buf).unwrap();
                        self.try_run_trade::<false>(&exact_single_to_trade_info(
                            token_out.as_ref(),
                            token_in.as_ref(),
                            amount_out,
                            fee,
                        ));
                    } else if selector == UNISWAP_V3_MULTI_CALL {
                        debug!("🦄1 multicall");
                        let multi_call = UniswapV3MultiCall::decode(buf).unwrap();
                        for call in multi_call.data.iter() {
                            self.wrangle_transaction(&TransactionInfo {
                                to: tx.to,
                                value: tx.value,
                                input: call.as_ref(),
                            });
                        }
                    } else if selector == UNISWAP_V3_MULTI_CALL_DEADLINE {
                        debug!("🦄1 multicall deadline");
                        let multi_call = UniswapV3MultiCallDeadline::decode(buf)
                            .map_err(|err| {
                                warn!("{:02x?}", buf);
                                err
                            })
                            .unwrap();
                        for call in multi_call.data.iter() {
                            self.wrangle_transaction(&TransactionInfo {
                                to: tx.to,
                                value: tx.value,
                                input: call.as_ref(),
                            });
                        }
                    } else {
                        debug!("unhandled 🦄1: {:02x?}", selector);
                    }
                }
                RouterId::UniswapV3RouterV2 => {
                    if selector == UNISWAP_V3_V2_EXACT_INPUT {
                        debug!("🦄2 exact input");
                        let swap = UniswapV3ExactInputParamsV2::decode(buf).unwrap();
                        self.v3_path_to_trade_info::<true>(swap.path.as_ref(), swap.amount_in);
                    } else if selector == UNISWAP_V3_V2_EXACT_OUTPUT {
                        debug!("🦄2 exact output");
                        let swap = UniswapV3ExactOutputParamsV2::decode(buf).unwrap();
                        self.v3_path_to_trade_info::<false>(swap.path.as_ref(), swap.amount_out);
                    } else if selector == UNISWAP_V3_V2_EXACT_INPUT_SINGLE {
                        debug!("🦄2 exact input single");
                        let UniswapV3ExactInputSingleParamsV2 {
                            token_in,
                            token_out,
                            amount_in,
                            fee,
                            ..
                        } = UniswapV3ExactInputSingleParamsV2::decode(buf).unwrap();
                        self.try_run_trade::<true>(&exact_single_to_trade_info(
                            token_in.as_ref(),
                            token_out.as_ref(),
                            amount_in,
                            fee,
                        ));
                    } else if selector == UNISWAP_V3_V2_EXACT_OUTPUT_SINGLE {
                        debug!("🦄1 exact output single");
                        let UniswapV3ExactOutputSingleParamsV2 {
                            token_in,
                            token_out,
                            amount_out,
                            fee,
                            ..
                        } = UniswapV3ExactOutputSingleParamsV2::decode(buf).unwrap();
                        self.try_run_trade::<false>(&exact_single_to_trade_info(
                            token_out.as_ref(),
                            token_in.as_ref(),
                            amount_out,
                            fee,
                        ));
                    } else if selector == UNISWAP_V3_MULTI_CALL {
                        debug!("🦄2 multicall");
                        let multi_call = UniswapV3MultiCall::decode(buf).unwrap();
                        for call in multi_call.data.iter() {
                            self.wrangle_transaction(&TransactionInfo {
                                to: tx.to,
                                value: tx.value,
                                input: call.as_ref(),
                            });
                        }
                    } else if selector == UNISWAP_V3_MULTI_CALL_DEADLINE {
                        debug!("🦄2 multicall deadline");
                        let multi_call = UniswapV3MultiCallDeadline::decode(buf)
                            .map_err(|err| {
                                warn!("{:02x?}", buf);
                                err
                            })
                            .unwrap();
                        for call in multi_call.data.iter() {
                            self.wrangle_transaction(&TransactionInfo {
                                to: tx.to,
                                value: tx.value,
                                input: call.as_ref(),
                            });
                        }
                    } else {
                        debug!("unhandled 🦄2: {:02x?}", selector);
                    }
                }
                RouterId::UniswapV3UniversalRouter => {
                    if selector == UNISWAP_UNIVERSAL_ROUTER_EXECUTE
                        || selector == UNISWAP_UNIVERSAL_ROUTER_EXECUTE_DEADLINE
                    {
                        let params = UniswapV3UniversalExecuteParams::decode(buf).unwrap();
                        for (idx, command) in params.commands.as_ref().iter().enumerate() {
                            // V3_SWAP_EXACT_IN  0x00 https://docs.uniswap.org/contracts/universal-router/technical-reference
                            // V3_SWAP_EXACT_OUT 0x01 / 0b0000_0001
                            let command = command & 0x1f;
                            if command == 0x00_u8 {
                                debug!("🦄🌐 exact input {command}");
                                if let Ok(swap) = UniswapV3UniversalRouterSwapExactIn::decode(
                                    params.inputs[idx].as_ref(),
                                ) {
                                    self.v3_path_to_trade_info::<true>(
                                        swap.path.as_ref(),
                                        swap.amount_in,
                                    );
                                } else {
                                    warn!("{:02x?}", buf);
                                }
                            } else if command == 0x01_u8 {
                                debug!("🦄🌐 exact output {command}");
                                if let Ok(swap) = UniswapV3UniversalRouterSwapExactOut::decode(
                                    params.inputs[idx].as_ref(),
                                ) {
                                    self.v3_path_to_trade_info::<false>(
                                        swap.path.as_ref(),
                                        swap.amount_out,
                                    );
                                } else {
                                    warn!("{:02x?}", buf);
                                }
                            } else {
                                // command doing something we don't monitor
                                debug!("unhandled 🦄🌐: {:?}", command);
                            }
                        }
                    } else {
                        debug!("unhandled 🦄🌐: {:02x?}", selector);
                    }
                }
                // NB: we map v4 and V5 aggregator to same router Id
                RouterId::OneInch => {
                    debug!("🐴");
                    if selector == ONE_INCH_UNISWAP_V3_SWAP {
                        let params = OneInchUniswapV3Swap::decode(buf).unwrap();
                        let mut trade_info = TradeInfo {
                            amount: params.amount_in,
                            exchange_id: ExchangeId::Uniswap,
                            path: vec![],
                            unknown: vec![],
                        };
                        for pool in &params.pools {
                            let pool_bytes = pool.0;
                            let zero_for_one = pool_bytes[0] & 0x01 == 0;
                            let pool_address: [u8; 20] =
                                unsafe { *(&pool_bytes[12..32] as *const [u8] as *const [u8; 20]) };
                            if let Some(pool) = POOL_LOOKUP.get(&pool_address) {
                                if zero_for_one {
                                    trade_info.path.push((
                                        pool.token0,
                                        pool.token1,
                                        pool.fee as u32,
                                    ));
                                } else {
                                    trade_info.path.push((
                                        pool.token1,
                                        pool.token0,
                                        pool.fee as u32,
                                    ));
                                }
                            } else {
                                trade_info.unknown.push((
                                    pool_address.into(),
                                    pool_address.into(),
                                    0_u32,
                                ));
                            }
                        }
                        self.try_run_trade::<true>(&trade_info);
                    } else if selector == ONE_INCH_UNISWAP_V3_SWAP_TWP {
                        let params = OneInchUniswapV3SwapTWP::decode(buf).unwrap();
                        let mut trade_info = TradeInfo {
                            amount: params.amount_in,
                            exchange_id: ExchangeId::Uniswap,
                            path: vec![],
                            unknown: vec![],
                        };
                        for pool in &params.pools {
                            let pool_bytes = pool.0;
                            let zero_for_one = pool_bytes[0] & 0x01 == 0;
                            let pool_address: [u8; 20] =
                                unsafe { *(&pool_bytes[12..32] as *const [u8] as *const [u8; 20]) };
                            if let Some(pool) = POOL_LOOKUP.get(&pool_address) {
                                if zero_for_one {
                                    trade_info.path.push((
                                        pool.token0,
                                        pool.token1,
                                        pool.fee as u32,
                                    ));
                                } else {
                                    trade_info.path.push((
                                        pool.token1,
                                        pool.token0,
                                        pool.fee as u32,
                                    ));
                                }
                            } else {
                                trade_info.unknown.push((
                                    pool_address.into(),
                                    pool_address.into(),
                                    0_u32,
                                ));
                            }
                        }
                        self.try_run_trade::<true>(&trade_info);
                    } else if selector == ONE_INCH_UNISWAP_SWAP {
                        debug!("v2 swap 🐴 unhandled");
                    } else {
                        debug!("unhandled 🐴: {:02x?}", selector);
                    }
                }
                RouterId::ZeroEx => {
                    debug!("👌🙅‍♀️");
                    match selector {
                        ZERO_EX_TRANSFORM_ERC20 => {
                            use zero_ex::*;
                            let outer_transform: TransformErc20 =
                                <TransformErc20>::decode(buf).unwrap();
                            for t in outer_transform.transformations.0.as_slice() {
                                match t.deployment_nonce {
                                    FILL_QUOTE_TRANSFORMER_19 | FILL_QUOTE_TRANSFORMER_21 => {
                                        let data = Tuple::<FillQuoteTransformData>::decode(
                                            t.data.as_ref(),
                                        )
                                        .unwrap()
                                        .0;
                                        let orders = data.bridge_orders.0.as_slice();
                                        for order in orders {
                                            let protocol_id = order.source.0[15];
                                            info!(
                                                "👌🙅‍♀️ trade via: {}",
                                                core::str::from_utf8(&order.source.0[16..32])
                                                    .unwrap()
                                                    .trim_end()
                                            );
                                            if protocol_id == bridge_id::UNISWAPV3 {
                                                if !(data.fill_amount & *HIGH_BIT).is_zero() {
                                                    // 0x features allows specifying a ratio of user balance as fill amount
                                                    // we cant' simulate without pulling it from chain...
                                                    info!("0x can't simulate");
                                                    // TODO: signal skip via TradeInfo
                                                    return;
                                                }
                                                let v3_trade =
                                                    UniswapV3Mixin::decode(order.data.0).unwrap();
                                                self.v3_path_to_trade_info::<true>(
                                                    v3_trade.path.as_ref(),
                                                    data.fill_amount,
                                                )
                                            } else if protocol_id == bridge_id::UNISWAPV2 {
                                                let v2_trade =
                                                    UniswapV2Mixin::decode(order.data.0).unwrap();
                                                match v2_trade.router.0 {
                                                    &SUSHI_ROUTER => {
                                                        debug!("sushi via 1inch: {:?}", v2_trade);
                                                        // TODO: lookup fees from some constant
                                                        self.v2_path_to_trade_info::<true>(
                                                            v2_trade.path.as_slice(),
                                                            data.fill_amount,
                                                            300_u16,
                                                            ExchangeId::Sushi,
                                                        );
                                                    }
                                                    &CAMELOT_ROUTER => {
                                                        debug!("camelot via 1inch: {:?}", v2_trade);
                                                        self.v2_path_to_trade_info::<true>(
                                                            v2_trade.path.as_slice(),
                                                            data.fill_amount,
                                                            300_u16,
                                                            ExchangeId::Camelot,
                                                        );
                                                    }
                                                    _ => {
                                                        info!("uniswapV2 via 1inch: {:?}", v2_trade)
                                                    }
                                                }
                                            } else {
                                                // TODO: signal skip via TradeInfo
                                                info!("unhandled protocol Id: {:?}", protocol_id);
                                                return;
                                            }
                                        }
                                    }
                                    POSITIVE_SLIPPAGE_FEE_TRANSFORMER => (),
                                    PAY_TAKER_TRANSFORMER => (),
                                    AFFILIATE_FEE_TRANSFORMER => (),
                                    WETH_TRANSFORMER => (),
                                    _ => println!("unknown transformer: {:?}", t.deployment_nonce),
                                }
                            }
                        }
                        _ => debug!("unhandled 👌🙅‍♀️: {:02x?}", selector),
                    }
                }
                RouterId::Odos => {
                    // https://arbiscan.io/address/0xa0b07f9a11dfb01388149abbdbc5b4f2196600ab#code
                    // ODOS swap: simpler interface available non-opaque
                    // used by Chronos DeFi
                    // the bytecode is opaque and not publicly documented (ODOS wants to protect users from MEV)
                    // TODO: can atleast check which tokens are included and signal skip or not
                    if selector == ODOS_SWAP {
                        debug!("⏰ swap: {:?}", OdosSwap::decode(buf).unwrap());
                    } else {
                        debug!("⏰: {:02x?}", selector);
                    }
                }
                RouterId::SushiRouterV2 => {
                    // TODO: sushi 'RouteProcessor' needs scan also
                    if selector == SUSHI_SWAP_EXACT_ETH_FOR_TOKENS
                        || selector == SUSHI_SWAP_EXACT_ETH_FOR_TOKENS_SFOTT
                    {
                        let swap = SwapExactETHForTokens::decode(buf).unwrap();
                        self.v2_path_to_trade_info::<true>(
                            swap.path.as_slice(),
                            tx.value,
                            300_u16,
                            ExchangeId::Sushi,
                        );
                    } else if selector == SUSHI_SWAP_EXACT_TOKENS_FOR_ETH
                        || selector == SUSHI_SWAP_EXACT_TOKENS_FOR_ETH_SFOTT
                    {
                        let swap = SwapExactTokensForETH::decode(buf).unwrap();
                        self.v2_path_to_trade_info::<true>(
                            swap.path.as_slice(),
                            swap.amount_in,
                            300_u16,
                            ExchangeId::Sushi,
                        );
                    } else {
                        debug!("🍣: {:02x?} unhandled", selector);
                    }
                }
                RouterId::CamelotRouterV2 => {
                    if selector == CAMELOT_V2_SWAP_EXACT_ETH_FOR_TOKENS_SFOTT {
                        let swap = SwapExactETHForTokensSFOTT::decode(buf).unwrap();
                        self.v2_path_to_trade_info::<true>(
                            swap.path.as_slice(),
                            tx.value,
                            300_u16,
                            ExchangeId::Camelot,
                        );
                    } else if selector == CAMELOT_V2_SWAP_EXACT_TOKENS_FOR_ETH_SFOTT {
                        let swap = SwapExactTokensForEthSFOTT::decode(buf).unwrap();
                        self.v2_path_to_trade_info::<true>(
                            swap.path.as_slice(),
                            swap.amount_in,
                            300_u16,
                            ExchangeId::Camelot,
                        );
                    } else {
                        debug!("🛡️: {:02x?} unhandled", selector);
                    }
                }
                RouterId::Gmx => {}
                RouterId::ParaswapAugustus => {}
            }
        }
    }
    /// Build trade info from uniswap compliant `path` bytes
    fn v3_path_to_trade_info<const D: bool>(&mut self, path: &[u8], amount: U256) {
        if path.len() % 43 != 0 {
            return;
        }
        let trade_count = path.len() / 43; // 20 + 3 + 20 (uint160, uint24, uint160)
        let mut trade_info = TradeInfo {
            amount,
            exchange_id: ExchangeId::Uniswap,
            path: Vec::with_capacity(trade_count),
            unknown: vec![],
        };

        (0..trade_count).for_each(|idx| {
            let offset = idx * 43;
            let token_in: &[u8; 20] =
                &unsafe { *(&path[offset..offset + 20] as *const [u8] as *const [u8; 20]) };
            let fee = fee_from_path_bytes(&path[offset + 20..offset + 23]);
            let token_out: &[u8; 20] =
                &unsafe { *(&path[offset + 23..offset + 43] as *const [u8] as *const [u8; 20]) };

            let (a, b) = address_to_token(token_in, token_out);

            match (a, b) {
                (Some(a), Some(b)) => trade_info.path.push((a, b, fee)),
                _ => {
                    // trade is through a path we aren't monitoring locally
                    trade_info
                        .unknown
                        .push(((*token_in).into(), (*token_out).into(), fee));
                    debug!("{:02x?}/{:02x?}/{fee}", token_in, token_out);
                }
            }
        });

        self.try_run_trade::<D>(&trade_info);
    }
    /// Build trade info from uniswap compliant `path` bytes
    fn v2_path_to_trade_info<const D: bool>(
        &mut self,
        path: &[AddressZcp],
        amount: U256,
        fee: u16,
        exchange_id: ExchangeId,
    ) {
        let trade_count = path.len() - 1;
        let mut trade_info = TradeInfo {
            amount,
            exchange_id,
            path: Vec::with_capacity(trade_count),
            unknown: vec![],
        };

        (0..trade_count).for_each(|idx| {
            let token_in = path[idx].0;
            let token_out = path[idx + 1].0;
            let (a, b) = address_to_token(token_in, token_out);
            match (a, b) {
                (Some(a), Some(b)) => trade_info.path.push((a, b, fee as u32)),
                _ => {
                    // trade is through a path we aren't monitoring locally
                    trade_info
                        .unknown
                        .push(((*token_in).into(), (*token_out).into(), 0));
                    debug!("{:02x?}/{:02x?}/0", token_in, token_out);
                }
            }
        });

        self.try_run_trade::<D>(&trade_info);
    }
}

/// Build trade info from exact|output single
fn exact_single_to_trade_info(
    token_in: &[u8; 20],
    token_out: &[u8; 20],
    amount: U256,
    fee: u32,
) -> TradeInfo {
    let (a, b) = address_to_token(token_in, token_out);
    match (a, b) {
        (Some(a), Some(b)) => TradeInfo {
            path: vec![(a, b, fee)],
            unknown: vec![],
            amount,
            exchange_id: ExchangeId::Uniswap,
        },
        _ => TradeInfo {
            path: vec![],
            unknown: vec![(token_in.into(), token_out.into(), fee)],
            amount,
            exchange_id: ExchangeId::Uniswap,
        },
    }
}

/// Lookup token addresses returning corresponding `Token`s, if matched
fn address_to_token<'a>(
    token_in: &'a [u8; 20],
    token_out: &'a [u8; 20],
) -> (Option<Token>, Option<Token>) {
    (
        TOKEN_LOOKUP.get(token_in).copied(),
        TOKEN_LOOKUP.get(token_out).copied(),
    )
}

#[cfg(test)]
mod test {
    use crate::trade_router::*;
    use ethabi_static::DecodeStatic;
    use hex_literal::hex;

    #[test]
    fn test_execute_deadline() {
        let buf = hex!("000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000646ed6d700000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000ba43b740000000000000000000000000000000000000000000000098a1b3fd24f4d168ea200000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002bff970a61a04b1ca14834a43f5de4533ebddb5cc80001f4912ce59144191c1204e64559fe8253a0e49e6548000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000ba43b740000000000000000000000000000000000000000000000098b057a68577b20cfaa00000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000042ff970a61a04b1ca14834a43f5de4533ebddb5cc80001f482af49447d8a07e3bd95bd0d56f35241523fbab10001f4912ce59144191c1204e64559fe8253a0e49e6548000000000000000000000000000000000000000000000000000000000000");
        let params = UniswapV3UniversalExecuteParams::decode(&buf).unwrap();
        println!("{:?}", params);
        let trade = UniswapV3UniversalRouterSwapExactIn::decode(params.inputs[0].as_ref()).unwrap();
        println!("{:?}", trade);
    }

    #[test]
    fn test_decode_multicall_deadline() {
        let buf = hex!("000000000000000000000000000000000000000000000000000000006463053700000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000e404e45aaf000000000000000000000000ff970a61a04b1ca14834a43f5de4533ebddb5cc8000000000000000000000000fc5bed154d08f4e2edd24c348720b8f28ce3ad210000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000c084bede87eb4337e7176578c4e2096797063a670000000000000000000000000000000000000000000000000000000005f5e1000000000000000000000000000000000000000000000004306fd68967efb2b3b9000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
        assert!(UniswapV3MultiCallDeadline::decode(&buf).is_ok());
    }

    #[test]
    fn test_decode_exact_input() {
        let buf = hex!("000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000006464d2af0000000000000000000000000000000000000000000000000000000000000002000c0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000001600000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000009896800000000000000000000000000000000000000000000000000013c09453027baa00000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002bff970a61a04b1ca14834a43f5de4533ebddb5cc80001f482af49447d8a07e3bd95bd0d56f35241523fbab1000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000013c09453027baa");
        let res = UniswapV3UniversalExecuteParams::decode(&buf);
        assert!(res.is_ok());

        println!("{:?}", res);

        let buf2 = hex!("ff970a61a04b1ca14834a43f5de4533ebddb5cc80001f482af49447d8a07e3bd95bd0d56f35241523fbab1");
        let res = UniswapV3UniversalRouterSwapExactIn::decode(&buf2);
        // let buf3 = [0_u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 152, 150, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19, 192, 148, 83, 2, 123, 170, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 43, 255, 151, 10, 97, 160, 75, 28, 161, 72, 52, 164, 63, 93, 228, 83, 62, 189, 219, 92, 200, 0, 1, 244, 130, 175, 73, 68, 125, 138, 7, 227, 189, 149, 189, 13, 86, 243, 82, 65, 82, 63, 186, 177, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        // assert!(UniswapV3UniversalRouterSwapExactIn::decode(&buf3).is_ok(), "inner swap");
    }

    #[test]
    fn test_decode_exact_output() {
        /*
        #	Name	Type	Data
        0	commands	bytes	0x0b010c
        1	inputs	bytes[]	0x000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000001f3da9a3c20ba32
        0x0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000db5858000000000000000000000000000000000000000000000000001f3da9a3c20ba3200000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002bff970a61a04b1ca14834a43f5de4533ebddb5cc80001f482af49447d8a07e3bd95bd0d56f35241523fbab1000000000000000000000000000000000000000000
        0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000
        2	deadline	uint256	1684340123
         */

        // let buf = hex!("000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000006464d9b400000000000000000000000000000000000000000000000000000000000000030a000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000160000000000000000000000000912ce59144191c1204e64559fe8253a0e49e6548000000000000000000000000ffffffffffffffffffffffffffffffffffffffff00000000000000000000000000000000000000000000000000000000648c658600000000000000000000000000000000000000000000000000000000000000000000000000000000000000004c60051384bd2d3c01bfc845cf5f4b44bcbe9de5000000000000000000000000000000000000000000000000000000006464df8e00000000000000000000000000000000000000000000000000000000000000e00000000000000000000000000000000000000000000000000000000000000041d9abb27c758e59594b2777221a85688a6ef38e0f9b62b30c9ddc33afcca9835d7863b96f838b0d477057e314b29e1583397f7c9257b967bfd8a2aafd9fedb5121c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000008ac7230489e800000000000000000000000000000000000000000000000000000016d6163267606b00000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002b912ce59144191c1204e64559fe8253a0e49e65480001f482af49447d8a07e3bd95bd0d56f35241523fbab1000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000016d6163267606b");
        // let res = UniswapV3UniversalExecuteDeadlineParams::decode(&buf);
        // assert!(res.is_ok());
        // println!("{:?}", res);

        let buf2 = hex!("000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000001f3da9a3c20ba32");
        let res = UniswapV3UniversalRouterSwapExactOut::decode(&buf2);
        println!("{:?}", res);
        assert!(res.is_ok());
    }

    #[test]
    fn one_inch_v3_swap() {
        let buf = hex!("0000000000000000000000000000000000000000000000000000000000c2cab70000000000000000000000000000000000000000000000000018be73ce4ce1ea00000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000e754841b77c874135caca3386676e886459c2d61cfee7c08");
        let swap = OneInchUniswapV3Swap::decode(&buf).unwrap();
        println!("{:?}", swap);

        for pool in &swap.pools {
            let pool_bytes = pool.0;
            let zero_for_one = pool_bytes[0] & 0x01 == 0;
            let pool_address: [u8; 20] =
                unsafe { *(&pool_bytes[12..32] as *const [u8] as *const [u8; 20]) };
            assert_eq!(
                pool_address,
                hex!("e754841b77c874135caca3386676e886459c2d61")
            );
            assert!(zero_for_one);
        }

        assert!(false);
    }
}
