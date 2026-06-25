//! Server configuration loaded from TOML.

use std::path::{Path, PathBuf};

use opennodia_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Top-level OpenNodia daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// HTTP/web server settings.
    #[serde(default)]
    pub server: ServerConfig,
    /// Algorand node (algod) connection settings.
    #[serde(default)]
    pub algod: AlgodConfig,
    /// Key management daemon (kmd) connection settings.
    #[serde(default)]
    pub kmd: KmdConfig,
    /// Optional Algorand Indexer for asset search and transaction history.
    /// When disabled, all existing functionality works unchanged.
    #[serde(default)]
    pub indexer: IndexerConfig,
    /// Persistent PostgreSQL cache for transactions involving registered
    /// OpenNodia wallet addresses.
    #[serde(default)]
    pub wallet_history: WalletHistoryConfig,
    /// Local DEX transaction settings.
    #[serde(default)]
    pub dex: DexConfig,
    /// Native LP/AMM settings.
    #[serde(default)]
    pub lp: LpConfig,
    /// External AMM liquidity settings.
    #[serde(default)]
    pub external_liquidity: ExternalLiquidityConfig,
    /// Where persistent data (PIN hash, sessions DB) is stored.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

/// Local DEX transaction configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexConfig {
    /// Enable endpoints that construct, sign, or submit escrow transactions.
    ///
    /// This defaults to false so an installation cannot expose an unvalidated
    /// contract write path merely by upgrading the application.
    #[serde(default)]
    pub write_enabled: bool,
    /// Prepared transaction intent lifetime in seconds.
    #[serde(default = "default_dex_intent_ttl")]
    pub intent_ttl_secs: u64,
    /// Interval between on-chain order reconciliation sweeps.
    #[serde(default = "default_dex_reconcile_interval")]
    pub reconcile_interval_secs: u64,
}

/// Native LP/AMM configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LpConfig {
    /// OpenNodia native AMM registry app ID for this network.
    ///
    /// When set, new native pool creation is atomically registered on-chain.
    #[serde(default)]
    pub native_registry_app_id: Option<u64>,
    /// Require registry-backed pool creation. Enable this for mainnet writes.
    #[serde(default)]
    pub require_registry: bool,
    /// Allow native AMM writes on mainnet after an independent contract audit.
    ///
    /// This stays false by default so deployments fail closed on mainnet even
    /// when registry settings are present.
    #[serde(default)]
    pub mainnet_write_enabled_after_audit: bool,
}

/// External AMM liquidity configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExternalLiquidityConfig {
    /// Enable transaction construction and submission for external AMM swaps.
    ///
    /// Discovery, quoting, and LP position reads remain available when this is
    /// false. Swap writes stay disabled by default because OpenNodia cannot
    /// audit third-party contracts the same way it audits its native AMM.
    #[serde(default)]
    pub swap_enabled: bool,
    /// Enable transaction construction and submission for external AMM LP
    /// add/remove operations.
    ///
    /// This is separate from swap_enabled so deployments can validate external
    /// swaps without also enabling LP position mutation.
    #[serde(default)]
    pub liquidity_enabled: bool,
}

/// HTTP / web server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Bind address, e.g. `127.0.0.1` or `0.0.0.0` for remote access.
    #[serde(default = "default_bind")]
    pub bind: String,
    /// TCP port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Session token lifetime in seconds (default: 8 hours).
    #[serde(default = "default_session_ttl")]
    pub session_ttl_secs: u64,
    /// Max PIN attempts before lockout.
    #[serde(default = "default_max_attempts")]
    pub max_pin_attempts: u32,
    /// Lockout duration in seconds after exceeding max attempts.
    #[serde(default = "default_lockout_secs")]
    pub lockout_secs: u64,
}

/// Algorand node (algod REST API) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgodConfig {
    /// algod REST base URL, e.g. `http://localhost:4001`.
    #[serde(default = "default_algod_url")]
    pub url: String,
    /// Optional read-only algod REST base URL for current ledger state.
    ///
    /// Docker deployments can point this at the follower node so discovery and
    /// quote reads use a lightweight local data source while writes continue to
    /// use the participation node configured by `url`.
    #[serde(default)]
    pub read_url: Option<String>,
    /// algod API token.
    #[serde(default = "default_algod_token")]
    pub token: String,
    /// Optional file containing the algod API token.
    #[serde(default)]
    pub token_file: Option<PathBuf>,
    /// Optional token for the read-only algod endpoint.
    ///
    /// When omitted, the write algod token is reused.
    #[serde(default)]
    pub read_token: String,
    /// Optional file containing the read-only algod API token.
    ///
    /// When omitted, `token_file` is reused.
    #[serde(default)]
    pub read_token_file: Option<PathBuf>,
    /// Target network.
    #[serde(default = "default_network")]
    pub network: opennodia_core::Network,
    /// Whether to fall back to the public relay API when the local node
    /// is unreachable or still catching up. Default: true.
    #[serde(default = "default_true")]
    pub use_public_fallback: bool,
}

