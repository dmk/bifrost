//! Mock Memcached backend for integration testing
//!
//! Provides a configurable mock memcached server that can:
//! - Respond with predefined responses
//! - Simulate failures and timeouts
//! - Record requests for assertions
//! - Simulate latency
//! - Support pipelining

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

/// Response mode for the mock backend
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some variants are for future test scenarios
pub enum ResponseMode {
    /// Normal operation - parse and respond to commands
    Normal,
    /// Always return NOT_FOUND for GET requests
    AlwaysMiss,
    /// Always return STORED for SET requests
    AlwaysSuccess,
    /// Simulate backend failure (close connection)
    Failure,
    /// Simulate slow backend (add latency)
    Slow(Duration),
    /// Custom response for any request
    Custom(String),
}

/// Statistics tracked by the mock backend
#[derive(Debug, Clone, Default)]
pub struct MockStats {
    pub requests: usize,
    pub gets: usize,
    pub sets: usize,
    pub deletes: usize,
    #[allow(dead_code)] // For future use
    pub errors: usize,
}

/// Recorded request for test assertions
#[derive(Debug, Clone)]
#[allow(dead_code)] // For future use in advanced test scenarios
pub struct RecordedRequest {
    pub command: String,
    pub timestamp: std::time::Instant,
}

/// Mock memcached backend
pub struct MockMemcached {
    addr: String,
    response_mode: Arc<Mutex<ResponseMode>>,
    latency: Arc<Mutex<Option<Duration>>>,
    storage: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    stats: Arc<Mutex<MockStats>>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MockMemcached {
    /// Create a new mock memcached instance with default settings
    pub async fn new() -> std::io::Result<Self> {
        MockMemcachedBuilder::new().build().await
    }

    /// Get the address the mock is listening on
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Set the response mode
    #[allow(dead_code)] // For dynamic test scenarios
    pub fn set_response_mode(&self, mode: ResponseMode) {
        *self.response_mode.lock().unwrap() = mode;
    }

    /// Set artificial latency
    #[allow(dead_code)] // For dynamic test scenarios
    pub fn set_latency(&self, duration: Option<Duration>) {
        *self.latency.lock().unwrap() = duration;
    }

    /// Get current statistics
    pub fn stats(&self) -> MockStats {
        self.stats.lock().unwrap().clone()
    }

    /// Get recorded requests
    #[allow(dead_code)] // For advanced test assertions
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Clear recorded requests
    #[allow(dead_code)] // For test cleanup
    pub fn clear_requests(&self) {
        self.requests.lock().unwrap().clear();
    }

    /// Get a value from the mock storage
    pub fn get_stored_value(&self, key: &str) -> Option<Vec<u8>> {
        self.storage.lock().unwrap().get(key).cloned()
    }

    /// Set a value in the mock storage
    #[allow(dead_code)] // For dynamic test data
    pub fn set_stored_value(&self, key: String, value: Vec<u8>) {
        self.storage.lock().unwrap().insert(key, value);
    }

    /// Clear all stored values
    #[allow(dead_code)] // For test cleanup
    pub fn clear_storage(&self) {
        self.storage.lock().unwrap().clear();
    }

    /// Shutdown the mock server
    pub fn shutdown(self) {
        if let Some(tx) = self.shutdown_tx {
            let _ = tx.send(());
        }
    }
}

/// Builder for MockMemcached with fluent configuration
pub struct MockMemcachedBuilder {
    bind_addr: String,
    response_mode: ResponseMode,
    initial_latency: Option<Duration>,
    initial_storage: HashMap<String, Vec<u8>>,
}

impl MockMemcachedBuilder {
    pub fn new() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".to_string(),
            response_mode: ResponseMode::Normal,
            initial_latency: None,
            initial_storage: HashMap::new(),
        }
    }

    #[allow(dead_code)] // For custom port binding in tests
    pub fn bind_addr(mut self, addr: String) -> Self {
        self.bind_addr = addr;
        self
    }

    pub fn response_mode(mut self, mode: ResponseMode) -> Self {
        self.response_mode = mode;
        self
    }

    pub fn latency(mut self, duration: Duration) -> Self {
        self.initial_latency = Some(duration);
        self
    }

    pub fn with_data(mut self, key: String, value: Vec<u8>) -> Self {
        self.initial_storage.insert(key, value);
        self
    }

    pub async fn build(self) -> std::io::Result<MockMemcached> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        let addr = listener.local_addr()?.to_string();

        let response_mode = Arc::new(Mutex::new(self.response_mode));
        let latency = Arc::new(Mutex::new(self.initial_latency));
        let storage = Arc::new(Mutex::new(self.initial_storage));
        let stats = Arc::new(Mutex::new(MockStats::default()));
        let requests = Arc::new(Mutex::new(Vec::new()));

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

        // Spawn the accept loop
        let response_mode_clone = Arc::clone(&response_mode);
        let latency_clone = Arc::clone(&latency);
        let storage_clone = Arc::clone(&storage);
        let stats_clone = Arc::clone(&stats);
        let requests_clone = Arc::clone(&requests);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, _)) => {
                                let response_mode = Arc::clone(&response_mode_clone);
                                let latency = Arc::clone(&latency_clone);
                                let storage = Arc::clone(&storage_clone);
                                let stats = Arc::clone(&stats_clone);
                                let requests = Arc::clone(&requests_clone);

                                tokio::spawn(async move {
                                    let _ = handle_connection(
                                        stream,
                                        response_mode,
                                        latency,
                                        storage,
                                        stats,
                                        requests,
                                    )
                                    .await;
                                });
                            }
                            Err(_) => break,
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        Ok(MockMemcached {
            addr,
            response_mode,
            latency,
            storage,
            stats,
            requests,
            shutdown_tx: Some(shutdown_tx),
        })
    }
}

