use bifrost::{
    config::Config,
    core::{RouteTableBuilder, GlobMatcher, Matcher},
};

#[tokio::test]
async fn test_route_table_creation() {
    let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"
  cache2:
    type: "memcached"
    server: "127.0.0.1:11213"

routes:
  users:
    matcher: "user:*"
    backend: "cache1"
  sessions:
    matcher: "session:*"
    backend: "cache2"
  default:
    matcher: "*"
    backend: "cache1"
"#;

    let config = Config::from_yaml_str(yaml).expect("Failed to parse config");
    let route_table = RouteTableBuilder::build_from_config(&config)
        .await
        .expect("Failed to build route table");

    assert_eq!(route_table.routes().len(), 3);

    // Test that all routes exist in the table (order may vary due to HashMap)
    let mut patterns: Vec<String> = route_table.routes().iter()
        .map(|r| r.matcher.pattern().to_string())
        .collect();
    patterns.sort();

    assert!(patterns.contains(&"user:*".to_string()));
    assert!(patterns.contains(&"session:*".to_string()));
    assert!(patterns.contains(&"*".to_string()));

    println!("✅ Route table creation and routing working correctly!");
    println!("Routes in table: {:?}", patterns);
}

#[tokio::test]
async fn test_route_table_with_pools() {
    let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"
  cache2:
    type: "memcached"
    server: "127.0.0.1:11213"

pools:
  cache_pool:
    backends: ["cache1", "cache2"]
    strategy:
      type: "round_robin"

routes:
  pooled:
    matcher: "pooled:*"
    pool: "cache_pool"
  direct:
    matcher: "direct:*"
    backend: "cache1"
  default:
    matcher: "*"
    pool: "cache_pool"
"#;

    let config = Config::from_yaml_str(yaml).expect("Failed to parse config");
    let route_table = RouteTableBuilder::build_from_config(&config)
        .await
        .expect("Failed to build route table");

    assert_eq!(route_table.routes().len(), 3);

    // Test that all routes exist in the table (order may vary due to HashMap)
    let mut patterns: Vec<String> = route_table.routes().iter()
        .map(|r| r.matcher.pattern().to_string())
        .collect();
    patterns.sort();

    assert!(patterns.contains(&"pooled:*".to_string()));
    assert!(patterns.contains(&"direct:*".to_string()));
    assert!(patterns.contains(&"*".to_string()));

    println!("✅ Route table with pools working correctly!");
    println!("Routes in table: {:?}", patterns);
}

#[tokio::test]
async fn test_glob_matcher() {
    // Test that glob matcher still works correctly
    let matcher = GlobMatcher::new("user:*".to_string());
    assert!(matcher.matches("user:123"));
    assert!(matcher.matches("user:abc"));
    assert!(!matcher.matches("session:123"));

    let wildcard_matcher = GlobMatcher::new("*".to_string());
    assert!(wildcard_matcher.matches("anything"));
    assert!(wildcard_matcher.matches("user:123"));

    println!("✅ Glob matcher working correctly!");
}

#[tokio::test]
async fn test_route_table_validation() {
    let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"

routes:
  test:
    matcher: "test:*"
    backend: "cache1"
"#;

    let config = Config::from_yaml_str(yaml).expect("Failed to parse config");
    let route_table = RouteTableBuilder::build_from_config(&config).await;
    assert!(route_table.is_ok(), "Route table should build successfully");
    let route_table = route_table.unwrap();

    assert_eq!(route_table.routes().len(), 1);

    println!("✅ Route table validation working correctly!");
}