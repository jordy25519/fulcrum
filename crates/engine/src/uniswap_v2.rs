//! Uniswap v2 price source
use ethabi_static::DecodeStatic;
use ethers::{
    abi::{encode_packed, Token as ABIToken},
    utils::keccak256,
};

use crate::types::{Address, Pair, U256};

pub const FEE_DENOMINATOR: u128 = 100_000;

/// Mirror router 'getAmountOut' calculation
pub fn get_amount_out(fee: u16, amount_in: u128, reserve_in: u128, reserve_out: u128) -> u128 {
    let amount_in_with_fee = U256::from(amount_in * (FEE_DENOMINATOR - fee as u128));
    // y0 = (y.x0)  / (x + x0)
    let amount_out = (U256::from(reserve_out) * amount_in_with_fee)
        / ((U256::from(reserve_in) * U256::from(FEE_DENOMINATOR)) + amount_in_with_fee);

    amount_out.as_u128()
}

/// Mirror router 'getAmountOut' calculation
pub fn get_amount_in(fee: u16, amount_out: u128, reserve_in: u128, reserve_out: u128) -> u128 {
    let numerator = reserve_in * amount_out * FEE_DENOMINATOR;
    let denominator = reserve_out - (amount_out * (FEE_DENOMINATOR - fee as u128));
    (numerator / denominator) + 1
}

/// `get_amount_out` with float (speed > precision)
pub fn get_amount_out_f(fee: u16, amount_in: u128, reserve_in: u128, reserve_out: u128) -> f64 {
    let amount_in_with_fee = (amount_in * (FEE_DENOMINATOR - fee as u128)) as f64;
    // y0 = (y.x0)  / (x + x0)
    let amount_out = ((reserve_out as f64) * amount_in_with_fee)
        / ((reserve_in as f64 * FEE_DENOMINATOR as f64) + amount_in_with_fee);

    amount_out
}

/// Calculate the canonical UniswapV2 pair address for the given `Pair` and `factory`
/// ```solidity
/// function pairFor(address factory, address tokenA, address tokenB) internal pure returns (address pair) {
///     let init_code_hash = hex!("96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f");
///     (address token0, address token1) = sortTokens(tokenA, tokenB);
///     pair = address(uint(keccak256(abi.encodePacked(
///             hex'ff',
///             factory,
///             keccak256(abi.encodePacked(token0, token1)),
///             hex'96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f' // init code hash
///         ))));
/// }
/// ```
pub fn pair_address_for(pair: &Pair, factory: Address, init_code_hash: &[u8; 32]) -> Address {
    let (a, b) = pair.tokens();
    let token_0 = a.address();
    let token_1 = b.address();

    let encoded = encode_packed(&[
        ABIToken::Bytes(vec![0xff]),
        ABIToken::Address(factory),
        ABIToken::FixedBytes(
            keccak256(
                encode_packed(&[ABIToken::Address(token_0), ABIToken::Address(token_1)])
                    .expect("it encodes")
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
pub struct UniswapV2Reserves {
    pub reserve_0: u128,
    pub reserve_1: u128,
    // custom: u32, unused
}

#[cfg(test)]
mod test {
    use hex_literal::hex;

    use crate::{
        constant::arbitrum::{CAMELOT_FACTORY, CAMELOT_INIT_CODE_HASH},
        types::{Address, ExchangeId, Pair, Token},
    };

    use super::*;

    #[test]
    fn pair_address() {
        let expected = Address::from(hex!("84652bb2539513BAf36e225c930Fdd8eaa63CE27"));
        assert_eq!(
            pair_address_for(
                &Pair::new(Token::WETH, Token::USDC, 0, ExchangeId::Camelot),
                CAMELOT_FACTORY.into(),
                &CAMELOT_INIT_CODE_HASH
            ),
            expected
        );
        assert_eq!(
            pair_address_for(
                &Pair::new(Token::WETH, Token::USDC, 500, ExchangeId::Camelot),
                CAMELOT_FACTORY.into(),
                &CAMELOT_INIT_CODE_HASH
            ),
            expected
        );
    }

    #[test]
    fn get_amount_out_contract() {
        assert_eq!(
            get_amount_out(
                9970,
                5000000000000000000,
                2757113099049556297952,
                5176991819833
            ),
            9343369893
        );
    }
}
