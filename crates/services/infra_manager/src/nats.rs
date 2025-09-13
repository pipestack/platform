use anyhow::Result;
use async_nats::Client;
use nats_io_jwt::{
    Account, Export, Exports, Import, Imports, JetStreamLimits, JetStreamTieredLimits,
    OperatorLimits, Permission, RenamingSubject, SigningKeys, StringList, Subject, Token, User,
};
use nkeys::{KeyPair, KeyPairType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsCredentials {
    pub account_nkey: String,
    pub account_jwt: String,
    pub user_nkey: String,
    pub user_jwt: String,
    pub user_seed: String,
}

#[derive(Debug, Clone)]
pub struct NatsAccountConfig {
    pub workspace_slug: String,
    // pub max_connections: Option<i64>,
    // pub max_data: Option<i64>,
    // pub max_exports: Option<i64>,
    // pub max_imports: Option<i64>,
    // pub max_subscriptions: Option<i64>,
    // pub tiered_limits: Option<NatsAccountTieredLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsAccountTieredLimits {
    pub r1: NatsJetStreamLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsJetStreamLimits {
    pub mem_storage: i64,
    pub disk_storage: i64,
    pub streams: i64,
    pub consumer: i64,
}

#[derive(Debug, Clone)]
pub struct NatsUserConfig {
    pub name: String,
    pub max_subscriptions: Option<i64>,
    pub max_data: Option<i64>,
    pub max_payload: Option<i64>,
}

pub struct NatsManager {
    operator_keypair: KeyPair,
    pipestack_account_keypair: KeyPair,
    client: Client,
    client_sys: Client,
}

impl NatsManager {
    /// Create a new NatsManager with an operator keypair, pipestack account keypair, and NATS client
    pub fn new(
        operator_seed: String,
        pipestack_account_seed: String,
        client: Client,
        client_sys: Client,
    ) -> Result<Self> {
        let operator_keypair = KeyPair::from_seed(&operator_seed)?;
        let pipestack_account_keypair = KeyPair::from_seed(&pipestack_account_seed)?;

        Ok(Self {
            operator_keypair,
            pipestack_account_keypair,
            client,
            client_sys,
        })
    }

    /// Create a new NATS account for a workspace and update the resolver
    pub async fn create_account(
        &self,
        config: NatsAccountConfig,
        pool: &PgPool,
    ) -> Result<(String, String)> {
        info!(
            "Creating NATS account for workspace: {}",
            config.workspace_slug
        );

        let account_keypair = KeyPair::new_account();
        let account_signing_key = KeyPair::new_account();
        let tiered_limits = {
            let mut tiered_map = HashMap::new();
            let jetstream_limits = JetStreamLimits {
                mem_storage: -1,
                disk_storage: -1,
                streams: -1,
                consumer: -1,
                ..Default::default()
            };
            tiered_map.insert("R1".to_string(), jetstream_limits);
            JetStreamTieredLimits(tiered_map)
        };
        let account_limits = OperatorLimits {
            subs: -1,
            max_ack_pending: -1,
            tiered_limits: Some(tiered_limits),
            ..Default::default()
        };
        let imports = Some(Imports(vec![Import {
            account: Some(self.pipestack_account_keypair.public_key()),
            local_subject: Some(RenamingSubject("wadm.api.>".to_string())),
            name: Some(format!("{}.wadm.api.>", config.workspace_slug)),
            subject: Some(Subject(format!(
                "{}.wadm.api.>",
                account_keypair.public_key()
            ))),
            type_: Some(nats_io_jwt::ExportType::Service),
            ..Default::default()
        }]));
        let exports = Some(Exports(vec![
            Export {
                name: Some("wasmbus.ctl.>".to_string()),
                response_type: Some(nats_io_jwt::ResponseType::Stream),
                subject: Some(Subject("wasmbus.ctl.>".to_string())),
                type_: Some(nats_io_jwt::ExportType::Service),
                ..Default::default()
            },
            Export {
                name: Some("wasmbus.evt.>".to_string()),
                subject: Some(Subject("wasmbus.evt.>".to_string())),
                type_: Some(nats_io_jwt::ExportType::Stream),
                ..Default::default()
            },
        ]));
        let account: Account = Account::builder()
            .signing_keys(SigningKeys::from(&account_signing_key))
            .imports(imports)
            .exports(exports)
            .limits(account_limits)
            .try_into()
            .expect("Account to be valid");
        let account_jwt = Token::new(account_keypair.public_key())
            .name(format!("{}_account", config.workspace_slug))
            .claims(account)
            .sign(&self.operator_keypair);

        // Update the NATS resolver with the new account JWT
        self.update_account_resolver(&account_jwt).await?;

        // Update pipestack_account with import from this workspace account
        self.update_pipestack_account_import(&config.workspace_slug, &account_keypair.public_key())
            .await?;

        // Persist the account's public key in the workspace database
        crate::database::update_workspace_nats_account(
            pool,
            &config.workspace_slug,
            &account_keypair.public_key(),
        )
        .await?;

        info!(
            "Successfully created NATS account for workspace: {}",
            config.workspace_slug
        );

        Ok((account_keypair.seed()?, account_jwt))
    }

    /// Create a new NATS user for an account
    pub fn create_user(
        &self,
        account_keypair: &KeyPair,
        config: NatsUserConfig,
        workspace_slug: &str,
    ) -> Result<(String, String)> {
        info!("Creating NATS user: {} for account", config.name);

        // Generate user keypair
        let user_keypair = KeyPair::new(KeyPairType::User);
        let user_public_key = user_keypair.public_key();

        // Create user with permissions and limits
        let mut user = User::builder();
        user = user.bearer_token(false);

        // Set user limits if provided
        if let Some(max_subscriptions) = config.max_subscriptions {
            user = user.subs(max_subscriptions);
        }
        if let Some(max_data) = config.max_data {
            user = user.data(max_data);
        }
        if let Some(max_payload) = config.max_payload {
            user = user.payload(max_payload);
        }

        // Add basic permissions for workspace isolation
        let pub_permissions = Permission {
            allow: Some(StringList(vec![
                "$JS.>".to_string(),
                "$KV.>".to_string(),
                "_INBOX.>".to_string(),
                "pipestack.>".to_string(),
                format!("{workspace_slug}.>"),
                "wasmbus.>".to_string(),
                "_R_.>".to_string(),
            ])),
            deny: None,
        };

        let sub_permissions = Permission {
            allow: Some(StringList(vec![
                "_INBOX.>".to_string(),
                "pipestack.>".to_string(),
                format!("{workspace_slug}.>"),
                "wasmbus.>".to_string(),
            ])),
            deny: None,
        };

        user = user.pub_(pub_permissions).sub(sub_permissions);

        let user_claims: User = user.try_into()?;

        // Create and sign the JWT token
        let user_jwt = Token::new(user_public_key.clone())
            .name(&config.name)
            .claims(user_claims)
            .sign(account_keypair);

        info!("Successfully created NATS user: {}", config.name);

        Ok((user_keypair.seed()?, user_jwt))
    }

    /// Create complete NATS credentials for a workspace
    pub async fn create_workspace_credentials(
        &self,
        workspace_slug: &str,
        pool: &PgPool,
    ) -> Result<NatsCredentials> {
        info!(
            "Creating complete NATS credentials for workspace: {}",
            workspace_slug
        );

        // Create account configuration
        let account_config = NatsAccountConfig {
            workspace_slug: workspace_slug.to_string(),
            // max_connections: Some(-1),
            // max_data: Some(-1),
            // max_exports: Some(-1),
            // max_imports: Some(-1),
            // max_subscriptions: Some(-1),
            // tiered_limits: Some(NatsAccountTieredLimits {
            //     r1: NatsJetStreamLimits {
            //         mem_storage: -1,
            //         disk_storage: -1,
            //         streams: -1,
            //         consumer: -1,
            //     },
            // }),
        };

        // Create account (will automatically update the resolver)
        let (account_seed, account_jwt) = self.create_account(account_config, pool).await?;
        let account_keypair = KeyPair::from_seed(&account_seed)?;

        // Create user configuration
        let user_config = NatsUserConfig {
            name: format!("wasmcloud_host_{}", workspace_slug),
            max_subscriptions: Some(-1),
            max_data: Some(-1),
            max_payload: Some(-1),
        };

        // Create user
        let (user_seed, user_jwt) =
            self.create_user(&account_keypair, user_config, workspace_slug)?;

        let credentials = NatsCredentials {
            account_nkey: account_keypair.public_key(),
            account_jwt,
            user_nkey: KeyPair::from_seed(&user_seed)?.public_key(),
            user_jwt,
            user_seed,
        };

        info!(
            "Successfully created complete NATS credentials for workspace: {}",
            workspace_slug
        );

        Ok(credentials)
    }

    /// Create and add a new import if it doesn't already exist
    fn create_and_add_import(
        existing_imports: &mut Vec<Import>,
        workspace_slug: &str,
        workspace_account_public_key: &str,
        subject_prefix: Option<&str>,
        subject: &str,
        export_type: nats_io_jwt::ExportType,
    ) {
        let new_import = Import {
            account: Some(workspace_account_public_key.to_string()),
            local_subject: Some(RenamingSubject(format!(
                "{}.{}.{}",
                subject_prefix.unwrap_or_default(),
                workspace_account_public_key,
                subject
            ))),
            name: Some(format!("{}-{}", workspace_slug, subject)),
            subject: Some(Subject(subject.to_string())),
            type_: Some(export_type),
            ..Default::default()
        };

        // Check if this import already exists (based on name)
        let import_name = new_import.name.as_ref().unwrap();
        if !existing_imports
            .iter()
            .any(|imp| imp.name.as_ref() == Some(import_name))
        {
            debug!("Adding new import: {:?}", new_import);
            existing_imports.push(new_import);
        }
    }

    /// Update the pipestack_account with an import from a workspace account
    async fn update_pipestack_account_import(
        &self,
        workspace_slug: &str,
        workspace_account_public_key: &str,
    ) -> Result<()> {
        info!(
            "Updating pipestack_account with import from workspace: {}",
            workspace_slug
        );

        // Get existing imports for pipestack account
        let mut existing_imports = self
            .get_existing_pipestack_imports()
            .await
            .unwrap_or_else(|_| Vec::new());
        debug!("Existing imports: {:?}", existing_imports);

        // Create the new wasmbus.ctl import for the workspace account's wasmbus service
        Self::create_and_add_import(
            &mut existing_imports,
            workspace_slug,
            workspace_account_public_key,
            None,
            "wasmbus.ctl.>",
            nats_io_jwt::ExportType::Service,
        );

        // Create the new wasmbus.evt import for the workspace account's wasmbus service
        Self::create_and_add_import(
            &mut existing_imports,
            workspace_slug,
            workspace_account_public_key,
            Some("mt."),
            "wasmbus.evt.>",
            nats_io_jwt::ExportType::Stream,
        );
        debug!("New imports: {:?}", existing_imports);

        // Recreate the pipestack account with all imports
        self.recreate_pipestack_account_with_imports(existing_imports)
            .await?;

        info!(
            "Successfully updated pipestack_account with import from workspace: {}",
            workspace_slug
        );
        Ok(())
    }

    /// Get existing imports from the pipestack account
    async fn get_existing_pipestack_imports(&self) -> Result<Vec<Import>> {
        let pipestack_account_public_key = self.pipestack_account_keypair.public_key();

        // Request the current account JWT from the resolver
        let response = self
            .client_sys
            .request(
                format!(
                    "$SYS.REQ.ACCOUNT.{}.CLAIMS.LOOKUP",
                    pipestack_account_public_key
                ),
                "".into(),
            )
            .await;

        // If the request fails, assume no existing imports (account might not exist yet)
        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                tracing::warn!(
                    "Failed to lookup existing account JWT: {}. Assuming no existing imports.",
                    e
                );
                return Ok(Vec::new());
            }
        };

        let response_str = match String::from_utf8(response.payload.to_vec()) {
            Ok(s) => s,
            Err(_) => {
                tracing::warn!(
                    "Invalid UTF-8 in account lookup response. Assuming no existing imports."
                );
                return Ok(Vec::new());
            }
        };
        tracing::info!("pipestack_account JWT lookup response: {}", response_str);

        // Handle error responses or empty responses
        if response_str.is_empty()
            || response_str.starts_with("Error")
            || response_str == "not found"
        {
            tracing::info!(
                "No existing account found or empty response. Starting with no imports."
            );
            return Ok(Vec::new());
        }

        // Try to parse imports from the JWT
        match Self::parse_jwt_imports(&response_str) {
            Ok(imports) => {
                tracing::info!("Successfully parsed {} existing imports", imports.len());
                Ok(imports)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse existing JWT imports: {}. Starting with no imports.",
                    e
                );
                Ok(Vec::new())
            }
        }
    }

    /// Parse raw JWT string and extract imports
    fn parse_jwt_imports(jwt_str: &str) -> Result<Vec<Import>> {
        // Split JWT into parts (header.payload.signature)
        let parts: Vec<&str> = jwt_str.trim().split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!(
                "Invalid JWT format: expected 3 parts, got {}",
                parts.len()
            ));
        }

        // Decode the payload (base64url)
        let payload = parts[1];
        let decoded_payload = Self::base64url_decode(payload)
            .map_err(|e| anyhow::anyhow!("Failed to decode JWT payload: {}", e))?;
        let payload_str = String::from_utf8(decoded_payload)
            .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in JWT payload: {}", e))?;

        // Parse payload as JSON
        let payload_json: Value = serde_json::from_str(&payload_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse JWT payload as JSON: {}", e))?;

        // Extract imports from the nats claim
        if let Some(nats_claims) = payload_json.get("nats")
            && let Some(imports_array) = nats_claims.get("imports").and_then(|i| i.as_array())
        {
            let mut imports = Vec::new();

            for (idx, import_json) in imports_array.iter().enumerate() {
                let import = Import {
                    account: import_json
                        .get("account")
                        .and_then(|a| a.as_str())
                        .map(|s| s.to_string()),
                    local_subject: import_json
                        .get("local_subject")
                        .and_then(|t| t.as_str())
                        .map(|s| RenamingSubject(s.to_string())),
                    name: import_json
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string()),
                    subject: import_json
                        .get("subject")
                        .and_then(|s| s.as_str())
                        .map(|s| Subject(s.to_string())),
                    type_: import_json
                        .get("type")
                        .and_then(|t| t.as_str())
                        .and_then(|t| match t {
                            "service" => Some(nats_io_jwt::ExportType::Service),
                            "stream" => Some(nats_io_jwt::ExportType::Stream),
                            _ => {
                                tracing::warn!("Unknown import type '{}' at index {}", t, idx);
                                None
                            }
                        }),
                    ..Default::default()
                };
                imports.push(import);
            }

            tracing::debug!("Parsed {} imports from JWT", imports.len());
            return Ok(imports);
        }

        // Check if there are any imports at all in the nats claims
        if payload_json.get("nats").is_some() {
            tracing::debug!("NATS claims found but no imports array");
        } else {
            tracing::debug!("No NATS claims found in JWT payload");
        }

        Ok(Vec::new())
    }

    /// Decode base64url (JWT uses base64url, not standard base64)
    fn base64url_decode(input: &str) -> Result<Vec<u8>> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

        URL_SAFE_NO_PAD
            .decode(input)
            .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))
    }

    /// Recreate the pipestack account with the given imports
    async fn recreate_pipestack_account_with_imports(&self, imports: Vec<Import>) -> Result<()> {
        let account_signing_key = KeyPair::new_account();
        let tiered_limits = {
            let mut tiered_map = HashMap::new();
            let jetstream_limits = JetStreamLimits {
                mem_storage: -1,
                disk_storage: -1,
                streams: -1,
                consumer: -1,
                ..Default::default()
            };
            tiered_map.insert("R1".to_string(), jetstream_limits);
            JetStreamTieredLimits(tiered_map)
        };
        let account_limits = OperatorLimits {
            subs: -1,
            max_ack_pending: -1,
            tiered_limits: Some(tiered_limits),
            ..Default::default()
        };

        // Set up imports
        let imports_list = if imports.is_empty() {
            None
        } else {
            Some(Imports(imports))
        };
        debug!(
            "Total imports: {:?}",
            imports_list.as_ref().map(|imports| imports.0.len())
        );

        // Create standard exports for pipestack account (WADM API)
        let exports = Some(Exports(vec![Export {
            name: Some("wadm.api.>".to_string()),
            subject: Some(Subject("*.wadm.api.>".to_string())),
            type_: Some(nats_io_jwt::ExportType::Service),
            account_token_position: Some(1),
            ..Default::default()
        }]));

        let account: Account = Account::builder()
            .signing_keys(SigningKeys::from(&account_signing_key))
            .imports(imports_list)
            .exports(exports)
            .limits(account_limits)
            .try_into()
            .expect("Account to be valid");
        debug!("New pipestack_account: {:?}", &account);

        // Create and sign the JWT
        let updated_jwt = Token::new(self.pipestack_account_keypair.public_key())
            .name("pipestack_account")
            .claims(account)
            .sign(&self.operator_keypair);
        debug!("New pipestack_account JWT: {:?}", &updated_jwt);

        // Update the resolver with the new JWT
        self.update_account_resolver(&updated_jwt).await?;

        Ok(())
    }

    /// Update the NATS resolver with an account JWT
    async fn update_account_resolver(&self, account_jwt: &str) -> Result<()> {
        info!("Updating NATS resolver with new account JWT");

        // Send the JWT to the NATS system account for resolver update
        self.client
            .publish(
                "$SYS.REQ.CLAIMS.UPDATE",
                account_jwt.as_bytes().to_vec().into(),
            )
            .await?;

        info!("Successfully updated NATS resolver with account JWT");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nats_io_jwt::{ExportType, Import, RenamingSubject, Subject};

    #[test]
    fn test_create_and_add_import_new_import() {
        let mut existing_imports = Vec::new();
        let workspace_slug = "test-workspace";
        let workspace_account_public_key =
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR";
        let subject = "wasmbus.ctl.>";
        let export_type = ExportType::Service;

        NatsManager::create_and_add_import(
            &mut existing_imports,
            workspace_slug,
            workspace_account_public_key,
            None,
            subject,
            export_type,
        );

        assert_eq!(existing_imports.len(), 1);
        let import = &existing_imports[0];

        assert_eq!(
            import.account,
            Some(workspace_account_public_key.to_string())
        );
        assert_eq!(
            import.local_subject,
            Some(RenamingSubject(format!(
                "{}.{}",
                workspace_account_public_key, subject
            )))
        );
        assert_eq!(import.name, Some(format!("{}-{}", workspace_slug, subject)));
        assert_eq!(import.subject, Some(Subject(subject.to_string())));
        assert_eq!(import.type_, Some(export_type));
    }

    #[test]
    fn test_create_and_add_import_duplicate_import() {
        let workspace_slug = "test-workspace";
        let workspace_account_public_key =
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR";
        let subject = "wasmbus.ctl.>";
        let export_type = ExportType::Service;

        // Create an existing import with the same name
        let existing_import = Import {
            account: Some(workspace_account_public_key.to_string()),
            local_subject: Some(RenamingSubject(format!(
                "{}.{}",
                workspace_account_public_key, subject
            ))),
            name: Some(format!("{}-{}", workspace_slug, subject)),
            subject: Some(Subject(subject.to_string())),
            type_: Some(export_type),
            ..Default::default()
        };

        let mut existing_imports = vec![existing_import];
        let original_len = existing_imports.len();

        NatsManager::create_and_add_import(
            &mut existing_imports,
            workspace_slug,
            workspace_account_public_key,
            None,
            subject,
            export_type,
        );

        // Should not add duplicate import
        assert_eq!(existing_imports.len(), original_len);
    }

    #[test]
    fn test_create_and_add_import_different_workspaces() {
        let mut existing_imports = Vec::new();
        let workspace_account_public_key =
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR";
        let subject = "wasmbus.ctl.>";
        let export_type = ExportType::Service;

        // Add import for first workspace
        NatsManager::create_and_add_import(
            &mut existing_imports,
            "workspace1",
            workspace_account_public_key,
            None,
            subject,
            export_type,
        );

        // Add import for second workspace
        NatsManager::create_and_add_import(
            &mut existing_imports,
            "workspace2",
            workspace_account_public_key,
            None,
            subject,
            export_type,
        );

        assert_eq!(existing_imports.len(), 2);

        let names: Vec<_> = existing_imports
            .iter()
            .filter_map(|imp| imp.name.as_ref())
            .collect();

        assert!(names.contains(&&"workspace1-wasmbus.ctl.>".to_string()));
        assert!(names.contains(&&"workspace2-wasmbus.ctl.>".to_string()));
    }

    #[test]
    fn test_parse_jwt_imports_valid_jwt() {
        // Create a mock JWT payload with imports
        let payload = r#"{
            "iss": "OABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
            "sub": "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
            "iat": 1234567890,
            "nats": {
                "imports": [
                    {
                        "name": "workspace1-wasmbus.ctl.>",
                        "subject": "wasmbus.ctl.>",
                        "account": "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
                        "to": "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR.wasmbus.ctl.>",
                        "type": "service"
                    },
                    {
                        "name": "workspace1-wasmbus.evt.>",
                        "subject": "wasmbus.evt.>",
                        "account": "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
                        "to": "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR.wasmbus.evt.>",
                        "type": "stream"
                    }
                ]
            }
        }"#;

        // Base64url encode the payload
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let encoded_payload = URL_SAFE_NO_PAD.encode(payload.as_bytes());

        // Create a mock JWT (header.payload.signature)
        let jwt = format!(
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.{}.signature",
            encoded_payload
        );

        let imports = NatsManager::parse_jwt_imports(&jwt).unwrap();

        assert_eq!(imports.len(), 2);

        // Check first import
        assert_eq!(
            imports[0].name,
            Some("workspace1-wasmbus.ctl.>".to_string())
        );
        assert_eq!(
            imports[0].subject,
            Some(Subject("wasmbus.ctl.>".to_string()))
        );
        assert_eq!(imports[0].type_, Some(ExportType::Service));

        // Check second import
        assert_eq!(
            imports[1].name,
            Some("workspace1-wasmbus.evt.>".to_string())
        );
        assert_eq!(
            imports[1].subject,
            Some(Subject("wasmbus.evt.>".to_string()))
        );
        assert_eq!(imports[1].type_, Some(ExportType::Stream));
    }

    // Integration-style test that combines both functions
    #[test]
    fn test_import_management_workflow() {
        let mut existing_imports = Vec::new();

        // Simulate adding imports for multiple workspaces
        NatsManager::create_and_add_import(
            &mut existing_imports,
            "workspace1",
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
            None,
            "wasmbus.ctl.>",
            ExportType::Service,
        );

        NatsManager::create_and_add_import(
            &mut existing_imports,
            "workspace1",
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
            Some("mt."),
            "wasmbus.evt.>",
            ExportType::Stream,
        );

        NatsManager::create_and_add_import(
            &mut existing_imports,
            "workspace2",
            "AABC456DEFGHIJKLMNOPQRSTUVWXYZ789012345ABCDEFGHIJKLMNOPQR",
            None,
            "wasmbus.ctl.>",
            ExportType::Service,
        );

        assert_eq!(existing_imports.len(), 3);

        // Verify all imports have unique names
        let names: Vec<_> = existing_imports
            .iter()
            .filter_map(|imp| imp.name.as_ref())
            .collect();

        assert!(names.contains(&&"workspace1-wasmbus.ctl.>".to_string()));
        assert!(names.contains(&&"workspace1-wasmbus.evt.>".to_string()));
        assert!(names.contains(&&"workspace2-wasmbus.ctl.>".to_string()));

        // Try to add duplicate - should not increase count
        NatsManager::create_and_add_import(
            &mut existing_imports,
            "workspace1",
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
            None,
            "wasmbus.ctl.>",
            ExportType::Service,
        );

        assert_eq!(existing_imports.len(), 3); // Should remain the same
    }

    #[test]
    fn test_create_and_add_import_edge_cases() {
        let mut existing_imports = Vec::new();

        // Test with empty workspace slug
        NatsManager::create_and_add_import(
            &mut existing_imports,
            "",
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
            None,
            "test.>",
            ExportType::Service,
        );

        assert_eq!(existing_imports.len(), 1);
        assert_eq!(existing_imports[0].name, Some("-test.>".to_string()));

        // Test with special characters in subject
        NatsManager::create_and_add_import(
            &mut existing_imports,
            "workspace-with-dashes",
            "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
            None,
            "special.subject.*.>",
            ExportType::Stream,
        );

        assert_eq!(existing_imports.len(), 2);
        assert_eq!(
            existing_imports[1].name,
            Some("workspace-with-dashes-special.subject.*.>".to_string())
        );
    }

    #[test]
    fn test_create_and_add_import_preserves_existing_order() {
        let mut existing_imports = Vec::new();

        // Add multiple imports and verify order is preserved
        let workspaces = vec!["workspace1", "workspace2", "workspace3"];
        let subjects = vec!["ctl.>", "evt.>", "mgmt.>"];

        for workspace in &workspaces {
            for subject in &subjects {
                NatsManager::create_and_add_import(
                    &mut existing_imports,
                    workspace,
                    "AABC123DEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJKLMNOPQR",
                    None,
                    subject,
                    ExportType::Service,
                );
            }
        }

        assert_eq!(existing_imports.len(), 9); // 3 workspaces * 3 subjects

        // Verify first few imports are in expected order
        assert_eq!(
            existing_imports[0].name,
            Some("workspace1-ctl.>".to_string())
        );
        assert_eq!(
            existing_imports[1].name,
            Some("workspace1-evt.>".to_string())
        );
        assert_eq!(
            existing_imports[2].name,
            Some("workspace1-mgmt.>".to_string())
        );
        assert_eq!(
            existing_imports[3].name,
            Some("workspace2-ctl.>".to_string())
        );
    }
}
