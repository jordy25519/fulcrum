//! Trade routing utilities

use ethabi_static::{AddressZcp, Bytes32, BytesZcp, DecodeStatic};
use ethers::types::{Address, U256};
use hex_literal::hex;
use once_cell::sync::Lazy;

use crate::{
    constant::arbitrum::{
        CAMELOT_ROUTER, ODOS_ROUTER, ONE_INCH_ROUTER_V4, ONE_INCH_ROUTER_V5, PARASWAP_AUGUSTUS,
        SUSHI_ROUTER, UNISWAP_V3_ROUTER_V1, UNISWAP_V3_ROUTER_V2, UNISWAP_V3_UNIVERSAL_ROUTER,
        ZERO_EX_ROUTER,
    },
    types::{ExchangeId, Pair, RouterId, Token},
    util::AddressMap,
};

pub const UNISWAP_V3_V1_EXACT_INPUT: [u8; 4] = hex!("c04b8d59");
pub const UNISWAP_V3_V1_EXACT_INPUT_SINGLE: [u8; 4] = hex!("414bf389");
pub const UNISWAP_V3_V1_EXACT_OUTPUT: [u8; 4] = hex!("f28c0498");
pub const UNISWAP_V3_V1_EXACT_OUTPUT_SINGLE: [u8; 4] = hex!("db3e2198");

pub const UNISWAP_V3_V2_EXACT_INPUT: [u8; 4] = hex!("b858183f");
pub const UNISWAP_V3_V2_EXACT_INPUT_SINGLE: [u8; 4] = hex!("04e45aaf");
pub const UNISWAP_V3_V2_EXACT_OUTPUT: [u8; 4] = hex!("09b81346");
pub const UNISWAP_V3_V2_EXACT_OUTPUT_SINGLE: [u8; 4] = hex!("5023b4df");
pub const UNISWAP_V3_MULTI_CALL: [u8; 4] = hex!("ac9650d8");
pub const UNISWAP_V3_MULTI_CALL_DEADLINE: [u8; 4] = hex!("5ae401dc");

pub const UNISWAP_UNIVERSAL_ROUTER_EXECUTE_DEADLINE: [u8; 4] = hex!("24856bc3");
pub const UNISWAP_UNIVERSAL_ROUTER_EXECUTE: [u8; 4] = hex!("3593564c");

pub const ONE_INCH_UNISWAP_V3_SWAP: [u8; 4] = hex!("e449022e");
pub const ONE_INCH_UNISWAP_V3_SWAP_TWP: [u8; 4] = hex!("e449022e"); // with permit
/// 1inch V2 swap
pub const ONE_INCH_UNISWAP_SWAP: [u8; 4] = hex!("12aa3caf");

pub const ZERO_EX_TRANSFORM_ERC20: [u8; 4] = hex!("415565b0");

// pub const IT_BUY_1: [u8; 4] = hex!("a6f2ae3a");
// pub const IT_SELL_1: [u8; 4] = hex!("45710074");

pub const ODOS_SWAP: [u8; 4] = hex!("f17a4546");

#[derive(Debug, DecodeStatic)]
pub struct SwapExactTokensForETH<'a> {
    pub amount_in: U256,
    amount_out_min: U256,
    pub path: Vec<AddressZcp<'a>>,
    // address to,
    // uint256 deadline
}
pub const SUSHI_SWAP_EXACT_TOKENS_FOR_ETH: [u8; 4] = hex!("18cbafe5");
pub const SUSHI_SWAP_EXACT_TOKENS_FOR_ETH_SFOTT: [u8; 4] = hex!("791ac947");
// #[derive(Debug, DecodeStatic)]
// pub struct SwapExactTokensForETHSupportingFeeOnTransferTokens<'a> {
//     amount_in: U256,
//     amount_out_min: U256,
//     path: Vec<AddressZcp<'a>>,
//     // address to,
//     // uint256 deadline
// }
pub const SUSHI_SWAP_EXACT_ETH_FOR_TOKENS: [u8; 4] = hex!("7ff36ab5");
pub const SUSHI_SWAP_EXACT_ETH_FOR_TOKENS_SFOTT: [u8; 4] = hex!("b6f9de95");
#[derive(Debug, DecodeStatic)]
pub struct SwapExactETHForTokens<'a> {
    pub amount_out_min: U256,
    pub path: Vec<AddressZcp<'a>>,
    // address to,
    // uint deadline
}
// #[derive(Debug, DecodeStatic)]
// pub struct SwapExactETHForTokensSupportingFeeOnTransferTokens<'a> {
//     pub amount_out_min: U256,
//     pub path: Vec<AddressZcp<'a>>,
//     // address to,
//     // uint deadline
// }

