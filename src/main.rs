use bifrost::{config::Config, server::BifrostServer};
use clap::Parser;
use tracing::{error, info};
use tracing_appender::non_blocking;
// no EnvFilter feature; use a simple level switch via RUST_LOG

static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

#[derive(Parser)]
#[command(name = "bifrost")]
#[command(about = "Intelligent Memcached Proxy")]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "examples/simple.yaml")]
    config: String,
}

fn init_logging() {
    let (non_blocking_writer, guard) = non_blocking(std::io::stderr());
    // Keep guard alive for the program lifetime to avoid log loss
    let _ = LOG_GUARD.set(guard);

    let fmt = tracing_subscriber::fmt()
        .with_writer(non_blocking_writer)
        .with_ansi(true)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_level(true)
        .compact();

    // Map RUST_LOG to a max level (debug/info/warn/error/trace)
    let level = match std::env::var("RUST_LOG")
        .unwrap_or_else(|_| "info".to_string())
        .to_lowercase()
        .as_str()
    {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    let _ = fmt.with_max_level(level).try_init();
}

async fn run_with_config_path_and_shutdown(
    config_path: &str,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_yaml_file(config_path).await?;
    if let (Some(listener), Some(backend)) = (
        config.listeners.values().next(),
        config.backends.values().next(),
    ) {
        info!("Proxy: {} -> {}", listener.bind, backend.server);
    }
    let server = BifrostServer::new(config).await?;
    tokio::select! {
        result = server.start() => {
            if let Err(e) = result {
                error!("Server error: {}", e);
                return Err(e.into());
            }
        }
        _ = shutdown => {
            info!("Shutdown signal received");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    let args = Args::parse();
    info!("Starting Bifrost - Intelligent Memcached Proxy");
    run_with_config_path_and_shutdown(&args.config, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    info!("Bifrost proxy stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_temp_config() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("bifrost_test_{}.yaml", uuid::Uuid::new_v4()));
        let mut f = File::create(&dir).unwrap();
        let yaml = r#"
listeners:
  main: { bind: "127.0.0.1:0" }
backends:
  b1: { type: "memcached", server: "127.0.0.1:0" }
routes:
  r1: { matcher: "*", backend: "b1" }
"#;
        f.write_all(yaml.as_bytes()).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_run_with_config_shutdown_quickly() {
        init_logging();
        let path = write_temp_config();
        let res = run_with_config_path_and_shutdown(
            path.to_str().unwrap(),
            tokio::time::sleep(std::time::Duration::from_millis(50)),
        )
        .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_config_returns_error_when_server_start_fails() {
        init_logging();
        // Write a config with an invalid bind address to force server.start() to return Err
        let mut path = std::env::temp_dir();
        path.push(format!("bifrost_test_bad_{}.yaml", uuid::Uuid::new_v4()));
        let mut f = File::create(&path).unwrap();
        let yaml = r#"
listeners:
  bad: { bind: "256.256.256.256:12345" }
backends:
  b1: { type: "memcached", server: "127.0.0.1:0" }
routes:
  r1: { matcher: "*", backend: "b1" }
"#;
        f.write_all(yaml.as_bytes()).unwrap();

        let res = run_with_config_path_and_shutdown(
            path.to_str().unwrap(),
            tokio::time::sleep(std::time::Duration::from_millis(200)),
        )
        .await;
        assert!(res.is_err());
    }

    #[test]
    fn test_args_parse_from() {
        let tmp = "/tmp/cfg.yaml";
        let args = Args::parse_from(["bifrost", "--config", tmp]);
        assert_eq!(args.config, tmp);
    }
}
