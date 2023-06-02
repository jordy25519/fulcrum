//! Uniswap V3 price source
use ethabi_static::DecodeStatic;
use ethers::{
    abi::{encode, encode_packed, Token as ABIToken},
    types::U512,
    utils::keccak256,
};
use once_cell::sync::Lazy;

use crate::types::{Address, Pair, U256};

/// 2 ** 96
pub static X96: Lazy<U256> = Lazy::new(|| U256::from(2_u128.pow(96_u32)));
pub static Q96: Lazy<U256> = Lazy::new(|| U256::from(96));
static X96_F: Lazy<f64> = Lazy::new(|| 2_f64.powi(96));

pub fn get_next_sqrt_price_amount_0(
    liquidity: &U256,
    current_sqrt_p_x96: &U256,
    amount_0_in: &U256,
) -> U256 {
    let numerator_1 = liquidity << *Q96;
    let product = amount_0_in * current_sqrt_p_x96;
    let denominator = U512::from(numerator_1 + product);
    U256::try_from((U512::from(numerator_1) * U512::from(current_sqrt_p_x96)) / denominator)
        .expect("no overflow")
}

pub fn get_next_sqrt_price_amount_0_f(
    liquidity: f64,
    current_sqrt_p_x96: f64,
    amount_0_in: f64,
) -> f64 {
    let numerator_1 = liquidity * *X96_F;
    let product = amount_0_in * current_sqrt_p_x96;
    let denominator = numerator_1 + product;
    (numerator_1 * current_sqrt_p_x96) / denominator
}

pub fn get_next_sqrt_price_amount_1(
    liquidity: &U256,
    current_sqrt_p_x96: &U256,
    amount_1_in: &U256,
) -> U256 {
    let quotient = (amount_1_in << *Q96) / liquidity;
    current_sqrt_p_x96 + quotient
}

pub fn get_next_sqrt_price_amount_1_f(
    liquidity: f64,
    current_sqrt_p_x96: f64,
    amount_1_in: f64,
) -> f64 {
    let quotient = (amount_1_in * *X96_F) / liquidity;
    current_sqrt_p_x96 + quotient
}

pub fn get_next_sqrt_price_amount_0_output(
    liquidity: &U256,
    current_sqrt_p_x96: &U256,
    amount_out: &U256,
) -> U256 {
    let numerator_1 = liquidity << *Q96;
    let product = amount_out * current_sqrt_p_x96;
    let denominator = numerator_1 - product;

    ((U512::from(numerator_1) * U512::from(current_sqrt_p_x96)) / denominator)
        .try_into()
        .expect("fits 256")
}

pub fn get_next_sqrt_price_amount_1_output(
    liquidity: &U256,
    current_sqrt_p_x96: &U256,
    amount_out: &U256,
) -> U256 {
    // assume fits 160bits
    let quotient: U256 = ((U512::from(amount_out) << *Q96) / liquidity)
        .try_into()
        .expect("fits 256");
    current_sqrt_p_x96 - quotient
}

/// Get the amount0 delta between two prices
pub fn get_amount_0_delta_f(liquidity: f64, sqrt_ratio_aX96: f64, sqrt_ratio_bX96: f64) -> f64 {
    let (sqrt_ratio_aX96, sqrt_ratio_bX96) = if sqrt_ratio_aX96 > sqrt_ratio_bX96 {
        (sqrt_ratio_bX96, sqrt_ratio_aX96)
    } else {
        (sqrt_ratio_aX96, sqrt_ratio_bX96)
    };

    let liquidity = liquidity * *X96_F;
    let delta_sqrt_p = (sqrt_ratio_bX96 - sqrt_ratio_aX96).abs();

    ((liquidity * delta_sqrt_p) / sqrt_ratio_bX96) / sqrt_ratio_aX96
}

/// Get the amount0 delta between two prices
pub fn get_amount_0_delta(
    liquidity: &U256,
    sqrt_ratio_aX96: &U256,
    sqrt_ratio_bX96: &U256,
) -> U256 {
    let numerator_1 = liquidity << *Q96;
    let (sqrt_ratio_aX96, sqrt_ratio_bX96) = if sqrt_ratio_aX96 > sqrt_ratio_bX96 {
        (sqrt_ratio_bX96, sqrt_ratio_aX96)
    } else {
        (sqrt_ratio_aX96, sqrt_ratio_bX96)
    };
    let numerator_2 = sqrt_ratio_bX96 - sqrt_ratio_aX96;

    ((U512::from(numerator_1) * U512::from(numerator_2) / sqrt_ratio_bX96) / sqrt_ratio_aX96)
        .try_into()
        .expect("fits u256")
}

