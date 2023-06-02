//! Price graph provides a data structure for finding price arbitrage opportunities
use std::fmt::{self};

use ethers::types::U256;
use log::{debug, trace};
use once_cell::sync::Lazy;

use crate::{
    types::{ExchangeId, Pair, Position, Token},
    uniswap_v2, uniswap_v3,
    util::{NoopHasherU32, U32Map},
};

/// Lookup table from token decimals to one whole token
/// Used to calculate edge scores
static ONE_LOOKUP_TABLE: Lazy<[u128; N]> = Lazy::new(|| {
    let mut lookup_table = <[u128; N]>::default();
    lookup_table[Token::USDC as usize] = 5000 * 10_u128.pow(6_u32);
    lookup_table[Token::USDT as usize] = 5000 * 10_u128.pow(6_u32);
    lookup_table[Token::WBTC as usize] = 1 * 10_u128.pow(7_u32);
    lookup_table[Token::WETH as usize] = 3 * 10_u128.pow(18_u32);
    lookup_table[Token::ARB as usize] = 4_500 * 10_u128.pow(18_u32);

    lookup_table
});

// TODO: `core::mem::variant_count` when stable
/// Max edges in the price graph
const N: usize = Token::VARIANT_COUNT;
const _: () = assert!(N <= 64, "update pair identity hash");

/// Unique edge identifier
type EdgeId = u32;

/// A graph edge (weight, exchange)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    UniV2 {
        reserve_in: u128,
        reserve_out: u128,
        fee: u16,
        exchange_id: ExchangeId,
    },
    UniV3 {
        // sqrt price ratio x 2**96
        sqrt_p_x96: U256,
        liquidity: U256,
        fee: u16,
        /// Is this edge a token0 => token1 trade
        zero_for_one: bool,
    },
}

