use crate::config::Config;
use crate::core::{Backend, Strategy, Protocol};
use crate::core::backend::MemcachedBackend;
use crate::core::strategy::BlindForwardStrategy;
use crate::core::protocol::BlindForwardProtocol;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tracing::{info, error, debug};

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

    // Connect to backend
    let backend_socket = backend.connect()
        .await
        .map_err(|e| ServerError::BackendConnectionFailed(e.to_string()))?;

    debug!("Connected to backend: {}", backend.server());

    // Use protocol to handle the connection
    if let Err(e) = protocol.handle_connection(client_socket, backend_socket).await {
        debug!("Protocol handling ended: {}", e);
    }

    debug!("Connection closed");
    Ok(())
}

/// Proxy data bidirectionally between client and backend
pub async fn proxy_bidirectional(client_socket: TcpStream, backend_socket: TcpStream) -> Result<(), std::io::Error> {
    let (client_read, client_write) = client_socket.into_split();
    let (backend_read, backend_write) = backend_socket.into_split();

    // Forward client -> backend
    let client_to_backend = proxy_stream(client_read, backend_write, "client", "backend");

    // Forward backend -> client
    let backend_to_client = proxy_stream(backend_read, client_write, "backend", "client");

    // Run both directions concurrently
    tokio::select! {
        result = client_to_backend => {
            if let Err(e) = result {
                debug!("Client to backend forwarding ended: {}", e);
            }
        }
        result = backend_to_client => {
            if let Err(e) = result {
                debug!("Backend to client forwarding ended: {}", e);
            }
        }
    }

    Ok(())
}

/// Forward data from reader to writer
async fn proxy_stream<R, W>(
    mut reader: R,
    mut writer: W,
    from: &str,
    to: &str,
) -> Result<(), std::io::Error>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut buffer = [0; 4096];

    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                debug!("{} closed connection", from);
                break;
            }
            Ok(n) => {
                debug!("Forwarding {} bytes from {} to {}", n, from, to);
                writer.write_all(&buffer[..n]).await?;
                writer.flush().await?;
            }
            Err(e) => {
                debug!("Error reading from {}: {}", from, e);
                return Err(e);
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