/// Get the amount1 delta between two prices
/// https://github.com/Uniswap/v3-core/blob/fc2107bd5709cdee6742d5164c1eb998566bcb75/contracts/libraries/SqrtPriceMath.sol#L182
pub fn get_amount_1_delta(
    liquidity: &U256,
    sqrt_ratio_aX96: &U256,
    sqrt_ratio_bX96: &U256,
) -> U256 {
    let delta_sqrt_p = sqrt_ratio_aX96.abs_diff(*sqrt_ratio_bX96);

    U256::try_from((U512::from(liquidity) * U512::from(delta_sqrt_p)) / U512::from(*X96))
        .expect("fits u256")
}

/// Get the amount1 delta between two prices
/// https://github.com/Uniswap/v3-core/blob/fc2107bd5709cdee6742d5164c1eb998566bcb75/contracts/libraries/SqrtPriceMath.sol#L182
pub fn get_amount_1_delta_f(liquidity: f64, sqrt_ratio_aX96: f64, sqrt_ratio_bX96: f64) -> f64 {
    (liquidity * (sqrt_ratio_bX96 - sqrt_ratio_aX96).abs()) / *X96_F
}

/// Get the amount out given some amount in
///
/// - `current_sqrt_p_x96` The √P.96
/// - `liquidity` The liquidity value
/// - `amount_in` the amount of tokens to input
///
/// Returns the amount of tokens output
pub fn get_amount_out(
    amount_in: u128,
    current_sqrt_p_x96: &U256,
    liquidity: &U256,
    fee_pips: u32,
    zero_for_one: bool,
) -> (U256, u128) {
    // calculate the expected price shift then return the amount out (i.e. price target is set exactly to required price shift)
    let amount_in_less_fee =
        U256::from(amount_in * (1_000_000_u32 - fee_pips) as u128) / U256::from(1_000_000_u128);
    if zero_for_one {
        let next_sqrt_p_x96 =
            get_next_sqrt_price_amount_0(liquidity, current_sqrt_p_x96, &amount_in_less_fee);
        (
            next_sqrt_p_x96,
            get_amount_1_delta(liquidity, &next_sqrt_p_x96, current_sqrt_p_x96).as_u128(), // TODO needs round up
        )
    } else {
        let next_sqrt_p_x96 =
            get_next_sqrt_price_amount_1(liquidity, current_sqrt_p_x96, &amount_in_less_fee);
        (
            next_sqrt_p_x96,
            get_amount_0_delta(liquidity, current_sqrt_p_x96, &next_sqrt_p_x96).as_u128(), // TODO: needs round up
        )
    }
}

pub fn get_amount_out_f(
    amount_in: u128,
    current_sqrt_p_x96: f64,
    liquidity: f64,
    fee_pips: u32,
    zero_for_one: bool,
) -> f64 {
    // calculate the expected price shift then return the amount out (i.e. price target is set exactly to required price shift)
    let amount_in_less_fee = (amount_in as f64 * (1_000_000_u32 - fee_pips) as f64) / 1_000_000_f64;
    if zero_for_one {
        let next_sqrt_p_x96 =
            get_next_sqrt_price_amount_0_f(liquidity, current_sqrt_p_x96, amount_in_less_fee);

        get_amount_1_delta_f(liquidity, next_sqrt_p_x96, current_sqrt_p_x96)
    } else {
        let next_sqrt_p_x96 =
            get_next_sqrt_price_amount_1_f(liquidity, current_sqrt_p_x96, amount_in_less_fee);

        get_amount_0_delta_f(liquidity, current_sqrt_p_x96, next_sqrt_p_x96)
    }
}

/// Get the amount in given some amount out
///
/// - `current_sqrt_p_x96` The √P.96
/// - `liquidity` The liquidity value
/// - `amount_out` the amount of tokens to output
///
/// Returns the amount of tokens to input and the new price
pub fn get_amount_in(
    amount_out: u128,
    current_sqrt_p_x96: &U256,
    liquidity: &U256,
    fee_pips: u32,
    zero_for_one: bool,
) -> (U256, u128) {
    // calculate the expected price shift then return the amount out (i.e. price target is set exactly to required price shift)
    let amount_out = &amount_out.into();
    if zero_for_one {
        // expect the order filled within one tick
        // trading in an amount of of token
        let next_sqrt_p_x96 =
            get_next_sqrt_price_amount_1_output(liquidity, current_sqrt_p_x96, amount_out);
        (
            next_sqrt_p_x96,
            ((get_amount_0_delta(liquidity, &next_sqrt_p_x96, current_sqrt_p_x96)
                * U256::from(1_000_000 - fee_pips))
                / U256::from(1_000_000))
            .as_u128(),
        )
    } else {
        // expect the order filled within one tick
        let next_sqrt_p_x96 =
            get_next_sqrt_price_amount_0_output(liquidity, current_sqrt_p_x96, amount_out);
        (
            next_sqrt_p_x96,
            ((get_amount_1_delta(liquidity, current_sqrt_p_x96, &next_sqrt_p_x96)
                * U256::from(1_000_000 - fee_pips))
                / U256::from(1_000_000))
            .as_u128(),
        )
    }
}

