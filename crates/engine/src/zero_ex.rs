//! 0x protocol utilities

use ethabi_static::{AddressZcp, Bytes32, BytesZcp, DecodeStatic, Tuple, Tuples};
use ethers::types::U256;
use log::debug;
use once_cell::sync::Lazy;

pub static HIGH_BIT: Lazy<U256> = Lazy::new(|| U256::from(2).pow(U256::from(255)));

pub mod bridge_id {
    #![allow(dead_code)]
    pub const UNKNOWN: u8 = 0;
    pub const CURVE: u8 = 1;
    pub const UNISWAPV2: u8 = 2;
    pub const UNISWAP: u8 = 3;
    pub const BALANCER: u8 = 4;
    pub const KYBER: u8 = 5; // Not used: deprecated.
    pub const MOONISWAP: u8 = 6;
    pub const MSTABLE: u8 = 7;
    pub const OASIS: u8 = 8; // Not used: deprecated.
    pub const SHELL: u8 = 9;
    pub const DODO: u8 = 10;
    pub const DODOV2: u8 = 11;
    pub const CRYPTOCOM: u8 = 12;
    pub const BANCOR: u8 = 13;
    pub const COFIX: u8 = 14; // Not used: deprecated.
    pub const NERVE: u8 = 15;
    pub const MAKERPSM: u8 = 16;
    pub const BALANCERV2: u8 = 17;
    pub const UNISWAPV3: u8 = 18;
    pub const KYBERDMM: u8 = 19;
    pub const CURVEV2: u8 = 20;
    pub const LIDO: u8 = 21;
    pub const CLIPPER: u8 = 22; // Not used: Clipper is now using PLP interface
    pub const AAVEV2: u8 = 23;
    pub const COMPOUND: u8 = 24;
    pub const BALANCERV2BATCH: u8 = 25;
    pub const GMX: u8 = 26;
    pub const PLATYPUS: u8 = 27;
    pub const BANCORV3: u8 = 28;
    pub const SOLIDLY: u8 = 29;
    pub const SYNTHETIX: u8 = 30;
    pub const WOOFI: u8 = 31;
    pub const AAVEV3: u8 = 32;
    pub const KYBERELASTIC: u8 = 33;
    pub const BARTER: u8 = 34;
    pub const TRADERJOEV2: u8 = 35;
}

pub const FILL_QUOTE_TRANSFORMER_21: u32 = 21;
pub const FILL_QUOTE_TRANSFORMER_19: u32 = 19;
pub const POSITIVE_SLIPPAGE_FEE_TRANSFORMER: u32 = 17;
pub const PAY_TAKER_TRANSFORMER: u32 = 16;
pub const AFFILIATE_FEE_TRANSFORMER: u32 = 15;
pub const WETH_TRANSFORMER: u32 = 4;

#[derive(DecodeStatic, Debug, PartialEq)]
pub struct LimitOrderInfo<'a> {
    order: LimitOrder<'a>,
    // LibSignature.Signature signature;
    // Maximum taker token amount of this limit order to fill.
    // maxTakerTokenFillAmount;
}

#[derive(DecodeStatic, Debug, PartialEq)]
pub struct RfqOrderInfo<'a> {
    order: RfqOrder<'a>,
    // LibSignature.Signature signature;
    // Maximum taker token amount of this limit order to fill.
    // maxTakerTokenFillAmount;
}

#[derive(DecodeStatic, Debug, PartialEq)]
pub struct OtcOrderInfo<'a> {
    order: OtcOrder<'a>,
    // LibSignature.Signature signature;
    // Maximum taker token amount of this limit order to fill.
    // maxTakerTokenFillAmount;
}

#[derive(DecodeStatic, Debug, PartialEq)]
pub struct Transformation<'a> {
    // The deployment nonce for the transformer.
    // The address of the transformer contract will be derived from this
    // value.
    pub deployment_nonce: u32,
    // Arbitrary data to pass to the transformer.
    // the transformation type is not known until runtime depending on `deployment_nonce`
    pub data: BytesZcp<'a>,
}

