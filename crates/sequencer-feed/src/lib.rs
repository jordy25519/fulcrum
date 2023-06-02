//! low latency Arbitrum sequencer feed decoder
#![cfg_attr(feature = "bench", feature(test))]
#![allow(dead_code)]
use std::time::Instant;

use http::Uri;
use log::{debug, error};
use ws_tool::{
    codec::{AsyncFrameCodec, PMDConfig},
    connector::{async_tcp_connect, async_wrap_tls, get_host, TlsStream},
    frame::{Header, OpCode, OwnedFrame},
    ClientBuilder,
};

mod deser;
mod types;
use types::{decode_arbitrum_tx, FeedError};
pub use types::{TransactionInfo, TxBuffer};

/// Arbitrum one sequencer feed
const SEQUENCER_WSS: &str = "wss://arb1.arbitrum.io/feed";
/// Arbitrum One nitro genesis block number
/// https://github.com/OffchainLabs/arbitrum-subgraphs/blob/fa8e55b7aec8609b6c8a6cad704d44a0b2fde3b9/packages/subgraph-common/config/nitro-mainnet.json#L14
const NITRO_GENESIS_BLOCK_NUMBER: u64 = 22_207_817_u64;

/// Sequencer feed
///
/// The caller should drive the feed by `await`ing on `next_message` and then
/// passing the result to `handle_frame`
/// This allows deserialization of feed messages as zero copy
pub struct SequencerFeed {
    pub client: AsyncFrameCodec<TlsStream>,
}

impl SequencerFeed {
    pub async fn arbitrum_one() -> Self {
        // Arbitrum one sequencer feed
        let uri = SEQUENCER_WSS.parse().unwrap();
        let mut feed = Self {
            client: sequencer_feed_with_uri(&uri).await,
        };
        // the first message is a huuge un-parasable JSON dump, drop it
        feed.first_message().await;

        feed
    }
    /// await first message and drop it
    pub async fn first_message(&mut self) {
        let _ = self.next_message().await;
    }
    /// Await the next message from the feed
    pub async fn next_message(&mut self) -> Result<OwnedFrame, FeedError> {
        match self.client.receive().await {
            Ok(frame) => Ok(frame),
            Err(err) => {
                error!("feed ws frame: {:?}", err);
                Err(FeedError::Internal)
            }
        }
    }
    /// Handle next ws frame from the sequencer feed
    pub async fn handle_frame<'bump: 'a, 'a>(
        &mut self,
        header: &Header,
        payload: &'a mut [u8],
        tx_buffer: &mut TxBuffer<'bump, 'a>,
    ) -> Result<(), FeedError> {
        match header.opcode() {
            OpCode::Text => {
                let t0: Instant = Instant::now();
                if let Ok(block_number) = decode_feed_message(payload, tx_buffer) {
                    tx_buffer.set_block_number(block_number);
                    debug!(
                        "process feed tx: {:?} for ⛓{block_number}",
                        Instant::now() - t0
                    );
                }
            }
            OpCode::Ping => {
                self.client
                    .send(OpCode::Pong, payload)
                    .await
                    .expect("pong ok");
                self.client.flush().await.expect("flush ok");
            }
            OpCode::Pong => return Ok(()),
            OpCode::Binary => {
                debug!("unhandled binary frame: {:?}", header.opcode());
                debug!("{:02x?}", payload);
                return Ok(());
            }
            OpCode::Close => return Err(FeedError::Closed),
            OpCode::Continue => panic!("unhandled continuation frame"),
            _ => {
                debug!("unhandled frame: {:?}", header.opcode());
                return Err(FeedError::Internal);
            }
        }

        Ok(())
    }
}

/// Arbitrum sequencer feed from the given `uri`
async fn sequencer_feed_with_uri(uri: &Uri) -> AsyncFrameCodec<TlsStream> {
    let stream = async_tcp_connect(uri).await.expect("tcp connect ok");
    let stream = async_wrap_tls(stream, get_host(uri).unwrap(), vec![])
        .await
        .expect("TLS support");

    // TODO: modify this to allow setting frame config
    let client = ClientBuilder::new()
        .extension(PMDConfig::default().ext_string())
        .async_with_stream(uri.clone(), stream, AsyncFrameCodec::check_fn)
        .await
        .expect("start client");

    client
}

