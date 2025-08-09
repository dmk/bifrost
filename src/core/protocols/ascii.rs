use super::{Protocol, ProtocolError};
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// ASCII protocol command types
#[derive(Debug, Clone, PartialEq)]
pub enum AsciiCommand {
    Get {
        keys: Vec<String>,
    },
    Set {
        key: String,
        flags: u32,
        exptime: u32,
        bytes: usize,
        noreply: bool,
    },
    Add {
        key: String,
        flags: u32,
        exptime: u32,
        bytes: usize,
        noreply: bool,
    },
    Replace {
        key: String,
        flags: u32,
        exptime: u32,
        bytes: usize,
        noreply: bool,
    },
    Delete {
        key: String,
        noreply: bool,
    },
    Incr {
        key: String,
        value: u64,
        noreply: bool,
    },
    Decr {
        key: String,
        value: u64,
        noreply: bool,
    },
    FlushAll {
        exptime: Option<u32>,
        noreply: bool,
    },
    Stats {
        args: Option<String>,
    },
    Version,
    Quit,
}

/// ASCII protocol response types
#[derive(Debug, Clone, PartialEq)]
pub enum AsciiResponse {
    Value {
        key: String,
        flags: u32,
        bytes: usize,
        data: Vec<u8>,
    },
    Stored,
    NotStored,
    Exists,
    NotFound,
    Deleted,
    Touched,
    Ok,
    Error(String),
    ClientError(String),
    ServerError(String),
    Stat {
        key: String,
        value: String,
    },
    Version(String),
    End,
}

impl AsciiCommand {
    /// Parse an ASCII command from a line
    pub fn parse(line: &str) -> Result<Self, ProtocolError> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return Err(ProtocolError::ParseError("Empty command".to_string()));
        }

        let command = parts[0].to_uppercase();
        match command.as_str() {
            "GET" => {
                if parts.len() < 2 {
                    return Err(ProtocolError::ParseError(
                        "GET requires at least one key".to_string(),
                    ));
                }
                let keys = parts[1..].iter().map(|s| s.to_string()).collect();
                Ok(AsciiCommand::Get { keys })
            }
            "SET" | "ADD" | "REPLACE" => {
                if parts.len() < 5 {
                    return Err(ProtocolError::ParseError(format!(
                        "{} requires key, flags, exptime, bytes",
                        command
                    )));
                }
                let key = parts[1].to_string();
                let flags = parts[2]
                    .parse::<u32>()
                    .map_err(|_| ProtocolError::ParseError("Invalid flags".to_string()))?;
                let exptime = parts[3]
                    .parse::<u32>()
                    .map_err(|_| ProtocolError::ParseError("Invalid exptime".to_string()))?;
                let bytes = parts[4]
                    .parse::<usize>()
                    .map_err(|_| ProtocolError::ParseError("Invalid bytes".to_string()))?;
                let noreply = parts.len() > 5 && parts[5] == "noreply";

                match command.as_str() {
                    "SET" => Ok(AsciiCommand::Set {
                        key,
                        flags,
                        exptime,
                        bytes,
                        noreply,
                    }),
                    "ADD" => Ok(AsciiCommand::Add {
                        key,
                        flags,
                        exptime,
                        bytes,
                        noreply,
                    }),
                    "REPLACE" => Ok(AsciiCommand::Replace {
                        key,
                        flags,
                        exptime,
                        bytes,
                        noreply,
                    }),
                    _ => unreachable!(),
                }
            }
            "DELETE" => {
                if parts.len() < 2 {
                    return Err(ProtocolError::ParseError("DELETE requires key".to_string()));
                }
                let key = parts[1].to_string();
                let noreply = parts.len() > 2 && parts[2] == "noreply";
                Ok(AsciiCommand::Delete { key, noreply })
            }
            "INCR" | "DECR" => {
                if parts.len() < 3 {
                    return Err(ProtocolError::ParseError(format!(
                        "{} requires key and value",
                        command
                    )));
                }
                let key = parts[1].to_string();
                let value = parts[2]
                    .parse::<u64>()
                    .map_err(|_| ProtocolError::ParseError("Invalid value".to_string()))?;
                let noreply = parts.len() > 3 && parts[3] == "noreply";

                match command.as_str() {
                    "INCR" => Ok(AsciiCommand::Incr {
                        key,
                        value,
                        noreply,
                    }),
                    "DECR" => Ok(AsciiCommand::Decr {
                        key,
                        value,
                        noreply,
                    }),
                    _ => unreachable!(),
                }
            }
            "FLUSH_ALL" => {
                let exptime =
                    if parts.len() > 1 && parts[1] != "noreply" {
                        Some(parts[1].parse::<u32>().map_err(|_| {
                            ProtocolError::ParseError("Invalid exptime".to_string())
                        })?)
                    } else {
                        None
                    };
                let noreply = parts.len() > 1 && parts[parts.len() - 1] == "noreply";
                Ok(AsciiCommand::FlushAll { exptime, noreply })
            }
            "STATS" => {
                let args = if parts.len() > 1 {
                    Some(parts[1..].join(" "))
                } else {
                    None
                };
                Ok(AsciiCommand::Stats { args })
            }
            "VERSION" => Ok(AsciiCommand::Version),
            "QUIT" => Ok(AsciiCommand::Quit),
            _ => Err(ProtocolError::ParseError(format!(
                "Unknown command: {}",
                command
            ))),
        }
    }

    /// Extract the primary key from this command (for routing)
    pub fn primary_key(&self) -> Option<&str> {
        match self {
            AsciiCommand::Get { keys } => keys.first().map(|s| s.as_str()),
            AsciiCommand::Set { key, .. } => Some(key),
            AsciiCommand::Add { key, .. } => Some(key),
            AsciiCommand::Replace { key, .. } => Some(key),
            AsciiCommand::Delete { key, .. } => Some(key),
            AsciiCommand::Incr { key, .. } => Some(key),
            AsciiCommand::Decr { key, .. } => Some(key),
            _ => None,
        }
    }

    /// Check if this command expects a response
    pub fn expects_response(&self) -> bool {
        match self {
            AsciiCommand::Set { noreply, .. } => !noreply,
            AsciiCommand::Add { noreply, .. } => !noreply,
            AsciiCommand::Replace { noreply, .. } => !noreply,
            AsciiCommand::Delete { noreply, .. } => !noreply,
            AsciiCommand::Incr { noreply, .. } => !noreply,
            AsciiCommand::Decr { noreply, .. } => !noreply,
            AsciiCommand::FlushAll { noreply, .. } => !noreply,
            AsciiCommand::Quit => false,
            _ => true,
        }
    }
}