pub const CAMELOT_V2_SWAP_EXACT_TOKENS_FOR_ETH_SFOTT: [u8; 4] = hex!("52aa4c22");
pub const CAMELOT_V2_SWAP_EXACT_ETH_FOR_TOKENS_SFOTT: [u8; 4] = hex!("b4822be3");
#[derive(Debug, DecodeStatic)]
pub struct SwapExactETHForTokensSFOTT<'a> {
    pub amount_out_min: U256,
    pub path: Vec<AddressZcp<'a>>,
    // address to,
    // address referrer
    // uint deadline
}
#[derive(Debug, DecodeStatic)]
pub struct SwapExactTokensForEthSFOTT<'a> {
    pub amount_in: U256,
    pub amount_out_min: U256,
    pub path: Vec<AddressZcp<'a>>,
    // address to,
    // address referrer
    // uint deadline
}

/// https://github.com/odos-xyz/router_v1/blob/581d4400f29aed9538ab94a860afae0c1dbd97c7/OdosRouter.sol#LL22C1-L22C89
/// @dev Contains all information needed to describe an input token being swapped from
#[derive(Debug, DecodeStatic)]
pub struct InputTokenOdos<'a> {
    pub address: AddressZcp<'a>,
    pub amount_in: U256,
    // address receiver
    // bytes permit
}
/// @dev Contains all information needed to describe an output token being swapped to
#[derive(Debug, DecodeStatic)]
pub struct OutputTokenOdos<'a> {
    address: AddressZcp<'a>,
    relative_value: U256,
    // receiver
}
#[derive(Debug, DecodeStatic)]
pub struct OdosSwap<'a> {
    pub input_tokens: Vec<InputTokenOdos<'a>>,
    pub output_tokens: Vec<OutputTokenOdos<'a>>,
    pub amount_out_quote: U256,
    pub amount_out_min: U256,
    pub executor: AddressZcp<'a>,
    pub path: BytesZcp<'a>,
}

#[derive(Debug, DecodeStatic)]
pub struct SwapDescription<'a> {
    pub token_in: AddressZcp<'a>,
    pub token_out: AddressZcp<'a>,
    #[ethabi(skip)]
    _source_receiver: U256,
    #[ethabi(skip)]
    _dst_receiver: U256,
    pub amount: U256,
    // min_return_amount: U256,
    // flags: U256
}

/// https://arbiscan.io/address/0x0A9f824C05A74F577A536A8A0c673183a872Dff4#writeContract
#[derive(Debug, DecodeStatic)]
pub struct OneInchSwap<'a> {
    pub executor: AddressZcp<'a>,
    pub swap: SwapDescription<'a>,
    #[ethabi(skip)]
    pub permit: BytesZcp<'a>,
    pub data: BytesZcp<'a>,
}

#[derive(Debug, DecodeStatic)]
pub struct OneInchUniswapV3Swap<'a> {
    pub amount_in: U256,
    pub amount_out_min: U256,
    pub pools: Vec<Bytes32<'a>>,
}

