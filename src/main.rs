use bifrost::{config::Config, server::BifrostServer};
use clap::Parser;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "bifrost")]
#[command(about = "Intelligent Memcached Proxy")]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "examples/simple.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    info!("Starting Bifrost - Intelligent Memcached Proxy");

    // Load configuration
    let config = Config::from_yaml_file(&args.config).await?;
    info!("Configuration loaded from: {}", args.config);

    // Log proxy setup
    if let (Some(listener), Some(backend)) = (
        config.listeners.values().next(),
        config.backends.values().next(),
    ) {
        info!("Proxy: {} -> {}", listener.bind, backend.server);
    }

    // Create and start server
    let server = BifrostServer::new(config).await?;

    // Set up graceful shutdown
    tokio::select! {
        result = server.start() => {
            if let Err(e) = result {
                error!("Server error: {}", e);
                return Err(e.into());
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down gracefully...");
        }
    }

    info!("Bifrost proxy stopped");
    Ok(())
}