impl AsciiResponse {
    /// Parse an ASCII response from a line
    pub fn parse(line: &str) -> Result<Self, ProtocolError> {
        let line = line.trim();

        if line.starts_with("VALUE ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return Err(ProtocolError::ParseError(
                    "Invalid VALUE response".to_string(),
                ));
            }
            let key = parts[1].to_string();
            let flags = parts[2]
                .parse::<u32>()
                .map_err(|_| ProtocolError::ParseError("Invalid flags".to_string()))?;
            let bytes = parts[3]
                .parse::<usize>()
                .map_err(|_| ProtocolError::ParseError("Invalid bytes".to_string()))?;
            Ok(AsciiResponse::Value {
                key,
                flags,
                bytes,
                data: Vec::new(),
            })
        } else {
            match line {
                "STORED" => Ok(AsciiResponse::Stored),
                "NOT_STORED" => Ok(AsciiResponse::NotStored),
                "EXISTS" => Ok(AsciiResponse::Exists),
                "NOT_FOUND" => Ok(AsciiResponse::NotFound),
                "DELETED" => Ok(AsciiResponse::Deleted),
                "TOUCHED" => Ok(AsciiResponse::Touched),
                "OK" => Ok(AsciiResponse::Ok),
                "END" => Ok(AsciiResponse::End),
                _ if line.starts_with("ERROR") => Ok(AsciiResponse::Error(line.to_string())),
                _ if line.starts_with("CLIENT_ERROR") => {
                    Ok(AsciiResponse::ClientError(line.to_string()))
                }
                _ if line.starts_with("SERVER_ERROR") => {
                    Ok(AsciiResponse::ServerError(line.to_string()))
                }
                _ if line.starts_with("STAT ") => {
                    let parts: Vec<&str> = line.splitn(3, ' ').collect();
                    if parts.len() >= 3 {
                        Ok(AsciiResponse::Stat {
                            key: parts[1].to_string(),
                            value: parts[2].to_string(),
                        })
                    } else {
                        Err(ProtocolError::ParseError(
                            "Invalid STAT response".to_string(),
                        ))
                    }
                }
                _ if line.starts_with("VERSION ") => {
                    let version = line.strip_prefix("VERSION ").unwrap_or("").to_string();
                    Ok(AsciiResponse::Version(version))
                }
                _ => Err(ProtocolError::ParseError(format!(
                    "Unknown response: {}",
                    line
                ))),
            }
        }
    }

    /// Format this response as ASCII protocol string
    pub fn format(&self) -> String {
        match self {
            AsciiResponse::Value {
                key, flags, bytes, ..
            } => {
                format!("VALUE {} {} {}\r\n", key, flags, bytes)
            }
            AsciiResponse::Stored => "STORED\r\n".to_string(),
            AsciiResponse::NotStored => "NOT_STORED\r\n".to_string(),
            AsciiResponse::Exists => "EXISTS\r\n".to_string(),
            AsciiResponse::NotFound => "NOT_FOUND\r\n".to_string(),
            AsciiResponse::Deleted => "DELETED\r\n".to_string(),
            AsciiResponse::Touched => "TOUCHED\r\n".to_string(),
            AsciiResponse::Ok => "OK\r\n".to_string(),
            AsciiResponse::End => "END\r\n".to_string(),
            AsciiResponse::Error(msg) => format!("ERROR {}\r\n", msg),
            AsciiResponse::ClientError(msg) => format!("CLIENT_ERROR {}\r\n", msg),
            AsciiResponse::ServerError(msg) => format!("SERVER_ERROR {}\r\n", msg),
            AsciiResponse::Stat { key, value } => format!("STAT {} {}\r\n", key, value),
            AsciiResponse::Version(version) => format!("VERSION {}\r\n", version),
        }
    }
}

/// ASCII protocol handler - parses memcached ASCII protocol
#[derive(Debug)]
pub struct AsciiProtocol {
    pub name: String,
}

impl AsciiProtocol {
    pub fn new() -> Self {
        Self {
            name: "ascii".to_string(),
        }
    }

