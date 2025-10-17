use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info, warn};

use crate::{WorkspaceNotification, config::AppConfig, nats::NatsCredentials};

#[derive(Debug, Serialize)]
struct RailwayServiceSource {
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

#[derive(Debug, Deserialize)]
struct RailwayDomainCreateResponse {
    data: Option<RailwayDomainCreateData>,
    errors: Option<Vec<RailwayError>>,
}

#[derive(Debug, Deserialize)]
struct RailwayDomainCreateData {
    #[serde(rename = "serviceDomainCreate")]
    service_domain_create: Option<RailwayDomain>,
}

#[derive(Debug, Deserialize)]
struct RailwayDomain {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DeploymentListResponse {
    data: Option<DeploymentListData>,
    errors: Option<Vec<RailwayError>>,
}

#[derive(Debug, Deserialize)]
struct DeploymentListData {
    deployments: DeploymentEdges,
}

#[derive(Debug, Deserialize)]
struct DeploymentEdges {
    edges: Vec<DeploymentEdge>,
}

#[derive(Debug, Deserialize, Clone)]
struct DeploymentEdge {
    node: DeploymentNode,
}

#[derive(Debug, Deserialize, Clone)]
struct DeploymentNode {
    #[serde(rename = "createdAt")]
    created_at: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct ServiceInstanceUpdateInput {
    builder: String,
    #[serde(rename = "railwayConfigFile")]
    railway_config_file: String,
    region: String,
    #[serde(rename = "rootDirectory")]
    root_directory: String,
}

#[derive(Debug, Serialize)]
struct RailwayServiceInput {
    branch: String,
    #[serde(rename = "environmentId")]
    environment_id: String,
    name: String,
    #[serde(rename = "projectId")]
    project_id: String,
    source: RailwayServiceSource,
    variables: std::collections::HashMap<String, String>,
}

pub async fn try_to_create_service(
    app_config: &AppConfig,
    workspace: WorkspaceNotification,
    nats_credentials: &NatsCredentials,
) {
    // Try to create Railway service with retries
    let mut retry_count = 0;
    let mut success = false;

    while retry_count < app_config.service.max_retries && !success {
        match create_railway_service(app_config, &workspace, nats_credentials).await {
            Ok(_) => {
                success = true;
                info!(
                    "Successfully created Railway service for workspace {} on attempt {}",
                    workspace.slug,
                    retry_count + 1
                );
            }
            Err(e) => {
                retry_count += 1;
                error!(
                    "Failed to create Railway service for workspace {} (attempt {}): {}",
                    workspace.slug, retry_count, e
                );

                if retry_count < app_config.service.max_retries {
                    info!("Retrying in {}ms...", app_config.service.retry_delay_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        app_config.service.retry_delay_ms,
                    ))
                    .await;
                }
            }
        }
    }

    if !success {
        error!(
            "Failed to create Railway service for workspace {} after {} attempts",
            workspace.slug, app_config.service.max_retries
        );
    }
}

async fn create_railway_service(
    app_config: &AppConfig,
    workspace: &WorkspaceNotification,
    nats_credentials: &NatsCredentials,
) -> Result<()> {
    let service_name = format!("{}-{}", app_config.service.name_prefix, &workspace.slug);

    let mutation = r#"
            mutation ServiceCreate($input: ServiceCreateInput!) {
                serviceCreate(input: $input) {
                    id
                    name
                }
            }
        "#;

    let mut env_variables = std::collections::HashMap::new();
    env_variables.insert(
            "RUST_LOG".to_string(),
            "debug,hyper=info,async_nats=info,oci_client=info,cranelift_codegen=warn,opentelemetry-http=warn".to_string(),
        );
    env_variables.insert(
        "WASMCLOUD_CTL_HOST".to_string(),
        "${{nats.RAILWAY_PRIVATE_DOMAIN}}".to_string(),
    );
    env_variables.insert("WASMCLOUD_LOG_LEVEL".to_string(), "debug".to_string());
    env_variables.insert(
        "WASMCLOUD_OCI_ALLOWED_INSECURE".to_string(),
        "${{registry.RAILWAY_PRIVATE_DOMAIN}}:5000".to_string(),
    );
    env_variables.insert(
        "WASMCLOUD_RPC_HOST".to_string(),
        "${{nats.RAILWAY_PRIVATE_DOMAIN}}".to_string(),
    );
    env_variables.insert(
        "WASMCLOUD_OBSERVABILITY_ENABLED".to_string(),
        "true".to_string(),
    );
    env_variables.insert(
        "OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
        "http://${{otelcol.RAILWAY_PRIVATE_DOMAIN}}:4318".to_string(),
    );
    env_variables.insert("WASMCLOUD_LATTICE".to_string(), workspace.slug.clone());
    env_variables.insert("WASMCLOUD_JS_DOMAIN".to_string(), "pipestack".to_string());

    env_variables.insert(
        "WASMCLOUD_NATS_JWT".to_string(),
        nats_credentials.user_jwt.clone(),
    );
    env_variables.insert(
        "WASMCLOUD_NATS_SEED".to_string(),
        nats_credentials.user_seed.clone(),
    );

    let variables = json!({
        "input": RailwayServiceInput {
            branch: app_config.railway.default_branch.clone(),
            environment_id: app_config.railway.environment_id.clone(),
            name: service_name.clone(),
            project_id: app_config.railway.project_id.clone(),
            source: RailwayServiceSource {
                repo: app_config.railway.default_template_repo.clone(),
            },
            variables: env_variables,
        }
    });

    info!(
        "Creating Railway service: {}. GraphQL query variables: {:?}",
        service_name, variables
    );

    let response_text =
        make_railway_graphql_request(app_config, mutation, variables, "service creation").await?;

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
            update_service_instance(app_config, &service.id).await?;

            // Create a domain for the service
            create_service_domain(app_config, &service.id, &workspace.slug).await?;

            // Redeploy the service instance
            redeploy_service_instance(app_config, &service.id).await?;

            // Wait for deployment to succeed
            wait_for_deployment_success(app_config, &service.id).await?;

            // Notify pipeline manager about the new deployment
            notify_pipeline_manager(&workspace.slug).await?;
        } else {
            warn!("Railway service creation succeeded but no service data returned");
        }
    } else {
        warn!("Railway service creation response contained no data");
    }

