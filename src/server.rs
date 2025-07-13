use crate::config::Config;
use crate::core::{Backend, Strategy, Protocol};
use crate::core::backend::MemcachedBackend;
use crate::core::strategy::BlindForwardStrategy;
use crate::core::protocol::BlindForwardProtocol;
use tokio::net::{TcpListener, TcpStream};
use std::sync::Arc;
use tracing::{info, error, debug};

/// Optimize client socket for low latency (same as backend optimization)
fn optimize_client_socket(stream: &TcpStream) {
    // Disable Nagle's algorithm for lower latency
    let _ = stream.set_nodelay(true);

    // Additional socket optimizations using socket2
    let socket_ref = socket2::SockRef::try_from(stream).unwrap();
    // Set socket to reuse address for faster reconnection
    let _ = socket_ref.set_reuse_address(true);

    // Optimize send/receive buffer sizes for cache workloads
    let _ = socket_ref.set_send_buffer_size(32768);
    let _ = socket_ref.set_recv_buffer_size(32768);
}

pub struct BifrostServer {
    config: Arc<Config>,
    backends: Arc<Vec<Box<dyn Backend>>>,
    strategy: Arc<dyn Strategy>,
    protocol: Arc<dyn Protocol>,
}

impl BifrostServer {
    pub fn new(config: Config) -> Self {
        // Create backends from config
        let backends: Vec<Box<dyn Backend>> = config.backends
            .iter()
            .map(|(name, backend_config)| {
                Box::new(MemcachedBackend::new(name.clone(), backend_config.server.clone())) as Box<dyn Backend>
            })
            .collect();

        // For now, use simple strategies
        let strategy = Arc::new(BlindForwardStrategy::new()) as Arc<dyn Strategy>;
        let protocol = Arc::new(BlindForwardProtocol::new()) as Arc<dyn Protocol>;

        Self {
            config: Arc::new(config),
            backends: Arc::new(backends),
            strategy,
            protocol,
        }
    }

    /// Start the server and listen for connections
    pub async fn start(&self) -> Result<(), ServerError> {
        // For now, just start the first listener
        let (listener_name, listener_config) = self.config.listeners
            .iter()
            .next()
            .ok_or(ServerError::NoListeners)?;

        info!("Starting Bifrost server");
        info!("Listener '{}' binding to: {}", listener_name, listener_config.bind);

        let listener = TcpListener::bind(&listener_config.bind)
            .await
            .map_err(|e| ServerError::BindFailed(e.to_string()))?;

        info!("Server listening on {}", listener_config.bind);

        loop {
            match listener.accept().await {
                Ok((client_socket, addr)) => {
                    debug!("New connection from: {}", addr);

                    // Optimize client socket immediately
                    optimize_client_socket(&client_socket);

                    // Clone necessary data for the task
                    let backends = Arc::clone(&self.backends);
                    let strategy = Arc::clone(&self.strategy);
                    let protocol = Arc::clone(&self.protocol);

                    // Handle connection in a separate task
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(client_socket, backends, strategy, protocol).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

/// Handle a client connection using the trait-based architecture
async fn handle_connection(
    client_socket: TcpStream,
    backends: Arc<Vec<Box<dyn Backend>>>,
    strategy: Arc<dyn Strategy>,
    protocol: Arc<dyn Protocol>,
) -> Result<(), ServerError> {
    // Use strategy to select backend
    let backend = strategy.select_backend(&backends)
        .await
        .ok_or(ServerError::NoBackends)?;

    debug!("Using backend '{}' at: {}", backend.name(), backend.server());

    // Connect to backend (optimized with comprehensive socket settings)
    let backend_socket = backend.connect()
        .await
        .map_err(|e| ServerError::BackendConnectionFailed(e.to_string()))?;

    debug!("Connected to backend: {}", backend.server());

    // Use protocol to handle the connection
    // Note: This consumes both client and backend connections
    if let Err(e) = protocol.handle_connection(client_socket, backend_socket).await {
        debug!("Protocol handling ended: {}", e);
    }

    debug!("Connection closed");
    Ok(())
}

/// Proxy data bidirectionally between client and backend (optimized version)
pub async fn proxy_bidirectional(client_socket: TcpStream, backend_socket: TcpStream) -> Result<(), std::io::Error> {
    // Use tokio's optimized bidirectional copy for zero-copy forwarding
    let (mut client_read, mut client_write) = client_socket.into_split();
    let (mut backend_read, mut backend_write) = backend_socket.into_split();

    // Forward client -> backend and backend -> client concurrently
    // This is more efficient than manual buffering
    tokio::select! {
        result = tokio::io::copy(&mut client_read, &mut backend_write) => {
            match result {
                Ok(bytes) => debug!("Client to backend forwarding completed: {} bytes", bytes),
                Err(e) => debug!("Client to backend forwarding ended: {}", e),
            }
        }
        result = tokio::io::copy(&mut backend_read, &mut client_write) => {
            match result {
                Ok(bytes) => debug!("Backend to client forwarding completed: {} bytes", bytes),
                Err(e) => debug!("Backend to client forwarding ended: {}", e),
            }
        }
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("No listeners configured")]
    NoListeners,
    #[error("No backends configured")]
    NoBackends,
    #[error("No routes configured")]
    NoRoutes,
    #[error("Backend '{0}' not found in configuration")]
    BackendNotFound(String),
    #[error("Bind failed: {0}")]
    BindFailed(String),
    #[error("Backend connection failed: {0}")]
    BackendConnectionFailed(String),
    #[error("IO error: {0}")]
    IoError(String),
}