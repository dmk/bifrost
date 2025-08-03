use crate::config::Config;
use crate::core::connection_pool::MemcachedConnectionManager;
use crate::core::metrics::BackendMetrics;
use crate::core::protocols::AsciiProtocol;
use crate::core::route_table::ResolvedTarget;
use crate::core::{Protocol, RouteTable, RouteTableBuilder};
use std::collections::HashSet;
use std::ops::DerefMut;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

/// Optimize client socket for low latency (same as backend optimization)
fn optimize_client_socket(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    let socket_ref = socket2::SockRef::try_from(stream).unwrap();
    let _ = socket_ref.set_reuse_address(true);
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
        let route_table = RouteTableBuilder::build_from_config(&config)
            .await
            .map_err(|e| ServerError::RouteTableBuildFailed(e.to_string()))?;

        info!(
            "Route table built with {} routes",
            route_table.routes().len()
        );

        let protocol = Arc::new(AsciiProtocol::new()) as Arc<dyn Protocol>;

        Ok(Self {
            config: Arc::new(config),
            route_table,
            protocol,
        })
    }

    /// Start the server and listen for connections
    pub async fn start(&self) -> Result<(), ServerError> {
        let (listener_name, listener_config) = self
            .config
            .listeners
            .iter()
            .next()
            .ok_or(ServerError::NoListeners)?;

        info!("Starting Bifrost server");
        info!(
            "Listener '{}' binding to: {}",
            listener_name, listener_config.bind
        );

        let listener = TcpListener::bind(&listener_config.bind)
            .await
            .map_err(|e| ServerError::BindFailed(e.to_string()))?;

        info!("Server listening on {}", listener_config.bind);

        loop {
            match listener.accept().await {
                Ok((client_socket, addr)) => {
                    debug!("New connection from: {}", addr);
                    optimize_client_socket(&client_socket);

                    let route_table = Arc::clone(&self.route_table);
                    let protocol = Arc::clone(&self.protocol);

                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_connection_with_routing(client_socket, route_table, protocol)
                                .await
                        {
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

async fn handle_concurrent_pool_request(
    client_reader: tokio::io::BufReader<TcpStream>,
    pool: &Arc<dyn crate::core::Pool>,
    key: &str,
    first_line: &str,
) -> Result<(), ServerError> {
    use tokio::io::AsyncWriteExt;

    debug!("Handling concurrent pool request for key: {}", key);
    let request_data = first_line.as_bytes().to_vec();

    match pool.handle_concurrent_request(key, &request_data).await {
        Ok(response) => {
            debug!(
                "Concurrent pool returned response: {} bytes",
                response.len()
            );
            let mut client_stream = client_reader.into_inner();
            client_stream.write_all(&response).await.map_err(|e| {
                ServerError::IoError(format!("Failed to write response to client: {}", e))
            })?;
            client_stream.flush().await.map_err(|e| {
                ServerError::IoError(format!("Failed to flush client response: {}", e))
            })?;
            Ok(())
        }
        Err(e) => {
            error!("Concurrent pool request failed: {}", e);
            let error_response = b"SERVER_ERROR backend pool error\r\n";
            let mut client_stream = client_reader.into_inner();
            client_stream
                .write_all(error_response)
                .await
                .map_err(|e| ServerError::IoError(format!("Failed to write error response: {}", e)))?;
            client_stream
                .flush()
                .await
                .map_err(|e| ServerError::IoError(format!("Failed to flush error response: {}", e)))?;
            Ok(())
        }
    }
}

async fn handle_stats_command(
    mut client_socket: TcpStream,
    route_table: &Arc<RouteTable>,
) -> Result<(), ServerError> {
    use tokio::io::AsyncWriteExt;

    let mut response = String::new();
    response.push_str("STAT version 1.0.0\r\n");
    response.push_str(&format!(
        "STAT routes {}\r\n",
        route_table.routes().len()
    ));

    let mut unique_pools = HashSet::new();
    let mut unique_backends = HashSet::new();

    for route in route_table.routes() {
        match &route.target {
            ResolvedTarget::Backend(backend) => {
                if unique_backends.insert(backend.name().to_string()) {
                    let metrics = backend.metrics().get_snapshot().await;
                    response.push_str(&format!(
                        "STAT backend_{}_server {}\r\n",
                        metrics.backend_name,
                        backend.server()
                    ));
                    response.push_str(&format!(
                        "STAT backend_{}_connections {}\r\n",
                        metrics.backend_name, metrics.current_connections
                    ));
                }
            }
            ResolvedTarget::Pool(pool) => {
                if unique_pools.insert(pool.name().to_string()) {
                    response.push_str(&format!("STAT pool_{}_backends {}\r\n", pool.name(), pool.backends().len()));
                    for backend in pool.backends() {
                        let metrics = backend.metrics().get_snapshot().await;
                        response.push_str(&format!(
                            "STAT pool_{}_backend_{}_server {}\r\n",
                            pool.name(),
                            metrics.backend_name,
                            backend.server()
                        ));
                    }
                }
            }
        }
    }

    response.push_str("END\r\n");

    client_socket
        .write_all(response.as_bytes())
        .await
        .map_err(|e| ServerError::IoError(e.to_string()))?;
    client_socket
        .flush()
        .await
        .map_err(|e| ServerError::IoError(e.to_string()))?;

    Ok(())
}

enum BackendConnection<'a> {
    Pooled(bb8::PooledConnection<'a, MemcachedConnectionManager>),
    Direct(TcpStream),
}

impl<'a> DerefMut for BackendConnection<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            BackendConnection::Pooled(conn) => conn.deref_mut(),
            BackendConnection::Direct(conn) => conn,
        }
    }
}

impl<'a> std::ops::Deref for BackendConnection<'a> {
    type Target = TcpStream;

    fn deref(&self) -> &Self::Target {
        match self {
            BackendConnection::Pooled(conn) => conn,
            BackendConnection::Direct(conn) => conn,
        }
    }
}

async fn handle_connection_with_routing(
    client_socket: TcpStream,
    route_table: Arc<RouteTable>,
    _protocol: Arc<dyn Protocol>,
) -> Result<(), ServerError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut client_reader = BufReader::new(client_socket);
    let mut first_line = String::new();
    client_reader
        .read_line(&mut first_line)
        .await
        .map_err(|e| ServerError::IoError(format!("Failed to read first line: {}", e)))?;

    if first_line.is_empty() {
        return Ok(());
    }

    if first_line.trim() == "stats" {
        return handle_stats_command(client_reader.into_inner(), &route_table).await;
    }

    let key = extract_key_from_request(&first_line).unwrap_or_else(|| "default".to_string());
    let route = route_table
        .find_route(&key)
        .ok_or(ServerError::NoRoutes)?;

    let mut backend_connection = match &route.target {
        ResolvedTarget::Backend(backend) => {
            if backend.uses_connection_pool() {
                let conn = backend
                    .get_pooled_stream()
                    .await
                    .map_err(|e| ServerError::BackendConnectionFailed(e.to_string()))?;
                BackendConnection::Pooled(conn)
            } else {
                let conn = backend
                    .connect()
                    .await
                    .map_err(|e| ServerError::BackendConnectionFailed(e.to_string()))?;
                BackendConnection::Direct(conn)
            }
        }
        ResolvedTarget::Pool(pool) => {
            if pool.supports_concurrent_requests() {
                return handle_concurrent_pool_request(client_reader, pool, &key, &first_line)
                    .await;
            }
            let selected_backend = pool
                .select_backend(&key)
                .await
                .map_err(|e| {
                    ServerError::BackendConnectionFailed(format!("Pool selection failed: {}", e))
                })?;

            if selected_backend.uses_connection_pool() {
                let conn = selected_backend
                    .get_pooled_stream()
                    .await
                    .map_err(|e| ServerError::BackendConnectionFailed(e.to_string()))?;
                BackendConnection::Pooled(conn)
            } else {
                let conn = selected_backend
                    .connect()
                    .await
                    .map_err(|e| ServerError::BackendConnectionFailed(e.to_string()))?;
                BackendConnection::Direct(conn)
            }
        }
    };

    backend_connection
        .write_all(first_line.as_bytes())
        .await
        .map_err(|e| {
            ServerError::IoError(format!("Failed to write first line to backend: {}", e))
        })?;
    backend_connection.flush().await.map_err(|e| ServerError::IoError(format!("Failed to flush backend: {}", e)))?;


    let buffered_data = client_reader.buffer();
    if !buffered_data.is_empty() {
        backend_connection
            .write_all(buffered_data)
            .await
            .map_err(|e| {
                ServerError::IoError(format!("Failed to forward initial buffer: {}", e))
            })?;
        backend_connection.flush().await.map_err(|e| ServerError::IoError(format!("Failed to flush backend: {}", e)))?;
    }

    let mut client_stream = client_reader.into_inner();
    let result = proxy_bidirectional(&mut client_stream, &mut backend_connection)
        .await
        .map_err(|e| ServerError::IoError(format!("Proxy error: {}", e)));

    // Log when the connection is about to be returned to pool
    match &backend_connection {
        BackendConnection::Pooled(_) => {
            tracing::info!("🔄 Returning pooled connection to pool");
        }
        BackendConnection::Direct(_) => {
            tracing::debug!("📴 Closing direct connection");
        }
    }

    result
}

fn extract_key_from_request(line: &str) -> Option<String> {
    line.trim().split_whitespace().nth(1).map(String::from)
}

pub async fn proxy_bidirectional(
    client_socket: &mut TcpStream,
    backend_socket: &mut TcpStream,
) -> Result<(), std::io::Error> {
    tokio::io::copy_bidirectional(client_socket, backend_socket).await?;
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
