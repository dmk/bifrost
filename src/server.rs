use crate::config::Config;
use crate::core::connection_pool::MemcachedConnectionManager;
use crate::core::metrics::BackendMetrics;
use crate::core::protocols::AsciiProtocol; // No AsciiCommand usage here
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
    let socket_ref = socket2::SockRef::from(stream);
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
            "Listener '{}' binding to {}",
            listener_name, listener_config.bind
        );

        let listener = TcpListener::bind(&listener_config.bind)
            .await
            .map_err(|e| ServerError::BindFailed(e.to_string()))?;

        info!("Server listening on {}", listener_config.bind);

        loop {
            match listener.accept().await {
                Ok((client_socket, addr)) => {
                    debug!("new connection from {}", addr);
                    optimize_client_socket(&client_socket);

                    let route_table = Arc::clone(&self.route_table);
                    let protocol = Arc::clone(&self.protocol);

                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_connection_with_routing(client_socket, route_table, protocol)
                                .await
                        {
                            error!("connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("failed to accept connection: {}", e);
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

    debug!("handling concurrent pool request for key {}", key);
    let request_data = first_line.as_bytes().to_vec();

    match pool.handle_concurrent_request(key, &request_data).await {
        Ok(response) => {
            debug!(
                "concurrent pool returned response: {} bytes",
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
            error!("concurrent pool request failed: {}", e);
            let error_response = b"SERVER_ERROR backend pool error\r\n";
            let mut client_stream = client_reader.into_inner();
            client_stream.write_all(error_response).await.map_err(|e| {
                ServerError::IoError(format!("Failed to write error response: {}", e))
            })?;
            client_stream.flush().await.map_err(|e| {
                ServerError::IoError(format!("Failed to flush error response: {}", e))
            })?;
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
    response.push_str(&format!("STAT routes {}\r\n", route_table.routes().len()));

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
                    response.push_str(&format!(
                        "STAT pool_{}_backends {}\r\n",
                        pool.name(),
                        pool.backends().len()
                    ));
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

impl DerefMut for BackendConnection<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            BackendConnection::Pooled(conn) => conn.deref_mut(),
            BackendConnection::Direct(conn) => conn,
        }
    }
}

impl std::ops::Deref for BackendConnection<'_> {
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
    let route = route_table.find_route(&key).ok_or(ServerError::NoRoutes)?;

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
            let selected_backend = pool.select_backend(&key).await.map_err(|e| {
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
            ServerError::IoError(format!("failed to write first line to backend: {}", e))
        })?;
    backend_connection
        .flush()
        .await
        .map_err(|e| ServerError::IoError(format!("failed to flush backend: {}", e)))?;

    let buffered_data = client_reader.buffer();
    if !buffered_data.is_empty() {
        backend_connection
            .write_all(buffered_data)
            .await
            .map_err(|e| {
                ServerError::IoError(format!("failed to forward initial buffer: {}", e))
            })?;
        backend_connection
            .flush()
            .await
            .map_err(|e| ServerError::IoError(format!("failed to flush backend: {}", e)))?;
    }

    let mut client_stream = client_reader.into_inner();
    let result = proxy_bidirectional(&mut client_stream, &mut backend_connection)
        .await
        .map_err(|e| ServerError::IoError(format!("proxy error: {}", e)));

    // Log when the connection is about to be returned to pool
    match &backend_connection {
        BackendConnection::Pooled(_) => {
            tracing::debug!("returning pooled connection to pool");
        }
        BackendConnection::Direct(_) => {
            tracing::debug!("closing direct connection");
        }
    }

    result
}

fn extract_key_from_request(line: &str) -> Option<String> {
    line.split_whitespace().nth(1).map(String::from)
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

#[cfg(test)]
mod tests {
    use crate::core::backend::{Backend, BackendError};
    use crate::core::metrics::AtomicBackendMetrics;
    use crate::core::pool::BasicPool;
    use crate::core::protocols::AsciiProtocol;
    use crate::core::route_table::{Matcher, ResolvedTarget, Route, RouteTable};
    use crate::core::strategy::BlindForwardStrategy;
    use crate::config::{Config, ListenerConfig};
    use super::{BifrostServer, ServerError};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct ExactMatcher {
        pattern: String,
    }
    impl Matcher for ExactMatcher {
        fn matches(&self, key: &str) -> bool {
            key == self.pattern
        }
        fn pattern(&self) -> &str {
            &self.pattern
        }
    }

    struct TestBackend {
        name: String,
        addr: String,
        metrics: Arc<AtomicBackendMetrics>,
    }

    #[async_trait::async_trait]
    impl Backend for TestBackend {
        async fn connect(&self) -> Result<tokio::net::TcpStream, BackendError> {
            let s = tokio::net::TcpStream::connect(&self.addr)
                .await
                .map_err(|e| BackendError::ConnectionFailed(e.to_string()))?;
            Ok(s)
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn server(&self) -> &str {
            &self.addr
        }
        fn uses_connection_pool(&self) -> bool {
            false
        }
        async fn get_pooled_stream(
            &self,
        ) -> Result<
            bb8::PooledConnection<'_, crate::core::connection_pool::MemcachedConnectionManager>,
            BackendError,
        > {
            Err(BackendError::NoConnectionPool)
        }
        fn metrics(&self) -> Arc<AtomicBackendMetrics> {
            Arc::clone(&self.metrics)
        }
    }

    #[test]
    fn test_extract_key_from_request_variants() {
        assert_eq!(
            super::extract_key_from_request("get k\r\n"),
            Some("k".to_string())
        );
        assert_eq!(
            super::extract_key_from_request("get   k   \r\n"),
            Some("k".to_string())
        );
        assert_eq!(
            super::extract_key_from_request("set key 0 0 1\r\n"),
            Some("key".to_string())
        );
        assert_eq!(super::extract_key_from_request("stats\r\n"), None);
        assert_eq!(super::extract_key_from_request(""), None);
    }

    #[tokio::test]
    async fn test_optimize_client_socket_sets_nodelay() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Connect a client
        let client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (_server_side, _peer) = tokio::join!(
            async {
                // Accept to complete handshake
                let (stream, _) = listener.accept().await.unwrap();
                stream
            },
            async { Ok::<(), ()>(()) }
        );

        super::optimize_client_socket(&client);
        assert!(client.nodelay().unwrap());
    }

    #[tokio::test]
    async fn test_proxy_bidirectional_echo() {
        // Client side pair
        let client_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_listener.local_addr().unwrap();
        let client_peer_fut =
            tokio::spawn(async move { tokio::net::TcpStream::connect(client_addr).await.unwrap() });
        let (mut client_socket, _) = client_listener.accept().await.unwrap();
        let mut client_peer = client_peer_fut.await.unwrap();

        // Backend side pair
        let backend_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap();
        let mut backend_socket = tokio::net::TcpStream::connect(backend_addr).await.unwrap();
        let (mut backend_peer, _) = backend_listener.accept().await.unwrap();

        // Simple echo on backend peer: read once and write back
        let echo_task = tokio::spawn(async move {
            let mut buf = vec![0u8; 64];
            let n = backend_peer.read(&mut buf).await.unwrap();
            backend_peer.write_all(&buf[..n]).await.unwrap();
            backend_peer.flush().await.unwrap();
        });

        // Run proxy
        let proxy_task = tokio::spawn(async move {
            super::proxy_bidirectional(&mut client_socket, &mut backend_socket)
                .await
                .unwrap();
        });

        // Send from client and expect echo
        client_peer.write_all(b"ping\r\n").await.unwrap();
        client_peer.flush().await.unwrap();

        let mut out = vec![0u8; 64];
        let n = client_peer.read(&mut out).await.unwrap();
        assert_eq!(&out[..n], b"ping\r\n");

        // Close client side to let proxy finish
        drop(client_peer);
        let _ = echo_task.await;
        let _ = proxy_task.await;
    }

    #[tokio::test]
    async fn test_handle_stats_command_outputs_end() {
        // Build route table with one backend and one pool
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        // Accept and immediately close (we won't actually connect in this test)
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                drop(stream);
            }
        });

        let backend = Arc::new(TestBackend {
            name: "b1".into(),
            addr: addr.clone(),
            metrics: Arc::new(AtomicBackendMetrics::new("b1".into())),
        }) as Arc<dyn Backend>;
        let pool_backends = vec![Box::new(TestBackend {
            name: "b2".into(),
            addr,
            metrics: Arc::new(AtomicBackendMetrics::new("b2".into())),
        }) as Box<dyn Backend>];
        let pool = Arc::new(BasicPool::new(
            "p1".into(),
            pool_backends,
            Box::new(BlindForwardStrategy::new()),
        ));

        let routes = vec![
            Route {
                matcher: Box::new(ExactMatcher {
                    pattern: "*".into(),
                }),
                target: ResolvedTarget::Backend(backend),
            },
            Route {
                matcher: Box::new(ExactMatcher {
                    pattern: "*".into(),
                }),
                target: ResolvedTarget::Pool(pool),
            },
        ];
        let table = Arc::new(RouteTable::new(routes));

        // Client socket
        let listener_client = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_client = listener_client.local_addr().unwrap();
        let client =
            tokio::spawn(async move { tokio::net::TcpStream::connect(addr_client).await.unwrap() });
        let (server_side, _) = listener_client.accept().await.unwrap();
        let mut client_peer = client.await.unwrap();

        // Call stats handler
        super::handle_stats_command(server_side, &table)
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client_peer.read(&mut buf).await.unwrap();
        let s = String::from_utf8_lossy(&buf[..n]);
        assert!(s.contains("STAT version"));
        assert!(s.contains("END"));
    }

    #[tokio::test]
    async fn test_handle_connection_with_routing_forwards_and_responds() {
        // Backend side to echo END
        let backend_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (mut s, _) = backend_listener.accept().await.unwrap();
            let mut line = String::new();
            use tokio::io::AsyncBufReadExt;
            let mut r = tokio::io::BufReader::new(&mut s);
            let _ = r.read_line(&mut line).await.unwrap();
            use tokio::io::AsyncWriteExt;
            s.write_all(b"END\r\n").await.unwrap();
            s.flush().await.unwrap();
        });

        // Route table with exact matcher and test backend
        let test_backend = Arc::new(TestBackend {
            name: "b1".into(),
            addr: backend_addr,
            metrics: Arc::new(AtomicBackendMetrics::new("b1".into())),
        }) as Arc<dyn Backend>;
        let routes = vec![Route {
            matcher: Box::new(ExactMatcher {
                pattern: "key".into(),
            }),
            target: ResolvedTarget::Backend(test_backend),
        }];
        let table = Arc::new(RouteTable::new(routes));

        // Client pair
        let client_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_listener.local_addr().unwrap();
        let client_peer_task =
            tokio::spawn(async move { tokio::net::TcpStream::connect(client_addr).await.unwrap() });
        let (client_stream_for_handler, _) = client_listener.accept().await.unwrap();
        let mut client_peer = client_peer_task.await.unwrap();

        // Spawn handler
        let table_clone = Arc::clone(&table);
        let proto = Arc::new(AsciiProtocol::new()) as Arc<dyn crate::core::Protocol>;
        let h = tokio::spawn(async move {
            super::handle_connection_with_routing(client_stream_for_handler, table_clone, proto)
                .await
                .unwrap();
        });

        client_peer.write_all(b"get key\r\n").await.unwrap();
        client_peer.flush().await.unwrap();
        let mut out = [0u8; 64];
        let n = client_peer.read(&mut out).await.unwrap();
        assert!(String::from_utf8_lossy(&out[..n]).contains("END"));
        drop(client_peer);
        let _ = h.await;
    }

    #[tokio::test]
    async fn test_start_no_listeners() {
        // Empty config -> new() ok, start() should return NoListeners immediately
        let cfg = Config { listeners: std::collections::HashMap::new(), backends: std::collections::HashMap::new(), pools: std::collections::HashMap::new(), routes: std::collections::HashMap::new() };
        let server = BifrostServer::new(cfg).await.unwrap();
        let err = server.start().await.err().unwrap();
        match err { ServerError::NoListeners => (), _ => panic!("expected NoListeners") }
    }

    #[tokio::test]
    async fn test_start_bind_failed_invalid_address() {
        // Invalid bind address should fail to bind
        let mut listeners = std::collections::HashMap::new();
        listeners.insert("bad".into(), ListenerConfig { bind: "256.256.256.256:12345".into() });
        let cfg = Config { listeners, backends: std::collections::HashMap::new(), pools: std::collections::HashMap::new(), routes: std::collections::HashMap::new() };
        let server = BifrostServer::new(cfg).await.unwrap();
        let err = server.start().await.err().unwrap();
        match err { ServerError::BindFailed(_) => (), _ => panic!("expected BindFailed") }
    }

    #[tokio::test]
    async fn test_handle_connection_empty_first_line_ok() {
        // Create connected pair and immediately close client writer to produce EOF
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = tokio::spawn(async move { tokio::net::TcpStream::connect(addr).await.unwrap() });
        let (server_side, _) = listener.accept().await.unwrap();
        let mut client_peer = client.await.unwrap();
        // Close client to make server see empty line
        client_peer.shutdown().await.unwrap();

        // Empty route table is fine; handler should early-return Ok when first_line is empty
        let table = Arc::new(RouteTable::new(Vec::new()));
        let proto = Arc::new(AsciiProtocol::new()) as Arc<dyn crate::core::Protocol>;
        let res = super::handle_connection_with_routing(server_side, table, proto).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_no_routes_error() {
        // Backend listener for completeness (won't be used)
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { let _ = listener.accept().await; });

        // Route table with a key that won't match
        let routes = vec![Route { matcher: Box::new(ExactMatcher { pattern: "matchme".into() }), target: ResolvedTarget::Backend(Arc::new(TestBackend { name: "b".into(), addr: addr.to_string(), metrics: Arc::new(AtomicBackendMetrics::new("b".into())) }) as Arc<dyn Backend>) }];
        let table = Arc::new(RouteTable::new(routes));

        // Client pair
        let client_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_listener.local_addr().unwrap();
        let client = tokio::spawn(async move { tokio::net::TcpStream::connect(client_addr).await.unwrap() });
        let (server_side, _) = client_listener.accept().await.unwrap();
        let mut client_peer = client.await.unwrap();

        // Write a request that won't match any route
        client_peer.write_all(b"get otherkey\r\n").await.unwrap();
        client_peer.flush().await.unwrap();

        let proto = Arc::new(AsciiProtocol::new()) as Arc<dyn crate::core::Protocol>;
        let err = super::handle_connection_with_routing(server_side, table, proto).await.err().unwrap();
        match err { ServerError::NoRoutes => (), _ => panic!("expected NoRoutes") }
    }
}
