//! Sequencer feed types
#![allow(dead_code)]
use bumpalo::{collections, Bump};
use ethers::types::{Address, U256};
use log::{debug, info, warn};
use rlp::Rlp;
use serde::Deserialize;

/// Optimized buffer for deserialized transaction info
pub struct TxBuffer<'bump, 'a> {
    /// The transaction info
    txs: collections::Vec<'bump, TransactionInfo<'a>>,
    /// The associated block number of the stored txs
    block_number: u64,
}
impl<'bump, 'a> TxBuffer<'bump, 'a>
where
    'bump: 'a,
{
    pub fn new(bump: &'bump Bump) -> Self {
        // let bump = Bump::with_capacity((52 + 1024) * 1024); // 100kib buffer;
        Self {
            txs: collections::Vec::<'bump, TransactionInfo>::with_capacity_in(100, bump),
            block_number: 0,
        }
    }
    /// Add a tx to the buffer
    pub fn push(&mut self, v: TransactionInfo<'a>) {
        self.txs.push(v)
    }
    /// Set the associated block number of the stored txs
    pub fn set_block_number(&mut self, block_number: u64) {
        self.block_number = block_number;
    }
    /// Add a tx to the buffer
    pub fn as_slice(&self) -> &[TransactionInfo<'a>] {
        self.txs.as_slice()
    }
    /// Get the associated block number of the stored txs
    pub fn block_number(&self) -> u64 {
        self.block_number
    }
}

#[derive(Debug, PartialEq)]
pub enum FeedError {
    /// Invalid base64 during decoding
    InvalidBase64,
    /// Invalid rlp during decoding
    InvalidRlp,
    /// Invalid JSON during decoding
    InvalidJson,
    /// Connection closed
    Closed,
    /// Some internal ws error
    Internal,
}

// Arbitrum sequencer feed types
#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastMessage<'a> {
    // #[serde(skip)]
    // pub version: u64,
    #[serde(borrow = "'a")]
    pub messages: Option<[BroadcastFeedMessage<'a>; 1]>,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastFeedMessage<'a> {
    pub sequence_number: u64,
    #[serde(borrow = "'a")]
    pub message: MessageWithMetadata<'a>,
    // #[serde(skip)]
    // pub signature: Vec<u8>,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageWithMetadata<'a> {
    #[serde(borrow = "'a")]
    pub message: L1IncomingMessageHeader<'a>,
    // #[serde(skip)]
    // pub delayed_messages_read: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct L1IncomingMessageHeader<'a> {
    pub header: Header,
    #[serde(rename = "l2Msg", borrow = "'a")]
    pub l2msg: &'a [u8],
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub kind: u8,
    // #[serde(skip)]
    // pub sender: String,
    // #[serde(skip)]
    // pub block_number: u64,
    // #[serde(skip)]
    // pub timestamp: u64,
    // #[serde(skip)]
    // pub request_id: [u8; 32],
    // #[serde(skip)]
    // pub base_fee_l1: U256,
}

pub(crate) enum L1MsgType {
    L2Message = 3,
    EndOfBlock = 6,
    L2FundedByL1 = 7,
    RollupEvent = 8,
    SubmitRetryable = 9,
    BatchForGasEstimation = 10, // probably won't use this in practice
    Initialize = 11,
    EthDeposit = 12,
    BatchPostingReport = 13,
    Invalid = 0xFF,
}

#[derive(Debug)]
pub(crate) enum L2MsgKind {
    UnsignedUserTx = 0,
    ContractTx = 1,
    NonMutatingCall = 2,
    Batch = 3,
    SignedTx = 4,
    Reserved5 = 5, // 5 is reserved
    Heartbeat = 6, // deprecated
    SignedCompressedTx = 7,
    Reserved8 = 8, // 8 is reserved for BLS signed batch,
    Unknown = 255,
}
impl L2MsgKind {
    fn quick_from(val: u8) -> Self {
        match val {
            0 => Self::UnsignedUserTx,
            1 => Self::ContractTx,
            2 => Self::NonMutatingCall,
            3 => Self::Batch,
            4 => Self::SignedTx,
            5 => Self::Reserved5,
            6 => Self::Heartbeat,
            7 => Self::SignedCompressedTx,
            8 => Self::Reserved8,
            _ => Self::Unknown,
        }
    }
}

/// Subset of transaction fields useful for the trading engine
#[derive(Debug, PartialEq)]
pub struct TransactionInfo<'a> {
    pub to: Address,
    pub value: U256,
    pub input: &'a [u8],
}