/// Top-level 0x 'TransformERC20' call
#[derive(DecodeStatic, Debug, PartialEq)]
pub struct TransformErc20<'a> {
    pub token_in: AddressZcp<'a>,
    pub token_out: AddressZcp<'a>,
    pub amount_in: U256,
    pub amount_out_min: U256,
    pub transformations: Tuples<Transformation<'a>>,
}

#[derive(DecodeStatic, Debug, PartialEq)]
/// @dev A standard OTC or OO limit order.
struct LimitOrder<'a> {
    pub maker_token: AddressZcp<'a>,
    pub taker_token: AddressZcp<'a>,
    pub maker_amount: u128,
    pub taker_amount: u128,
    pub taker_token_fee_amount: u128,
    #[ethabi(skip)]
    maker: U256,
    #[ethabi(skip)]
    taker: U256,
    #[ethabi(skip)]
    sender: U256,
    #[ethabi(skip)]
    fee_recipient: U256,
    pub pool: Bytes32<'a>,
    // #[ethabi(skip)]
    // expiry: u64,
    // #[ethabi(skip)]
    // salt: U256,
}

#[derive(DecodeStatic, Debug, PartialEq)]
/// @dev An RFQ limit order.
struct RfqOrder<'a> {
    pub maker_token: AddressZcp<'a>,
    pub taker_token: AddressZcp<'a>,
    pub maker_amount: u128,
    pub taker_amount: u128,
    #[ethabi(skip)]
    maker: U256,
    #[ethabi(skip)]
    taker: U256,
    #[ethabi(skip)]
    tx_origin: U256,
    pub pool: Bytes32<'a>,
    // #[ethabi(skip)]
    // expiry: u64,
    // #[ethabi(skip)]
    // salt: U256,
}

#[derive(DecodeStatic, Debug, PartialEq)]
/// @dev An OTC limit order.
pub struct OtcOrder<'a> {
    pub maker_token: AddressZcp<'a>,
    pub taker_token: AddressZcp<'a>,
    pub maker_amount: u128,
    pub taker_amount: u128,
    // address maker;
    // address taker;
    // address txOrigin;
    // uint256 expiryAndNonce; // [uint64 expiry, uint64 nonceBucket, uint128 nonce]
}

#[derive(DecodeStatic, Debug, PartialEq)]
pub struct BridgeOrder<'a> {
    // Upper 16 bytes: uint128 protocol ID (right-aligned)
    // Lower 16 bytes: ASCII source name (left-aligned)
    pub source: Bytes32<'a>,
    pub taker_amount: U256,
    pub maker_amount: U256,
    pub data: BytesZcp<'a>, // data to pass to the bridge `source`
}

/// @dev Transform data to ABI-encode and pass into `transform()`.
#[derive(DecodeStatic, Debug, PartialEq)]
pub struct FillQuoteTransformData<'a> {
    // Whether we are performing a market sell or buy.
    pub side: U256,
    // The token being sold.
    pub sell_token: AddressZcp<'a>,
    // The token being bought.
    pub buy_token: AddressZcp<'a>,
    // External liquidity bridge orders. Sorted by fill sequence.
    pub bridge_orders: Tuples<BridgeOrder<'a>>,
    // Native limit orders. Sorted by fill sequence.
    pub limit_orders: Tuples<LimitOrderInfo<'a>>,
    // Native RFQ orders. Sorted by fill sequence.
    pub rfq_orders: Tuples<RfqOrderInfo<'a>>,
    // The sequence to fill the orders in. Each item will fill the next
    // order of that type in either `bridgeOrders`, `limitOrders`,
    // or `rfqOrders.`
    pub fill_sequence_offset: Vec<u32>,
    // Amount of `sellToken` to sell or `buyToken` to buy.
    // For sells, setting the high-bit indicates that
    // `sellAmount & LOW_BITS` should be treated as a `1e18` fraction of
    // the current balance of `sellToken`, where
    // `1e18+ == 100%` and `0.5e18 == 50%`, etc.
    pub fill_amount: U256,
    // Who to transfer unused protocol fees to.
    // May be a valid address or one of:
    // `address(0)`: Stay in flash wallet.
    // `address(1)`: Send to the taker.
    // `address(2)`: Send to the sender (caller of `transformERC20()`).
    #[ethabi(skip)]
    refund_receiver: U256,
    // Otc orders. Sorted by fill sequence.
    pub otc_orders: Tuples<OtcOrderInfo<'a>>,
}