    Ok(())
}

async fn make_railway_graphql_request(
    app_config: &AppConfig,
    mutation: &str,
    variables: serde_json::Value,
    operation_name: &str,
) -> Result<String> {
    let request_body = json!({
        "query": mutation,
        "variables": variables
    });

    info!("Making Railway GraphQL request: {}", operation_name);

    let response = Client::new()
        .post(&app_config.railway.api_url)
        .header(
            "Authorization",
            format!("Bearer {}", app_config.railway.token),
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

async fn update_service_instance(app_config: &AppConfig, service_id: &str) -> Result<()> {
    let mutation = r#"
        mutation ServiceInstanceUpdate($serviceId: String!, $environmentId: String, $input: ServiceInstanceUpdateInput!) {
            serviceInstanceUpdate(serviceId: $serviceId, environmentId: $environmentId, input: $input)
        }
    "#;

    let variables = json!({
        "serviceId": service_id,
        "environmentId": app_config.railway.environment_id,
        "input": ServiceInstanceUpdateInput {
            builder: "NIXPACKS".to_string(),
            railway_config_file: "./services/wasmcloud/railway.json".to_string(),
            region: "us-east4-eqdc4a".to_string(),
            root_directory: "/services/wasmcloud".to_string(),
        }
    });

    info!("Updating Railway service instance: {}", service_id);

    make_railway_graphql_request(app_config, mutation, variables, "service instance update")
        .await?;

    info!(
        "Successfully updated Railway service instance: {}",
        service_id
    );
    Ok(())
}

async fn create_service_domain(
    app_config: &AppConfig,
    service_id: &str,
    workspace_slug: &str,
) -> Result<()> {
    let mutation = r#"
        mutation ServiceDomainCreate($input: ServiceDomainCreateInput!) {
            serviceDomainCreate(input: $input) {
                id
            }
        }
    "#;

    let variables = json!({
        "input": {
            "environmentId": app_config.railway.environment_id,
            "serviceId": service_id,
            "targetPort": 8000
        }
    });

    info!("Creating domain for Railway service: {}", service_id);

    let response_text =
        make_railway_graphql_request(app_config, mutation, variables, "service domain create")
            .await?;

    let domain_response: RailwayDomainCreateResponse = serde_json::from_str(&response_text)?;

    if let Some(errors) = domain_response.errors {
        for error in errors {
            error!("Railway domain creation error: {}", error.message);
        }
        return Err(anyhow::anyhow!(
            "Railway domain creation API returned errors"
        ));
    }

    if let Some(data) = domain_response.data {
        if let Some(domain) = data.service_domain_create {
            info!(
                "Successfully created domain for Railway service: {} (Domain ID: {})",
                service_id, domain.id
            );

            // Update the domain with a better name
            update_service_domain(app_config, &domain.id, service_id, workspace_slug).await?;
        } else {
            return Err(anyhow::anyhow!(
                "Domain creation succeeded but no domain data returned"
            ));
        }
    } else {
        return Err(anyhow::anyhow!(
            "Domain creation response contained no data"
        ));
    }

    Ok(())
}

async fn update_service_domain(
    app_config: &AppConfig,
    domain_id: &str,
    service_id: &str,
    workspace_slug: &str,
) -> Result<()> {
    let domain_name = format!("pipestack-{workspace_slug}.up.railway.app");

    let mutation = r#"
        mutation serviceDomainUpdate($input: ServiceDomainUpdateInput!) {
            serviceDomainUpdate(input: $input)
        }
    "#;

    let variables = json!({
        "input": {
            "domain": domain_name,
            "environmentId": app_config.railway.environment_id,
            "serviceDomainId": domain_id,
            "serviceId": service_id,
            "targetPort": 8000
        }
    });

    info!(
        "Updating domain for Railway service: {} with domain name: {}",
        service_id, domain_name
    );

    make_railway_graphql_request(app_config, mutation, variables, "service domain update").await?;

    info!(
        "Successfully updated domain for Railway service: {} to {}",
        service_id, domain_name
    );
    Ok(())
}

async fn redeploy_service_instance(app_config: &AppConfig, service_id: &str) -> Result<()> {
    let mutation = r#"
        mutation serviceInstanceRedeploy($serviceId: String!, $environmentId: String!) {
            serviceInstanceRedeploy(serviceId: $serviceId, environmentId: $environmentId)
        }
    "#;

    let variables = json!({
        "serviceId": service_id,
        "environmentId": app_config.railway.environment_id
    });

    info!("Redeploying Railway service instance: {}", service_id);

    make_railway_graphql_request(app_config, mutation, variables, "service instance redeploy")
        .await?;

    info!(
        "Successfully redeployed Railway service instance: {}",
        service_id
    );
    Ok(())
}

async fn wait_for_deployment_success(app_config: &AppConfig, service_id: &str) -> Result<()> {
    let query = r#"
        query GetDeployments($input: DeploymentListInput!) {
            deployments(input: $input) {
                edges {
                    node {
                        createdAt
                        status
                    }
                }
            }
        }
    "#;

    let max_attempts = 90;
    let sleep_duration = tokio::time::Duration::from_secs(5);
    let mut attempts = 0;

    info!("Checking deployment status for service: {}", service_id);

    loop {
        attempts += 1;

        if attempts > max_attempts {
            return Err(anyhow::anyhow!(
                "Deployment did not succeed within the timeout period"
            ));
        }

        let variables = json!({
            "input": {
                "environmentId": app_config.railway.environment_id,
                "serviceId": service_id.to_string(),
                "includeDeleted": false
            }
        });

        match make_railway_graphql_request(app_config, query, variables, "deployment status check")
            .await
        {
            Ok(response_text) => {
                let deployment_response: DeploymentListResponse =
                    match serde_json::from_str(&response_text) {
                        Ok(response) => response,
                        Err(e) => {
                            warn!("Failed to parse deployment status response: {}", e);
                            tokio::time::sleep(sleep_duration).await;
                            continue;
                        }
                    };

                if let Some(errors) = deployment_response.errors {
                    for error in errors {
                        error!("Railway deployment status error: {}", error.message);
                    }
                    tokio::time::sleep(sleep_duration).await;
                    continue;
                }

                if let Some(data) = deployment_response.data {
                    let deployments = &data.deployments.edges;

                    if deployments.len() >= 2 {
                        // Sort deployments by createdAt (most recent first)
                        let mut sorted_deployments = deployments.to_vec();
                        sorted_deployments
                            .sort_by(|a, b| b.node.created_at.cmp(&a.node.created_at));

                        let most_recent = &sorted_deployments[0].node;

                        if most_recent.status == "SUCCESS" {
                            info!(
                                "Deployment succeeded! Status: {}, Created: {}",
                                most_recent.status, most_recent.created_at
                            );
                            return Ok(());
                        } else {
                            info!(
                                "Most recent deployment status: {} (waiting for SUCCESS)",
                                most_recent.status
                            );
                        }
                    } else {
                        info!(
                            "Waiting for deployments to appear (found: {})",
                            deployments.len()
                        );
                    }
                } else {
                    warn!("Deployment status response contained no data");
                }
            }
            Err(e) => {
                warn!("Failed to check deployment status: {}", e);
            }
        }

        tokio::time::sleep(sleep_duration).await;
    }
}

async fn notify_pipeline_manager(workspace_slug: &str) -> Result<()> {
    let url = "http://pipeline-manager.railway.internal:3000/deploy-providers";

    let payload = json!({
        "workspaceSlug": workspace_slug
    });

    info!(
        "Notifying pipeline manager for workspace: {}",
        workspace_slug
    );

    let response = Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        info!(
            "Successfully notified pipeline manager for workspace: {}",
            workspace_slug
        );
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        error!(
            "Failed to notify pipeline manager. Status: {}, Error: {}",
            status, error_text
        );
        return Err(anyhow::anyhow!(
            "Pipeline manager notification failed with status: {}",
            status
        ));
    }

    Ok(())
}