// NB: we don't use proper error/option in this functions because a the input should always be well formed or Arbitrum goes down
// and 2 for performance.
/// Decode a `Transaction` from the sequencer feed
pub(crate) fn decode_arbitrum_tx<'bump: 'a, 'a>(
    buf: &'a [u8],
    tx_buffer: &mut TxBuffer<'bump, 'a>,
) {
    let kind = L2MsgKind::quick_from(unsafe { *buf.get_unchecked(0) });
    // debug!("outer kind: {:?}", kind);
    match kind {
        L2MsgKind::Batch => decode_batch(&buf[1..], tx_buffer),
        L2MsgKind::SignedTx => {
            if let Some(tx_info) = decode_tx_info_legacy(&buf[1..]) {
                tx_buffer.push(tx_info);
            }
        }
        L2MsgKind::Unknown => {
            debug!("unknown l2 msg kind");
        }
        _ => {
            debug!("unhandled l2 msg");
        }
    }
}

/// Decode a batch of RLP encoded transactions from `buf` into `tx_buffer`
pub(crate) fn decode_batch<'bump: 'a, 'a>(buf: &'a [u8], tx_buffer: &mut TxBuffer<'bump, 'a>) {
    let mut offset: usize = 0;
    // The batch size depends on tx size but we don't know how that translates to tx count exactly
    // MaxL2MessageSize = 256 * 1024
    let len = buf.len();
    for _ in 0..128 {
        let msg_length = as_usize(&buf[offset..]);
        offset += 8_usize;
        // let kind: L2MsgKind = L2MsgKind::quick_from(buf[offset]);
        // debug!("inner kind: {:?}", kind);
        if let Some(tx_info) = decode_tx_info_legacy(&buf[offset + 1..]) {
            tx_buffer.push(tx_info);
        }

        offset += msg_length;
        if offset + 9 > len {
            break;
        }
    }
}

/// Decode Ethereum Transaction data from RLP `buf`
/// Matches behaviour of the nitro node
fn decode_tx_info(buf: &[u8]) -> Option<TransactionInfo> {
    // list == legacy tx type
    if buf[0] > 0x7f {
        return decode_base_legacy(buf);
    }
    // if it is not enveloped then we need to use rlp.as_raw instead of rlp.data
    let data = Rlp::new(buf).data().unwrap();
    let first_byte = data[0];
    let rest = &data[1..];

    match first_byte {
        2 => decode_base_eip1559(rest),
        1 => decode_base_eip2930(rest),
        _ => {
            warn!("unhandled tx: {:02x?}", buf);
            None
        }
    }
}

/// Decode Ethereum Transaction data from RLP `buf`
/// matches the behaviour of ethers-rs
pub fn decode_tx_info_legacy(buf: &[u8]) -> Option<TransactionInfo> {
    // list == legacy tx type
    if buf[0] >= 0xc0 {
        return decode_base_legacy(buf);
    }
    // if it is not enveloped then we need to use rlp.as_raw instead of rlp.data
    let buf = Rlp::new(buf);
    let mut data: &[u8] = buf.as_raw();
    let mut first_byte = data[0];
    // tx may have longer bytes
    if first_byte > 0x7f {
        match buf.data() {
            Ok(inner) => data = inner,
            Err(_err) => {
                info!("{:02x?}", data);
                panic!();
            }
        }
        first_byte = data[0];
    }
    match first_byte {
        0x02 => {
            let rest = &data[1..];
            decode_base_eip1559(rest)
        }
        0x01 => {
            let rest = &data[1..];
            decode_base_eip2930(rest)
        }
        _ => {
            info!("{:02x?}", buf);
            unimplemented!();
        }
    }
}

