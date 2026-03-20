use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("S-KVM daemon starting...");

    // Load configuration
    let config = s_kvm_config::load_config()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    tracing::info!(
        peer_id = %config.peer_id,
        machine = %config.machine_name,
        "Configuration loaded"
    );

    // TODO: Initialize subsystems (network, input, video, audio, etc.)
    // TODO: Start actor system
    // TODO: Wait for shutdown signal

    tracing::info!("S-KVM daemon running. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;

    tracing::info!("Shutting down...");
    Ok(())
}
