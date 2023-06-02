//! Engine provides main loop
use std::time::Instant;

use bumpalo::Bump;
use ethers_providers::Middleware;
use log::{debug, error, info, warn};

use fulcrum_sequencer_feed::{SequencerFeed, TxBuffer};
use fulcrum_ws_cli::FastWsClient;

use crate::{
    order::OrderService, price::PriceService, price_graph::Path, trade_simulator::TradeSimulator,
    types::Position,
};

/// The Fulcrum trading engine
pub struct Engine<M: Middleware + 'static> {
    /// Provides price information
    price_service: PriceService<M>,
    /// Provide trade order execution
    order_service: OrderService<M>,
    /// Sequencer tx feed
    sequencer_feed: SequencerFeed,
}

impl<M> Engine<M>
where
    M: Middleware<Provider = FastWsClient> + 'static,
{
    /// Initialize a new trading engine
    pub fn new(
        price_service: PriceService<M>,
        order_service: OrderService<M>,
        sequencer_feed: SequencerFeed,
    ) -> Self {
        Self {
            sequencer_feed,
            price_service,
            order_service,
        }
    }
    /// Start the trading engine loop
    ///
    /// `search_paths` - trade paths to search for arbitrage opportunities (given some start position)
    /// `min_profit` the minimum profit required for trade execution, expressed as a percent e.g 0.007f64 = 0.007%
    /// `dry_run` when true runs passive mode/disallows tx submission for trades
    pub async fn run(
        mut self,
        search_paths: &[(Position, &[Path])],
        min_profit: f64,
        dry_run: bool,
    ) {
        let min_profit_threshold = 1.0_f64 + min_profit;
        let bump = Bump::with_capacity(1024 * 1_000); // 1mib bump allocator for hot loop
        let mut syncing = false;

        let (price_requests, price_queue) = self.price_service.start().await;
        let trade_requests = self.order_service.start(dry_run).await;

        while let Ok(frame) = self.sequencer_feed.next_message().await {
            let mut t0 = Instant::now();
            // handling frame here is strange but need the ownership of the received message at the top level
            // to avoid copying
            let (header, mut payload) = frame.parts();
            let mut tx_buffer = TxBuffer::new(&bump);
            if let Err(err) = self
                .sequencer_feed
                .handle_frame(&header, payload.as_mut(), &mut tx_buffer)
                .await
            {
                error!("tx feed: {:?}", err);
                syncing = true;
                continue;
            }

            // feed message is not useful
            if tx_buffer.block_number() == 0 {
                debug!("nothing to simulate, skip");
                continue;
            }

            // drive the sequencer feed until it is syncing in time with the price source
            // assuming a fast local, full node this can be improved to use an event driven setup, for now this is effective for syncing a remote full node
            if syncing {
                let price_service_block = self.price_service.block_number().await;
                let _ = price_queue.try_recv(); // ensure price queue is empty
                if tx_buffer.block_number() <= price_service_block {
                    info!(
                        "awaiting feed <> price sync 🔄: {}/{}",
                        tx_buffer.block_number(),
                        price_service_block,
                    );
                    continue;
                }
                // we got update for block B, price source already processed update at block B
                // so we are lagging slightly
                info!("price feed sync'd ⚡️⚡️⚡️: {}", tx_buffer.block_number());
                let _ = price_requests.send(tx_buffer.block_number()).await;
                syncing = false;
                continue;
            }

            // acting as minimal light client, simulate all txs we care about based on the sequencer feed
            // for feed block N, requires price information for block N - 1
            // - execute any arbs
            // - sync real prices from a proper full node for next round (concurrently)
            let _ = price_requests.send(tx_buffer.block_number()).await;
            // check if prices for current block ready
            let mut price_graph_ref = price_queue.recv_ref().await.expect("price graph ready");
            let price_graph = match price_graph_ref.as_mut() {
                Some(price_graph) => price_graph,
                None => {
                    // prices were not fetched, either due to error or deadline
                    // its likely we can't execute arbs fast enough at this point, skip the price sync for this block
                    info!(
                        "skip batch: #{} unable to fetch block: #{}",
                        tx_buffer.block_number(),
                        tx_buffer.block_number() - 1,
                    );
                    // if here, the queued price graph ref is probably wasted
                    syncing = true;
                    continue;
                }
            };

            info!(
                "🛠️ applying txs from batch: #{} to block: #{} {:?}",
                tx_buffer.block_number(),
                price_graph.block_number(),
                Instant::now() - t0
            );

            // try simulate new trades
            t0 = Instant::now();
            let mut trade_simulator = TradeSimulator::new(price_graph);
            for tx in tx_buffer.as_slice() {
                trade_simulator.wrangle_transaction(tx);
                // we can't faithfully simulate all the transactions, skip this round
                if trade_simulator.skipped() {
                    warn!("skipped trade simulation");
                    break;
                }
            }
            debug!("simulated txs ⚙️: {:?}", Instant::now() - t0);

            t0 = Instant::now();
            if !trade_simulator.skipped() && price_graph.touched() {
                let mut best_trade_percent = min_profit_threshold;
                let mut best_trade = None;
                // TODO: only consider 'touched' paths
                for (position, path) in search_paths {
                    if let Some((amount_out, trade_path)) = price_graph.find_arb(position, path) {
                        let profit_percent = amount_out as f64 / position.amount as f64;
                        if profit_percent > best_trade_percent {
                            info!("arb found 💵: {profit_percent}%\n{}", &trade_path);
                            best_trade_percent = profit_percent;
                            best_trade = Some((position.amount, trade_path));
                        }
                    }
                }
                if let Some((amount, path)) = best_trade {
                    trade_requests
                        .send((amount, path))
                        .await
                        .expect("trade sent");
                    // trace!("{}", price_graph);
                }
                info!(
                    "checked arbs 🔎 (#{}): {:?}",
                    price_graph.block_number(),
                    Instant::now() - t0
                );
            }
        }
    }
}

/// Utility method for building a price graph at block and dumping the output
pub async fn prices_at<M: Middleware<Provider = FastWsClient> + 'static>(
    price_service: PriceService<M>,
    at: u64,
) {
    let (price_requests, price_queue) = price_service.start().await;
    price_requests.send(at).await.expect("price sync request");
    let price_graph = price_queue.recv_ref().await.expect("price graph ready");
    println!("{}", price_graph.as_ref().expect("price graph built"));
}
