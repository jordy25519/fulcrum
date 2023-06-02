//! A stripped down Ethereum JSON-RPC WS client based on ethers-providers
//! Allows some room for optimization of the networking and serialization steps
//! It is not fully featured e.g. does not provide subscriptions

#![allow(missing_docs)]
mod backend;
mod cli;
mod manager;
mod types;

use std::time::Duration;

use isahc::{
    config::{DnsCache, SslOption, VersionNegotiation},
    prelude::Configurable,
};
pub use isahc::{AsyncBody, HttpClient};

pub use cli::FastWsClient;
pub use types::*;

/// Create a pooled HTTP(S) client
pub fn make_http_client(keep_alive: Duration) -> HttpClient {
    HttpClient::builder()
        .default_headers(&[("Content-Type", "application/json")])
        .dns_cache(DnsCache::Forever)
        .ip_version(isahc::config::IpVersion::V4)
        .ssl_options(SslOption::DANGER_ACCEPT_INVALID_CERTS)
        .tcp_keepalive(keep_alive)
        .tcp_nodelay()
        .version_negotiation(VersionNegotiation::http2())
        .connection_cache_size(2)
        .connection_cache_ttl(keep_alive)
        .build()
        .expect("built client")
}

/// Response type for async HTTP requests
pub type Response = isahc::Response<AsyncBody>;

#[cfg(test)]
mod test {
    use isahc::{
        config::{SslOption, VersionNegotiation},
        prelude::Configurable,
        HttpClient,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn http_post_isahc() {
        use env_logger::TimestampPrecision;

        env_logger::builder()
            .format_timestamp(Some(TimestampPrecision::Micros))
            .init();

        let n_req = 10;
        let mut total = Duration::ZERO;

        let client = HttpClient::builder()
            .default_headers(&[("Content-Type", "application/json")])
            .dns_cache(Duration::from_secs(60))
            .ip_version(isahc::config::IpVersion::V4)
            .ssl_options(SslOption::DANGER_ACCEPT_INVALID_CERTS)
            .tcp_keepalive(Duration::from_secs(15))
            .tcp_nodelay()
            .version_negotiation(VersionNegotiation::http2())
            .connection_cache_size(1)
            .connection_cache_ttl(Duration::from_secs(15))
            .build()
            .expect("built client");

        // let _resp = client::connect("https://arb1-sequencer.arbitrum.io/rpc", r#"{"id":704211,"jsonrpc":"2.0","method":"eth_getBalance","params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1","latest"]}"#.as_bytes()).expect("post ok");

        // 46ms? hitting cloud flare auto response
        // w/out keep-alive: ~690ms
        // w keep-alive: ~392ms, 512ms
        // 296ms per request..
        // x-envoy-upstream-service-time Contains the time in milliseconds spent by the upstream host processing the request.
        for _ in 0..n_req {
            let t0 = Instant::now();
            let resp = client.post("https://arb1-sequencer.arbitrum.io/rpc", r#"{"id":704211,"jsonrpc":"2.0","method":"eth_getBalance","params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1","latest"]}"#.as_bytes()).expect("post ok");
            total += Instant::now().checked_duration_since(t0).unwrap();
            println!("{:?}", Instant::now() - t0);
            std::thread::sleep(Duration::from_millis(500));
        }
        println!("\n");
        for _ in 0..n_req {
            let t0 = Instant::now();
            let resp = client.post("https://arb1.arbitrum.io/rpc", r#"{"id":704211,"jsonrpc":"2.0","method":"eth_getBalance","params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1","latest"]}"#.as_bytes()).expect("post ok");
            total += Instant::now().checked_duration_since(t0).unwrap();
            println!("{:?}", Instant::now() - t0);
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}
