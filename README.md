# Fulcrum

Fulcrum is an experimental, low-latency engine for arbitrage trading on Arbitrum L2.  
Primary design goal is to measure strategy latency in single-digit µ seconds.  
Created out of interest in understanding HFT systems and the arbitrum rollup architecture.  

## Motivation/Design

Fulcrum builds a price graph on each block and simulates new transactions from the raw sequencer feed directly against the local price graph.  
It does not run a full EVM and so relies on a full node to sync prices at sequencer batch block - 1 (i.e trades accuracy for latency).  

```bash
crates
├── engine         # primary trading engine
├── sequencer-feed # fast feed deserializer
└── ws-cli         # fast(er), minimal fork of ethers-providers
```

## Run
```bash
$  ./target/release/fulcrum \
    --chain arbitrum --ws <WsEndpoint> \
    run --min-profit 0.0002 \
    --key <PrivateSeed> \
    --executor <ExecutorContract> \
    --dry-run
```

## Profile (MacOS)
```bash
$ cargo install samply

$ samply record ./target/profiling/fulcrum
```

## Test
```bash
$  cargo test --workspace
```

## Bench
```
$  cargo +nightly bench --features=bench --profile=release  
```

## Build
```bash
$  sudo apt install build-essential pkg-config libssl-dev

$ RUSTFLAGS='-C target-cpu=native' cargo build --release
```

## Contracts
```bash
$ forge test --fork-url <rpc-url> -vvv
```