impl Edge {
    /// quick edge hash
    /// a - token in
    /// b - token out
    /// c - exchange id
    /// d - pool fee (0 for v2 edges)
    pub fn hash(a: u8, b: u8, c: u8, fee: u16) -> u32 {
        // 8bit in | 8bit out | 8bit exchange | 16bit (fee)
        ((a & 63_u8) as u32)
            | (((b & 63_u8) as u32) << 5)
            | (((c & 63_u8) as u32) << 10)
            | ((fee as u32) << 16)
    }
    /// Get unique id of the edge
    pub fn id(&self, token_in: Token, token_out: Token) -> EdgeId {
        match self {
            Edge::UniV2 {
                exchange_id, fee, ..
            } => Edge::hash(token_in as u8, token_out as u8, *exchange_id as u8, *fee),
            Edge::UniV3 { fee, .. } => Edge::hash(
                token_in as u8,
                token_out as u8,
                ExchangeId::Uniswap as u8,
                *fee,
            ),
        }
    }
    /// Return the inverse edge
    pub fn inverse(self) -> Edge {
        match self {
            Edge::UniV2 {
                reserve_in,
                reserve_out,
                fee,
                exchange_id,
            } => Edge::new_v2(reserve_out, reserve_in, fee, exchange_id),
            Edge::UniV3 {
                sqrt_p_x96,
                liquidity,
                fee,
                zero_for_one,
            } => Edge::new_v3(sqrt_p_x96, liquidity, fee, !zero_for_one),
        }
    }
    /// Create a new Uniswap V2 style edge
    pub fn new_v2(reserve_in: u128, reserve_out: u128, fee: u16, exchange_id: ExchangeId) -> Edge {
        Edge::UniV2 {
            reserve_in,
            reserve_out,
            fee,
            exchange_id,
        }
    }
    /// Create a new Uniswap V3 style edge
    pub fn new_v3(sqrt_p_x96: U256, liquidity: U256, fee: u16, zero_for_one: bool) -> Edge {
        Edge::UniV3 {
            sqrt_p_x96,
            liquidity,
            fee,
            zero_for_one,
        }
    }
    pub fn fee(&self) -> u16 {
        match self {
            Self::UniV2 { fee, .. } => *fee,
            Self::UniV3 { fee, .. } => *fee,
        }
    }
    pub fn exchange_id(&self) -> ExchangeId {
        match self {
            Self::UniV2 { exchange_id, .. } => *exchange_id,
            Self::UniV3 { .. } => ExchangeId::Uniswap,
        }
    }
    /// calculate the amount out given `amount_in` for the edge (fast, less precise)
    pub fn calculate_amount_out_f(&self, amount_in: u128) -> f64 {
        match self {
            Self::UniV2 {
                fee,
                reserve_in,
                reserve_out,
                ..
            } => uniswap_v2::get_amount_out_f(*fee, amount_in, *reserve_in, *reserve_out),
            Self::UniV3 {
                sqrt_p_x96,
                liquidity,
                zero_for_one,
                fee,
                ..
            } => {
                uniswap_v3::get_amount_out_f(
                    amount_in,
                    sqrt_p_x96.as_u128() as f64, // maybe this blows up
                    liquidity.as_u128() as f64,
                    *fee as u32,
                    *zero_for_one,
                )
            }
        }
    }
    /// calculate the amount out given `amount_in` for the edge
    pub fn calculate_amount_out(&self, amount_in: u128) -> u128 {
        match self {
            Self::UniV2 {
                fee,
                reserve_in,
                reserve_out,
                ..
            } => uniswap_v2::get_amount_out(*fee, amount_in, *reserve_in, *reserve_out),
            Self::UniV3 {
                sqrt_p_x96,
                liquidity,
                zero_for_one,
                fee,
                ..
            } => {
                uniswap_v3::get_amount_out(
                    amount_in,
                    sqrt_p_x96,
                    liquidity,
                    *fee as u32,
                    *zero_for_one,
                )
                .1
            }
        }
    }
    /// Calculate output amount and shifts the price (as if applying the trade)
    /// Returns amount out given `amount_in`
    pub fn calculate_amount_out_updating(&mut self, amount_in: u128) -> u128 {
        match self {
            Self::UniV2 {
                fee,
                reserve_in,
                reserve_out,
                ..
            } => {
                let amount_out =
                    uniswap_v2::get_amount_out(*fee, amount_in, *reserve_in, *reserve_out);
                *reserve_in += amount_in;
                *reserve_out -= amount_out;
                amount_out
            }
            Self::UniV3 {
                sqrt_p_x96,
                liquidity,
                zero_for_one,
                fee,
                ..
            } => {
                let (new_sqrt_p_x96, amount_out) = uniswap_v3::get_amount_out(
                    amount_in,
                    sqrt_p_x96,
                    liquidity,
                    *fee as u32,
                    *zero_for_one,
                );
                *sqrt_p_x96 = new_sqrt_p_x96;
                amount_out
            }
        }
    }
    /// Calculate the input amount required to take `amount_out` of the edge and shifts the price (as if applying the trade)
    /// Returns `amount_in` owed
    pub fn calculate_amount_in_updating(&mut self, amount_out: u128) -> u128 {
        match self {
            Self::UniV2 {
                fee,
                reserve_in,
                reserve_out,
                ..
            } => {
                let amount_in =
                    uniswap_v2::get_amount_out(*fee, amount_out, *reserve_in, *reserve_out);
                *reserve_in += amount_in;
                *reserve_out -= amount_out;
                amount_out
            }
            Self::UniV3 {
                sqrt_p_x96,
                liquidity,
                zero_for_one,
                fee,
                ..
            } => {
                let (new_sqrt_p_x96, amount_in) = uniswap_v3::get_amount_in(
                    amount_out,
                    sqrt_p_x96,
                    liquidity,
                    *fee as u32,
                    *zero_for_one,
                );
                *sqrt_p_x96 = new_sqrt_p_x96;
                amount_in
            }
        }
    }
}

/// Part of a `CompositeTrade`
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Trade {
    /// Fulcrum Id of the token to sell
    pub token_in: u8,
    /// Fulcrum Id of the token to receive
    pub token_out: u8,
    /// The pool fee tier (generally 0 for uniswap v2 pairs)
    pub fee_tier: u16,
    /// Fulcrum Id of the exchange to execute the trade
    pub exchange_id: u8,
}
impl Trade {
    pub fn new(token_in: u8, token_out: u8, fee_tier: u16, exchange_id: u8) -> Self {
        Self {
            token_in,
            token_out,
            fee_tier,
            exchange_id,
        }
    }
}
/// A trade path consisting of 2 or 3 `Trades`
/// The 3rd trade may be a semantic noop
#[derive(Copy, Clone, Default, Debug, PartialEq)]
pub struct CompositeTrade {
    pub path: [Trade; 3],
}

impl fmt::Display for CompositeTrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = write!(f, "Trade: ");
        for trade in self.path {
            write!(
                f,
                "{}/{}/{}/{} ->",
                trade.token_in, trade.token_out, trade.fee_tier, trade.exchange_id
            )?;
        }
        Ok(())
    }
}

