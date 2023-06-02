//! Constants
use hex_literal::hex;

pub mod arbitrum {
    #![allow(unused)]
    //! Arbitrum mainnet constants
    use super::*;
    pub const ARBIDEX_FACTORY: [u8; 20] = hex!("1c6e968f2e6c9dec61db874e28589fd5ce3e1f2c");
    pub const ARBIDEX_INIT_CODE_HASH: [u8; 32] =
        hex!("724c966696ee786bca53d9ebb15f34f4961a6a2d55b17599d6dd4681a335275a");
    pub const ARBIDEX_ROUTER: [u8; 20] = hex!("3E48298A5Fe88E4d62985DFf65Dee39a25914975");
    pub const CAMELOT_INIT_CODE_HASH: [u8; 32] =
        hex!("a856464ae65f7619087bc369daaf7e387dae1e5af69cfa7935850ebf754b04c1");
    pub const CAMELOT_FACTORY: [u8; 20] = hex!("6eccab422d763ac031210895c81787e87b43a652 ");
    pub const CAMELOT_ROUTER: [u8; 20] = hex!("c873fEcbd354f5A56E00E710B90EF4201db2448d");
    pub const SUSHI_FACTORY: [u8; 20] = hex!("c35dadb65012ec5796536bd9864ed8773abc74c4");
    pub const SUSHI_INIT_CODE_HASH: [u8; 32] =
        hex!("e18a34eb0e04b04f7a0ac29a6e80748dca96319b42c54d679cb821dca90c6303");
    pub const SUSHI_ROUTER: [u8; 20] = hex!("1b02dA8Cb0d097eB8D57A175b88c7D8b47997506");
    // https://arbiscan.io/address/0xfc506aaa1340b4dedffd88be278bee058952d674#writeContract
    pub const SUSHI_ROUTE_PROCESSOR_3: [u8; 20] = hex!("0000900e00070d8090169000D2B090B67f0c1050");
    pub const UNISWAP_V3_FACTORY: [u8; 20] = hex!("1F98431c8aD98523631AE4a59f267346ea31F984");
    pub const UNISWAP_V3_INIT_CODE_HASH: [u8; 32] =
        hex!("e34f199b19b2b4f47f68442619d555527d244f78a3297ea89325f843f87b8b54");
    pub const UNISWAP_V3_UNIVERSAL_ROUTER: [u8; 20] =
        hex!("4C60051384bd2d3C01bfc845Cf5F4b44bcbE9de5");
    pub const UNISWAP_V3_ROUTER_V1: [u8; 20] = hex!("E592427A0AEce92De3Edee1F18E0157C05861564");
    pub const UNISWAP_V3_ROUTER_V2: [u8; 20] = hex!("68b3465833fb72A70ecDF485E0e4C7bD8665Fc45");
    pub const LAYER_ZERO_SWAP_BRIDGE: [u8; 20] = hex!("0A9f824C05A74F577A536A8A0c673183a872Dff4");
    pub const PARASWAP_AUGUSTUS: [u8; 20] = hex!("DEF171Fe48CF0115B1d80b88dc8eAB59176FEe57");
    pub const ONE_INCH_ROUTER_V5: [u8; 20] = hex!("1111111254eeb25477b68fb85ed929f73a960582");
    pub const ONE_INCH_ROUTER_V4: [u8; 20] = hex!("1111111254fb6c44bAC0beD2854e76F90643097d");
    pub const ZERO_EX_ROUTER: [u8; 20] = hex!("Def1C0ded9bec7F1a1670819833240f027b25EfF");
    pub const CHRONOS_ROUTER: [u8; 20] = hex!("E708aA9E887980750C040a6A2Cb901c37Aa34f3b");
    pub const GMX_ROUTER: [u8; 20] = hex!("aBBc5F99639c9B6bCb58544ddf04EFA6802F4064");
    pub const ODOS_ROUTER: [u8; 20] = hex!("dd94018F54e565dbfc939F7C44a16e163FaAb331");

    /// Arbitrum WETH token address
    pub const WETH: [u8; 20] = hex!("82aF49447D8a07e3bd95BD0d56f35241523fBab1");
    /// Arbitrum USDC token address
    pub const USDC: [u8; 20] = hex!("FF970A61A04b1cA14834A43f5dE4533eBDDB5CC8");
    /// Arbitrum USDT token address
    pub const USDT: [u8; 20] = hex!("Fd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9");
    /// Arbitrum DAI token address
    pub const DAI: [u8; 20] = hex!("DA10009cBd5D07dd0CeCc66161FC93D7c9000da1");
    /// Arbitrum WBTC token address
    pub const WBTC: [u8; 20] = hex!("2f2a2543B76A4166549F7aaB2e75Bef0aefC5B0f");
    /// Arbitrum ARB token address
    pub const ARB: [u8; 20] = hex!("912CE59144191C1204E64559FE8253a0e49E6548");
    /// Arbitrum GMX token address
    pub const GMX: [u8; 20] = hex!("fc5A1A6EB076a2C7aD06eD22C90d7E710E35ad0a");
    /// Arbitrum RDNT token address
    pub const RDNT: [u8; 20] = hex!("3082CC23568eA640225c2467653dB90e9250AaA0");
}
