//! OpenNodia server binary entrypoint.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use opennodia_server::{run, Config};

/// OpenNodia self-hosted daemon.
#[derive(Debug, Parser)]
#[command(name = "opennodia-server", version, about)]
struct Args {
    /// Path to the configuration TOML file.
    #[arg(short, long, default_value = "opennodia.toml")]
    config: PathBuf,

    /// Directory containing the built frontend (SPA) to serve.
    /// If omitted, the API runs without a web UI.
    #[arg(long, env = "OPENNODIA_WEB_DIR")]
    web_dir: Option<PathBuf>,

    /// Override the bind address.
    #[arg(long)]
    bind: Option<String>,

    /// Override the port.
    #[arg(short, long)]
    port: Option<u16>,

    /// Override the algod URL.
    #[arg(long)]
    algod_url: Option<String>,

    /// Override the read-only algod URL.
    #[arg(long, env = "OPENNODIA_ALGOD_READ_URL")]
    algod_read_url: Option<String>,

    /// Override the algod token.
    #[arg(long, env = "OPENNODIA_ALGOD_TOKEN")]
    algod_token: Option<String>,

    /// Read the algod token from a file.
    #[arg(long, env = "OPENNODIA_ALGOD_TOKEN_FILE")]
    algod_token_file: Option<PathBuf>,

    /// Read the read-only algod token from a file.
    #[arg(long, env = "OPENNODIA_ALGOD_READ_TOKEN_FILE")]
    algod_read_token_file: Option<PathBuf>,

    /// Target network (overrides config file if set).
    #[arg(long, value_enum)]
    network: Option<NetworkArg>,

    /// Recent rounds retained by the local Indexer.
    #[arg(long, env = "OPENNODIA_INDEXER_HISTORY_ROUNDS")]
    indexer_history_rounds: Option<u64>,
}

#[derive(Debug, Clone, ValueEnum)]
enum NetworkArg {
    Mainnet,
    Testnet,
    Betanet,
    Local,
}

impl NetworkArg {
    fn to_network(&self) -> opennodia_core::Network {
        match self {
            NetworkArg::Mainnet => opennodia_core::Network::Mainnet,
            NetworkArg::Testnet => opennodia_core::Network::Testnet,
            NetworkArg::Betanet => opennodia_core::Network::Betanet,
            NetworkArg::Local => opennodia_core::Network::Local,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opennodia=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();

    // Load config, then apply CLI overrides.
    let mut config = Config::load(&args.config)?;
    if let Some(bind) = args.bind {
        config.server.bind = bind;
    }
    if let Some(port) = args.port {
        config.server.port = port;
    }
    if let Some(url) = args.algod_url {
        config.algod.url = url;
    }
    if let Some(url) = args.algod_read_url {
        config.algod.read_url = Some(url);
    }
    if let Some(token) = args.algod_token {
        config.algod.token = token;
        config.algod.token_file = None;
    }
    if let Some(token_file) = args.algod_token_file {
        config.algod.token_file = Some(token_file);
    }
    if let Some(token_file) = args.algod_read_token_file {
        config.algod.read_token_file = Some(token_file);
    }
    if let Some(network) = args.network {
        config.algod.network = network.to_network();
    }
    if let Some(rounds) = args.indexer_history_rounds {
        if rounds < 1_000 {
            anyhow::bail!("indexer history retention must be at least 1000 rounds");
        }
        config.indexer.history_retention_rounds = rounds;
    }

    tracing::info!(
        bind = %config.server.bind,
        port = config.server.port,
        algod = %config.algod.url,
        read_algod = ?config.algod.read_url,
        network = %config.algod.network,
        "starting OpenNodia server"
    );

    run(config, args.web_dir).await
}