/// Key management daemon (kmd REST API) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KmdConfig {
    /// kmd REST base URL, e.g. `http://localhost:7833`.
    #[serde(default = "default_kmd_url")]
    pub url: String,
    /// kmd API token. Ign if `token_file` is set.
    #[serde(default = "default_kmd_token")]
    pub token: String,
    /// Optional path to a file containing the kmd API token.
    /// Useful in Docker where kmd generates a random token on each start.
    /// The file is read at startup; if it exists, its contents override `token`.
    #[serde(default)]
    pub token_file: Option<PathBuf>,
}

/// Algorand Indexer configuration.
///
/// The indexer provides asset search by name/symbol, transaction history,
/// and application (LP pool) discovery. It is a read-only service that does
/// not interfere with algod consensus or block rewards.
///
/// The indexer is always part of the stack. When the local indexer is still
/// bootstrapping (or unreachable), OpenNodia automatically falls back to the
/// public indexer relay so that search and transaction history remain
/// available at all times.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    /// Local indexer REST base URL, e.g. `http://localhost:8980`.
    #[serde(default = "default_indexer_url")]
    pub url: String,
    /// Indexer API token (often empty for local instances).
    #[serde(default)]
    pub token: String,
    /// Whether to fall back to the public indexer relay when the local
    /// indexer is unreachable or still bootstrapping. Default: true.
    #[serde(default = "default_true")]
    pub use_public_fallback: bool,
    /// Whether the local database contains complete current ledger state.
    ///
    /// Keep this false for the lightweight recent-history deployment because
    /// it starts near the network tip and intentionally omits historical state.
    /// The legacy `local_data_complete` key is accepted as an alias.
    #[serde(default, alias = "local_data_complete")]
    pub local_state_complete: bool,
    /// Number of recent rounds retained by the local Indexer database.
    #[serde(default = "default_indexer_history_rounds")]
    pub history_retention_rounds: u64,
    /// Optional catchpoint to start indexing from a specific round for faster
    /// bootstrap. Format: `"<round>#<hash>"`. When `None`, the indexer starts
    /// from genesis (complete history). Does NOT affect algod consensus or
    /// block rewards.
    #[serde(default)]
    pub catchpoint: Option<String>,
}

/// PostgreSQL-backed permanent history for registered wallet addresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletHistoryConfig {
    /// Enable the wallet-only transaction cache.
    #[serde(default)]
    pub enabled: bool,
    /// PostgreSQL connection string. Prefer the
    /// file-based Docker secret configuration in production.
    #[serde(default)]
    pub database_url: String,
    /// Maximum number of transactions fetched per Indexer page.
    #[serde(default = "default_wallet_history_page_size")]
    pub page_size: u32,
    /// Number of public historical pages imported per sync cycle.
    #[serde(default = "default_wallet_history_pages_per_sync")]
    pub pages_per_sync: u32,
    /// Background synchronization interval in seconds.
    #[serde(default = "default_wallet_history_sync_interval")]
    pub sync_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            algod: AlgodConfig::default(),
            kmd: KmdConfig::default(),
            indexer: IndexerConfig::default(),
            wallet_history: WalletHistoryConfig::default(),
            dex: DexConfig::default(),
            lp: LpConfig::default(),
            external_liquidity: ExternalLiquidityConfig::default(),
            data_dir: default_data_dir(),
        }
    }
}