impl CompositeTrade {
    pub fn new(path: [Trade; 3]) -> Self {
        Self { path }
    }
    /// Return whether the trade paths intersect at any point
    pub fn intersects(self, other: Self) -> bool {
        // compiler should infer the slice indexes are in bounds
        let own: u32 = 1_u32 << self.path[0].token_in
            | 1_u32 << self.path[0].token_out
            | 1_u32 << self.path[1].token_out;

        let other: u32 = 1_u32 << other.path[0].token_in
            | 1_u32 << other.path[0].token_out
            | 1_u32 << other.path[1].token_out;

        own & other > 0
    }
}
/// A reflexive path type
pub type ReflexivePath = [(usize, usize); 2]; // storing twice is technically redundant as its always a/b, b/a
/// A triangle path type
pub type TrianglePath = [(usize, usize); 3];
/// An abstract, prebuilt price graph path e.g 'weth/usdc <> usdc/weth'
/// The exact edges are determined at runtime by the price graph
#[derive(Clone, Debug, PartialEq)]
pub enum Path {
    /// Path with immediate neighbor from start
    /// `base_id` uniquely identifies the base (1st) edge
    Reflexive { path: ReflexivePath, base_id: u16 },
    /// Path with 2nd degree neighbor from start
    /// `base_id` uniquely identifies the base (1st) edge
    Triangle { path: TrianglePath, base_id: u16 },
}

impl Path {
    fn reflexive(path: [(usize, usize); 2]) -> Path {
        Path::Reflexive {
            path,
            base_id: Self::pair_identity(path[0].0 as u8, path[0].1 as u8),
        }
    }
    fn triangular(path: [(usize, usize); 3]) -> Path {
        Path::Triangle {
            path,
            base_id: Self::pair_identity(path[0].0 as u8, path[0].1 as u8),
        }
    }
    // Convert the path to a slice
    fn as_slice(&self) -> &[(usize, usize)] {
        match self {
            Self::Reflexive { path, .. } => path,
            Self::Triangle { path, .. } => path,
        }
    }
    /// Return the Path's base pair Id
    fn base_id(&self) -> u16 {
        match self {
            Self::Reflexive { base_id, .. } => *base_id,
            Self::Triangle { base_id, .. } => *base_id,
        }
    }
    /// simple pair 'hash' for two positive integers
    fn pair_identity(a: u8, b: u8) -> u16 {
        ((a as u16) << 8) | b as u16
    }
}

/// Maintains a sorted list of scores for the `S` best candidate edges
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreArray<const S: usize> {
    /// The score of all known edges from a/b e.g. WETH/USDC
    scores: [(f64, u32); S],
}

impl Default for ScoreArray<5> {
    fn default() -> Self {
        Self {
            scores: Default::default(),
        }
    }
}

impl<const S: usize> ScoreArray<S> {
    #[cfg(test)]
    /// Create a new score array from given values
    fn new(scores: [(f64, u32); S]) -> Self {
        Self { scores }
    }
    /// Insert score into the array at `index`
    fn update_at(&mut self, index: usize, edge_id: u32, new_score: f64) {
        unsafe {
            *self.scores.get_unchecked_mut(index) = (new_score, edge_id);
        }
    }
    /// Insert a new candidate score into the array based on existing scores
    fn insert(&mut self, edge_id: u32, new_score: f64) {
        let mut insert_score = new_score;
        let mut insert_edge_id = edge_id;
        for idx in 0..S {
            let (index_score, index_edge_id) = self.scores[idx];
            // empty score
            if index_score == 0.0 {
                self.scores[idx] = (insert_score, insert_edge_id);
                break;
            } else if insert_score >= index_score {
                // found place to insert, keep iterating to move the replaced value along
                self.scores[idx] = (insert_score, insert_edge_id);
                insert_score = index_score;
                insert_edge_id = index_edge_id;
            } else {
                // new score is < index_score
                // keep searching
                // could be removed entirely if more than `N` candidates
            }
        }
    }
    /// demote the top score in the array based on its new score
    fn demote(&mut self, new_score: f64) {
        if let Some(val) = self.scores.get_mut(0) {
            val.0 = new_score;
        }

        for idx in 0..S - 1 {
            if self.scores[idx + 1].0 > new_score {
                self.scores.swap(idx, idx + 1);
            }
        }
    }
    /// promote the edge as best, it may or may not exist already as a candidate
    fn promote(&mut self, edge_id: u32, new_score: f64) {
        let mut current_edge;
        let mut insert_edge = (new_score, edge_id);
        for idx in 0..S {
            current_edge = self.scores[idx];
            self.scores[idx] = insert_edge;
            if current_edge.1 == edge_id {
                break;
            }
            insert_edge = current_edge;
        }
    }
    /// Return the best score in the array (score, edge Id)
    fn best(&self) -> (f64, u32) {
        self.scores[0]
    }
    /// Return the runner up score in the array (score, edge Id)
    fn runner_up(&self) -> (f64, u32) {
        self.scores[1]
    }
}

