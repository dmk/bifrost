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
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
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
            "stats" => {
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
                let parts: Vec<&str> = line.trim().split_whitespace().collect();
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
}