#[derive(Debug, DecodeStatic, PartialEq)]
pub struct UniswapV3Mixin<'a> {
    pub router: AddressZcp<'a>,
    pub path: BytesZcp<'a>,
}

#[derive(Debug, DecodeStatic, PartialEq)]
pub struct UniswapV2Mixin<'a> {
    pub router: AddressZcp<'a>,
    pub path: Vec<AddressZcp<'a>>,
}

/// Decode a 0x ERC20 transform and its inner typed transforms for processing
pub fn decode_erc20_transform<'a>(buf: &'a [u8]) {
    let outer_transform: TransformErc20 = <TransformErc20>::decode(buf).unwrap();
    for t in outer_transform.transformations.0.iter() {
        match t.deployment_nonce {
            FILL_QUOTE_TRANSFORMER_19 | FILL_QUOTE_TRANSFORMER_21 => {
                let data = Tuple::<FillQuoteTransformData>::decode(t.data.as_ref())
                    .unwrap()
                    .0;
                let orders = data.bridge_orders.0.as_slice();
                for order in orders {
                    let protocol_id = order.source.0[15];
                    // println!("protocol name: {:?}", core::str::from_utf8(&bridge_order.source.0[16..32]).unwrap());
                    if protocol_id == bridge_id::UNISWAPV3 {
                        if !(data.fill_amount & *HIGH_BIT).is_zero() {
                            // 0x features allows specifying a ratio of user balance as fill amount
                            // we cant' simulate without pulling it from chain...
                            debug!("0x can't simulate");
                            return;
                        }
                        let v3_trade = UniswapV3Mixin::decode(order.data.0).unwrap();
                        println!("{:?}", v3_trade);
                    } else {
                        println!("unhandled protocol Id: {:?}", protocol_id);
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

#[cfg(test)]
mod test {
    use super::*;
    use ethabi_static::Tuple;
    use hex_literal::hex;

    const TEST_PAYLOAD: &[u8] = &hex!("000000000000000000000000da10009cbd5d07dd0cecc66161fc93d7c9000da1000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee00000000000000000000000000000000000000000000003653b274ef1636605f00000000000000000000000000000000000000000000000007a9e28bd6e7dcba00000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000044000000000000000000000000000000000000000000000000000000000000004e000000000000000000000000000000000000000000000000000000000000005a000000000000000000000000000000000000000000000000000000000000000150000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000036000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000da10009cbd5d07dd0cecc66161fc93d7c9000da100000000000000000000000082af49447d8a07e3bd95bd0d56f35241523fbab100000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000320000000000000000000000000000000000000000000000000000000000000032000000000000000000000000000000000000000000000000000000000000002e000000000000000000000000000000000000000000000003653b274ef1636605f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000012556e697377617056330000000000000000000000000000000000000000000000000000000000003653b274ef1636605f00000000000000000000000000000000000000000000000007a9e28bd6e7dcba000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000e592427a0aece92de3edee1f18e0157c0586156400000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000042da10009cbd5d07dd0cecc66161fc93d7c9000da1000064fd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb90001f482af49447d8a07e3bd95bd0d56f35241523fbab100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000004000000000000000000000000082af49447d8a07e3bd95bd0d56f35241523fbab1ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff000000000000000000000000000000000000000000000000000000000000001100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee00000000000000000000000000000000000000000000000007aa178106c612a4000000000000000000000000af5889d80b0f6b2850ec5ef8aad0625788eeb9030000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000001000000000000000000000000da10009cbd5d07dd0cecc66161fc93d7c9000da10000000000000000000000000000000000000000000000000000000000000000869584cd00000000000000000000000008a3c2a819e3de7aca384c798269b3ce1cd0e437000000000000000000000000000000000000000000000037c98f4c43646b63e0");

    #[test]
    fn decode_bridge_order() {
        let data = hex!("00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000da10009cbd5d07dd0cecc66161fc93d7c9000da100000000000000000000000082af49447d8a07e3bd95bd0d56f35241523fbab100000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000320000000000000000000000000000000000000000000000000000000000000032000000000000000000000000000000000000000000000000000000000000002e000000000000000000000000000000000000000000000003653b274ef1636605f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000012556e697377617056330000000000000000000000000000000000000000000000000000000000003653b274ef1636605f00000000000000000000000000000000000000000000000007a9e28bd6e7dcba000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000e592427a0aece92de3edee1f18e0157c0586156400000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000042da10009cbd5d07dd0cecc66161fc93d7c9000da1000064fd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb90001f482af49447d8a07e3bd95bd0d56f35241523fbab1000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
        let res = Tuple::<FillQuoteTransformData>::decode(&data)
            .expect("decodes")
            .0;
        let bridge_order = &res.bridge_orders.0[0];
        println!("{:?}", bridge_order);
        println!("protocol Id: {:?}", bridge_order.source.0[15]);
        println!(
            "protocol name: {:?}",
            core::str::from_utf8(&bridge_order.source.0[16..32]).unwrap()
        );
        assert!(core::str::from_utf8(&bridge_order.source.0[16..32])
            .unwrap()
            .contains("UniswapV3"));
        assert_eq!(bridge_order.source.0[15], 18);
    }

    #[test]
    fn decode_erc20_transform_ok() {
        decode_erc20_transform(TEST_PAYLOAD);
    }

    #[test]
    fn decode_transform() {
        let outer_transform: TransformErc20 = <TransformErc20>::decode(TEST_PAYLOAD).unwrap();
        let transformations = outer_transform.transformations.0;
        assert_eq!(transformations[0].deployment_nonce, 21);
        assert_eq!(transformations[1].deployment_nonce, 4);
        assert_eq!(transformations[2].deployment_nonce, 17);
        assert_eq!(transformations[3].deployment_nonce, 16);

        // https://arbiscan.io/address/0x29f80c1f685e19ae1807063eda432f431ac623d0#events
        // 21 == FillQuoteTransformer
        // 17 == PositiveSlippageFeeTransformer
        // 16 == PayTakerTransformer
        // 15 == AffiliateFeeTransformer 970e318b8f074c20bf0cee06970f01dc7a761e50
        // 4 == WETH transformer 0x10e968968f49dd66a5efeebbb2edcb9c49c4fc49
        /*
        https://github.com/0xProject/0x-api/blob/7460c00ac437761a92cfe5334af32b427c45cc92/src/asset-swapper/quote_consumers/quote_consumer_utils.ts#L24-L29
        0x29f80c1f685e19aE1807063eDa432F431ac623D0

            https://arbiscan.io/tx/0xe2a230cf0f3ce16a016b02a9ba0cd8f8ab516b1930378fbef44f975da22b150f
            1   inputToken	address	0xDA10009cBd5D07dd0CeCc66161FC93D7c9000da1
            2	outputToken	address	0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE
            3	inputTokenAmount	uint256	1002155191401536970847
            4	minOutputTokenAmount	uint256	552221519563447482
                transformations.deploymentNonce	uint32	21
            4	transformations.data	bytes	0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000da10009cbd5d07dd0cecc66161fc93d7c9000da100000000000000000000000082af49447d8a07e3bd95bd0d56f35241523fbab100000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000320000000000000000000000000000000000000000000000000000000000000032000000000000000000000000000000000000000000000000000000000000002e000000000000000000000000000000000000000000000003653b274ef1636605f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000012556e697377617056330000000000000000000000000000000000000000000000000000000000003653b274ef1636605f00000000000000000000000000000000000000000000000007a9e28bd6e7dcba000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000e592427a0aece92de3edee1f18e0157c0586156400000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000042da10009cbd5d07dd0cecc66161fc93d7c9000da1000064fd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb90001f482af49447d8a07e3bd95bd0d56f35241523fbab1000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
            5	transformations.deploymentNonce	uint32	4
            5	transformations.data	bytes	0x00000000000000000000000082af49447d8a07e3bd95bd0d56f35241523fbab1ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
            6	transformations.deploymentNonce	uint32	17
            6   transformations.data    bytes   0x000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee00000000000000000000000000000000000000000000000007aa178106c612a4000000000000000000000000af5889d80b0f6b2850ec5ef8aad0625788eeb903
         */
    }
}