/// Provides a searchable data structure for prices
#[derive(Clone, Debug)]
pub struct PriceGraph {
    /// Best graph edges
    hyper_loop: [[Option<Edge>; N]; N],
    /// Best edge scores (used in graph construction step)
    scores: [[ScoreArray<5>; N]; N],
    // All known edges
    all: U32Map<Edge>,
    /// Edges touched during a round of price updates.
    touched: bool,
    /// Block number for which the graph was built
    block_number: u64,
}

impl fmt::Display for PriceGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\n      ")?;
        for idx in 0..N {
            write!(f, "{:1?} ", Token::from_usize(idx))?;
        }
        writeln!(f)?;
        for (row_idx, row) in self.hyper_loop.iter().enumerate() {
            write!(f, "{:5?} ", Token::from_usize(row_idx))?;
            for col in row.iter() {
                match col {
                    Some(_) => write!(f, "[ x ]")?,
                    None => write!(f, "[   ]")?,
                }
            }
            writeln!(f)?;
        }
        writeln!(f, "scores")?;
        for scores in &self.scores {
            for score_a in scores {
                writeln!(f, "{:?}", score_a)?;
            }
            writeln!(f)?;
        }
        writeln!(f, "all")?;
        for (id, edge) in &self.all {
            writeln!(f, "{:?} - {:?}", id, edge)?;
        }
        Ok(())
    }
}

impl Default for PriceGraph {
    fn default() -> Self {
        Self {
            all: U32Map::<Edge>::with_capacity_and_hasher(50, NoopHasherU32::default()),
            hyper_loop: Default::default(),
            scores: Default::default(),
            touched: false,
            block_number: 0,
        }
    }
}

