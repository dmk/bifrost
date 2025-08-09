use async_trait::async_trait;
use bb8::Pool;
use tokio::net::TcpStream;

use crate::core::backend::optimize_socket_for_latency;

/// Connection manager for bb8 that creates TCP connections to memcached servers.
#[derive(Debug, Clone)]
pub struct MemcachedConnectionManager {
    pub server_address: String,
}

impl MemcachedConnectionManager {
    pub fn new(server_address: String) -> Self {
        Self { server_address }
    }
}

#[async_trait]
impl bb8::ManageConnection for MemcachedConnectionManager {
    type Connection = TcpStream;
    type Error = std::io::Error;

    /// Connect to the memcached server.
    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        tracing::debug!("bb8 creating new connection to {}", self.server_address);
        let stream = TcpStream::connect(&self.server_address).await?;
        optimize_socket_for_latency(&stream);
        tracing::debug!("bb8 connection established to {}", self.server_address);
        Ok(stream)
    }

    /// Check if the connection is still valid.
    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        // We use `try_write` with a zero-byte slice to check if the connection is writable.
        // This is a lightweight, non-blocking way to detect a closed or broken connection.
        match conn.try_write(&[]) {
            Ok(_) => {
                tracing::trace!(
                    "bb8 connection validation passed for {}",
                    self.server_address
                );
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Write buffer is full, but connection is still valid.
                Ok(())
            }
            Err(e) => {
                tracing::debug!(
                    "bb8 connection validation failed for {}: {}",
                    self.server_address,
                    e
                );
                Err(e)
            }
        }
    }

    /// Determines if the connection is broken.
    /// Use a non-intrusive check that doesn't corrupt the protocol.
    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        // For a proxy scenario, we need to check if our connection to the BACKEND is broken,
        // not if the client connection is broken. Since we can't easily distinguish between
        // client disconnection vs backend disconnection, we'll be conservative and assume
        // the connection is still good unless we can definitively prove it's broken.

        // Try a very lightweight check: see if the socket is readable without blocking
        match conn.try_read(&mut [0u8; 0]) {
            Ok(0) => {
                tracing::debug!(
                    "bb8 detected broken connection to {} (read 0 bytes)",
                    self.server_address
                );
                true
            }
            Ok(_) => false,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(e) => {
                tracing::debug!(
                    "bb8 detected broken connection to {}: {}",
                    self.server_address,
                    e
                );
                true
            }
        }
    }
}

/// Type alias for our connection pool.
pub type MemcachedPool = Pool<MemcachedConnectionManager>;

#[cfg(test)]
mod tests {
    use super::*;
    use bb8::ManageConnection;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_manager_connect_and_validate() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Accept connections in background to complete handshake
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    // Keep stream open briefly
                    std::mem::drop(tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        drop(stream);
                    }));
                }
            }
        });

        let manager = MemcachedConnectionManager::new(addr.to_string());
        let mut conn = manager.connect().await.unwrap();
        manager.is_valid(&mut conn).await.unwrap();
        // has_broken should usually be false immediately after connect
        assert!(!manager.has_broken(&mut conn));
    }
}