/// Calculate the canonical UniswapV2 pair address for the given `Pair` and `factory`
pub fn pool_address_from_pair(pair: Pair, factory: Address, init_code_hash: &[u8; 32]) -> Address {
    let token_0 = pair.token0.address();
    let token_1 = pair.token1.address();
    pool_address_for(token_0, token_1, pair.fee as u32, factory, init_code_hash)
}

/// Calculate the canonical UniswapV3 pair address for the given tokens,fee and `factory`
/// ```solidity
/// function pairFor(address factory, address tokenA, address tokenB) internal pure returns (address pair) {
///     let init_code_hash = hex!("96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f");
///     (address token0, address token1) = sortTokens(tokenA, tokenB);q
///     pair = address(uint(keccak256(abi.encodePacked(
///             hex'ff',
///             factory,
///             keccak256(abi.encodePacked(token0, token1)),
///             hex'96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f' // init code hash
///         ))));
/// }
/// ```
pub fn pool_address_for(
    token_0: Address,
    token_1: Address,
    fee: u32,
    factory: Address,
    init_code_hash: &[u8; 32],
) -> Address {
    let encoded = encode_packed(&[
        ABIToken::Bytes(vec![0xff]),
        ABIToken::Address(factory),
        ABIToken::FixedBytes(
            keccak256(
                encode(&[
                    ABIToken::Address(token_0),
                    ABIToken::Address(token_1),
                    ABIToken::Uint(fee.into()),
                ])
                .as_slice(),
            )
            .to_vec(),
        ),
        ABIToken::Bytes(init_code_hash.to_vec()),
    ])
    .expect("it encodes");

    let address_raw: [u8; 20] = keccak256(encoded)[12..].try_into().expect("32 byte value");
    address_raw.into()
}

#[derive(Debug, PartialEq, DecodeStatic)]
pub struct UniswapV3Slot0 {
    pub sqrt_p_x96: U256,
    pub liquidity: u128,
}

#[inline(always)]
pub fn fee_from_path_bytes(buf: &[u8]) -> u32 {
    // OPTIMIZATION: nothing sensible should ever be longer than 2 ** 16 so we ignore the other bytes
    // ((unsafe { *buf.get_unchecked(0) } as u32) << 16) +
    ((unsafe { *buf.get_unchecked(1) } as u32) << 8) + (unsafe { *buf.get_unchecked(2) } as u32)
}

#[cfg(test)]
mod test {
    use ethers::types::Address;
    use hex_literal::hex;

    use super::*;
    use crate::{
        constant::arbitrum::{UNISWAP_V3_FACTORY, UNISWAP_V3_INIT_CODE_HASH},
        types::{ExchangeId, Pair, Token},
    };

    #[test]
    fn pool_address_for_works() {
        let actual = pool_address_from_pair(
            Pair::new(Token::WETH, Token::USDC, 100_u16, ExchangeId::Uniswap),
            Address::from(UNISWAP_V3_FACTORY),
            &UNISWAP_V3_INIT_CODE_HASH,
        );
        assert_eq!(
            actual,
            Address::from(hex!("E754841B77C874135caCA3386676e886459c2d61"))
        );

        let actual = pool_address_from_pair(
            Pair::new(Token::USDC, Token::WETH, 500, ExchangeId::Uniswap),
            Address::from(UNISWAP_V3_FACTORY),
            &UNISWAP_V3_INIT_CODE_HASH,
        );
        assert_eq!(
            actual,
            Address::from(hex!("C31E54c7a869B9FcBEcc14363CF510d1c41fa443"))
        );
    }

    #[test]
    fn get_amount_out_contract() {
        let two_arb = 2_u128 * 10_u128.pow(18_u32);
        let sqrt_p_x96 = U256::from(2910392625228200618462908431436_u128);
        let liquidity = U256::from(3055895843484221589591460_u128);

        let amount_out = super::get_amount_out(
            two_arb,
            &sqrt_p_x96,
            &U256::from(3055895843484221589591460_u128),
            500_u32,
            true,
        );
        dbg!(amount_out);

        assert_eq!(
            amount_out.1,
            // 2697406212000332726834 1:1 U256 port...
            // 2697_727195625540073615 0.0119%
            2697_730325051490989803_u128, // arb
        );
    }

    #[test]
    fn get_amount_1_delta_overflow() {
        let current_sqrt_p_x96 = U256::from(3379669370077374717864357_u128);
        let liquidity = U256::from(20928880794762457722_u128);
        let fee_pips = 500;
        let zero_for_one = true;

        let amount_in = 125000000000000000_u128;
        get_amount_out(
            amount_in,
            &current_sqrt_p_x96,
            &liquidity,
            fee_pips,
            zero_for_one,
        );
    }
}
