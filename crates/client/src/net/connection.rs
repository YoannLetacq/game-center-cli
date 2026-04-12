use std::sync::Arc;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;
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
    /// Connect to the game server via WebSocket over TLS.
    /// Accepts self-signed certificates for development.
    pub async fn connect(url: &str) -> Result<Self, String> {
        // Build a TLS config that accepts any certificate (dev-friendly).
        // In production, this should use proper certificate validation.
        let tls_config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyCert))
            .with_no_client_auth();

        let connector = tokio_tungstenite::Connector::Rustls(Arc::new(tls_config));

        let (ws_stream, _response) =
            tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector))
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

/// TLS certificate verifier that accepts any certificate.
/// Used for development with self-signed certs.
#[derive(Debug)]
struct AcceptAnyCert;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error>
    {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