    /// Parse a complete ASCII request from the client
    async fn parse_request(
        &self,
        client: &mut BufReader<TcpStream>,
    ) -> Result<(AsciiCommand, Vec<u8>), ProtocolError> {
        let mut line = String::new();
        client.read_line(&mut line).await.map_err(|e| {
            ProtocolError::ParseError(format!("Failed to read command line: {}", e))
        })?;

        if line.is_empty() {
            return Err(ProtocolError::ParseError("Connection closed".to_string()));
        }

        let command = AsciiCommand::parse(&line)?;

        // For storage commands, we need to read the data block
        let data = match &command {
            AsciiCommand::Set { bytes, .. }
            | AsciiCommand::Add { bytes, .. }
            | AsciiCommand::Replace { bytes, .. } => {
                let mut data = vec![0u8; *bytes];
                client.read_exact(&mut data).await.map_err(|e| {
                    ProtocolError::ParseError(format!("Failed to read data block: {}", e))
                })?;

                // Read the trailing \r\n
                let mut trailing = [0u8; 2];
                client.read_exact(&mut trailing).await.map_err(|e| {
                    ProtocolError::ParseError(format!("Failed to read trailing CRLF: {}", e))
                })?;

                data
            }
            _ => Vec::new(),
        };

        Ok((command, data))
    }

    /// Forward a request to the backend and return the response
    async fn forward_request(
        &self,
        command: &AsciiCommand,
        data: &[u8],
        backend: &mut TcpStream,
    ) -> Result<Vec<u8>, ProtocolError> {
        // Reconstruct the original request
        let request = match command {
            AsciiCommand::Get { keys } => {
                format!("GET {}\r\n", keys.join(" "))
            }
            AsciiCommand::Set {
                key,
                flags,
                exptime,
                bytes,
                noreply,
            } => {
                format!(
                    "SET {} {} {} {}{}\r\n",
                    key,
                    flags,
                    exptime,
                    bytes,
                    if *noreply { " noreply" } else { "" }
                )
            }
            AsciiCommand::Add {
                key,
                flags,
                exptime,
                bytes,
                noreply,
            } => {
                format!(
                    "ADD {} {} {} {}{}\r\n",
                    key,
                    flags,
                    exptime,
                    bytes,
                    if *noreply { " noreply" } else { "" }
                )
            }
            AsciiCommand::Replace {
                key,
                flags,
                exptime,
                bytes,
                noreply,
            } => {
                format!(
                    "REPLACE {} {} {} {}{}\r\n",
                    key,
                    flags,
                    exptime,
                    bytes,
                    if *noreply { " noreply" } else { "" }
                )
            }
            AsciiCommand::Delete { key, noreply } => {
                format!(
                    "DELETE {}{}\r\n",
                    key,
                    if *noreply { " noreply" } else { "" }
                )
            }
            AsciiCommand::Incr {
                key,
                value,
                noreply,
            } => {
                format!(
                    "INCR {} {}{}\r\n",
                    key,
                    value,
                    if *noreply { " noreply" } else { "" }
                )
            }
            AsciiCommand::Decr {
                key,
                value,
                noreply,
            } => {
                format!(
                    "DECR {} {}{}\r\n",
                    key,
                    value,
                    if *noreply { " noreply" } else { "" }
                )
            }
            AsciiCommand::FlushAll { exptime, noreply } => match exptime {
                Some(exp) => format!(
                    "FLUSH_ALL {}{}\r\n",
                    exp,
                    if *noreply { " noreply" } else { "" }
                ),
                None => format!("FLUSH_ALL{}\r\n", if *noreply { " noreply" } else { "" }),
            },
            AsciiCommand::Stats { args } => match args {
                Some(a) => format!("STATS {}\r\n", a),
                None => "STATS\r\n".to_string(),
            },
            AsciiCommand::Version => "VERSION\r\n".to_string(),
            AsciiCommand::Quit => "QUIT\r\n".to_string(),
        };

        // Write the request
        backend.write_all(request.as_bytes()).await.map_err(|e| {
            ProtocolError::ForwardingFailed(format!("Failed to write request: {}", e))
        })?;

        // Write data if present
        if !data.is_empty() {
            backend.write_all(data).await.map_err(|e| {
                ProtocolError::ForwardingFailed(format!("Failed to write data: {}", e))
            })?;
            backend.write_all(b"\r\n").await.map_err(|e| {
                ProtocolError::ForwardingFailed(format!("Failed to write trailing CRLF: {}", e))
            })?;
        }

        backend
            .flush()
            .await
            .map_err(|e| ProtocolError::ForwardingFailed(format!("Failed to flush: {}", e)))?;

        // Read response if expected
        if command.expects_response() {
            self.read_response(backend).await
        } else {
            Ok(Vec::new())
        }
    }