#[derive(Debug, DecodeStatic)]
pub struct OneInchUniswapV3SwapTWP<'a> {
    #[ethabi(skip)]
    pub recipient: U256,
    #[ethabi(skip)]
    pub token: U256,
    pub amount_in: U256,
    pub amount_out_min: U256,
    pub pools: Vec<Bytes32<'a>>,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3ExactOutputSingleParamsV1<'a> {
    pub token_in: AddressZcp<'a>,
    pub token_out: AddressZcp<'a>,
    pub fee: u32,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    #[ethabi(skip)]
    pub deadline: U256,
    pub amount_out: U256,
    pub amount_in_max: U256,
    #[ethabi(skip)]
    pub sqrtPriceLimitX96: U256,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3ExactOutputSingleParamsV2<'a> {
    pub token_in: AddressZcp<'a>,
    pub token_out: AddressZcp<'a>,
    pub fee: u32,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    pub amount_out: U256,
    pub amount_in_max: U256,
    #[ethabi(skip)]
    pub sqrtPriceLimitX96: U256,
}

#[derive(Debug, Default, DecodeStatic)]
pub struct UniswapV3ExactOutputParamsV2<'a> {
    pub path: BytesZcp<'a>,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    pub amount_out: U256,
    pub amount_in_max: U256,
}

#[derive(Debug, Default, DecodeStatic)]
pub struct UniswapV3ExactOutputParamsV1<'a> {
    pub path: BytesZcp<'a>,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    #[ethabi(skip)]
    pub deadline: U256,
    pub amount_out: U256,
    pub amount_in_max: U256,
}

#[derive(Debug, Default, DecodeStatic)]
pub struct UniswapV3ExactInputParamsV2<'a> {
    pub path: BytesZcp<'a>,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    pub amount_in: U256,
    pub amount_out_min: U256,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3ExactInputSingleParamsV2<'a> {
    pub token_in: AddressZcp<'a>,
    pub token_out: AddressZcp<'a>,
    pub fee: u32,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    pub amount_in: U256,
    pub amount_out_min: U256,
    #[ethabi(skip)]
    pub sqrtPriceLimitX96: U256,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3ExactInputParamsV1<'a> {
    pub path: BytesZcp<'a>,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    #[ethabi(skip)]
    pub deadline: U256,
    pub amount_in: U256,
    pub amount_out_min: U256,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3ExactInputSingleParamsV1<'a> {
    pub token_in: AddressZcp<'a>,
    pub token_out: AddressZcp<'a>,
    pub fee: u32,
    #[ethabi(skip)]
    pub recipient: Option<Address>,
    #[ethabi(skip)]
    pub deadline: U256,
    pub amount_in: U256,
    pub amount_out_min: U256,
    pub sqrtPriceLimitX96: U256,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3UniversalExecuteParams<'a> {
    pub commands: BytesZcp<'a>,
    pub inputs: Vec<BytesZcp<'a>>,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3UniversalExecuteDeadlineParams<'a> {
    pub commands: BytesZcp<'a>,
    pub inputs: Vec<BytesZcp<'a>>,
    #[ethabi(skip)]
    pub deadline: U256,
}

// https://docs.uniswap.org/contracts/universal-router/technical-reference#v3_swap_exact_in
#[derive(Debug, DecodeStatic)]
pub struct UniswapV3UniversalRouterSwapExactIn<'a> {
    #[ethabi(skip)]
    pub recipient: Address,
    pub amount_in: U256,
    pub min_amount_out: U256,
    pub path: BytesZcp<'a>,
    #[ethabi(skip)]
    pub sender_or_router: bool,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3UniversalRouterSwapExactOut<'a> {
    #[ethabi(skip)]
    pub recipient: Address,
    pub amount_out: U256,
    pub amount_in_max: U256,
    pub path: BytesZcp<'a>,
    #[ethabi(skip)]
    pub sender_or_router: bool,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3MultiCall<'a> {
    pub data: Vec<BytesZcp<'a>>,
}

#[derive(Debug, DecodeStatic)]
pub struct UniswapV3MultiCallDeadline<'a> {
    #[ethabi(skip)]
    pub deadline: U256,
    pub data: Vec<BytesZcp<'a>>,
}

/// Info extracted from an external trade
/// we only care about 'sells'
#[derive(Debug)]
pub struct TradeInfo {
    pub amount: U256,
    pub path: Vec<(Token, Token, u32)>,
    pub exchange_id: ExchangeId,
    pub unknown: Vec<(Address, Address, u32)>,
}

