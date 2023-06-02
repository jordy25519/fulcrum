//! Terminal cli stuff
use argh::FromArgs;
use ethers_middleware::core::types::Chain;
use fulcrum_engine::types::Address;

#[derive(FromArgs)]
/// Low latency arbitrage engine
pub struct FulcrumCli {
    #[argh(subcommand)]
    pub sub_command: SubCommand,
    #[argh(option)]
    /// websocket connection string
    pub ws: String,
    #[argh(option, from_str_fn(parse_chain))]
    /// network/chain to connect with
    pub chain: Chain,
}

#[derive(FromArgs)]
#[argh(subcommand)]
pub enum SubCommand {
    Run(RunCommand),
    Prices(PricesCommand),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "prices")]
/// Fetch prices at target block and dump the output
pub struct PricesCommand {
    #[argh(option, from_str_fn(parse_block_number))]
    /// block number to fetch prices at
    pub at: u64,
}

#[derive(FromArgs)]
#[argh(subcommand, name = "run")]
/// Run the fulcrum trade engine
pub struct RunCommand {
    #[argh(option, from_str_fn(parse_key))]
    /// the private key for tx execution account
    pub key: Option<String>,
    #[argh(option, from_str_fn(parse_min_profit))]
    /// minimum profit required for trade execution
    pub min_profit: f64,
    #[argh(switch)]
    /// activate listen only mode
    pub dry_run: bool,
    #[argh(option, from_str_fn(parse_address))]
    /// deployed executor contract address
    pub executor: Address,
}

fn parse_block_number(s: &str) -> Result<u64, String> {
    s.parse::<u64>().map_err(|_| "valid block number".into())
}

fn parse_address(raw_address: &str) -> Result<Address, String> {
    let raw_address = if let Some(raw_address) = raw_address.strip_prefix("0x") {
        raw_address
    } else {
        raw_address
    }
    .to_lowercase();

    let mut dst = <[u8; 20]>::default();
    faster_hex::hex_decode(raw_address.as_bytes(), &mut dst).expect("valid address");

    Ok(Address::from(dst))
}

fn parse_min_profit(raw_min_profit: &str) -> Result<f64, String> {
    let min_profit = raw_min_profit.parse::<f64>().expect("it is a float");
    if min_profit > 1.0 {
        return Err("use a value < 1.0".to_string());
    }

    Ok(min_profit)
}

fn parse_chain(raw_chain: &str) -> Result<Chain, String> {
    match raw_chain.to_lowercase().as_str() {
        "optimisim" => Ok(Chain::Optimism),
        "arbitrum" => Ok(Chain::Arbitrum),
        _ => Ok(Chain::Arbitrum),
    }
}

/// Parse an ECDSA private key
/// We expect it to be hex-ified
fn parse_key(raw_key: &str) -> Result<String, String> {
    // TODO: load from file path
    let raw_key = if let Some(raw_key) = raw_key.strip_prefix("0x") {
        raw_key
    } else {
        raw_key
    }
    .to_lowercase();

    // no regex in stdlib can't be bothered with another crate, let consumer handle this error
    // if raw_key.matches("([0-9a-f]+)").count() != 1 {
    //     return Err("key not hex".to_string());
    // }
    Ok(raw_key)
}
