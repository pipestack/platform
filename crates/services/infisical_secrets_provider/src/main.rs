mod backend;
mod config;
mod encryption;
mod infisical_client;
mod jwt;
mod types;

use anyhow::Result;
use backend::InfisicalSecretsBackend;
use config::AppConfig;
use tracing::{error, info, warn};

/// Main entry point for the Infisical secrets provider service
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber for logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "infisical_secrets_provider=info,warn".into()),
        )
        .init();

    info!("Starting Infisical Secrets Provider for wasmCloud");

    // Load configuration from environment variables
    let config = match AppConfig::new() {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            print_configuration_help();
            std::process::exit(1);
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        error!("Configuration validation failed: {}", e);
        print_configuration_help();
        std::process::exit(1);
    }

    info!("Configuration loaded successfully");
    info!("Backend name: {}", config.backend.name);
    info!("API version: {}", config.backend.api_version);
    info!("Infisical base URL: {}", config.infisical.base_url);
    info!("Infisical project ID: {}", config.infisical.project_id);
    info!("Infisical environment: {}", config.infisical.environment);
    info!("NATS URL: {}", config.nats.url);

    // Create and start the secrets backend
    let backend = match InfisicalSecretsBackend::new(config).await {
        Ok(backend) => backend,
        Err(e) => {
            error!("Failed to initialize Infisical secrets backend: {}", e);
            std::process::exit(1);
        }
    };

    info!("Infisical secrets backend initialized successfully");
    info!("Server public key: {}", backend.public_key());
    info!("Instance ID: {}", backend.instance_id());

    // Install signal handlers for graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
    };

    // Run the backend with graceful shutdown
    tokio::select! {
        result = backend.run() => {
            match result {
                Ok(_) => {
                    warn!("Infisical secrets backend stopped unexpectedly");
                }
                Err(e) => {
                    error!("Infisical secrets backend failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ = shutdown_signal => {
            info!("Received shutdown signal, stopping Infisical secrets backend...");
        }
    }

    info!("Infisical secrets backend stopped gracefully");
    Ok(())
}

/// Prints help information about required configuration
fn print_configuration_help() {
    eprintln!();
    eprintln!("=== Infisical Secrets Provider Configuration ===");
    eprintln!();
    eprintln!("Required environment variables:");
    eprintln!("  INFISICAL_CLIENT_ID      - Your Infisical Universal Auth client ID");
    eprintln!("  INFISICAL_CLIENT_SECRET  - Your Infisical Universal Auth client secret");
    eprintln!("  INFISICAL_PROJECT_ID     - Your Infisical project ID");
    eprintln!();
    eprintln!("Optional environment variables:");
    eprintln!(
        "  INFISICAL_BASE_URL       - Infisical instance URL (default: https://app.infisical.com)"
    );
    eprintln!("  INFISICAL_ENVIRONMENT    - Infisical environment (default: prod)");
    eprintln!("  NATS_URL                 - NATS server URL (default: nats://localhost:4222)");
    eprintln!("  NATS_SUBJECT_PREFIX      - NATS subject prefix (default: wasmcloud.secrets)");
    eprintln!("  BACKEND_NAME             - Backend name (default: infisical)");
    eprintln!("  API_VERSION              - API version (default: v1alpha1)");
    eprintln!();
    eprintln!("Example usage:");
    eprintln!("  export INFISICAL_CLIENT_ID=your_client_id");
    eprintln!("  export INFISICAL_CLIENT_SECRET=your_client_secret");
    eprintln!("  export INFISICAL_PROJECT_ID=your_project_id");
    eprintln!("  export INFISICAL_ENVIRONMENT=dev");
    eprintln!("  cargo run");
    eprintln!();
    eprintln!("For more information, visit:");
    eprintln!("  - wasmCloud secrets: https://wasmcloud.com/docs/deployment/security/secrets/");
    eprintln!("  - Infisical documentation: https://infisical.com/docs");
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_help() {
        // This test just ensures the help function doesn't panic
        print_configuration_help();
    }
}