    /// Read a complete response from the backend
    async fn read_response(&self, backend: &mut TcpStream) -> Result<Vec<u8>, ProtocolError> {
        let mut response = Vec::new();
        let mut backend_reader = BufReader::new(backend);

        loop {
            let mut line = String::new();
            let bytes_read = backend_reader.read_line(&mut line).await.map_err(|e| {
                ProtocolError::ForwardingFailed(format!("Failed to read response line: {}", e))
            })?;

            if bytes_read == 0 {
                return Err(ProtocolError::ForwardingFailed(
                    "Backend closed connection".to_string(),
                ));
            }

            response.extend(line.as_bytes());

            // Check if this is a VALUE response that needs data
            if line.starts_with("VALUE ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let bytes = parts[3].parse::<usize>().map_err(|_| {
                        ProtocolError::ParseError("Invalid bytes in VALUE response".to_string())
                    })?;

                    // Read the data block
                    let mut data = vec![0u8; bytes];
                    backend_reader.read_exact(&mut data).await.map_err(|e| {
                        ProtocolError::ForwardingFailed(format!("Failed to read value data: {}", e))
                    })?;
                    response.extend(&data);

                    // Read the trailing \r\n
                    let mut trailing = [0u8; 2];
                    backend_reader
                        .read_exact(&mut trailing)
                        .await
                        .map_err(|e| {
                            ProtocolError::ForwardingFailed(format!(
                                "Failed to read trailing CRLF: {}",
                                e
                            ))
                        })?;
                    response.extend(&trailing);
                }
            }

            // Check for end conditions
            let line_trimmed = line.trim();
            if line_trimmed == "END"
                || line_trimmed == "STORED"
                || line_trimmed == "NOT_STORED"
                || line_trimmed == "EXISTS"
                || line_trimmed == "NOT_FOUND"
                || line_trimmed == "DELETED"
                || line_trimmed == "TOUCHED"
                || line_trimmed == "OK"
                || line_trimmed.starts_with("ERROR")
                || line_trimmed.starts_with("CLIENT_ERROR")
                || line_trimmed.starts_with("SERVER_ERROR")
                || line_trimmed.starts_with("VERSION ")
                || line_trimmed.parse::<u64>().is_ok()
            // INCR/DECR response
            {
                break;
            }
        }

        Ok(response)
    }
}

impl Default for AsciiProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for AsciiProtocol {
    async fn handle_connection(
        &self,
        client: TcpStream,
        mut backend: TcpStream,
    ) -> Result<(), ProtocolError> {
        let mut client_reader = BufReader::new(client);

        loop {
            // Parse the request from the client
            let (command, data) = match self.parse_request(&mut client_reader).await {
                Ok(result) => result,
                Err(ProtocolError::ParseError(msg)) if msg.contains("Connection closed") => {
                    tracing::debug!("Client connection closed");
                    break;
                }
                Err(e) => return Err(e),
            };

            tracing::debug!("Parsed command: {:?}", command);

            // Handle QUIT command locally
            if matches!(command, AsciiCommand::Quit) {
                tracing::debug!("Client requested quit");
                break;
            }

            // Forward the request to the backend
            let response = self.forward_request(&command, &data, &mut backend).await?;

            // Send response back to client (if any)
            if !response.is_empty() {
                let client_stream = client_reader.get_mut();
                client_stream.write_all(&response).await.map_err(|e| {
                    ProtocolError::ForwardingFailed(format!(
                        "Failed to write response to client: {}",
                        e
                    ))
                })?;
                client_stream.flush().await.map_err(|e| {
                    ProtocolError::ForwardingFailed(format!(
                        "Failed to flush client response: {}",
                        e
                    ))
                })?;
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn test_get_command_parsing() {
        // Single key GET
        let result = AsciiCommand::parse("GET mykey\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Get {
                keys: vec!["mykey".to_string()]
            }
        );

        // Multi-key GET
        let result = AsciiCommand::parse("GET key1 key2 key3\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Get {
                keys: vec!["key1".to_string(), "key2".to_string(), "key3".to_string()]
            }
        );

        // Case insensitive
        let result = AsciiCommand::parse("get mykey\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Get {
                keys: vec!["mykey".to_string()]
            }
        );

        // Error: no key
        assert!(AsciiCommand::parse("GET\r\n").is_err());
    }

    #[test]
    fn test_set_command_parsing() {
        // Basic SET
        let result = AsciiCommand::parse("SET mykey 0 300 5\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Set {
                key: "mykey".to_string(),
                flags: 0,
                exptime: 300,
                bytes: 5,
                noreply: false
            }
        );

