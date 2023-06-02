use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use ethers_providers::{ConnectionDetails, WsClientError};
use log::{debug, error, trace};
use serde_json::value::to_raw_value;
use tokio::select;

use crate::{
    backend::{BackendDriver, WsBackend},
    cli::FastWsClient as WsClient,
    types::{PreserializedCallRequest, PubSubItem, Request},
};

pub const DEFAULT_RECONNECTS: usize = 5;

/// The `RequestManager` holds copies of all pending requests (as `InFlight`),
/// and active subscriptions (as `ActiveSub`). When reconnection occurs, all
/// pending requests are re-dispatched to the new backend, and all active subs
/// are re-subscribed
///
///  `RequestManager` holds a `BackendDriver`, to communicate with the current
/// backend. Reconnection is accomplished by instantiating a new `WsBackend` and
/// swapping out the manager's `BackendDriver`.
///
/// In order to provide continuity of subscription IDs to the client, the
/// `RequestManager` also keeps a `SubscriptionManager`. See the
/// `SubscriptionManager` docstring for more complete details
///
/// The behavior is accessed by the WsClient frontend, which implements ]
/// `JsonRpcClient`. The `WsClient` is cloneable, so no need for an arc :). It
/// communicates to the request manager via a channel, and receives
/// notifications in a shared map for the client to retrieve
///
/// The `RequestManager` shuts down and drops when all `WsClient` instances have
/// been dropped (because all instruction channel `UnboundedSender` instances
/// will have dropped).
pub struct RequestManager {
    // Next JSON-RPC Request ID
    id: AtomicU64,
    // How many times we should reconnect the backend before erroring
    reconnects: usize,
    // Requests for which a response has not been received
    reqs: BTreeMap<u64, PreserializedCallRequest>,
    // Control of the active WS backend
    backend: BackendDriver,
    // The URL and optional auth info for the connection
    conn: ConnectionDetails,
    // requests from the user-facing providers
    requests: tokio::sync::mpsc::UnboundedReceiver<PreserializedCallRequest>,
}

impl RequestManager {
    fn next_id(&mut self) -> u64 {
        self.id.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn connect(conn: ConnectionDetails) -> Result<(Self, WsClient), WsClientError> {
        Self::connect_with_reconnects(conn, DEFAULT_RECONNECTS).await
    }

    pub async fn connect_with_reconnects(
        conn: ConnectionDetails,
        reconnects: usize,
    ) -> Result<(Self, WsClient), WsClientError> {
        let (ws, backend) = WsBackend::connect(conn.clone()).await?;

        let (requests_tx, requests_rx) = tokio::sync::mpsc::unbounded_channel();

        ws.spawn();

        Ok((
            Self {
                id: Default::default(),
                reconnects,
                reqs: Default::default(),
                backend,
                conn,
                requests: requests_rx,
            },
            WsClient {
                requests: requests_tx,
            },
        ))
    }

    async fn reconnect(&mut self) -> Result<(), WsClientError> {
        debug!("ws manager reconnecting");
        if self.reconnects == 0 {
            return Err(WsClientError::TooManyReconnects);
        }
        self.reconnects -= 1;

        // create the new backend
        let (s, mut backend) = WsBackend::connect(self.conn.clone()).await?;

        // spawn the new backend
        s.spawn();

        // swap out the backend
        std::mem::swap(&mut self.backend, &mut backend);

        // rename for clarity
        let mut old_backend = backend;

        // Drain anything in the backend
        while let Some(to_handle) = old_backend.to_handle.recv().await {
            self.handle_response(to_handle);
        }

        // issue a shutdown command (even though it's likely gone)
        old_backend.shutdown();

        // reissue requests
        for (id, pre_request) in self.reqs.iter() {
            let req = Request::new(*id, pre_request.method(), Arc::deref(&pre_request.params));
            self.backend
                .dispatcher
                .send(to_raw_value(&req).expect("it serializes"))
                .map_err(|_| WsClientError::DeadChannel)?;
        }

        Ok(())
    }

    fn handle_response(&mut self, item: PubSubItem) {
        match item {
            PubSubItem::Success { id, result } => {
                if let Some(req) = self.reqs.remove(&id) {
                    if let Err(_) = req.sender.send(Ok(result)) {
                        trace!("send to channel: {id}");
                    }
                } else {
                    error!("lost channel: {id}");
                }
            }
            PubSubItem::Error { id, error } => {
                error!("ws response: {id}");
                if let Some(req) = self.reqs.remove(&id) {
                    // pending fut has been dropped, this is fine
                    if let Err(_) = req.sender.send(Err(error)) {
                        trace!("send to channel: {id}");
                    }
                } else {
                    error!("lost channel: {id}");
                }
            }
        }
    }

    /// Receives and dispatches a request from a ws frontend
    fn handle_request(
        &mut self,
        pre_request: PreserializedCallRequest,
    ) -> Result<(), WsClientError> {
        let id = self.next_id();
        // we could insert `req` but the necessary lifetimes make the whole ws-cli
        // un-ergonomic
        let req_json = to_raw_value(&Request::new(
            id,
            pre_request.method(),
            Arc::deref(&pre_request.params),
        ))
        .unwrap();

        self.backend
            .dispatcher
            .send(req_json)
            .map_err(|_| WsClientError::DeadChannel)?;

        self.reqs.insert(id, pre_request);

        Ok(())
    }

    pub fn spawn(mut self) {
        let fut = async move {
            let result: Result<(), WsClientError> = loop {
                // We bias the loop so that we always handle messages before
                // reconnecting, and always reconnect before dispatching new
                // requests
                select! {
                    biased;

                    item_opt = self.backend.to_handle.recv() => {
                        match item_opt {
                            Some(item) => self.handle_response(item),
                            // Backend is gone, so reconnect
                            None => if let Err(e) = self.reconnect().await {
                                break Err(e);
                            }
                        }
                    },
                    _ = &mut self.backend.error => {
                        if let Err(e) = self.reconnect().await {
                            error!("failure reconnecting, ws backed exiting..");
                            break Err(e);
                        }
                    },
                    // internal request from ws cli
                    cli_request = self.requests.recv() => {
                        match cli_request {
                            Some(request) => if let Err(e) = self.handle_request(request) { break Err(e)},
                            // User-facing side is gone, so just exit
                            None => break Err(WsClientError::DeadChannel),
                        }
                    }
                }
            };
            // Issue the shutdown command. we don't care if it is received
            self.backend.shutdown();
            if let Err(err) = result {
                panic!("ws error: {:?}", err);
            }
        };

        tokio::spawn(fut);
    }
}
