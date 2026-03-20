use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

mod actors;
mod coordinator;
mod ipc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,s_kvm=debug")),
        )
        .init();

    tracing::info!("S-KVM daemon starting...");

    // Load configuration
    let config = s_kvm_config::load_config()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    tracing::info!(
        peer_id = %config.peer_id,
        machine = %config.machine_name,
        port = config.network.listen_port,
        "Configuration loaded"
    );

    // Create root cancellation token
    let shutdown = CancellationToken::new();

    // Set up signal handlers
    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
        tracing::info!("Received CTRL+C, initiating shutdown...");
        shutdown_signal.cancel();
    });

    #[cfg(unix)]
    {
        let shutdown_term = shutdown.clone();
        tokio::spawn(async move {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Failed to install SIGTERM handler");
            sigterm.recv().await;
            tracing::info!("Received SIGTERM, initiating shutdown...");
            shutdown_term.cancel();
        });
    }

    // Start the coordinator (manages all subsystem actors)
    let coordinator = coordinator::Coordinator::new(config, shutdown.clone());
    coordinator.run().await?;

    tracing::info!("S-KVM daemon stopped.");
    Ok(())
}
