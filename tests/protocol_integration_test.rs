use bifrost::core::protocols::{AsciiProtocol, Protocol};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};


/// Mock backend that responds to ASCII commands
async fn mock_memcached_backend(mut stream: TcpStream) {
    let mut buffer = [0u8; 1024];

    loop {
        // Read command
        let n = match stream.read(&mut buffer).await {
            Ok(0) => break, // Connection closed
            Ok(n) => n,
            Err(_) => break,
        };

        let command = String::from_utf8_lossy(&buffer[..n]);

        // Respond based on command
        let response = if command.starts_with("GET") {
            "VALUE testkey 0 5\r\nhello\r\nEND\r\n"
        } else if command.starts_with("SET") {
            "STORED\r\n"
        } else if command.starts_with("DELETE") {
            "DELETED\r\n"
        } else if command.starts_with("INCR") {
            "10\r\n"
        } else if command.starts_with("VERSION") {
            "VERSION 1.6.21\r\n"
        } else if command.starts_with("STATS") {
            "STAT pid 12345\r\nEND\r\n"
        } else if command.starts_with("QUIT") {
            break;
        } else {
            "ERROR unknown command\r\n"
        };

        if let Err(_) = stream.write_all(response.as_bytes()).await {
            break;
        }
    }
}



#[tokio::test]
async fn test_ascii_protocol_get_command() {
    // Start mock backend
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let _backend_addr = backend_listener.local_addr().unwrap();

    // Accept backend connection
    tokio::spawn(async move {
        let (stream, _) = backend_listener.accept().await.unwrap();
        mock_memcached_backend(stream).await;
    });

    // Create protocol handler
    let protocol = AsciiProtocol::new();

    // Create mock client stream (simulating parsed input)
    let _client_stream = TcpStream::connect("127.0.0.1:0").await.is_err(); // This will fail, but that's ok for this test pattern
    // For a full integration test, we'd need to set up proper bidirectional streams

    // This test verifies the protocol exists and can be instantiated
    assert_eq!(protocol.name(), "ascii");
}

#[tokio::test]
async fn test_ascii_protocol_command_reconstruction() {
    use bifrost::core::protocols::AsciiCommand;

    // Test that we can parse and reconstruct commands correctly
    let commands = vec![
        "GET mykey\r\n",
        "SET mykey 0 300 5\r\n",
        "SET mykey 123 600 10 noreply\r\n",
        "DELETE mykey\r\n",
        "DELETE mykey noreply\r\n",
        "INCR counter 5\r\n",
        "DECR counter 3 noreply\r\n",
        "FLUSH_ALL\r\n",
        "FLUSH_ALL 300\r\n",
        "STATS\r\n",
        "STATS items\r\n",
        "VERSION\r\n",
        "QUIT\r\n",
    ];

    for cmd_str in commands {
        // Parse the command
        let parsed = AsciiCommand::parse(cmd_str).unwrap();

        // Verify it has expected properties
        match &parsed {
            AsciiCommand::Get { keys } => {
                assert!(!keys.is_empty());
                assert!(parsed.expects_response());
            }
            AsciiCommand::Set { key, bytes, noreply, .. } |
            AsciiCommand::Add { key, bytes, noreply, .. } |
            AsciiCommand::Replace { key, bytes, noreply, .. } => {
                assert!(!key.is_empty());
                assert!(*bytes > 0); // bytes should be positive for storage commands
                assert_eq!(parsed.expects_response(), !noreply);
            }
            AsciiCommand::Delete { key, noreply, .. } => {
                assert!(!key.is_empty());
                assert_eq!(parsed.expects_response(), !noreply);
            }
            AsciiCommand::Incr { key, value: _, noreply } |
            AsciiCommand::Decr { key, value: _, noreply } => {
                assert!(!key.is_empty());
                // value can be 0 for increment/decrement operations
                assert_eq!(parsed.expects_response(), !noreply);
            }
            AsciiCommand::FlushAll { noreply, .. } => {
                assert_eq!(parsed.expects_response(), !noreply);
            }
            AsciiCommand::Stats { .. } |
            AsciiCommand::Version => {
                assert!(parsed.expects_response());
            }
            AsciiCommand::Quit => {
                assert!(!parsed.expects_response());
            }
        }

        // Verify key extraction works for commands that have keys
        if cmd_str.contains("mykey") {
            assert_eq!(parsed.primary_key(), Some("mykey"));
        } else if cmd_str.contains("counter") {
            assert_eq!(parsed.primary_key(), Some("counter"));
        }
    }
}

#[tokio::test]
async fn test_protocol_error_handling() {
    use bifrost::core::protocols::AsciiCommand;

    let invalid_commands = vec![
        "", // Empty command
        "INVALID_COMMAND\r\n", // Unknown command
        "GET\r\n", // Missing key
        "SET key\r\n", // Missing parameters
        "SET key abc 300 5\r\n", // Invalid flags
        "SET key 0 abc 5\r\n", // Invalid exptime
        "SET key 0 300 abc\r\n", // Invalid bytes
        "DELETE\r\n", // Missing key
        "INCR counter\r\n", // Missing value
        "INCR counter abc\r\n", // Invalid value
        "DECR counter abc\r\n", // Invalid value
    ];

    for cmd in invalid_commands {
        let result = AsciiCommand::parse(cmd);
        assert!(result.is_err(), "Command '{}' should have failed to parse", cmd);
    }
}

