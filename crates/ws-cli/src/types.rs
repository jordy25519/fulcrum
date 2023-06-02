use core::fmt;
use std::sync::Arc;

use compact_str::CompactString;
use ethers_core::types::{Bytes, H256};
use ethers_providers::JsonRpcError;
use serde::{
    de::{self},
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_json::value::RawValue;

// Normal JSON-RPC response
pub type Response = Result<Box<RawValue>, JsonRpcError>;

fn is_zst<T>(_t: &T) -> bool {
    std::mem::size_of::<T>() == 0
}

#[derive(Deserialize, Serialize)]
pub struct SendRawTxResponse<'a> {
    id: u64,
    jsonrpc: &'a str,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_tx_hash"
    )]
    pub result: H256,
}

#[derive(Serialize, Deserialize, Debug)]
/// A JSON-RPC request
pub struct Request<'a, T> {
    id: u64,
    jsonrpc: &'a str,
    method: &'a str,
    #[serde(skip_serializing_if = "is_zst")]
    params: T,
}

impl<'a, T> Request<'a, T> {
    /// Creates a new JSON RPC request
    pub fn new(id: u64, method: &'a str, params: T) -> Self {
        Self {
            id,
            jsonrpc: "2.0",
            method,
            params,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PubSubItem {
    Success { id: u64, result: Box<RawValue> },
    Error { id: u64, error: JsonRpcError },
}

// FIXME: ideally, this could be auto-derived as an untagged enum, but due to
// https://github.com/serde-rs/serde/issues/1183 this currently fails
struct ResponseVisitor;
impl<'de> de::Visitor<'de> for ResponseVisitor {
    type Value = PubSubItem;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid jsonrpc 2.0 response object")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        // response & error
        let mut id = 0_u64;
        // only response
        let mut result = None;
        // only error
        let mut error = None;

        while let Some(key) = map.next_key()? {
            match key {
                "id" => id = map.next_value()?,
                "result" => {
                    // TODO: alloc from object pool
                    let value: Box<RawValue> = map.next_value()?;
                    result = Some(value);
                }
                "error" => {
                    let value: JsonRpcError = map.next_value()?;
                    error = Some(value);
                }
                _ => {
                    let _ = de::MapAccess::next_value::<de::IgnoredAny>(&mut map);
                }
            }
        }

        if let Some(result) = result {
            Ok(PubSubItem::Success { id, result })
        } else {
            Ok(PubSubItem::Error {
                id,
                error: error.unwrap_or_else(|| JsonRpcError {
                    code: 0,
                    message: "missing error".to_string(),
                    data: None,
                }),
            })
        }
    }
}
impl<'de> Deserialize<'de> for PubSubItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ResponseVisitor)
    }
}

impl std::fmt::Display for PubSubItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PubSubItem::Success { id, .. } => write!(f, "Req success. ID: {id}"),
            PubSubItem::Error { id, .. } => write!(f, "Req error. ID: {id}"),
        }
    }
}

/// A JSON-RPC request for the `WsServer`.
#[derive(Debug)]
pub struct PreserializedCallRequest {
    pub method: CompactString,
    pub params: Arc<Box<RawValue>>,
    pub sender: tokio::sync::oneshot::Sender<Response>,
}

impl PreserializedCallRequest {
    /// Get the Ethereum JSON-RPC method name of the request
    pub fn method(&self) -> &str {
        self.method.as_str()
    }
}

/// Wrapper type around Bytes to deserialize/serialize "0x" prefixed ethereum hex strings
#[derive(Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FastBytes(
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    pub bytes::Bytes,
);

impl From<bytes::Bytes> for FastBytes {
    fn from(src: bytes::Bytes) -> Self {
        Self(src)
    }
}

impl FromIterator<u8> for FastBytes {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        iter.into_iter().collect::<bytes::Bytes>().into()
    }
}

impl<'a> FromIterator<&'a u8> for FastBytes {
    fn from_iter<T: IntoIterator<Item = &'a u8>>(iter: T) -> Self {
        iter.into_iter().copied().collect::<bytes::Bytes>().into()
    }
}

impl Into<Bytes> for FastBytes {
    fn into(self) -> Bytes {
        Bytes::from_iter(self.as_ref().iter())
    }
}

/// Serialize the given `buf`fer as hex
pub fn serialize_hex<T: AsRef<[u8]>>(buf: T) -> String {
    faster_hex::hex_string(buf.as_ref())
}

pub fn serialize_bytes<S, T>(x: T, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: AsRef<[u8]>,
{
    s.serialize_str(&format!("0x{}", faster_hex::hex_string(x.as_ref())))
}

pub fn deserialize_bytes<'de, D>(d: D) -> Result<bytes::Bytes, D::Error>
where
    D: Deserializer<'de>,
{
    let value: &str = Deserialize::deserialize(d)?;
    let mut buf = vec![0_u8; (value.len() - 2) / 2]; // hex nibbles - the '0x' prefix
    faster_hex::hex_decode(value[2..].as_bytes(), buf.as_mut_slice())
        .map_err(|err| de::Error::custom(err.to_string()))?;

    Ok(buf.into())
}

pub fn deserialize_tx_hash<'de, D>(d: D) -> Result<H256, D::Error>
where
    D: Deserializer<'de>,
{
    let mut decoded = [0_u8; 32];
    let buf: &[u8] = Deserialize::deserialize(d)?;
    // 2.. = skip 0x prefix
    if let Err(_err) = faster_hex::hex_decode(&buf[2..], decoded.as_mut_slice()) {
        return Err(de::Error::custom("invalid hex"));
    }

    Ok(decoded.into())
}

impl AsRef<[u8]> for FastBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_desers_pubsub_items() {
        let a = r#"{"jsonrpc":"2.0","id":1,"result":"0xcd0c3e8af590364c09d0fa6a1210faf5"}"#;
        serde_json::from_str::<PubSubItem>(a).unwrap();
    }
}
