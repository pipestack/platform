mod config;

use anyhow::Result;
use config::Config;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, postgres::PgListener};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct WorkspaceNotification {
    id: Uuid,
    name: String,
}

#[derive(Debug, Serialize)]
struct RailwayServiceInput {
    #[serde(rename = "environmentId")]
    environment_id: String,
    name: String,
    #[serde(rename = "projectId")]
    project_id: String,
    source: RailwayServiceSource,
}

#[derive(Debug, Serialize)]
struct RailwayServiceSource {
    #[serde(rename = "type")]
    repo: String,
}

#[derive(Debug, Deserialize)]
struct RailwayResponse {
    data: Option<RailwayData>,
    errors: Option<Vec<RailwayError>>,
}

#[derive(Debug, Deserialize)]
struct RailwayData {
    #[serde(rename = "serviceCreate")]
    service_create: Option<RailwayService>,
}

#[derive(Debug, Deserialize)]
struct RailwayService {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct RailwayError {
    message: String,
}

#[derive(Debug, Serialize)]
struct ServiceInstanceUpdateInput {
    #[serde(rename = "railwayConfigFile")]
    railway_config_file: String,
    region: String,
    #[serde(rename = "rootDirectory")]
    root_directory: String,
}

#[derive(Debug, sqlx::FromRow)]
struct Workspace {
    // id: Uuid,
    // name: String,
    // description: Option<String>,
    // created_at: chrono::DateTime<chrono::Utc>,
    // updated_at: chrono::DateTime<chrono::Utc>,
}

struct InfraManager {
    config: Config,
    pool: PgPool,
    http_client: Client,
}

impl InfraManager {
    async fn new() -> Result<Self> {
        let config = Config::from_env()?;
        config.validate()?;

        info!("Connecting to database: {}", config.database.url);
        let pool = PgPool::connect(&config.database.url).await?;

        // Test the connection
        sqlx::query("SELECT 1").execute(&pool).await?;
        info!("Database connection established successfully");

        Ok(Self {
            config,
            pool,
            http_client: Client::new(),
        })
    }

    async fn verify_workspaces_table(&self) -> Result<()> {
        info!("Verifying workspaces table exists...");

        // Check if workspaces table exists
        let check_table_query = r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' 
                AND table_name = 'workspaces'
            );
        "#;

        let exists: bool = sqlx::query_scalar(check_table_query)
            .fetch_one(&self.pool)
            .await?;

        if !exists {
            return Err(anyhow::anyhow!(
                "Workspaces table does not exist. This table should be created by another service."
            ));
        }

        info!("Workspaces table verified successfully");
        Ok(())
    }

    async fn setup_database_trigger(&self) -> Result<()> {
        info!("Setting up database trigger...");

        // Create the trigger function if it doesn't exist
        let trigger_function = r#"
            CREATE OR REPLACE FUNCTION notify_workspace_created()
            RETURNS TRIGGER AS $$
            BEGIN
                PERFORM pg_notify('workspace_created', 
                    json_build_object(
                        'id', NEW.id,
                        'name', NEW.name,
                        'created_at', NEW.created_at
                    )::text
                );
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        "#;

        sqlx::query(trigger_function).execute(&self.pool).await?;

        // Create the trigger if it doesn't exist
        let trigger_sql = r#"
            DROP TRIGGER IF EXISTS workspace_insert_trigger ON workspaces;
            CREATE TRIGGER workspace_insert_trigger
            AFTER INSERT ON workspaces
            FOR EACH ROW
            EXECUTE FUNCTION notify_workspace_created();
        "#;

        sqlx::query(trigger_sql).execute(&self.pool).await?;

        info!("Database trigger setup completed successfully");
        Ok(())
    }