impl Default for DexConfig {
    fn default() -> Self {
        Self {
            write_enabled: false,
            intent_ttl_secs: default_dex_intent_ttl(),
            reconcile_interval_secs: default_dex_reconcile_interval(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            session_ttl_secs: default_session_ttl(),
            max_pin_attempts: default_max_attempts(),
            lockout_secs: default_lockout_secs(),
        }
    }
}

impl Default for AlgodConfig {
    fn default() -> Self {
        Self {
            url: default_algod_url(),
            read_url: None,
            token: default_algod_token(),
            token_file: None,
            read_token: String::new(),
            read_token_file: None,
            network: default_network(),
            use_public_fallback: true,
        }
    }
}

impl Default for KmdConfig {
    fn default() -> Self {
        Self {
            url: default_kmd_url(),
            token: default_kmd_token(),
            token_file: None,
        }
    }
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            url: default_indexer_url(),
            token: String::new(),
            use_public_fallback: true,
            local_state_complete: false,
            history_retention_rounds: default_indexer_history_rounds(),
            catchpoint: None,
        }
    }
}

impl Default for WalletHistoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            database_url: String::new(),
            page_size: default_wallet_history_page_size(),
            pages_per_sync: default_wallet_history_pages_per_sync(),
            sync_interval_secs: default_wallet_history_sync_interval(),
        }
    }
}

impl WalletHistoryConfig {
    /// Resolve the database URL without requiring credentials in TOML.
    pub fn effective_database_url(&self) -> anyhow::Result<Option<String>> {
        if !self.database_url.trim().is_empty() {
            return Ok(Some(self.database_url.trim().to_string()));
        }
        if let Some(database_url) = std::env::var("OPENNODIA_WALLET_HISTORY_DATABASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            return Ok(Some(database_url));
        }

        let Some(password_file) =
            std::env::var_os("OPENNODIA_WALLET_HISTORY_DATABASE_PASSWORD_FILE")
        else {
            return Ok(None);
        };
        let password = read_alphanumeric_secret(
            Path::new(&password_file),
            "wallet history database password",
            24,
            128,
        )?;
        let host = env_or_default("OPENNODIA_WALLET_HISTORY_DATABASE_HOST", "postgres");
        let port = env_or_default("OPENNODIA_WALLET_HISTORY_DATABASE_PORT", "5432");
        let user = env_or_default("OPENNODIA_WALLET_HISTORY_DATABASE_USER", "algorand");
        let database = env_or_default("OPENNODIA_WALLET_HISTORY_DATABASE_NAME", "indexer");

        validate_host(&host)?;
        validate_port(&port)?;
        validate_identifier(&user, "wallet history database user")?;
        validate_identifier(&database, "wallet history database name")?;

        Ok(Some(format!(
            "host={host} port={port} user={user} password={password} \
             dbname={database} sslmode=disable"
        )))
    }
}

impl AlgodConfig {
    /// Resolve the algod token from a secret file before falling back to TOML.
    pub fn effective_token(&self) -> anyhow::Result<String> {
        if let Some(path) = self.token_file.as_ref() {
            return read_alphanumeric_secret(path, "algod API token", 64, 64);
        }
        Ok(self.token.clone())
    }

    /// Resolve the read-only algod token.
    pub fn effective_read_token(&self) -> anyhow::Result<String> {
        if let Some(path) = self.read_token_file.as_ref().or(self.token_file.as_ref()) {
            return read_alphanumeric_secret(path, "read-only algod API token", 64, 64);
        }
        if !self.read_token.trim().is_empty() {
            return Ok(self.read_token.clone());
        }
        Ok(self.token.clone())
    }
}

impl KmdConfig {
    /// Resolve the effective kmd API token.
    ///
    /// If `token_file` is set and the file exists, its trimmed contents are used.
    /// Otherwise, falls back to the static `token` field.
    pub fn effective_token(&self) -> String {
        if let Some(ref path) = self.token_file {
            // Retry for up to ~10 seconds in case the token file hasn't been
            // synced yet (e.g., Docker entrypoint wrapper is still copying it).
            for _ in 0..20 {
                if let Ok(text) = std::fs::read_to_string(path) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            tracing::warn!(
                ?path,
                "kmd token file not found after retries, using static token"
            );
        }
        self.token.clone()
    }
}

fn read_alphanumeric_secret(
    path: &Path,
    label: &str,
    minimum_length: usize,
    maximum_length: usize,
) -> anyhow::Result<String> {
    let secret = std::fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("read {label} file {}: {error}", path.display()))?;
    let secret = secret.trim();
    if secret.is_empty() {
        anyhow::bail!("{label} file is empty");
    }
    if !secret
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        anyhow::bail!("{label} contains unsupported characters");
    }
    if !(minimum_length..=maximum_length).contains(&secret.len()) {
        anyhow::bail!("{label} has an invalid length");
    }
    Ok(secret.to_string())
}