impl PriceGraph {
    /// Returns true if the price graph has been updated
    pub fn touched(&self) -> bool {
        self.touched
    }
    /// Reset price graph (calculated features only) for re-use at `block_number`
    pub fn reset(&mut self, block_number: u64) {
        self.hyper_loop = Default::default();
        self.scores = Default::default();
        self.touched = false;
        self.block_number = block_number;
    }
    /// Set the block number of the price graph
    pub fn set_block_number(&mut self, block_number: u64) {
        self.block_number = block_number;
    }
    /// Get the block number of the price graph
    pub fn block_number(&self) -> u64 {
        self.block_number
    }
    /// Create a new, empty price graph
    pub fn empty() -> Self {
        Self::default()
    }
    /// Add an edge to the price graph
    /// It is expected that a is token0 and b is token1 as in the uniswap token ordering
    pub fn add_edge(&mut self, a: Token, b: Token, edge_a_b: Edge) {
        self.score_edge_bidirectional(a, b, edge_a_b);
    }
    /// Update an edge in the graph with a trade adding `amount_in`
    pub fn update_edge_in(
        &mut self,
        token_in: Token,
        token_out: Token,
        edge_id: u32,
        amount_in: u128,
    ) -> Result<u128, ()> {
        let (amount_out, edge) = if let Some(edge) = self.all.get_mut(&edge_id) {
            debug!("before: {:?}", edge);
            self.touched = true;
            (edge.calculate_amount_out_updating(amount_in), *edge)
        } else {
            return Err(());
        };

        debug!("after: {:?}", edge);
        self.score_edge_bidirectional(token_in, token_out, edge);
        Ok(amount_out)
    }
    /// Update an edge in the graph with a trade taking `amount_out`
    pub fn update_edge_out(
        &mut self,
        token_out: Token,
        token_in: Token,
        edge_id: u32,
        amount_out: u128,
    ) -> Result<u128, ()> {
        let (amount_in, edge) = if let Some(edge) = self.all.get_mut(&edge_id) {
            debug!("before: {:?}", edge);
            self.touched = true;
            (edge.calculate_amount_in_updating(amount_out), *edge)
        } else {
            return Err(());
        };

        debug!("after: {:?}", edge);
        self.score_edge_bidirectional(token_in, token_out, edge);
        Ok(amount_in)
    }
    /// Score the bi-directional edge from a/b and b/a possibly noting it as the best edge
    /// i.e. call after the edge price has changed
    pub fn score_edge_bidirectional(&mut self, a: Token, b: Token, edge_ab: Edge) {
        let heuristic_amount_in_a = unsafe { *ONE_LOOKUP_TABLE.get_unchecked(a as usize) };
        let heuristic_amount_in_b = unsafe { *ONE_LOOKUP_TABLE.get_unchecked(b as usize) };
        let edge_ba = edge_ab.inverse();
        // could use sqrt(P)x96 as the heuristic
        // however very uniswap specific and requires tracking the token0/token1 ordering
        let new_score_ab = edge_ab.calculate_amount_out_f(heuristic_amount_in_a);
        let new_score_ba = edge_ba.calculate_amount_out_f(heuristic_amount_in_b);
        let edge_ab_id = edge_ab.id(a, b);
        let edge_ba_id = edge_ba.id(b, a);
        self.all.insert(edge_ab_id, edge_ab); // always reinsert the edge as it may've updated
        self.all.insert(edge_ba_id, edge_ba);

        let idx_a = a as usize;
        let idx_b = b as usize;
        if idx_a < N && idx_b < N {
            let scores = &mut self.scores[idx_a][idx_b];
            let (best_score, best_edge_id) = scores.best();

            if best_edge_id == edge_ab_id {
                // update the edge score if it is still the best otherwise promote the next best edge
                let (runner_up_score, runner_up_edge_id) = scores.runner_up();
                if runner_up_score > new_score_ab {
                    trace!("edge demote: {idx_a},{idx_b}");
                    self.hyper_loop[idx_a][idx_b] = self.all.get(&runner_up_edge_id).copied();
                    scores.demote(new_score_ab);
                } else {
                    trace!("edge update: {idx_a},{idx_b}");
                    // this edge is still the best
                    self.hyper_loop[idx_a][idx_b] = Some(edge_ab);
                    scores.update_at(0, best_edge_id, best_score);
                }
            } else if new_score_ab >= best_score {
                trace!("edge promote: {idx_a},{idx_b} > {best_edge_id}");
                self.hyper_loop[idx_a][idx_b] = Some(edge_ab);
                // 2 cases
                // 1) edge candidate is new, insert
                // 2) edge candidate exists, must update current score
                scores.promote(edge_ab_id, new_score_ab);
            } else {
                trace!("edge insert: {idx_a},{idx_b}");
                // edge is not and was not the best edge
                scores.insert(edge_ab_id, new_score_ab);
            }

            let scores = &mut self.scores[idx_b][idx_a];
            let (best_score, best_edge_id) = scores.best();
            if best_edge_id == edge_ba_id {
                // update the edge score if it is still the best otherwise promote the next best edge
                let (runner_up_score, runner_up_edge_id) = scores.runner_up();
                if runner_up_score > new_score_ba {
                    trace!("edge demote: {idx_b},{idx_a}");
                    self.hyper_loop[idx_b][idx_a] = self.all.get(&runner_up_edge_id).copied();
                    scores.demote(new_score_ba);
                } else {
                    trace!("edge update: {idx_b},{idx_a}");
                    // this edge is still the best
                    self.hyper_loop[idx_b][idx_a] = Some(edge_ba);
                    scores.update_at(0, best_edge_id, best_score);
                }
            } else if new_score_ba >= best_score {
                trace!("edge promote: {idx_b},{idx_a} > {best_edge_id}");
                self.hyper_loop[idx_b][idx_a] = Some(edge_ba);
                // 2 cases
                // 1) edge candidate is new, insert
                // 2) edge candidate exists, must update current score
                scores.promote(edge_ba_id, new_score_ba);
            } else {
                trace!("edge insert: {idx_b},{idx_a}");
                // edge is not and was not the best edge
                scores.insert(edge_ba_id, new_score_ba);
            }
        }
    }
    /// Find supported arbitrage paths for token `start` through the provided pairs list
    /// This is intended to be run once to produce searchable paths for `find_arb`
    pub fn find_paths(start: Token, pairs: &[Pair]) -> Vec<Path> {
        // reflex and triangles are always together and can be processed together for improved efficiency
        let mut paths = Vec::<Path>::with_capacity(2 * pairs.len());
        let start_idx = start as usize;
        // N possible edges from start node
        let mut edges = <[[Option<usize>; N]; N]>::default();
        for pair in pairs {
            let (a, b) = pair.tokens();
            edges[a as usize][b as usize] = Some(b as usize);
            edges[b as usize][a as usize] = Some(a as usize);
        }

        // find _supported_ paths
        for first_neighbor in edges[start_idx].into_iter().flatten() {
            for second_neighbor in edges[first_neighbor].into_iter().flatten() {
                if second_neighbor == start_idx {
                    paths.push(Path::reflexive([
                        (start_idx, first_neighbor),
                        (first_neighbor, start_idx),
                    ]));
                } else if edges[second_neighbor][start_idx].is_some() {
                    paths.push(Path::triangular([
                        (start_idx, first_neighbor),
                        (first_neighbor, second_neighbor),
                        (second_neighbor, start_idx),
                    ]));
                }
            }
        }

        paths
    }
    /// Find an arbitrage opportunity in the price graph
    ///
    /// Only prebuilt paths are checked i.e. from `PriceGraph::find_paths(start, pairs)`
    /// search paths are also filtered by edges given in `filter`
    pub fn find_arb(&self, start: &Position, paths: &[Path]) -> Option<(u128, CompositeTrade)> {
        let start_amount = start.amount;
        let mut best_output = start_amount;
        let mut best_trade: Option<usize> = None;
        let mut cache_amount_out = 0u128;
        let mut cache_base_id: u16 = 0;
        let mut edge: Edge;
        'outer: for (path_idx, path) in paths.iter().enumerate() {
            let mut current_output = start_amount;
            // is the previous path's base the same
            let set_cache = path.base_id() != cache_base_id;
            for (edge_idx, (a_idx, b_idx)) in path.as_slice().iter().enumerate() {
                debug!("trade output: {:?}", current_output);
                unsafe {
                    // TODO: jumps randomly around memory space
                    debug!("{a_idx},{b_idx}");
                    edge = (self.hyper_loop.get_unchecked(*a_idx).get_unchecked(*b_idx))
                        .expect("edge exists");
                }
                //  NB: could optimize with float calcs here, trade 100% exactness for speed is ok for flash swaps
                if edge_idx == 0 {
                    if set_cache {
                        cache_amount_out = edge.calculate_amount_out(current_output);
                        cache_base_id = path.base_id();
                    }
                    current_output = cache_amount_out;
                    continue;
                } else {
                    current_output = edge.calculate_amount_out(current_output);
                }
            }
            debug!("trade output: {:?}\nend trade\n", current_output);
            // check if we found a better path
            if current_output > best_output {
                best_trade = Some(path_idx);
                best_output = current_output;
            }
        }

