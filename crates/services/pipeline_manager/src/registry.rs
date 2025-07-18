use crate::config_converter::PipelineNodeType;
use crate::{DeployRequest, settings::Settings};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use tracing::{error, info};
use wash::lib::registry::{OciPushOptions, push_oci_artifact};

pub async fn test_registry_connectivity(
    registry_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;
    let version_url = format!("{registry_url}/v2/");

    info!("Testing registry connectivity at: {}", version_url);

    let response = match client.get(&version_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to connect to registry at {}: {}", registry_url, e);
            return Err(format!("Registry connection failed: {e}").into());
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response".to_string());
        error!("Registry API v2 check failed: HTTP {} - {}", status, body);
        return Err(format!("Registry API v2 not supported: HTTP {status} - {body}").into());
    }

    info!("Registry connectivity test successful");
    Ok(())
}

pub async fn publish_wasm_components(
    payload: &DeployRequest,
    settings: &Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Filter nodes to only processor-wasm types
    let wasm_nodes: Vec<_> = payload
        .pipeline
        .nodes
        .iter()
        .filter(|node| matches!(node.step_type, PipelineNodeType::ProcessorWasm))
        .collect();

    if wasm_nodes.is_empty() {
        info!("No processor-wasm nodes found in pipeline");
        return Ok(());
    }

    info!("Found {} processor-wasm nodes to publish", wasm_nodes.len());

    let r2_endpoint = format!(
        "https://{}.r2.cloudflarestorage.com/{}",
        &settings.cloudflare.account_id, &settings.cloudflare.r2_bucket
    );

    info!("Using R2 endpoint: {}", r2_endpoint);

    let mut failed_nodes = Vec::new();

    for node in wasm_nodes {
        let node_id = &node.name;
        info!("Processing wasm node: {}", node_id);

        // Construct R2 key path
        let r2_key = format!(
            "{}/pipeline/{}/{}/builder/components/nodes/processor/wasm/{}.wasm",
            payload.workspace_slug, payload.pipeline.name, payload.pipeline.version, node_id
        );

        // Fetch WASM component from Cloudflare R2
        let wasm_data = match fetch_wasm_from_r2(
            &client,
            &r2_endpoint,
            &r2_key,
            &settings.cloudflare.r2_access_key_id,
            &settings.cloudflare.r2_secret_access_key,
        )
        .await
        {
            Ok(data) => data,
            Err(e) => {
                error!(
                    "Failed to fetch WASM component from R2 for node {}: {}",
                    node_id, e
                );
                failed_nodes.push(node_id.clone());
                continue;
            }
        };

        // Publish to OCI registry
        info!(
            "Publishing node {} to registry at: {}",
            node_id, &settings.registry.url
        );

        // Test registry connectivity first
        info!(
            "Testing registry connectivity before publishing node: {}",
            node_id
        );
        if let Err(e) = test_registry_connectivity(&settings.registry.url).await {
            error!(
                "Registry connectivity test failed for node {}: {}",
                node_id, e
            );
            failed_nodes.push(node_id.clone());
            continue;
        }
        let image_name = format!(
            "{}/pipeline/{}/{}/builder/components/nodes/processor/wasm/{}",
            payload.workspace_slug, payload.pipeline.name, payload.pipeline.version, node_id
        );
        let tag = "1.0.0";

        // Create a temporary file for the WASM data
        let temp_file = format!("/tmp/{node_id}.wasm");
        if let Err(e) = tokio::fs::write(&temp_file, &wasm_data).await {
            error!("Failed to write WASM data to temporary file: {}", e);
            failed_nodes.push(node_id.clone());
            continue;
        }

        let full_image_ref = format!(
            "{}:{}",
            if settings.registry.url.starts_with("http://")
                || settings.registry.url.starts_with("https://")
            {
                let registry_without_protocol = &settings
                    .registry
                    .url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/');
                format!("{registry_without_protocol}/{image_name}")
            } else {
                format!("{}/{}", &settings.registry.url, image_name)
            },
            tag
        );
        info!("Full image ref to push: {}", &full_image_ref);

        let push_options = OciPushOptions {
            insecure: settings.registry.url.starts_with("http://"),
            ..Default::default()
        };

        match push_oci_artifact(full_image_ref, temp_file.clone(), push_options).await {
            Ok(_) => {
                info!("Successfully published {} to OCI registry", node_id);
                // Clean up temporary file
                if let Err(e) = tokio::fs::remove_file(&temp_file).await {
                    error!("Failed to clean up temporary file {}: {}", temp_file, e);
                }
            }
            Err(e) => {
                error!("Failed to publish {} to OCI registry: {}", node_id, e);
                failed_nodes.push(node_id.clone());
                // Clean up temporary file
                if let Err(e) = tokio::fs::remove_file(&temp_file).await {
                    error!("Failed to clean up temporary file {}: {}", temp_file, e);
                }
            }
        }
    }

    if !failed_nodes.is_empty() {
        return Err(format!(
            "Failed to publish {} nodes: {:?}",
            failed_nodes.len(),
            failed_nodes
        )
        .into());
    }

    Ok(())
}

async fn fetch_wasm_from_r2(
    client: &reqwest::Client,
    r2_endpoint: &str,
    key: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let r2_url = format!("{r2_endpoint}/{key}");

    info!("Fetching WASM component from R2: {}", r2_url);

    let mut request = client.get(&r2_url);

    // Add AWS Signature V4 authentication
    let url = reqwest::Url::parse(&r2_url)?;
    let host = url.host_str().ok_or("Invalid R2 URL")?;
    let path = url.path();

    // Create AWS Signature V4 headers
    let now = chrono::Utc::now();
    let date = now.format("%Y%m%d").to_string();
    let datetime = now.format("%Y%m%dT%H%M%SZ").to_string();

    let region = "auto"; // Cloudflare R2 uses "auto" as region
    let service = "s3";

    // Create canonical request
    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:UNSIGNED-PAYLOAD\nx-amz-date:{datetime}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request =
        format!("GET\n{path}\n\n{canonical_headers}\n{signed_headers}\nUNSIGNED-PAYLOAD");

    // Create string to sign
    let credential_scope = format!("{date}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{:x}",
        datetime,
        credential_scope,
        Sha256::digest(canonical_request.as_bytes())
    );

    // Calculate signature
    let signing_key = get_signing_key(secret_key, &date, region, service)?;
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    // Create authorization header
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    request = request
        .header("Authorization", authorization)
        .header("x-amz-date", datetime)
        .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD");

    let response = request.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response body".to_string());
        return Err(format!("Failed to fetch from R2: HTTP {status} - {body}").into());
    }

    let wasm_data = response.bytes().await?;
    info!("Successfully fetched {} bytes from R2", wasm_data.len());

    Ok(wasm_data.to_vec())
}

fn get_signing_key(
    secret_key: &str,
    date: &str,
    region: &str,
    service: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    Ok(k_signing)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}
