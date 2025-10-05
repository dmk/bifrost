//! Test client for making memcached requests
//!
//! Provides a simple client for integration tests that can:
//! - Send memcached commands
//! - Receive and parse responses
//! - Handle pipelined requests
//! - Provide typed command builders

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// Test client for memcached protocol
pub struct TestClient {
    stream: BufReader<TcpStream>,
}

impl TestClient {
    /// Connect to a memcached server (or proxy)
    pub async fn connect(addr: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    /// Send a raw command and read the response
    pub async fn send_command(&mut self, command: &str) -> std::io::Result<String> {
        // Ensure command ends with \r\n
        let command = if command.ends_with("\r\n") {
            command.to_string()
        } else {
            format!("{}\r\n", command)
        };

        // Write command
        self.stream.get_mut().write_all(command.as_bytes()).await?;
        self.stream.get_mut().flush().await?;

        // Read response
        self.read_response().await
    }

    /// Read a response from the server
    async fn read_response(&mut self) -> std::io::Result<String> {
        let mut response = String::new();
        let mut line = String::new();

        loop {
            line.clear();
            let n = self.stream.read_line(&mut line).await?;
            if n == 0 {
                break;
            }

            response.push_str(&line);

            // Check for end of response
            if line.starts_with("END")
                || line.starts_with("STORED")
                || line.starts_with("NOT_STORED")
                || line.starts_with("DELETED")
                || line.starts_with("NOT_FOUND")
                || line.starts_with("OK")
                || line.starts_with("ERROR")
                || line.starts_with("CLIENT_ERROR")
                || line.starts_with("SERVER_ERROR")
                || line.starts_with("VERSION")
                || line.starts_with("TOUCHED")
                || line.starts_with("EXISTS")
            {
                break;
            }

            // For VALUE responses, read until END
            if line.starts_with("VALUE") {
                // Read the data line
                line.clear();
                self.stream.read_line(&mut line).await?;
                response.push_str(&line);

                // Read END
                line.clear();
                self.stream.read_line(&mut line).await?;
                response.push_str(&line);
                break;
            }

            // For numeric responses (INCR/DECR)
            if line.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                break;
            }
        }

        Ok(response)
    }

    /// GET command
    pub async fn get(&mut self, key: &str) -> std::io::Result<Option<String>> {
        let response = self.send_command(&format!("GET {}", key)).await?;

        if response.starts_with("VALUE") {
            // Parse VALUE response
            let lines: Vec<&str> = response.lines().collect();
            if lines.len() >= 2 {
                return Ok(Some(lines[1].to_string()));
            }
        }

        Ok(None)
    }

    /// SET command
    pub async fn set(&mut self, key: &str, value: &str, exptime: u32) -> std::io::Result<bool> {
        let command = format!("SET {} 0 {} {}\r\n{}", key, exptime, value.len(), value);
        let response = self.send_command(&command).await?;
        Ok(response.starts_with("STORED"))
    }

    /// DELETE command
    pub async fn delete(&mut self, key: &str) -> std::io::Result<bool> {
        let response = self.send_command(&format!("DELETE {}", key)).await?;
        Ok(response.starts_with("DELETED"))
    }

    /// INCR command
    #[allow(dead_code)] // For future test scenarios
    pub async fn incr(&mut self, key: &str, value: u64) -> std::io::Result<Option<u64>> {
        let response = self
            .send_command(&format!("INCR {} {}", key, value))
            .await?;
        Ok(response.trim().parse().ok())
    }

    /// DECR command
    #[allow(dead_code)] // For future test scenarios
    pub async fn decr(&mut self, key: &str, value: u64) -> std::io::Result<Option<u64>> {
        let response = self
            .send_command(&format!("DECR {} {}", key, value))
            .await?;
        Ok(response.trim().parse().ok())
    }

    /// VERSION command
    pub async fn version(&mut self) -> std::io::Result<String> {
        let response = self.send_command("VERSION").await?;
        Ok(response
            .trim()
            .strip_prefix("VERSION ")
            .unwrap_or("")
            .to_string())
    }

    /// STATS command
    pub async fn stats(&mut self, arg: Option<&str>) -> std::io::Result<String> {
        let command = if let Some(arg) = arg {
            format!("STATS {}", arg)
        } else {
            "STATS".to_string()
        };
        self.send_command(&command).await
    }

    /// FLUSH_ALL command
    #[allow(dead_code)] // For future test scenarios
    pub async fn flush_all(&mut self) -> std::io::Result<bool> {
        let response = self.send_command("FLUSH_ALL").await?;
        Ok(response.starts_with("OK"))
    }

    /// QUIT command
    pub async fn quit(mut self) -> std::io::Result<()> {
        self.stream.get_mut().write_all(b"QUIT\r\n").await?;
        self.stream.get_mut().flush().await?;
        Ok(())
    }

    /// Send multiple commands in a pipeline
    #[allow(dead_code)] // For future pipelining tests
    pub async fn pipeline(&mut self, commands: &[&str]) -> std::io::Result<Vec<String>> {
        // Write all commands
        for command in commands {
            let cmd = if command.ends_with("\r\n") {
                command.to_string()
            } else {
                format!("{}\r\n", command)
            };
            self.stream.get_mut().write_all(cmd.as_bytes()).await?;
        }
        self.stream.get_mut().flush().await?;

        // Read all responses
        let mut responses = Vec::new();
        for _ in commands {
            responses.push(self.read_response().await?);
        }

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::helpers::mock_memcached::MockMemcached;

    #[tokio::test]
    async fn test_client_basic_commands() {
        let mock = MockMemcached::new().await.unwrap();
        let mut client = TestClient::connect(mock.addr()).await.unwrap();

        // Test SET
        assert!(client.set("testkey", "testvalue", 0).await.unwrap());

        // Test GET
        let value = client.get("testkey").await.unwrap();
        assert_eq!(value, Some("testvalue".to_string()));

        // Test DELETE
        assert!(client.delete("testkey").await.unwrap());

        // Test GET after DELETE
        let value = client.get("testkey").await.unwrap();
        assert_eq!(value, None);

        client.quit().await.unwrap();
        mock.shutdown();
    }

    #[tokio::test]
    async fn test_client_version() {
        let mock = MockMemcached::new().await.unwrap();
        let mut client = TestClient::connect(mock.addr()).await.unwrap();

        let version = client.version().await.unwrap();
        assert!(version.contains("mock"));

        client.quit().await.unwrap();
        mock.shutdown();
    }
}
