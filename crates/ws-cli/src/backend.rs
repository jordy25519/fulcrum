use ethers_providers::{ConnectionDetails, WsClientError};
use futures_util::{
    stream::{Fuse, StreamExt},
    SinkExt,
};
use log::error;
use serde_json::value::RawValue;
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self},
    MaybeTlsStream, WebSocketStream,
};
pub type Message = tungstenite::protocol::Message;
pub type WsError = tungstenite::Error;
pub type WsStreamItem = Result<Message, WsError>;

use super::PubSubItem;

pub type InternalStream = Fuse<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>;

/// `BackendDriver` drives a specific `WsBackend`. It can be used to issue
/// requests, receive responses, see errors, and shut down the backend.
pub struct BackendDriver {
    // Pubsub items from the backend, received via WS
    pub to_handle: mpsc::UnboundedReceiver<PubSubItem>,
    // Notification from the backend of a terminal error
    pub error: oneshot::Receiver<()>,

    // Requests that the backend should dispatch
    pub dispatcher: mpsc::UnboundedSender<Box<RawValue>>,
    // Notify the backend of intentional shutdown
    shutdown: oneshot::Sender<()>,
}

impl BackendDriver {
    pub fn shutdown(self) {
        // don't care if it fails, as that means the backend is gone anyway
        let _ = self.shutdown.send(());
    }
}

/// `WsBackend` dispatches requests and routes responses and notifications. It
/// also has a simple ping-based keepalive (when not compiled to wasm), to
/// prevent inactivity from triggering server-side closes
///
/// The `WsBackend` shuts down when instructed to by the `RequestManager` or
/// when the `RequestManager` drops (because the inbound channel will close)
pub struct WsBackend {
    server: InternalStream,
    // channel to the manager, through which to send items received via WS
    handler: mpsc::UnboundedSender<PubSubItem>,
    // notify manager of an error causing this task to halt
    error: oneshot::Sender<()>,

    // channel of inbound requests to dispatch
    to_dispatch: mpsc::UnboundedReceiver<Box<RawValue>>,
    // notification from manager of intentional shutdown
    shutdown: oneshot::Receiver<()>,
}

impl WsBackend {
    pub async fn connect(
        details: ConnectionDetails,
    ) -> Result<(Self, BackendDriver), WsClientError> {
        let (ws, _) = connect_async(details).await?;
        Ok(Self::new(ws.fuse()))
    }

    pub fn new(client: InternalStream) -> (Self, BackendDriver) {
        let (handler, to_handle) = mpsc::unbounded_channel();
        let (dispatcher, to_dispatch) = mpsc::unbounded_channel();
        let (error_tx, error_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        (
            WsBackend {
                server: client,
                handler,
                error: error_tx,
                to_dispatch,
                shutdown: shutdown_rx,
            },
            BackendDriver {
                to_handle,
                error: error_rx,
                dispatcher,
                shutdown: shutdown_tx,
            },
        )
    }

    // Handle incoming Websocket `Message::Text` data
    pub async fn handle_text(&mut self, t: &[u8]) -> Result<(), WsClientError> {
        match serde_json::from_slice(t) {
            Ok(item) => {
                if self.handler.send(item).is_err() {
                    return Err(WsClientError::DeadChannel);
                }
            }
            Err(e) => return Err(WsClientError::JsonError(e)),
        }
        Ok(())
    }

    /// Handle messages from the server
    async fn handle_incoming(&mut self, item: WsStreamItem) -> Result<(), WsClientError> {
        match item {
            Ok(item) => match item {
                Message::Text(t) => self.handle_text(t.as_bytes()).await,
                // https://github.com/snapview/tungstenite-rs/blob/314feea3055a93e585882fb769854a912a7e6dae/src/protocol/mod.rs#L172-L175
                Message::Ping(_) => Ok(()),
                Message::Pong(_) => Ok(()),
                Message::Frame(_) => Ok(()),
                Message::Binary(buf) => Err(WsClientError::UnexpectedBinary(buf)),
                Message::Close(_frame) => Err(WsClientError::UnexpectedClose),
            },
            Err(e) => Err(e.into()),
        }
    }

    pub fn spawn(mut self) {
        let fut = async move {
            let mut err = false;
            loop {
                select! {
                    biased;
                    resp = self.server.next() => {
                        match resp {
                            Some(item) => {
                                if let Err(e) = self.handle_incoming(item).await
                                {
                                    error!("handle ws response: {:?}", e);
                                    err = true;
                                    break;
                                }
                            },
                            None => {
                                err = true;
                                error!("unexpected empty response");
                                break
                            },
                        }
                    }
                    // we've received a new dispatch, so we send it via
                    // websocket
                    inst = self.to_dispatch.recv() => {
                                match inst {
                                    Some(msg) => {
                                        if let Err(_) = self.server.send(Message::Text(msg.to_string())).await {
                                            println!("err while send ws to server");
                                            err = true;
                                            break
                                        }
                                    },
                                    // dispatcher has gone away
                                    None => {
                                        println!("dispatcher finished");
                                        err = true;
                                        break
                                    },
                        }
                    },
                    // break on shutdown recv, or on shutdown recv error
                    _ = &mut self.shutdown => {
                        error!("ws shutdown");
                        break
                    },
                }
            }
            if err {
                let _ = self.error.send(());
            }
        };

        tokio::spawn(fut);
    }
}