    async fn make_railway_graphql_request(
        &self,
        mutation: &str,
        variables: serde_json::Value,
        operation_name: &str,
    ) -> Result<String> {
        let request_body = json!({
            "query": mutation,
            "variables": variables
        });

        info!("Making Railway GraphQL request: {}", operation_name);

        let response = self
            .http_client
            .post(&self.config.railway.api_url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.railway.token),
            )
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            return Err(anyhow::anyhow!(
                "Railway {} failed with status {}: {}",
                operation_name,
                status,
                body
            ));
        }

        let response_text = response.text().await?;
        info!("{} response: {}", operation_name, response_text);

        Ok(response_text)
    }

    async fn listen_for_notifications(&self) -> Result<()> {
        info!("Starting notification listener...");

        let mut listener = PgListener::connect(&self.config.database.url).await?;
        listener
            .listen(&self.config.database.notification_channel)
            .await?;

        info!(
            "Started listening for workspace changes on channel: {}",
            self.config.database.notification_channel
        );

        loop {
            let notification = listener.recv().await?;
            info!("Received notification: {}", notification.payload());

            match serde_json::from_str::<WorkspaceNotification>(notification.payload()) {
                Ok(workspace) => {
                    info!("Processing new workspace: {:?}", workspace);

                    // Try to create Railway service with retries
                    let mut retry_count = 0;
                    let mut success = false;

                    while retry_count < self.config.service.max_retries && !success {
                        match self.create_railway_service(&workspace).await {
                            Ok(_) => {
                                success = true;
                                info!(
                                    "Successfully created Railway service for workspace {} on attempt {}",
                                    workspace.id,
                                    retry_count + 1
                                );
                            }
                            Err(e) => {
                                retry_count += 1;
                                error!(
                                    "Failed to create Railway service for workspace {} (attempt {}): {}",
                                    workspace.id, retry_count, e
                                );

                                if retry_count < self.config.service.max_retries {
                                    info!(
                                        "Retrying in {}ms...",
                                        self.config.service.retry_delay_ms
                                    );
                                    tokio::time::sleep(tokio::time::Duration::from_millis(
                                        self.config.service.retry_delay_ms,
                                    ))
                                    .await;
                                }
                            }
                        }
                    }

                    if !success {
                        error!(
                            "Failed to create Railway service for workspace {} after {} attempts",
                            workspace.id, self.config.service.max_retries
                        );
                    }
                }
                Err(e) => {
                    error!("Failed to parse notification payload: {}", e);
                }
            }
        }
    }

    async fn create_railway_service(&self, workspace: &WorkspaceNotification) -> Result<()> {
        let service_name = format!(
            "{}-{}",
            self.config.service.name_prefix,
            workspace
                .name
                .to_lowercase()
                .replace(" ", "-")
                .replace("_", "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
        );

        let mutation = r#"
            mutation ServiceCreate($input: ServiceCreateInput!) {
                serviceCreate(input: $input) {
                    id
                    name
                }
            }
        "#;

        let variables = json!({
            "input": RailwayServiceInput {
                environment_id: self.config.railway.environment_id.clone(),
                name: service_name.clone(),
                project_id: self.config.railway.project_id.clone(),
                source: RailwayServiceSource {
                    repo: self.config.railway.default_template_repo.clone(),
                }
            }
        });

        info!("Creating Railway service: {}", service_name);

        let response_text = self
            .make_railway_graphql_request(mutation, variables, "service creation")
            .await?;

        let railway_response: RailwayResponse = serde_json::from_str(&response_text)?;

        if let Some(errors) = railway_response.errors {
            for error in errors {
                error!("Railway API error: {}", error.message);
            }
            return Err(anyhow::anyhow!("Railway API returned errors"));
        }

        if let Some(data) = railway_response.data {
            if let Some(service) = data.service_create {
                info!(
                    "Successfully created Railway service: {} (ID: {})",
                    service.name, service.id
                );

                // Update the service instance configuration
                self.update_service_instance(&service.id).await?;

                // Redeploy the service instance
                self.redeploy_service_instance(&service.id).await?;
            } else {
                warn!("Railway service creation succeeded but no service data returned");
            }
        } else {
            warn!("Railway service creation response contained no data");
        }

        Ok(())
    }

    async fn update_service_instance(&self, service_id: &str) -> Result<()> {
        let mutation = r#"
            mutation serviceInstanceUpdate($serviceId: String!, $environmentId: String, $input: ServiceInstanceUpdateInput!) {
                serviceInstanceUpdate(serviceId: $serviceId, environmentId: $environmentId, input: $input)
            }
        "#;

        let variables = json!({
            "serviceId": service_id,
            "environmentId": self.config.railway.environment_id,
            "input": ServiceInstanceUpdateInput {
                railway_config_file: "./services/wasmcloud/railway.json".to_string(),
                region: "us-east4-eqdc4a".to_string(),
                root_directory: "/services/wasmcloud".to_string(),
            }
        });

        info!("Updating Railway service instance: {}", service_id);

        self.make_railway_graphql_request(mutation, variables, "service instance update")
            .await?;

        info!(
            "Successfully updated Railway service instance: {}",
            service_id
        );
        Ok(())
    }

    async fn redeploy_service_instance(&self, service_id: &str) -> Result<()> {
        let mutation = r#"
            mutation serviceInstanceRedeploy($serviceId: String!, $environmentId: String!) {
                serviceInstanceRedeploy(serviceId: $serviceId, environmentId: $environmentId)
            }
        "#;

        let variables = json!({
            "serviceId": service_id,
            "environmentId": self.config.railway.environment_id
        });

        info!("Redeploying Railway service instance: {}", service_id);

        self.make_railway_graphql_request(mutation, variables, "service instance redeploy")
            .await?;

        info!(
            "Successfully redeployed Railway service instance: {}",
            service_id
        );
        Ok(())
    }

    async fn get_all_workspaces(&self) -> Result<Vec<Workspace>> {
        let query = r#"
            SELECT id, name, description, created_at, updated_at
            FROM workspaces
            ORDER BY created_at DESC
        "#;

        let workspaces = sqlx::query_as::<_, Workspace>(query)
            .fetch_all(&self.pool)
            .await?;

        Ok(workspaces)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Infrastructure Manager service...");

    // Create the infra manager
    let infra_manager = InfraManager::new().await?;

    // Verify workspaces table exists (created by another service)
    if let Err(e) = infra_manager.verify_workspaces_table().await {
        error!("Failed to verify workspaces table: {}", e);
        return Err(e);
    }

    // Setup database trigger
    if let Err(e) = infra_manager.setup_database_trigger().await {
        error!("Failed to setup database trigger: {}", e);
        return Err(e);
    }

    // Test database connectivity with a simple query
    match infra_manager.get_all_workspaces().await {
        Ok(workspaces) => {
            info!("Found {} existing workspaces in database", workspaces.len());
        }
        Err(e) => {
            error!("Failed to query workspaces: {}", e);
            return Err(e);
        }
    }

    // Start listening for notifications
    info!("Infrastructure Manager service started successfully");
    infra_manager.listen_for_notifications().await?;

    Ok(())
}
