use crate::config::Config;
use crate::core::{Protocol, RouteTable, RouteTableBuilder};
use crate::core::protocols::AsciiProtocol;
use crate::core::route_table::ResolvedTarget;
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
    route_table: Arc<RouteTable>,
    protocol: Arc<dyn Protocol>,
}

impl BifrostServer {
    pub async fn new(config: Config) -> Result<Self, ServerError> {
        // Build route table from config
        let route_table = RouteTableBuilder::build_from_config(&config)
            .await
            .map_err(|e| ServerError::RouteTableBuildFailed(e.to_string()))?;

        info!("Route table built with {} routes", route_table.routes().len());

        // Use ASCII protocol to parse keys for routing
        let protocol = Arc::new(AsciiProtocol::new()) as Arc<dyn Protocol>;

        Ok(Self {
            config: Arc::new(config),
            route_table,
            protocol,
        })
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
                    let route_table = Arc::clone(&self.route_table);
                    let protocol = Arc::clone(&self.protocol);

                    // Handle connection in a separate task
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection_with_routing(client_socket, route_table, protocol).await {
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

/// Handle concurrent pool requests (miss failover)
async fn handle_concurrent_pool_request(
    mut client_reader: tokio::io::BufReader<TcpStream>,
    pool: &Arc<dyn crate::core::Pool>,
    key: &str,
    first_line: &str,
) -> Result<(), ServerError> {
    use tokio::io::AsyncWriteExt;

    debug!("Handling concurrent pool request for key: {}", key);

    // For "get" requests, we only need the first line
    let request_data = first_line.as_bytes().to_vec();

    debug!("Concurrent request size: {} bytes", request_data.len());

    // Send the request to the concurrent pool
    match pool.handle_concurrent_request(key, &request_data).await {
        Ok(response) => {
            debug!("Concurrent pool returned response: {} bytes", response.len());

            let mut client_stream = client_reader.into_inner();
            // Send response back to client
            client_stream.write_all(&response).await
                .map_err(|e| ServerError::IoError(format!("Failed to write response to client: {}", e)))?;
            client_stream.flush().await
                .map_err(|e| ServerError::IoError(format!("Failed to flush client response: {}", e)))?;

            debug!("Response sent to client successfully");

            // If the successful backend was not the first one, we might need to drain
            // the initial response from other backends that might have also responded.
            // This is complex and for now, we assume the memcached protocol is simple
            // enough that this isn't an immediate issue. The client will get the first
            // valid data and ignore anything else that might follow.

            Ok(())
        }
        Err(e) => {
            error!("Concurrent pool request failed: {}", e);

            // Send error response to client
            let error_response = b"SERVER_ERROR concurrent request failed\r\n";
            let mut client_stream = client_reader.into_inner();
            client_stream.write_all(error_response).await
                .map_err(|e| ServerError::IoError(format!("Failed to write error response: {}", e)))?;
            client_stream.flush().await
                .map_err(|e| ServerError::IoError(format!("Failed to flush error response: {}", e)))?;

            Ok(())
        }
    }
}

/// Check if a command requires reading data payload
fn is_storage_command(line: &str) -> bool {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    matches!(parts[0].to_uppercase().as_str(), "SET" | "ADD" | "REPLACE")
}

/// Extract byte count from storage command line (SET key flags exptime bytes [noreply])
fn extract_byte_count(line: &str) -> Option<usize> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.len() >= 5 {
        parts[4].parse().ok()
    } else {
        None
    }
}

/// Handle a client connection using our route table system
async fn handle_connection_with_routing(
    client_socket: TcpStream,
    route_table: Arc<RouteTable>,
    _protocol: Arc<dyn Protocol>,
) -> Result<(), ServerError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut client_reader = BufReader::new(client_socket);

    // Read the first line to extract the key
    let mut first_line = String::new();
    client_reader.read_line(&mut first_line).await
        .map_err(|e| ServerError::IoError(format!("Failed to read first line: {}", e)))?;

    if first_line.is_empty() {
        return Ok(()); // Connection closed
    }

    debug!("First request line: {}", first_line.trim());

    // Extract key from the request (simple parsing)
    let key = extract_key_from_request(&first_line)
        .unwrap_or("default".to_string());

    debug!("Extracted key: '{}' for routing", key);

    // Find route for this key
    let route = route_table.find_route(&key)
        .ok_or_else(|| ServerError::NoRoutes)?;

    debug!("Key '{}' matched route pattern: '{}'", key, route.matcher.pattern());

    // Select backend based on route target and connect using the appropriate method
    let mut backend_socket = match &route.target {
        ResolvedTarget::Backend(backend) => {
            debug!("Route targets backend: {}", backend.name());
            connect_to_backend(backend, &key).await?
        }
        ResolvedTarget::Pool(pool) => {
            debug!("Route targets pool: {} with strategy: {}", pool.name(), pool.strategy().name());

            // Check if this pool supports concurrent requests (miss failover)
            if pool.supports_concurrent_requests() {
                debug!("Pool supports concurrent requests - handling with miss failover");
                return handle_concurrent_pool_request(client_reader, pool, &key, &first_line).await;
            } else {
                // Regular pool - select one backend
                let selected = pool.select_backend(&key).await
                    .map_err(|e| ServerError::BackendConnectionFailed(format!("Pool selection failed: {}", e)))?;

                debug!("Pool selected backend: {}", selected.name());

                // Use the selected backend directly (it has the connection pool configuration)
                connect_to_backend_ref(selected, &key).await?
            }
        }
    };

    // Forward the first line that we already read
    backend_socket.write_all(first_line.as_bytes()).await
        .map_err(|e| ServerError::IoError(format!("Failed to write first line to backend: {}", e)))?;

    backend_socket.flush().await
        .map_err(|e| ServerError::IoError(format!("Failed to flush backend: {}", e)))?;

    debug!("Forwarded first line to backend: {}", first_line.trim());

    // Forward any data remaining in the client reader's buffer
    let buffered_data = client_reader.buffer();
    if !buffered_data.is_empty() {
        backend_socket.write_all(buffered_data).await
            .map_err(|e| ServerError::IoError(format!("Failed to forward initial buffer: {}", e)))?;
        backend_socket.flush().await
            .map_err(|e| ServerError::IoError(format!("Failed to flush initial buffer: {}", e)))?;
        debug!("Forwarded {} bytes from initial client buffer", buffered_data.len());
    }

    // Now, proxy data bidirectionally
    let mut client_stream = client_reader.into_inner();
    proxy_bidirectional(&mut client_stream, &mut backend_socket).await
        .map_err(|e| ServerError::IoError(format!("Proxy error: {}", e)))
}

/// Helper function to connect to a backend by its Arc reference
async fn connect_to_backend(backend: &Arc<dyn crate::core::Backend>, key: &str) -> Result<TcpStream, ServerError> {
    connect_to_backend_ref(backend.as_ref(), key).await
}

/// Connect to a backend using a direct reference (trait object)
async fn connect_to_backend_ref(backend: &dyn crate::core::Backend, key: &str) -> Result<TcpStream, ServerError> {
    if backend.uses_connection_pool() {
        debug!("🏊 Backend {} has connection pooling configured", backend.name());
        let stream = backend.get_pooled_stream().await
            .map_err(|e| ServerError::BackendConnectionFailed(format!("Connection failed: {}", e)))?;

        info!("🎯 Key '{}' routed to backend: {} ({}) [POOL-READY]", key, backend.name(), backend.server());
        Ok(stream)
    } else {
        debug!("🔗 Using direct connection for backend: {}", backend.name());
        let stream = backend.connect().await
            .map_err(|e| ServerError::BackendConnectionFailed(e.to_string()))?;

        info!("🎯 Key '{}' routed to backend: {} ({}) [DIRECT]", key, backend.name(), backend.server());
        Ok(stream)
    }
}

/// Extract key from memcached request line (simple implementation)
fn extract_key_from_request(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();

    if parts.len() < 2 {
        return None;
    }

    match parts[0].to_uppercase().as_str() {
        "GET" | "SET" | "ADD" | "REPLACE" | "DELETE" | "INCR" | "DECR" => {
            Some(parts[1].to_string())
        }
        _ => None,
    }
}

/// Proxy data bidirectionally between client and backend (optimized version)
pub async fn proxy_bidirectional(client_socket: &mut TcpStream, backend_socket: &mut TcpStream) -> Result<(), std::io::Error> {
    // Use tokio's optimized bidirectional copy for zero-copy forwarding
    // This is more robust than the select! loop as it handles half-closed connections
    let (client_bytes, backend_bytes) = tokio::io::copy_bidirectional(client_socket, backend_socket).await?;

    debug!("Bidirectional proxy completed. client->backend: {} bytes, backend->client: {} bytes", client_bytes, backend_bytes);

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
    #[error("Failed to build route table: {0}")]
    RouteTableBuildFailed(String),
}