        // SET with noreply
        let result = AsciiCommand::parse("SET mykey 123 600 10 noreply\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Set {
                key: "mykey".to_string(),
                flags: 123,
                exptime: 600,
                bytes: 10,
                noreply: true
            }
        );

        // Case insensitive
        let result = AsciiCommand::parse("set mykey 0 0 5\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Set {
                key: "mykey".to_string(),
                flags: 0,
                exptime: 0,
                bytes: 5,
                noreply: false
            }
        );

        // Error: missing parameters
        assert!(AsciiCommand::parse("SET mykey 0 300\r\n").is_err());
        assert!(AsciiCommand::parse("SET mykey\r\n").is_err());
    }

    #[test]
    fn test_add_replace_command_parsing() {
        // ADD command
        let result = AsciiCommand::parse("ADD newkey 0 3600 8\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Add {
                key: "newkey".to_string(),
                flags: 0,
                exptime: 3600,
                bytes: 8,
                noreply: false
            }
        );

        // REPLACE command with noreply
        let result = AsciiCommand::parse("REPLACE oldkey 456 0 12 noreply\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Replace {
                key: "oldkey".to_string(),
                flags: 456,
                exptime: 0,
                bytes: 12,
                noreply: true
            }
        );
    }

    #[test]
    fn test_delete_command_parsing() {
        // Basic DELETE
        let result = AsciiCommand::parse("DELETE mykey\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Delete {
                key: "mykey".to_string(),
                noreply: false
            }
        );

        // DELETE with noreply
        let result = AsciiCommand::parse("DELETE mykey noreply\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Delete {
                key: "mykey".to_string(),
                noreply: true
            }
        );

        // Case insensitive
        let result = AsciiCommand::parse("delete mykey\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Delete {
                key: "mykey".to_string(),
                noreply: false
            }
        );

        // Error: no key
        assert!(AsciiCommand::parse("DELETE\r\n").is_err());
    }

    #[test]
    fn test_incr_decr_command_parsing() {
        // INCR command
        let result = AsciiCommand::parse("INCR counter 5\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Incr {
                key: "counter".to_string(),
                value: 5,
                noreply: false
            }
        );

        // DECR command with noreply
        let result = AsciiCommand::parse("DECR counter 3 noreply\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Decr {
                key: "counter".to_string(),
                value: 3,
                noreply: true
            }
        );

        // Error: missing value
        assert!(AsciiCommand::parse("INCR counter\r\n").is_err());

        // Error: invalid value
        assert!(AsciiCommand::parse("INCR counter abc\r\n").is_err());
    }

    #[test]
    fn test_flush_all_command_parsing() {
        // Basic FLUSH_ALL
        let result = AsciiCommand::parse("FLUSH_ALL\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::FlushAll {
                exptime: None,
                noreply: false
            }
        );

        // FLUSH_ALL with exptime
        let result = AsciiCommand::parse("FLUSH_ALL 300\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::FlushAll {
                exptime: Some(300),
                noreply: false
            }
        );

        // FLUSH_ALL with noreply
        let result = AsciiCommand::parse("FLUSH_ALL noreply\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::FlushAll {
                exptime: None,
                noreply: true
            }
        );

        // FLUSH_ALL with exptime and noreply
        let result = AsciiCommand::parse("FLUSH_ALL 600 noreply\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::FlushAll {
                exptime: Some(600),
                noreply: true
            }
        );
    }

    #[test]
    fn test_stats_version_quit_commands() {
        // STATS command
        let result = AsciiCommand::parse("STATS\r\n").unwrap();
        assert_eq!(result, AsciiCommand::Stats { args: None });

        // STATS with arguments
        let result = AsciiCommand::parse("STATS items\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Stats {
                args: Some("items".to_string())
            }
        );

        // VERSION command
        let result = AsciiCommand::parse("VERSION\r\n").unwrap();
        assert_eq!(result, AsciiCommand::Version);

        // QUIT command
        let result = AsciiCommand::parse("QUIT\r\n").unwrap();
        assert_eq!(result, AsciiCommand::Quit);
    }

    #[test]
    fn test_unknown_command() {
        assert!(AsciiCommand::parse("UNKNOWN_COMMAND\r\n").is_err());
        assert!(AsciiCommand::parse("INVALID key value\r\n").is_err());
    }

    #[test]
    fn test_empty_command() {
        assert!(AsciiCommand::parse("").is_err());
        assert!(AsciiCommand::parse("\r\n").is_err());
        assert!(AsciiCommand::parse("   \r\n").is_err());
    }

    #[test]
    fn test_primary_key_extraction() {
        // GET command - first key
        let cmd = AsciiCommand::Get {
            keys: vec!["key1".to_string(), "key2".to_string()],
        };
        assert_eq!(cmd.primary_key(), Some("key1"));

        // Storage commands
        let cmd = AsciiCommand::Set {
            key: "mykey".to_string(),
            flags: 0,
            exptime: 0,
            bytes: 5,
            noreply: false,
        };
        assert_eq!(cmd.primary_key(), Some("mykey"));

        let cmd = AsciiCommand::Delete {
            key: "delkey".to_string(),
            noreply: false,
        };
        assert_eq!(cmd.primary_key(), Some("delkey"));

        // Commands without keys
        let cmd = AsciiCommand::Version;
        assert_eq!(cmd.primary_key(), None);

        let cmd = AsciiCommand::FlushAll {
            exptime: None,
            noreply: false,
        };
        assert_eq!(cmd.primary_key(), None);
    }

    #[test]
    fn test_expects_response() {
        // Commands that expect responses
        assert!(AsciiCommand::Get {
            keys: vec!["key".to_string()]
        }
        .expects_response());
        assert!(AsciiCommand::Version.expects_response());
        assert!(AsciiCommand::Stats { args: None }.expects_response());

        // Commands with noreply=false
        assert!(AsciiCommand::Set {
            key: "key".to_string(),
            flags: 0,
            exptime: 0,
            bytes: 5,
            noreply: false
        }
        .expects_response());

        // Commands with noreply=true
        assert!(!AsciiCommand::Set {
            key: "key".to_string(),
            flags: 0,
            exptime: 0,
            bytes: 5,
            noreply: true
        }
        .expects_response());

        assert!(!AsciiCommand::Delete {
            key: "key".to_string(),
            noreply: true
        }
        .expects_response());

        // QUIT never expects response
        assert!(!AsciiCommand::Quit.expects_response());
    }

    #[test]
    fn test_response_parsing() {
        // VALUE response
        let result = AsciiResponse::parse("VALUE mykey 123 5").unwrap();
        assert_eq!(
            result,
            AsciiResponse::Value {
                key: "mykey".to_string(),
                flags: 123,
                bytes: 5,
                data: Vec::new()
            }
        );

        // Simple responses
        assert_eq!(
            AsciiResponse::parse("STORED").unwrap(),
            AsciiResponse::Stored
        );
        assert_eq!(
            AsciiResponse::parse("NOT_STORED").unwrap(),
            AsciiResponse::NotStored
        );
        assert_eq!(
            AsciiResponse::parse("EXISTS").unwrap(),
            AsciiResponse::Exists
        );
        assert_eq!(
            AsciiResponse::parse("NOT_FOUND").unwrap(),
            AsciiResponse::NotFound
        );
        assert_eq!(
            AsciiResponse::parse("DELETED").unwrap(),
            AsciiResponse::Deleted
        );
        assert_eq!(
            AsciiResponse::parse("TOUCHED").unwrap(),
            AsciiResponse::Touched
        );
        assert_eq!(AsciiResponse::parse("OK").unwrap(), AsciiResponse::Ok);
        assert_eq!(AsciiResponse::parse("END").unwrap(), AsciiResponse::End);

        // Error responses
        assert_eq!(
            AsciiResponse::parse("ERROR").unwrap(),
            AsciiResponse::Error("ERROR".to_string())
        );
        assert_eq!(
            AsciiResponse::parse("CLIENT_ERROR bad command").unwrap(),
            AsciiResponse::ClientError("CLIENT_ERROR bad command".to_string())
        );
        assert_eq!(
            AsciiResponse::parse("SERVER_ERROR out of memory").unwrap(),
            AsciiResponse::ServerError("SERVER_ERROR out of memory".to_string())
        );

        // STAT response
        let result = AsciiResponse::parse("STAT pid 12345").unwrap();
        assert_eq!(
            result,
            AsciiResponse::Stat {
                key: "pid".to_string(),
                value: "12345".to_string()
            }
        );

        // VERSION response
        let result = AsciiResponse::parse("VERSION 1.6.21").unwrap();
        assert_eq!(result, AsciiResponse::Version("1.6.21".to_string()));

        // Invalid VALUE response
        assert!(AsciiResponse::parse("VALUE key").is_err());
        assert!(AsciiResponse::parse("VALUE key abc 5").is_err());

        // Unknown response
        assert!(AsciiResponse::parse("UNKNOWN_RESPONSE").is_err());
    }

    #[test]
    fn test_response_formatting() {
        // VALUE response
        let response = AsciiResponse::Value {
            key: "mykey".to_string(),
            flags: 123,
            bytes: 5,
            data: Vec::new(),
        };
        assert_eq!(response.format(), "VALUE mykey 123 5\r\n");

        // Simple responses
        assert_eq!(AsciiResponse::Stored.format(), "STORED\r\n");
        assert_eq!(AsciiResponse::NotFound.format(), "NOT_FOUND\r\n");
        assert_eq!(AsciiResponse::End.format(), "END\r\n");

        // Error responses
        let response = AsciiResponse::Error("bad command".to_string());
        assert_eq!(response.format(), "ERROR bad command\r\n");

        // STAT response
        let response = AsciiResponse::Stat {
            key: "uptime".to_string(),
            value: "12345".to_string(),
        };
        assert_eq!(response.format(), "STAT uptime 12345\r\n");

        // VERSION response
        let response = AsciiResponse::Version("1.6.21".to_string());
        assert_eq!(response.format(), "VERSION 1.6.21\r\n");
    }

    #[test]
    fn test_edge_cases() {
        // Leading/trailing whitespace
        let result = AsciiCommand::parse("  GET mykey  \r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Get {
                keys: vec!["mykey".to_string()]
            }
        );

        // Multiple spaces between parts
        let result = AsciiCommand::parse("SET   mykey   0   300   5\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Set {
                key: "mykey".to_string(),
                flags: 0,
                exptime: 300,
                bytes: 5,
                noreply: false
            }
        );

        // Keys with special characters (valid memcached keys)
        let result = AsciiCommand::parse("GET my:key-123_test\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Get {
                keys: vec!["my:key-123_test".to_string()]
            }
        );

        // Large numbers
        let result = AsciiCommand::parse("SET key 4294967295 4294967295 1000000\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Set {
                key: "key".to_string(),
                flags: 4294967295,
                exptime: 4294967295,
                bytes: 1000000,
                noreply: false
            }
        );

        // Zero values
        let result = AsciiCommand::parse("INCR counter 0\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Incr {
                key: "counter".to_string(),
                value: 0,
                noreply: false
            }
        );
    }

    #[tokio::test]
    async fn test_parse_request_reads_data_block_and_crlf() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client_task =
            tokio::spawn(async move { tokio::net::TcpStream::connect(addr).await.unwrap() });
        let (server_stream, _) = listener.accept().await.unwrap();
        let mut client_stream = client_task.await.unwrap();

        // Write a SET with 4 bytes and trailing CRLF
        client_stream
            .write_all(b"SET key 0 0 4\r\nDATA\r\n")
            .await
            .unwrap();
        client_stream.flush().await.unwrap();

        let proto = AsciiProtocol::new();
        let mut reader = BufReader::new(server_stream);
        let (cmd, data) = proto.parse_request(&mut reader).await.unwrap();

        assert_eq!(
            cmd,
            AsciiCommand::Set {
                key: "key".into(),
                flags: 0,
                exptime: 0,
                bytes: 4,
                noreply: false
            }
        );
        assert_eq!(data, b"DATA");
    }

    #[tokio::test]
    async fn test_read_response_value_and_end() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();

        // Backend writes a VALUE response with data and END
        let backend_task = tokio::spawn(async move {
            backend_peer
                .write_all(b"VALUE k 0 4\r\nDATA\r\nEND\r\n")
                .await
                .unwrap();
            backend_peer.flush().await.unwrap();
        });

        let proto = AsciiProtocol::new();
        let bytes = proto.read_response(&mut client).await.unwrap();
        assert_eq!(&bytes, b"VALUE k 0 4\r\nDATA\r\nEND\r\n");
        let _ = backend_task.await;
    }

    #[tokio::test]
    async fn test_forward_request_get_roundtrip() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut backend = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();

        // Backend validates request then returns a simple NOT_FOUND
        let server = tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let n = backend_peer.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            assert_eq!(req, "GET key\r\n");
            backend_peer.write_all(b"END\r\n").await.unwrap();
        });

        let proto = AsciiProtocol::new();
        let cmd = AsciiCommand::Get {
            keys: vec!["key".into()],
        };
        let bytes = proto
            .forward_request(&cmd, &[], &mut backend)
            .await
            .unwrap();
        assert_eq!(&bytes, b"END\r\n");
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_forward_request_set_roundtrip_with_data() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut backend = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();

        // Backend validates request line and data block, then returns STORED
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let expected = b"SET key 0 0 4\r\n";
            let mut req_line = vec![0u8; expected.len()];
            backend_peer.read_exact(&mut req_line).await.unwrap();
            assert_eq!(&req_line, expected);

            let mut data = [0u8; 4];
            backend_peer.read_exact(&mut data).await.unwrap();
            assert_eq!(&data, b"DATA");
            let mut crlf = [0u8; 2];
            backend_peer.read_exact(&mut crlf).await.unwrap();
            assert_eq!(&crlf, b"\r\n");

            backend_peer.write_all(b"STORED\r\n").await.unwrap();
        });

        let proto = AsciiProtocol::new();
        let cmd = AsciiCommand::Set {
            key: "key".into(),
            flags: 0,
            exptime: 0,
            bytes: 4,
            noreply: false,
        };
        let bytes = proto
            .forward_request(&cmd, b"DATA", &mut backend)
            .await
            .unwrap();
        assert_eq!(&bytes, b"STORED\r\n");
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_forward_request_set_noreply_does_not_wait_for_response() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut backend = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();

        // Backend just consumes the request; no response is sent
        let server = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let expected = b"SET key 0 0 4\r\n";
            let mut req_line = vec![0u8; expected.len()];
            backend_peer.read_exact(&mut req_line).await.unwrap();
            assert_eq!(&req_line, expected);
            let mut data = [0u8; 4];
            backend_peer.read_exact(&mut data).await.unwrap();
            assert_eq!(&data, b"DATA");
            let mut crlf = [0u8; 2];
            backend_peer.read_exact(&mut crlf).await.unwrap();
            assert_eq!(&crlf, b"\r\n");
        });

        let proto = AsciiProtocol::new();
        let cmd = AsciiCommand::Set {
            key: "key".into(),
            flags: 0,
            exptime: 0,
            bytes: 4,
            noreply: true,
        };
        let bytes = proto
            .forward_request(&cmd, b"DATA", &mut backend)
            .await
            .unwrap();
        assert!(bytes.is_empty());
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_forward_request_incr_numeric_response() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut backend = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();

        // Backend validates request and returns numeric response
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let expected = b"INCR k 2\r\n";
            let mut req_line = vec![0u8; expected.len()];
            backend_peer.read_exact(&mut req_line).await.unwrap();
            assert_eq!(&req_line, expected);
            backend_peer.write_all(b"123\r\n").await.unwrap();
            backend_peer.flush().await.unwrap();
        });

        let proto = AsciiProtocol::new();
        let cmd = AsciiCommand::Incr {
            key: "k".into(),
            value: 2,
            noreply: false,
        };
        let bytes = proto
            .forward_request(&cmd, &[], &mut backend)
            .await
            .unwrap();
        assert_eq!(&bytes, b"123\r\n");
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_read_response_stats_multiline() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();

        let backend_task = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            backend_peer
                .write_all(b"STAT a 1\r\nSTAT b 2\r\nEND\r\n")
                .await
                .unwrap();
            backend_peer.flush().await.unwrap();
        });

        let proto = AsciiProtocol::new();
        let bytes = proto.read_response(&mut client).await.unwrap();
        assert_eq!(&bytes, b"STAT a 1\r\nSTAT b 2\r\nEND\r\n");
        let _ = backend_task.await;
    }

    #[tokio::test]
    async fn test_handle_connection_quit_does_not_forward() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::time::Duration;

        // Prepare client side
        let client_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_listener.local_addr().unwrap();
        let client_peer_task =
            tokio::spawn(async move { tokio::net::TcpStream::connect(client_addr).await.unwrap() });
        let (client_for_handler, _) = client_listener.accept().await.unwrap();
        let mut client_peer = client_peer_task.await.unwrap();

        // Prepare backend side
        let backend_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap();
        let backend_for_handler = tokio::net::TcpStream::connect(backend_addr).await.unwrap();
        let (mut backend_peer, _) = backend_listener.accept().await.unwrap();

        let proto = AsciiProtocol::new();
        let run = tokio::spawn(async move {
            proto
                .handle_connection(client_for_handler, backend_for_handler)
                .await
                .unwrap();
        });

        // Send QUIT to the handler
        client_peer.write_all(b"QUIT\r\n").await.unwrap();
        client_peer.flush().await.unwrap();
        drop(client_peer);

        // Backend connection should be closed without receiving any data
        let mut buf = [0u8; 16];
        let n = tokio::time::timeout(Duration::from_secs(1), backend_peer.read(&mut buf))
            .await
            .expect("timed out waiting for backend EOF")
            .unwrap();
        assert_eq!(n, 0, "backend should see EOF and no data for QUIT");

        // Handler should complete promptly
        let _ = tokio::time::timeout(Duration::from_secs(1), run)
            .await
            .unwrap();
    }

    #[test]
    fn test_protocol_name_ascii() {
        let p = AsciiProtocol::new();
        assert_eq!(p.name(), "ascii");
    }

    #[tokio::test]
    async fn test_parse_request_missing_data_block_errors() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client_task = tokio::spawn(async move {
            let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
            s.write_all(b"SET key 0 0 4\r\nDA").await.unwrap();
            s.shutdown().await.unwrap();
        });
        let (server_stream, _) = listener.accept().await.unwrap();
        let _ = client_task.await;
        let proto = AsciiProtocol::new();
        let mut reader = BufReader::new(server_stream);
        let err = proto.parse_request(&mut reader).await.err().unwrap();
        match err {
            ProtocolError::ParseError(msg) => assert!(msg.contains("Failed to read data block")),
            _ => panic!("expected ParseError for missing data block"),
        }
    }

    #[tokio::test]
    async fn test_parse_request_missing_trailing_crlf_errors() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client_task = tokio::spawn(async move {
            let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
            s.write_all(b"SET key 0 0 4\r\nDATA").await.unwrap();
            s.shutdown().await.unwrap();
        });
        let (server_stream, _) = listener.accept().await.unwrap();
        let _ = client_task.await;
        let proto = AsciiProtocol::new();
        let mut reader = BufReader::new(server_stream);
        let err = proto.parse_request(&mut reader).await.err().unwrap();
        match err {
            ProtocolError::ParseError(msg) => assert!(msg.contains("Failed to read trailing CRLF")),
            _ => panic!("expected ParseError for missing trailing CRLF"),
        }
    }

    #[tokio::test]
    async fn test_read_response_backend_closed_errors() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (backend_peer, _) = listener.accept().await.unwrap();
        drop(backend_peer);
        let proto = AsciiProtocol::new();
        let err = proto.read_response(&mut client).await.err().unwrap();
        match err {
            ProtocolError::ForwardingFailed(msg) => {
                assert!(msg.contains("Backend closed connection"))
            }
            _ => panic!("expected ForwardingFailed for closed backend"),
        }
    }

    #[tokio::test]
    async fn test_read_response_invalid_value_bytes_errors() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();
        let backend_task = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            backend_peer.write_all(b"VALUE k 0 X\r\n").await.unwrap();
            backend_peer.shutdown().await.unwrap();
        });
        let proto = AsciiProtocol::new();
        let err = proto.read_response(&mut client).await.err().unwrap();
        let _ = backend_task.await;
        match err {
            ProtocolError::ParseError(msg) => {
                assert!(msg.contains("Invalid bytes in VALUE response"))
            }
            _ => panic!("expected ParseError for invalid VALUE bytes"),
        }
    }

    #[tokio::test]
    async fn test_forward_request_delete_noreply() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut backend = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();
        let server = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = [0u8; 64];
            let n = backend_peer.read(&mut buf).await.unwrap();
            let s = String::from_utf8_lossy(&buf[..n]).to_string();
            assert_eq!(s, "DELETE key noreply\r\n");
        });
        let proto = AsciiProtocol::new();
        let cmd = AsciiCommand::Delete {
            key: "key".into(),
            noreply: true,
        };
        let bytes = proto
            .forward_request(&cmd, &[], &mut backend)
            .await
            .unwrap();
        assert!(bytes.is_empty());
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_forward_request_flush_all_with_exptime_noreply() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut backend = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();
        let server = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = [0u8; 64];
            let n = backend_peer.read(&mut buf).await.unwrap();
            let s = String::from_utf8_lossy(&buf[..n]).to_string();
            assert_eq!(s, "FLUSH_ALL 10 noreply\r\n");
        });
        let proto = AsciiProtocol::new();
        let cmd = AsciiCommand::FlushAll {
            exptime: Some(10),
            noreply: true,
        };
        let bytes = proto
            .forward_request(&cmd, &[], &mut backend)
            .await
            .unwrap();
        assert!(bytes.is_empty());
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_forward_request_stats_with_args() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut backend = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut backend_peer, _) = listener.accept().await.unwrap();
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 64];
            let n = backend_peer.read(&mut buf).await.unwrap();
            let s = String::from_utf8_lossy(&buf[..n]).to_string();
            assert_eq!(s, "STATS items slabs\r\n");
            backend_peer.write_all(b"END\r\n").await.unwrap();
        });
        let proto = AsciiProtocol::new();
        let cmd = AsciiCommand::Stats {
            args: Some("items slabs".into()),
        };
        let bytes = proto
            .forward_request(&cmd, &[], &mut backend)
            .await
            .unwrap();
        assert_eq!(&bytes, b"END\r\n");
        let _ = server.await;
    }

    #[test]
    fn test_stats_args_join_multiple_words() {
        let result = AsciiCommand::parse("STATS items slabs\r\n").unwrap();
        assert_eq!(
            result,
            AsciiCommand::Stats {
                args: Some("items slabs".to_string())
            }
        );
    }
}