/// Map from contract address to known router Ids
pub static ROUTERS: Lazy<AddressMap<RouterId>> = Lazy::new(|| {
    let mut routers = AddressMap::<RouterId>::default();
    routers.insert(UNISWAP_V3_ROUTER_V1, RouterId::UniswapV3RouterV1);
    routers.insert(UNISWAP_V3_ROUTER_V2, RouterId::UniswapV3RouterV2);
    routers.insert(
        UNISWAP_V3_UNIVERSAL_ROUTER,
        RouterId::UniswapV3UniversalRouter,
    );
    routers.insert(CAMELOT_ROUTER, RouterId::CamelotRouterV2);
    routers.insert(SUSHI_ROUTER, RouterId::SushiRouterV2);
    routers.insert(PARASWAP_AUGUSTUS, RouterId::ParaswapAugustus);
    routers.insert(ONE_INCH_ROUTER_V5, RouterId::OneInch);
    routers.insert(ONE_INCH_ROUTER_V4, RouterId::OneInch);
    routers.insert(ZERO_EX_ROUTER, RouterId::ZeroEx);
    routers.insert(ODOS_ROUTER, RouterId::Odos);

    routers
});

/// Map from token address to know token Ids
pub static TOKEN_LOOKUP: Lazy<AddressMap<Token>> = Lazy::new(|| {
    let mut tokens = AddressMap::<Token>::default();
    tokens.insert(Token::USDC.address().into(), Token::USDC);
    tokens.insert(Token::WETH.address().into(), Token::WETH);
    tokens.insert(Token::USDT.address().into(), Token::USDT);
    tokens.insert(Token::ARB.address().into(), Token::ARB);

    tokens
});

// Map from pool/pair contract address to its two tokens
pub static POOL_LOOKUP: Lazy<AddressMap<Pair>> = Lazy::new(|| {
    // TODO: get from config 🤦‍♀️
    let mut pool_lookup = AddressMap::<Pair>::with_capacity(20);
    pool_lookup.insert(
        hex!("e754841b77c874135caca3386676e886459c2d61"),
        Pair::new(Token::WETH, Token::USDC, 100_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("c31e54c7a869b9fcbecc14363cf510d1c41fa443"),
        Pair::new(Token::WETH, Token::USDC, 500_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("17c14d2c404d167802b16c450d3c99f88f2c4f4d"),
        Pair::new(Token::WETH, Token::USDC, 3000_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("cda53b1f66614552f834ceef361a8d12a0b8dad8"),
        Pair::new(Token::ARB, Token::USDC, 500_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("81c48d31365e6b526f6bbadc5c9aafd822134863"),
        Pair::new(Token::ARB, Token::USDC, 3000_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("89a4026e9ade251c67b7fb38054931a39936d9c5"),
        Pair::new(Token::WETH, Token::ARB, 100_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("c6f780497a95e246eb9449f5e4770916dcd6396a"),
        Pair::new(Token::WETH, Token::ARB, 500_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("92c63d0e701caae670c9415d91c474f686298f00"),
        Pair::new(Token::WETH, Token::ARB, 3000_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("42161084d0672e1d3f26a9b53e653be2084ff19c"),
        Pair::new(Token::WETH, Token::USDT, 100_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("641c00a822e8b671738d32a431a4fb6074e5c79d"),
        Pair::new(Token::WETH, Token::USDT, 500_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("c82819f72a9e77e2c0c3a69b3196478f44303cf4"),
        Pair::new(Token::WETH, Token::USDT, 3000_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("8c9d230d45d6cfee39a6680fb7cb7e8de7ea8e71"),
        Pair::new(Token::USDT, Token::USDC, 100_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("b791ad21ba45c76629003b4a2f04c0d544406e37"),
        Pair::new(Token::ARB, Token::USDT, 500_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("97bca422ec0ee4851f2110ea743c1cd0a14835a1"),
        Pair::new(Token::ARB, Token::USDT, 3000_u16, ExchangeId::Uniswap),
    );
    pool_lookup.insert(
        hex!("80151aae63b24a7e1837fe578fb6be026ae8abba"),
        Pair::new(Token::ARB, Token::USDT, 10000_u16, ExchangeId::Uniswap),
    );

    pool_lookup
});