/// Decode a sequencer feed message
///
/// - `payload` of base64 encoded json bytes, the buffer will be used to decode in place
/// - `tx_buffer` storage buffer to fill with decoded transaction info
///
/// Returns the block number of the message, `0` indicates no txs
#[inline(always)]
fn decode_feed_message<'bump: 'a, 'a>(
    payload: &'a mut [u8],
    tx_buffer: &mut TxBuffer<'bump, 'a>,
) -> Result<u64, FeedError> {
    let (sequence_number, l2_msg) = deser::feed_json_from_input(payload);
    if let Some(l2_msg) = l2_msg {
        match base64_simd::forgiving_decode_inplace(l2_msg) {
            Ok(l2_msg) => {
                decode_arbitrum_tx(l2_msg, tx_buffer);
            }
            Err(_) => return Err(FeedError::InvalidBase64),
        }
    }

    if sequence_number == 0 {
        Ok(0)
    } else {
        Ok(sequence_number + NITRO_GENESIS_BLOCK_NUMBER - 1)
    }
}

#[cfg(test)]
mod test {
    use bumpalo::Bump;
    use ethers::types::{Address, U256};
    use hex_literal::hex;
    use std::str::FromStr;

    use crate::{
        decode_feed_message, deser,
        types::{decode_tx_info_legacy, TxBuffer},
        TransactionInfo, NITRO_GENESIS_BLOCK_NUMBER,
    };

    #[test]
    fn decode_sequencer_batch() {
        // the allocation is decoded inplace, hence the `mut`
        let mut batch_json = include_bytes!("../res/batch.json").to_owned();
        let bump = Bump::new();
        let mut tx_info = TxBuffer::new(&bump);

        assert!(decode_feed_message(batch_json.as_mut_slice(), &mut tx_info).is_ok());

        assert_eq!(
            tx_info.as_slice(),
            &[
                TransactionInfo {
                    to: Address::from_str("64fe52bccd0035daa698ab504631f98e0972c340").unwrap(),
                    value: U256::zero(),
                    input: &[
                        9, 94, 167, 179, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 180, 90, 45, 218, 153,
                        108, 50, 233, 59, 140, 71, 9, 142, 144, 237, 14, 122, 177, 142, 57, 255,
                        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                        255
                    ]
                },
                TransactionInfo {
                    to: Address::from_str("10acb149fac9867045ed6af86bb2e61f2602fa51").unwrap(),
                    value: U256::zero(),
                    input: &[
                        130, 126, 57, 118, 0, 0, 0, 0, 0, 15, 3, 0, 4, 3, 128, 81, 2, 208, 91, 4,
                        64, 91, 0, 0, 0, 0, 0, 0, 18, 38, 20, 3, 214, 9, 210, 114
                    ]
                },
                TransactionInfo {
                    to: Address::from_str("bf22f0f184bccbea268df387a49ff5238dd23e40").unwrap(),
                    value: U256::from(21_711_493_956_848_285_u128),
                    input: &[
                        17, 20, 205, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 7, 237, 127, 141, 220, 201, 8, 207, 251, 157, 162, 236, 244, 61, 240,
                        216, 249, 236, 138, 111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 71, 13, 228, 223, 130, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 70, 178, 241, 207, 7, 192, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 20, 7, 237, 127, 141,
                        220, 201, 8, 207, 251, 157, 162, 236, 244, 61, 240, 216, 249, 236, 138,
                        111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                    ]
                },
                TransactionInfo {
                    to: Address::from_str("7879e4523907bdaaf94416442d6a63a841181c91").unwrap(),
                    value: U256::zero(),
                    input: &[
                        84, 54, 62, 125, 32, 4, 42, 127, 132, 64, 5, 192, 11, 2, 0, 10, 15, 66, 64,
                        0, 1, 244, 6, 18, 8, 4, 11, 2, 0, 50, 15, 66, 64, 0, 9, 196, 6, 18
                    ]
                },
                TransactionInfo {
                    to: Address::from_str("e592427a0aece92de3edee1f18e0157c05861564").unwrap(),
                    value: U256::zero(),
                    input: &[
                        219, 62, 33, 152, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 145, 44, 229, 145,
                        68, 25, 28, 18, 4, 230, 69, 89, 254, 130, 83, 160, 228, 158, 101, 72, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 130, 175, 73, 68, 125, 138, 7, 227, 189, 149,
                        189, 13, 86, 243, 82, 65, 82, 63, 186, 177, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 244, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 77, 73, 202, 250, 51, 48, 118, 204, 88, 174,
                        231, 161, 40, 190, 69, 92, 4, 147, 12, 209, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 88, 80, 197, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 81,
                        90, 1, 241, 25, 87, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 1, 234, 52, 4, 241, 195, 194, 192, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0
                    ]
                },
                TransactionInfo {
                    to: Address::from_str("0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45").unwrap(),
                    value: U256::zero(),
                    input: &[
                        90, 228, 1, 220, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 88, 82, 165, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 80, 35, 180, 223,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 145, 44, 229, 145, 68, 25, 28, 18, 4,
                        230, 69, 89, 254, 130, 83, 160, 228, 158, 101, 72, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 253, 8, 107, 199, 205, 92, 72, 29, 204, 156, 133, 235, 228,
                        120, 161, 192, 182, 159, 203, 185, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 184, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 232, 21, 193, 154, 190, 244, 157, 26, 108, 238, 23,
                        154, 13, 3, 220, 217, 80, 68, 130, 105, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 102, 111, 149, 78, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 76, 156, 119,
                        172, 222, 182, 236, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                    ]
                },
                TransactionInfo {
                    to: Address::from_str("0x0000000001e4ef00d069e71d6ba041b0a16f7ea0").unwrap(),
                    value: U256::zero(),
                    input: &[
                        165, 249, 147, 27, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 141, 37, 179, 228,
                        21, 238, 21, 188, 64, 74, 123, 70, 221, 134, 111, 47, 134, 221, 191, 15, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 33, 41, 26, 24, 76, 243, 106, 211,
                        176, 160, 222, 244, 161, 124, 18, 203, 214, 106, 20, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 196,
                        161, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 15, 196, 161, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16, 62, 85, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15,
                        216, 234, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 90, 243, 16, 122, 64, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
                        32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 253, 8, 107, 199, 205, 92, 72, 29,
                        204, 156, 133, 235, 228, 120, 161, 192, 182, 159, 203, 185, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15,
                        66, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 253, 8, 107, 199, 205, 92, 72,
                        29, 204, 156, 133, 235, 228, 120, 161, 192, 182, 159, 203, 185, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0
                    ]
                },
            ]
        );
    }