        if let Some(best_trade) = best_trade {
            // make the trade path pretty for consumer
            let best_path = unsafe { paths.get_unchecked(best_trade) };
            let mut trade = <[Trade; 3]>::default();
            for (idx, (a, b)) in best_path.as_slice().iter().enumerate() {
                // TODO: size hints to remove the unsafe
                unsafe {
                    let edge = self
                        .hyper_loop
                        .get_unchecked(*a)
                        .get_unchecked(*b)
                        .expect("edge exists");
                    *trade.get_unchecked_mut(idx) =
                        Trade::new(*a as u8, *b as u8, edge.fee(), edge.exchange_id() as u8);
                };
            }
            Some((best_output, CompositeTrade::new(trade)))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        price_graph::Trade,
        types::{ExchangeId, Pair, Position, Token},
    };

    use super::{Edge, Path, PriceGraph, ScoreArray};

    pub fn eth(wei: u32) -> u128 {
        wei as u128 * 10_u128.pow(18_u32)
    }

    #[test]
    pub fn find_paths_triangular() {
        let pairs = &[
            Pair::new(Token::USDC, Token::WETH, 0, ExchangeId::Camelot),
            Pair::new(Token::USDC, Token::ARB, 0, ExchangeId::Sushi),
            Pair::new(Token::WETH, Token::ARB, 500, ExchangeId::Uniswap),
        ];

        let paths = PriceGraph::find_paths(Token::USDC, pairs);
        assert_eq!(
            paths,
            vec![
                Path::reflexive([
                    (Token::USDC as usize, Token::WETH as usize),
                    (Token::WETH as usize, Token::USDC as usize)
                ]),
                Path::triangular([
                    (Token::USDC as usize, Token::WETH as usize),
                    (Token::WETH as usize, Token::ARB as usize),
                    (Token::ARB as usize, Token::USDC as usize)
                ]),
                Path::reflexive([
                    (Token::USDC as usize, Token::ARB as usize),
                    (Token::ARB as usize, Token::USDC as usize)
                ]),
                Path::triangular([
                    (Token::USDC as usize, Token::ARB as usize),
                    (Token::ARB as usize, Token::WETH as usize),
                    (Token::WETH as usize, Token::USDC as usize)
                ]),
            ]
        );
    }

