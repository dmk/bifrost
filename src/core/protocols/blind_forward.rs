use super::{Protocol, ProtocolError};
use async_trait::async_trait;
use tokio::net::TcpStream;

/// Simple blind forward protocol - forwards raw TCP data
#[derive(Debug)]
pub struct BlindForwardProtocol {
    pub name: String,
}

impl BlindForwardProtocol {
    pub fn new() -> Self {
        Self {
            name: "blind_forward".to_string(),
        }
    }
}

impl Default for BlindForwardProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for BlindForwardProtocol {
    async fn handle_connection(
        &self,
        mut client: TcpStream,
        mut backend: TcpStream,
    ) -> Result<(), ProtocolError> {
        crate::server::proxy_bidirectional(&mut client, &mut backend)
            .await
            .map_err(|e| ProtocolError::ForwardingFailed(e.to_string()))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_blind_forward_proxies_data() {
        let listener_client = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_client = listener_client.local_addr().unwrap();
        let client =
            tokio::spawn(async move { tokio::net::TcpStream::connect(addr_client).await.unwrap() });
        let (client_socket, _) = listener_client.accept().await.unwrap();
        let mut client_peer = client.await.unwrap();

        let listener_backend = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_backend = listener_backend.local_addr().unwrap();
        let backend = tokio::net::TcpStream::connect(addr_backend).await.unwrap();
        let (mut backend_peer, _) = listener_backend.accept().await.unwrap();

        // Backend echo
        let echo = tokio::spawn(async move {
            let mut buf = [0u8; 32];
            let n = backend_peer.read(&mut buf).await.unwrap();
            backend_peer.write_all(&buf[..n]).await.unwrap();
        });

        let protocol = BlindForwardProtocol::new();
        let run = tokio::spawn(async move {
            protocol
                .handle_connection(client_socket, backend)
                .await
                .unwrap();
        });

        client_peer.write_all(b"hello").await.unwrap();
        client_peer.flush().await.unwrap();
        let mut out = [0u8; 32];
        let n = client_peer.read(&mut out).await.unwrap();
        assert_eq!(&out[..n], b"hello");

        drop(client_peer);
        let _ = echo.await;
        let _ = run.await;
    }

    #[tokio::test]
    async fn test_blind_forward_closes_when_client_closes() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::time::{timeout, Duration};

        let listener_client = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_client = listener_client.local_addr().unwrap();
        let client =
            tokio::spawn(async move { tokio::net::TcpStream::connect(addr_client).await.unwrap() });
        let (client_socket, _) = listener_client.accept().await.unwrap();
        let mut client_peer = client.await.unwrap();

        let listener_backend = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_backend = listener_backend.local_addr().unwrap();
        let backend = tokio::net::TcpStream::connect(addr_backend).await.unwrap();
        let (mut backend_peer, _) = listener_backend.accept().await.unwrap();

        // Spawn protocol handler
        let protocol = BlindForwardProtocol::new();
        let run = tokio::spawn(async move {
            protocol
                .handle_connection(client_socket, backend)
                .await
                .unwrap();
        });

        // Write then close client; backend should see EOF eventually
        client_peer.write_all(b"hello").await.unwrap();
        client_peer.shutdown().await.unwrap();

        let mut buf = [0u8; 16];
        let n = backend_peer.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        // Close backend to let proxy finish in both directions
        backend_peer.shutdown().await.unwrap();
        let _ = timeout(Duration::from_secs(1), run).await.unwrap();
    }

    #[test]
    fn test_protocol_name_blind_forward() {
        let p = BlindForwardProtocol::new();
        assert_eq!(p.name(), "blind_forward");
    }
}
