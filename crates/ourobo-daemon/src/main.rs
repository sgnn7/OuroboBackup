mod daemon;

use anyhow::Result;
use ourobo_core::config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config_path = ourobo_core::config::default_config_path();
    tracing::info!("loading config from {}", config_path.display());

    let config = AppConfig::load_or_default(&config_path)?;
    daemon::run(config, config_path).await
}
