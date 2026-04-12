use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{error, info, warn};

use gc_shared::protocol::codec;
use gc_shared::protocol::messages::{ClientMsg, Envelope, ServerMsg};
use gc_shared::protocol::version::PROTOCOL_VERSION;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct Connection {
    sender: SplitSink<WsStream, Message>,
    receiver: SplitStream<WsStream>,
    seq: u64,
}

impl Connection {
    /// Connect to the game server via WebSocket (TLS handled by connector).
    pub async fn connect(url: &str) -> Result<Self, String> {
        let (ws_stream, _response) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| format!("connection failed: {e}"))?;

        info!("connected to {url}");

        let (sender, receiver) = ws_stream.split();

        Ok(Self {
            sender,
            receiver,
            seq: 0,
        })
    }

    /// Send a client message to the server.
    pub async fn send(&mut self, msg: ClientMsg) -> Result<(), String> {
        self.seq += 1;
        let envelope = Envelope {
            version: PROTOCOL_VERSION,
            seq: self.seq,
            payload: msg,
        };
        let bytes = codec::encode(&envelope).map_err(|e| e.to_string())?;
        self.sender
            .send(Message::Binary(bytes.into()))
            .await
            .map_err(|e| format!("send failed: {e}"))
    }

    /// Receive the next server message. Returns None if connection closed.
    pub async fn recv(&mut self) -> Option<ServerMsg> {
        loop {
            match self.receiver.next().await? {
                Ok(Message::Binary(data)) => match codec::decode::<Envelope<ServerMsg>>(&data) {
                    Ok(envelope) => return Some(envelope.payload),
                    Err(e) => {
                        warn!("failed to decode message: {e}");
                        continue;
                    }
                },
                Ok(Message::Ping(_)) => continue,
                Ok(Message::Close(_)) => return None,
                Ok(_) => continue,
                Err(e) => {
                    error!("receive error: {e}");
                    return None;
                }
            }
        }
    }

    /// Close the connection gracefully.
    pub async fn close(&mut self) {
        let _ = self.sender.close().await;
    }
}
