//! Common data types and traits

pub use ethers::types::{Address, U256};
use variant_count::VariantCount;

use crate::constant::arbitrum::{ARB, DAI, GMX, USDC, USDT, WBTC, WETH};

/// Represents an asset type
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, VariantCount)]
pub enum Token {
    // THIS ORDER MUST NOT CHANGE arbitrarily see contract/TradeExecutor.sol
    USDC = 0,
    WETH = 1,
    WBTC = 2,
    ARB = 3,
    USDT = 4,
    DAI = 5,
    GMX = 6,
}

impl Token {
    /// Cast usize into `Token`
    pub fn from_usize(x: usize) -> Self {
        match x {
            0 => Self::USDC,
            1 => Self::WETH,
            2 => Self::WBTC,
            3 => Self::ARB,
            4 => Self::USDT,
            5 => Self::DAI,
            6 => Self::GMX,
            _ => panic!("unsupported token index"),
        }
    }
    /// The onchain address of the token contract
    pub fn address(&self) -> Address {
        match self {
            Self::WETH => WETH.into(),
            Self::USDC => USDC.into(),
            Self::WBTC => WBTC.into(),
            Self::ARB => ARB.into(),
            Self::USDT => USDT.into(),
            Self::DAI => DAI.into(),
            Self::GMX => GMX.into(),
        }
    }
    pub fn from_address(a: [u8; 20]) -> Self {
        match a {
            WETH => Self::WETH,
            USDC => Self::USDC,
            WBTC => Self::WBTC,
            ARB => Self::ARB,
            USDT => Self::USDT,
            DAI => Self::DAI,
            GMX => Self::GMX,
            _ => unimplemented!(),
        }
    }
    /// The decimals of the token
    pub fn decimals(&self) -> u8 {
        match self {
            Self::USDC | Self::USDT => 6,
            Self::WBTC => 8,
            _ => 18,
        }
    }
}

/// A trading pair/pool
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    pub token0: Token,
    pub token1: Token,
    pub fee: u16,
    pub exchange_id: ExchangeId,
}

impl Pair {
    /// Return the pair's tokens
    pub fn tokens(&self) -> (Token, Token) {
        (self.token0, self.token1)
    }
    /// Return the pair's fee (as in uniswap v3 fee tier or uniswapV2 protocol wide fee)
    pub fn fee(&self) -> u16 {
        self.fee
    }
    /// Create a new pair (a, b) as given
    pub fn new_raw(a: Token, b: Token, fee: u16, exchange_id: ExchangeId) -> Self {
        Self {
            token0: a,
            token1: b,
            fee,
            exchange_id,
        }
    }
    /// Create a new pair (orders a/b based on their address as per Uniswap v2)
    /// `fee` denotes the pair's pool fee as in uniswap v3
    pub fn new(a: Token, b: Token, fee: u16, exchange_id: ExchangeId) -> Self {
        // optimization for univ2, always organize pair by address
        if a.address() < b.address() {
            Self {
                token0: a,
                token1: b,
                fee,
                exchange_id,
            }
        } else {
            Self {
                token0: b,
                token1: a,
                fee,
                exchange_id,
            }
        }
    }
}

/// Unique ID for a router contract
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouterId {
    UniswapV3RouterV1 = 0,
    UniswapV3RouterV2 = 1,
    UniswapV3UniversalRouter = 2,
    SushiRouterV2 = 3,
    CamelotRouterV2 = 4,
    Gmx = 5,
    ParaswapAugustus = 6,
    OneInch = 7,
    ZeroEx = 8,
    // Value([u8; 20]) = 9,
    Odos = 10,
}

/// Unique ID for an exchange
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExchangeId {
    /// UniswapV3
    Uniswap = 0,
    Camelot = 1,
    Sushi = 2,
    Chronos = 3,
    Zyber = 4,
    /// Non-production price source
    Test = 255,
}

/// Represents a token position
#[derive(Debug)]
pub struct Position {
    /// The amount this position holds in units
    /// We don't intend to managed positions > 2 ** 128
    pub amount: u128,
    /// The token this position is in
    pub token: Token,
}

impl Position {
    /// Create a new position with `amount` units and `token`
    pub fn new(amount: u128, token: Token) -> Self {
        Self { amount, token }
    }
    /// Create a position of `size` whole `token`s
    pub fn of(size: u32, token: Token) -> Self {
        Self::new(size as u128 * 10_u128.pow(token.decimals() as u32), token)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn token_id_order() {
        // THIS ORDER MUST NOT CHANGE arbitrarily see contract/TradeExecutor.sol
        assert_eq!(Token::from_usize(0), Token::USDC);
        assert_eq!(Token::from_usize(1), Token::WETH);
        assert_eq!(Token::from_usize(2), Token::WBTC);
        assert_eq!(Token::from_usize(3), Token::ARB);
        assert_eq!(Token::from_usize(4), Token::USDT);
        assert_eq!(Token::from_usize(5), Token::DAI);
        assert_eq!(Token::from_usize(6), Token::GMX);
    }
}