#[inline(always)]
fn as_usize(buf: &[u8]) -> usize {
    // OPTIMIZATION: nothing sensible should ever be longer than 2 ** 16 so we ignore the other bytes
    // ((unsafe { *buf.get_unchecked(28) } as usize) << 24)
    //     + ((unsafe { *buf.get_unchecked(29) } as usize) << 16)
    ((unsafe { *buf.get_unchecked(5) } as usize) << 16)
        + ((unsafe { *buf.get_unchecked(6) } as usize) << 8)
        + unsafe { *buf.get_unchecked(7) } as usize
}

/// Decodes fields of the type 2 transaction response starting at the RLP offset passed.
/// Increments the offset for each element parsed.
#[inline]
fn decode_base_eip1559(buf: &[u8]) -> Option<TransactionInfo> {
    // self.chain_id = Some(buf.val_at(*offset)?);
    //*offset += 1;
    // self.nonce = buf.val_at(*offset)?;
    //*offset += 1;
    // self.max_priority_fee_per_gas = Some(buf.val_at(*offset)?);
    //*offset += 1;
    // self.max_fee_per_gas = Some(buf.val_at(*offset)?);
    //*offset += 1;
    // self.gas = buf.val_at(*offset)?;
    //*offset += 1;
    let buf = Rlp::new(buf);
    let mut offset = 5;
    let to = if let Ok(to) = buf.val_at(offset) {
        to
    } else {
        return None;
    };
    offset += 1;
    let value = buf.val_at(offset).unwrap();
    offset += 1;
    let input = Rlp::new(buf.at(offset).unwrap().as_raw())
        .data()
        .expect("data");
    // self.access_list = Some(buf.val_at(*offset)?);
    //*offset += 1;

    Some(TransactionInfo { to, value, input })
}

/// Decodes fields of the type 1 transaction response based on the RLP offset passed.
/// Increments the offset for each element parsed.
fn decode_base_eip2930(buf: &[u8]) -> Option<TransactionInfo> {
    // self.chain_id = Some(buf.val_at(*offset)?);
    // *offset += 1;
    // // self.nonce = buf.val_at(*offset)?;
    // *offset += 1;
    // // self.gas_price = Some(buf.val_at(*offset)?);
    // *offset += 1;
    // // self.gas = buf.val_at(*offset)?;
    // *offset += 1;
    let buf = Rlp::new(buf);
    let mut offset = 4;
    let to = if let Ok(to) = buf.val_at(offset) {
        to
    } else {
        return None;
    };
    offset += 1;
    let value = buf.val_at(offset).unwrap();
    offset += 1;
    let input = buf.at(offset).unwrap().as_raw();
    // self.access_list = Some(buf.val_at(*offset)?);
    // *offset += 1;

    Some(TransactionInfo { to, value, input })
}

/// Decodes a legacy transaction starting at the RLP offset passed.
/// Increments the offset for each element parsed.
#[inline]
fn decode_base_legacy(buf: &[u8]) -> Option<TransactionInfo> {
    // self.nonce = buf.val_at(*offset)?;
    //*offset += 1;
    // self.gas_price = Some(buf.val_at(*offset)?);
    //*offset += 1;
    // self.gas = buf.val_at(*offset)?;
    //*offset += 1;
    let buf = Rlp::new(buf);
    let mut offset = 3;
    let to = if let Ok(to) = buf.val_at(offset) {
        to
    } else {
        return None;
    };
    offset += 1;
    let value = buf.val_at(offset).unwrap();
    offset += 1;
    let input = Rlp::new(buf.at(offset).unwrap().as_raw())
        .data()
        .expect("data");

    Some(TransactionInfo { to, value, input })
}