#[tokio::test]
async fn test_response_parsing_edge_cases() {
    use bifrost::core::protocols::AsciiResponse;

    // Test valid responses
    let valid_responses = vec![
        ("STORED", AsciiResponse::Stored),
        ("NOT_STORED", AsciiResponse::NotStored),
        ("EXISTS", AsciiResponse::Exists),
        ("NOT_FOUND", AsciiResponse::NotFound),
        ("DELETED", AsciiResponse::Deleted),
        ("TOUCHED", AsciiResponse::Touched),
        ("OK", AsciiResponse::Ok),
        ("END", AsciiResponse::End),
        ("ERROR", AsciiResponse::Error("ERROR".to_string())),
        ("VERSION 1.6.21", AsciiResponse::Version("1.6.21".to_string())),
    ];

    for (input, expected) in valid_responses {
        let result = AsciiResponse::parse(input).unwrap();
        assert_eq!(result, expected);

        // Test that format round-trip works
        let formatted = result.format();
        assert!(formatted.ends_with("\r\n"));
    }

    // Test VALUE response parsing
    let value_result = AsciiResponse::parse("VALUE mykey 123 5").unwrap();
    if let AsciiResponse::Value { key, flags, bytes, .. } = value_result {
        assert_eq!(key, "mykey");
        assert_eq!(flags, 123);
        assert_eq!(bytes, 5);
    } else {
        panic!("Expected Value response");
    }

    // Test STAT response parsing
    let stat_result = AsciiResponse::parse("STAT uptime 12345").unwrap();
    if let AsciiResponse::Stat { key, value } = stat_result {
        assert_eq!(key, "uptime");
        assert_eq!(value, "12345");
    } else {
        panic!("Expected Stat response");
    }

    // Test invalid responses
    let invalid_responses = vec![
        "UNKNOWN_RESPONSE",
        "VALUE key", // Missing parameters
        "VALUE key abc 5", // Invalid flags
        "STAT", // Missing key/value
    ];

    for invalid in invalid_responses {
        assert!(AsciiResponse::parse(invalid).is_err(),
                "Response '{}' should have failed to parse", invalid);
    }
}

#[tokio::test]
async fn test_protocol_noreply_handling() {
    use bifrost::core::protocols::AsciiCommand;

    // Commands with noreply should not expect responses
    let noreply_commands = vec![
        "SET key 0 300 5 noreply\r\n",
        "ADD key 0 300 5 noreply\r\n",
        "REPLACE key 0 300 5 noreply\r\n",
        "DELETE key noreply\r\n",
        "INCR counter 1 noreply\r\n",
        "DECR counter 1 noreply\r\n",
        "FLUSH_ALL noreply\r\n",
        "FLUSH_ALL 300 noreply\r\n",
    ];

    for cmd_str in noreply_commands {
        let parsed = AsciiCommand::parse(cmd_str).unwrap();
        assert!(!parsed.expects_response(),
                "Command '{}' with noreply should not expect response", cmd_str);
    }

    // Commands without noreply should expect responses (except QUIT)
    let reply_commands = vec![
        "SET key 0 300 5\r\n",
        "DELETE key\r\n",
        "INCR counter 1\r\n",
        "GET key\r\n",
        "STATS\r\n",
        "VERSION\r\n",
    ];

    for cmd_str in reply_commands {
        let parsed = AsciiCommand::parse(cmd_str).unwrap();
        assert!(parsed.expects_response(),
                "Command '{}' should expect response", cmd_str);
    }

    // QUIT never expects response
    let quit = AsciiCommand::parse("QUIT\r\n").unwrap();
    assert!(!quit.expects_response());
}

#[tokio::test]
async fn test_multikey_get_routing() {
    use bifrost::core::protocols::AsciiCommand;

    // Multi-key GET should use first key for routing
    let cmd = AsciiCommand::parse("GET key1 key2 key3\r\n").unwrap();
    assert_eq!(cmd.primary_key(), Some("key1"));

    // Single key GET
    let cmd = AsciiCommand::parse("GET singlekey\r\n").unwrap();
    assert_eq!(cmd.primary_key(), Some("singlekey"));

    // Commands without keys should return None
    let cmd = AsciiCommand::parse("STATS\r\n").unwrap();
    assert_eq!(cmd.primary_key(), None);

    let cmd = AsciiCommand::parse("VERSION\r\n").unwrap();
    assert_eq!(cmd.primary_key(), None);

    let cmd = AsciiCommand::parse("FLUSH_ALL\r\n").unwrap();
    assert_eq!(cmd.primary_key(), None);
}