    #[test]
    pub fn find_paths_no_triangle() {
        let pairs = &[
            Pair::new(Token::USDC, Token::WETH, 100, ExchangeId::Uniswap),
            Pair::new(Token::USDC, Token::WETH, 0, ExchangeId::Chronos),
            Pair::new(Token::WBTC, Token::WETH, 0, ExchangeId::Sushi),
        ];

        let paths = PriceGraph::find_paths(Token::USDC, pairs);
        assert_eq!(
            paths,
            vec![Path::reflexive([
                (Token::USDC as usize, Token::WETH as usize),
                (Token::WETH as usize, Token::USDC as usize)
            ]),]
        );
    }

    #[test]
    pub fn add_edges() {
        let mut graph: PriceGraph = PriceGraph::empty();

        // 3,000 usdc / 2 weth
        let p = (eth(2) - 15_000_000_u128) / 2999_999988_u128;
        let edge0 = Edge::new_v3(p.into(), 1_000_000.into(), 500, true);
        graph.add_edge(Token::USDC, Token::WETH, edge0);

        let edge1 = Edge::UniV2 {
            reserve_in: (eth(2) - 1_000_000_u128),
            reserve_out: 2999_000000_u128,
            fee: 9997_u16,
            exchange_id: ExchangeId::Sushi,
        };
        graph.add_edge(Token::USDC, Token::WETH, edge1);

        // 2.4 usdc / 2 ARB
        let edge2 = Edge::UniV2 {
            reserve_in: (eth(2) - 1_000_000_000_u128),
            reserve_out: 2_400000_u128,
            fee: 9997_u16,
            exchange_id: ExchangeId::Chronos,
        };
        graph.add_edge(Token::USDC, Token::ARB, edge2);

        let p = (eth(2) - 1_110_000_000_u128) / 2_410000_u128;
        let edge3 = Edge::new_v3(p.into(), 1_000_000.into(), 3000, true);
        graph.add_edge(Token::USDC, Token::ARB, edge3);

        let edge4 = Edge::UniV2 {
            reserve_in: (5_011 + 100_u128),
            reserve_out: 40_000_u128,
            fee: 9997_u16,
            exchange_id: ExchangeId::Camelot,
        };
        graph.add_edge(Token::ARB, Token::WETH, edge4);

        // could pretty this up with some to/from string type e.g.
        // "[][x][][][x][]"
        // "[][][x][][][]"
        // "[][][][][x][]"
        assert_eq!(
            graph.hyper_loop,
            [
                [None, Some(edge1), None, Some(edge2), None, None, None,],
                [
                    Some(edge0.inverse()),
                    None,
                    None,
                    Some(edge4.inverse()),
                    None,
                    None,
                    None,
                ],
                [None, None, None, None, None, None, None],
                [
                    Some(edge3.inverse()),
                    Some(edge4),
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                [None, None, None, None, None, None, None],
                [None, None, None, None, None, None, None],
                [None, None, None, None, None, None, None],
            ]
        );
    }

    #[test]
    pub fn find_arb_works() {
        let pairs = &[
            Pair::new(Token::USDC, Token::WETH, 500, ExchangeId::Uniswap),
            Pair::new(Token::USDC, Token::ARB, 0, ExchangeId::Chronos),
            Pair::new(Token::WETH, Token::ARB, 0, ExchangeId::Sushi),
        ];

        let edges = vec![
            // 3,000 usdc / 2 weth
            Edge::UniV3 {
                sqrt_p_x96: ((((eth(2) / 3000_000000_u128) as f64).sqrt() * 2_f64.powf(96_f64))
                    as u128)
                    .into(),
                liquidity: 1000_0000.into(),
                fee: 500_u16,
                zero_for_one: true,
            },
            // 2.4 usdc / 2 ARB
            Edge::UniV2 {
                reserve_in: (eth(2) - 1_000_000_000_u128),
                reserve_out: 2_400000_u128,
                fee: 9997_u16,
                exchange_id: ExchangeId::Chronos,
            },
            Edge::UniV2 {
                reserve_in: 5_011_u128 + 100_u128,
                reserve_out: 40_000_u128,
                fee: 9997_u16,
                exchange_id: ExchangeId::Camelot,
            },
        ];

        let mut graph = PriceGraph::empty();
        for (pair, edge) in pairs.iter().zip(edges.iter()) {
            let (a, b) = pair.tokens();
            graph.add_edge(a, b, *edge);
        }

        let search_paths = PriceGraph::find_paths(Token::USDC, pairs);
        let (_value, found) = graph
            .find_arb(
                &Position {
                    amount: 1_000000_u128,
                    token: Token::USDC,
                },
                search_paths.as_slice(),
            )
            .unwrap();

        assert_eq!(
            found.path,
            [
                Trade {
                    token_in: 0,
                    token_out: 3,
                    fee_tier: 9997,
                    exchange_id: 3
                },
                Trade {
                    token_in: 3,
                    token_out: 1,
                    fee_tier: 9997,
                    exchange_id: 1
                },
                Trade {
                    token_in: 1,
                    token_out: 0,
                    fee_tier: 500,
                    exchange_id: 0
                }
            ]
        );
    }

    #[test]
    fn score_array() {
        let mut scores = ScoreArray::<5>::default();
        scores.insert(1, 3_f64);
        scores.insert(2, 5_f64);
        scores.insert(3, 9_f64);
        scores.insert(4, 2_f64);
        scores.insert(5, 0_f64);
        scores.insert(6, 1_f64);
        scores.insert(7, 2_f64);

        assert_eq!(
            scores,
            ScoreArray::new([(9_f64, 3_u32), (5.0, 2), (3.0, 1), (2.0, 7), (2.0, 4)])
        );

        assert_eq!(scores.best(), (9.0_f64, 3_u32));
        assert_eq!(scores.runner_up(), (5.0_f64, 2_u32));
    }

    #[test]
    fn score_array_demote() {
        let mut scores = ScoreArray::<5>::default();
        scores.insert(1, 1_f64);
        scores.insert(2, 2_f64);
        scores.insert(3, 3_f64);
        scores.insert(4, 4_f64);
        scores.insert(5, 5_f64);

        scores.demote(0.0);

        assert_eq!(scores.best(), (4.0_f64, 4_u32));
        assert_eq!(scores.runner_up(), (3.0_f64, 3_u32));
        assert_eq!(
            scores,
            ScoreArray::new([(4_f64, 4_u32), (3.0, 3), (2.0, 2), (1.0, 1), (0.0, 5)])
        );

        scores.demote(2.0);
        assert_eq!(
            scores,
            ScoreArray::new([(3.0, 3), (2.0, 4), (2.0, 2), (1.0, 1), (0.0, 5)])
        );
    }

    #[test]
    fn score_array_promote() {
        let mut scores = ScoreArray::<5>::default();
        scores.insert(1, 1_f64);
        scores.insert(2, 2_f64);
        scores.insert(3, 3_f64);
        scores.insert(4, 4_f64);
        scores.insert(5, 5_f64);

        // promote existing candidate
        scores.promote(3, 6.0);
        assert_eq!(
            scores,
            ScoreArray::new([(6.0, 3), (5.0, 5), (4.0, 4), (2.0, 2), (1.0, 1)])
        );

        // promote non-existent candidate
        scores.promote(7, 7.0);
        assert_eq!(
            scores,
            ScoreArray::new([(7.0, 7), (6.0, 3), (5.0, 5), (4.0, 4), (2.0, 2)])
        );

        // promote last candidate
        scores.promote(2, 8.0);
        assert_eq!(
            scores,
            ScoreArray::new([(8.0, 2), (7.0, 7), (6.0, 3), (5.0, 5), (4.0, 4)])
        );
    }

    #[test]
    fn failed_arb() {
        // https://arbiscan.io/tx/0x2ab37dff17c2cb9a59126db424f3538c4889a428b124e24e4fd889e5628a5cdb
        let edge0 = Edge::new_v3(
            3114389877176987074020846470923_u128.into(),
            1723927183040205737131270_u128.into(),
            500,
            true,
        );
        let edge1 = Edge::new_v3(
            87870821403100236353039_u128.into(),
            27844909457789979040_u128.into(),
            500,
            true,
        );
        let edge2 = Edge::new_v3(
            3452096058233460125537444_u128.into(),
            116041370918690901_u128.into(),
            100,
            false,
        );

        let mut amount_in = 10_u128.pow(18);
        println!("{amount_in}");
        amount_in = edge0.calculate_amount_out(amount_in);
        println!("{amount_in}");
        amount_in = edge1.calculate_amount_out(amount_in);
        println!("{amount_in}");
        let amount_out = edge2.calculate_amount_out(amount_in);
        println!("{amount_out}");

        assert_eq!(amount_out, 999469051194078031_u128);
    }
}