fn env_or_default(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn validate_identifier(value: &str, label: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        anyhow::bail!("{label} contains unsupported characters");
    }
    Ok(())
}

fn validate_host(value: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        anyhow::bail!("wallet history database host contains unsupported characters");
    }
    Ok(())
}

fn validate_port(value: &str) -> anyhow::Result<()> {
    value
        .parse::<u16>()
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("wallet history database port is invalid"))
}

impl Config {
    /// Load configuration from a TOML file, falling back to defaults for
    /// missing fields. If the file does not exist, returns defaults.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            tracing::info!(?path, "config file not found, using defaults");
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        let cfg: Self =
            toml::from_str(&text).map_err(|e| Error::Config(format!("parse config: {e}")))?;
        Ok(cfg)
    }

    /// The full socket address to bind to.
    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.server.bind, self.server.port)
    }

    /// Ensure the data directory exists.
    pub fn ensure_data_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    /// Path to the PIN store file inside the data directory.
    pub fn pin_path(&self) -> PathBuf {
        self.data_dir.join("pin.hash")
    }
}

// --- Defaults -------------------------------------------------------------

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    30080
}

fn default_session_ttl() -> u64 {
    8 * 60 * 60 // 8 hours
}

fn default_max_attempts() -> u32 {
    5
}

fn default_lockout_secs() -> u64 {
    5 * 60 // 5 minutes
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("data")
}

fn default_algod_url() -> String {
    "http://localhost:4001".to_string()
}

fn default_algod_token() -> String {
    "a".repeat(64)
}

fn default_network() -> opennodia_core::Network {
    opennodia_core::Network::Testnet
}

fn default_true() -> bool {
    true
}

fn default_kmd_url() -> String {
    "http://localhost:7833".to_string()
}

fn default_kmd_token() -> String {
    "a".repeat(64)
}

fn default_indexer_url() -> String {
    "http://localhost:8980".to_string()
}

fn default_indexer_history_rounds() -> u64 {
    20_000
}

fn default_wallet_history_page_size() -> u32 {
    1_000
}

fn default_wallet_history_pages_per_sync() -> u32 {
    2
}

fn default_wallet_history_sync_interval() -> u64 {
    60
}

fn default_dex_intent_ttl() -> u64 {
    5 * 60
}

fn default_dex_reconcile_interval() -> u64 {
    60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let cfg = Config::default();
        assert_eq!(cfg.server.port, 30080);
        assert_eq!(cfg.socket_addr(), "127.0.0.1:30080");
        assert!(!cfg.dex.write_enabled);
        assert_eq!(cfg.server.max_pin_attempts, 5);
        assert_eq!(cfg.indexer.history_retention_rounds, 20_000);
        assert!(!cfg.wallet_history.enabled);
    }

    #[test]
    fn parse_partial_toml() {
        let toml = r#"
data_dir = "/tmp/opennodia"
[server]
port = 9999
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.server.port, 9999);
        assert_eq!(cfg.server.bind, "127.0.0.1"); // default kept
        assert_eq!(cfg.data_dir, PathBuf::from("/tmp/opennodia"));
    }

    #[test]
    fn legacy_local_data_complete_key_is_supported() {
        let cfg: Config = toml::from_str(
            r#"
[indexer]
local_data_complete = true
"#,
        )
        .unwrap();
        assert!(cfg.indexer.local_state_complete);
    }

    #[test]
    fn sample_configuration_parses() {
        let cfg: Config = toml::from_str(include_str!("../../../opennodia.sample.toml")).unwrap();
        assert_eq!(cfg.data_dir, PathBuf::from("data"));
        assert_eq!(cfg.indexer.history_retention_rounds, 20_000);
        assert!(cfg.wallet_history.enabled);
    }

    #[test]
    fn algod_token_file_overrides_inline_value() {
        let path = std::env::temp_dir().join(format!(
            "opennodia-algod-token-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let token = "b".repeat(64);
        std::fs::write(&path, format!("{token}\n")).unwrap();

        let config = AlgodConfig {
            token: "a".repeat(64),
            token_file: Some(path.clone()),
            ..AlgodConfig::default()
        };
        assert_eq!(config.effective_token().unwrap(), token);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_missing_file_uses_defaults() {
        let cfg = Config::load(Path::new("/nonexistent/opennodia.toml")).unwrap();
        assert_eq!(cfg.server.port, 30080);
    }
}
