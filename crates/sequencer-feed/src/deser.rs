use log::info;

/// Deserialize a sequencer feed JSON message into its base64 encoded 'L2' message
///
/// serde is reasonably efficient but degrades as it must scan the lengthy base64 'l2msg' >10kb
/// we can do better by searching from the msg tail for the end of the l2msg
pub fn feed_json_from_input(buf: &mut [u8]) -> (u64, Option<&mut [u8]>) {
    // {"version":1,"confirmedSequenceNumberMessage":{"sequenceNumber":69287376}}
    let mut index = 42_usize;
    // let version_key = &buf[1..10];
    // print_bytes(version_key);
    // let sequencer_number_0_key = &buf[26..42];
    // print_bytes(sequencer_number_0_key);

    /*
    arbitrum one nitro genesis block: 22207817
    // https://github.com/OffchainLabs/arbitrum-subgraphs/blob/fa8e55b7aec8609b6c8a6cad704d44a0b2fde3b9/packages/subgraph-common/config/nitro-mainnet.json#L14
    func MessageCountToBlockNumber(messageCount MessageIndex, genesisBlockNumber uint64) int64 {
        return int64(uint64(messageCount)+genesisBlockNumber) - 1
    }
     */
    if buf.len() <= 75 {
        // {"version":1,"confirmedSequenceNumberMessage":{"sequenceNumber":72346029}}
        // print_bytes(&buf);
        return (0, None);
    }
    index += 6;
    while buf[index] as char != ',' {
        index += 1;
    }
    let sequence_number =
        str::parse::<u64>(unsafe { core::str::from_utf8_unchecked(&buf[43..index]) })
            .expect("sequencer number");
    if buf.len() < 80 {
        return (sequence_number, None);
    }

    // index = 42;
    // length of the sequencer number can grow so we must search
    while buf[index] as char != '"' {
        index += 1;
    }
    /*
    let message_inner_key = &buf[index..index + 9];
    print_bytes(message_inner_key);
    index += 11;
    let message_inner_key = &buf[index..index + 9];
    print_bytes(message_inner_key);
    index += 11;
    let header_key = &buf[index..index + 8];
    print_bytes(header_key);
    index += 10;
    let kind_key = &buf[index..index + 6];
    print_bytes(kind_key);
    index+=7;
    */
    index += 39;
    let _kind_value = buf[index] - 0x30; // convert ascii digit to u8
                                         // println!("kind:{kind_value}");
                                         // skip this: `,"sender":"0xa4b000000000000000000073657175656e636572","blockNumber":`
                                         /*
                                         let block_number_start = index + 70;
                                         index += 70 + 7; // +7 hint since block # is atleast this length
                                         while buf[index] as char != ',' {
                                             index += 1;
                                         }
                                         print_bytes(&buf[block_number_start..index]);
                                         if let Ok(block_number) = str::parse::<u64>(unsafe {
                                             core::str::from_utf8_unchecked(&buf[block_number_start..index])
                                         }) {
                                             println!("block: {:?}", block_number);
                                         }
                                         */

    // skip to end of 'header' object
    // some of the fields are variable length so search to be safe
    while buf[index] as char != '}' {
        index += 1;
    }
    // index += 2;
    // let l2msg_key = &buf[index..index + 7];
    // print_bytes(l2msg_key);
    // index += 9; //+ :"
    index += 11;

    // for extremely long l2msgs its more efficient to
    // search from the end of the payload in reverse
    let mut tail_index = buf.len() - 1;
    let mut count = 4;
    while count > 0 {
        if buf[tail_index] as char == '}' {
            count -= 1;
        }
        tail_index -= 1;
    }
    let l2msg_value = buf[index..tail_index].as_mut();
    // print_bytes(l2msg_value);

    (sequence_number, Some(l2msg_value))
}

pub fn print_bytes(b: &[u8]) {
    info!("{}", unsafe { core::str::from_utf8_unchecked(b) });
}