    #[test]
    fn decode_sequencer_batch_big() {
        let mut feed_json = include_bytes!("../res/contract-create.json").to_owned();
        let bump = Bump::new();
        let mut tx_info = TxBuffer::new(&bump);

        assert!(decode_feed_message(feed_json.as_mut_slice(), &mut tx_info).is_ok());
        assert!(tx_info.as_slice().is_empty());
    }

    #[test]
    fn bespoke_decode_feed_msg() {
        let mut batch_json = include_bytes!("../res/small.json").to_owned();
        let (block_number, l2_msg) = deser::feed_json_from_input(batch_json.as_mut_slice());
        assert_eq!(l2_msg.unwrap(), b"myawsomemessageyaysocool");
        assert_eq!(block_number, 68938512 + NITRO_GENESIS_BLOCK_NUMBER - 1);
    }

    #[test]
    fn bespoke_decode_feed_msg_huuge() {
        let mut batch_json = include_bytes!("../res/huuge.json").to_owned();
        let _l2_msg = deser::feed_json_from_input(batch_json.as_mut_slice());
    }

    #[test]
    fn failing_tx() {
        let buf = hex!("047862412af18da4c549549630887dba1af6c0f20000000000000000000000000000000000000000000000004563918244f40000");
        let bump = Bump::new();
        let mut tx_info = TxBuffer::new(&bump);
        println!("{:?}", decode_tx_info_legacy(&buf));
        assert!(false);
    }

    #[test]
    fn failing_tx2() {
        let buf = include_bytes!("../res/test.base64");
        let l2msg = base64_simd::forgiving_decode_to_vec(buf).unwrap();
        println!("{:?}", l2msg);
        let bump = Bump::new();
        let mut tx_info = TxBuffer::new(&bump);
        println!("{:?}", decode_tx_info_legacy(&l2msg.as_slice()));
    }
}

#[cfg(feature = "bench")]
mod bench {
    extern crate test;
    use std::hint::black_box;
    use test::Bencher;

    use bumpalo::Bump;

    use crate::{decode_feed_message, TxBuffer};

    #[bench]
    fn decode_sequencer_feed_huuge(b: &mut Bencher) {
        let feed_json = include_bytes!("../res/huuge.json").to_owned();
        let bump = Bump::new();

        b.iter(|| {
            for _ in 0..100 {
                black_box({
                    let mut feed_json = feed_json.clone();
                    let mut tx_info = TxBuffer::new(&bump);
                    let _ = decode_feed_message(feed_json.as_mut_slice(), &mut tx_info);
                })
            }
        });
    }
}