impl Default for MockMemcachedBuilder {
    fn default() -> Self {
        Self::new()
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    response_mode: Arc<Mutex<ResponseMode>>,
    latency: Arc<Mutex<Option<Duration>>>,
    storage: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    stats: Arc<Mutex<MockStats>>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
) -> std::io::Result<()> {
    let mut buffer = vec![0u8; 4096];

    loop {
        // Apply latency if configured (get value without holding lock across await)
        let delay_duration = *latency.lock().unwrap();
        if let Some(delay) = delay_duration {
            sleep(delay).await;
        }

        // Read command
        let n = match stream.read(&mut buffer).await {
            Ok(0) => break, // Connection closed
            Ok(n) => n,
            Err(_) => break,
        };

        // Make an owned copy of the full request (including any data lines)
        let command_str = String::from_utf8_lossy(&buffer[..n]).to_string();
        let command_str_trimmed = command_str.trim();

        // Record the request
        {
            let mut reqs = requests.lock().unwrap();
            reqs.push(RecordedRequest {
                command: command_str_trimmed.to_string(),
                timestamp: std::time::Instant::now(),
            });
        }

        // Update stats
        {
            let mut stats = stats.lock().unwrap();
            stats.requests += 1;
            if command_str_trimmed.starts_with("GET") || command_str_trimmed.starts_with("get") {
                stats.gets += 1;
            } else if command_str_trimmed.starts_with("SET")
                || command_str_trimmed.starts_with("set")
            {
                stats.sets += 1;
            } else if command_str_trimmed.starts_with("DELETE")
                || command_str_trimmed.starts_with("delete")
            {
                stats.deletes += 1;
            }
        }

        // Determine response based on mode
        let response = {
            let mode = response_mode.lock().unwrap().clone();
            match mode {
                ResponseMode::Failure => {
                    // Close connection to simulate failure
                    break;
                }
                ResponseMode::Custom(resp) => resp,
                ResponseMode::Slow(delay) => {
                    sleep(delay).await;
                    handle_command_normal(command_str_trimmed, &storage, &mut buffer[n..]).await
                }
                ResponseMode::Normal => {
                    handle_command_normal(command_str_trimmed, &storage, &mut buffer[n..]).await
                }
                ResponseMode::AlwaysMiss => {
                    if command_str_trimmed.starts_with("GET")
                        || command_str_trimmed.starts_with("get")
                    {
                        "END\r\n".to_string()
                    } else {
                        handle_command_normal(command_str_trimmed, &storage, &mut buffer[n..]).await
                    }
                }
                ResponseMode::AlwaysSuccess => {
                    if command_str_trimmed.starts_with("SET")
                        || command_str_trimmed.starts_with("set")
                    {
                        "STORED\r\n".to_string()
                    } else {
                        handle_command_normal(command_str_trimmed, &storage, &mut buffer[n..]).await
                    }
                }
            }
        };

        // Send response
        if stream.write_all(response.as_bytes()).await.is_err() {
            break;
        }

        // Handle QUIT
        if command_str_trimmed.to_uppercase().starts_with("QUIT") {
            break;
        }
    }

    Ok(())
}

async fn handle_command_normal(
    command: &str,
    storage: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
    _data_buffer: &mut [u8],
) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return "ERROR\r\n".to_string();
    }

    match parts[0].to_uppercase().as_str() {
        "GET" | "GETS" => {
            if parts.len() < 2 {
                return "ERROR\r\n".to_string();
            }
            let key = parts[1];
            let storage = storage.lock().unwrap();
            if let Some(value) = storage.get(key) {
                format!(
                    "VALUE {} 0 {}\r\n{}\r\nEND\r\n",
                    key,
                    value.len(),
                    String::from_utf8_lossy(value)
                )
            } else {
                "END\r\n".to_string()
            }
        }
        "SET" | "ADD" | "REPLACE" => {
            if parts.len() < 5 {
                return "ERROR\r\n".to_string();
            }
            let key = parts[1];

            // For SET commands, the client sends data after the command line
            // The command format is: SET key flags exptime bytes [noreply]\r\ndata\r\n
            // We extract the data from the command string itself (it includes the data)
            let lines: Vec<&str> = command.lines().collect();
            let value = if lines.len() > 1 {
                // Data is on the second line
                lines[1].as_bytes().to_vec()
            } else {
                // No data provided, use a default
                b"stored".to_vec()
            };

            storage.lock().unwrap().insert(key.to_string(), value);
            "STORED\r\n".to_string()
        }
        "DELETE" => {
            if parts.len() < 2 {
                return "ERROR\r\n".to_string();
            }
            let key = parts[1];
            let mut storage = storage.lock().unwrap();
            if storage.remove(key).is_some() {
                "DELETED\r\n".to_string()
            } else {
                "NOT_FOUND\r\n".to_string()
            }
        }
        "INCR" | "DECR" => {
            if parts.len() < 3 {
                return "ERROR\r\n".to_string();
            }
            "10\r\n".to_string() // Simplified response
        }
        "VERSION" => "VERSION 1.6.21-mock\r\n".to_string(),
        "STATS" => {
            if parts.len() > 1 {
                format!("STAT {} 12345\r\nEND\r\n", parts[1])
            } else {
                "STAT pid 12345\r\nSTAT uptime 3600\r\nEND\r\n".to_string()
            }
        }
        "FLUSH_ALL" => "OK\r\n".to_string(),
        "QUIT" => String::new(),
        _ => "ERROR unknown command\r\n".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_memcached_basic() {
        let mock = MockMemcached::new().await.unwrap();
        assert!(!mock.addr().is_empty());
        mock.shutdown();
    }

    #[tokio::test]
    async fn test_mock_memcached_with_builder() {
        let mock = MockMemcachedBuilder::new()
            .response_mode(ResponseMode::AlwaysMiss)
            .latency(Duration::from_millis(10))
            .with_data("testkey".to_string(), b"testvalue".to_vec())
            .build()
            .await
            .unwrap();

        assert!(mock.get_stored_value("testkey").is_some());
        mock.shutdown();
    }
}
