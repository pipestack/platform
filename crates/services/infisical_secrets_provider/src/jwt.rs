use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// JWT claims structure for wasmCloud components and providers
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct JwtClaims {
    /// Issuer
    pub iss: Option<String>,
    /// Subject (component/provider ID)
    pub sub: Option<String>,
    /// Audience
    pub aud: Option<String>,
    /// Expiration time (Unix timestamp)
    pub exp: Option<i64>,
    /// Not before (Unix timestamp)
    pub nbf: Option<i64>,
    /// Issued at (Unix timestamp)
    pub iat: Option<i64>,
    /// JWT ID
    pub jti: Option<String>,
    /// wasmCloud specific claims
    pub wascap: Option<WascapClaims>,
    /// wasmCloud capability revision (added to match actual JWT structure)
    pub wascap_revision: Option<i64>,
}

/// wasmCloud specific claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct WascapClaims {
    pub hash: Option<String>,
    /// Component/provider name
    pub name: Option<String>,
    /// Component/provider capabilities
    pub caps: Option<Vec<String>>,
    /// Component/provider tags (changed from HashMap to Vec<String> to match actual JWT)
    pub tags: Option<Vec<String>>,
    /// Component revision (moved from nested ComponentInfo to match actual JWT)
    pub rev: Option<i32>,
    /// Component version (moved from nested ComponentInfo to match actual JWT)
    pub ver: Option<String>,
    /// Whether this is a provider (false for components, added to match actual JWT)
    pub prov: Option<bool>,
    /// Provider-specific information
    pub provider: Option<ProviderInfo>,
    /// Component-specific information
    pub component: Option<ComponentInfo>,
}

/// Provider-specific information
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider vendor
    pub vendor: Option<String>,
    /// Provider service
    pub service: Option<String>,
}

/// Component-specific information
#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Component revision
    pub rev: Option<i32>,
    /// Component version
    pub ver: Option<String>,
}

/// JWT validation result
#[derive(Debug)]
pub struct JwtValidationResult {
    pub claims: JwtClaims,
    pub is_valid: bool,
    pub validation_errors: Vec<String>,
}

/// JWT validator for wasmCloud components and providers
pub struct JwtValidator {
    /// Whether to enforce expiration checks
    enforce_expiration: bool,
    /// Maximum allowed clock skew in seconds
    clock_skew_seconds: i64,
}

impl Default for JwtValidator {
    fn default() -> Self {
        Self::new(false, 300) // 5 minutes clock skew by default
    }
}

impl JwtValidator {
    /// Creates a new JWT validator
    pub fn new(enforce_expiration: bool, clock_skew_seconds: i64) -> Self {
        Self {
            enforce_expiration,
            clock_skew_seconds,
        }
    }

    /// Validates a JWT token (basic validation without signature verification)
    /// In a production environment, you would want to verify the signature as well
    pub fn validate_token(&self, token: &str) -> Result<JwtValidationResult> {
        debug!("Validating JWT token");

        if token.is_empty() {
            return Ok(JwtValidationResult {
                claims: JwtClaims::default(),
                is_valid: false,
                validation_errors: vec!["Token is empty".to_string()],
            });
        }

        // Parse the JWT token
        let claims = match self.parse_token(token) {
            Ok(claims) => claims,
            Err(e) => {
                warn!("Failed to parse JWT token: {}", e);
                return Ok(JwtValidationResult {
                    claims: JwtClaims::default(),
                    is_valid: false,
                    validation_errors: vec![format!("Failed to parse token: {}", e)],
                });
            }
        };

        // Validate the claims
        let mut validation_errors = Vec::new();

        // Check expiration if enforced
        if self.enforce_expiration {
            if let Some(exp) = claims.exp {
                let now = chrono::Utc::now().timestamp();
                if exp < (now - self.clock_skew_seconds) {
                    validation_errors.push("Token has expired".to_string());
                }
            } else {
                validation_errors.push("Token missing expiration claim".to_string());
            }
        }

        // Check not before if present
        if let Some(nbf) = claims.nbf {
            let now = chrono::Utc::now().timestamp();
            if nbf > (now + self.clock_skew_seconds) {
                validation_errors.push("Token not yet valid".to_string());
            }
        }

        // Check that required claims are present
        if claims.sub.is_none() {
            validation_errors.push("Token missing subject claim".to_string());
        }

        let is_valid = validation_errors.is_empty();

        if is_valid {
            debug!("JWT token validation successful");
        } else {
            warn!("JWT token validation failed: {:?}", validation_errors);
        }

        Ok(JwtValidationResult {
            claims,
            is_valid,
            validation_errors,
        })
    }

    /// Parses a JWT token and extracts claims (without signature verification)
    fn parse_token(&self, token: &str) -> Result<JwtClaims> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid JWT format - expected 3 parts"));
        }

        // Decode the payload (second part)
        let payload_b64 = parts[1];
        let payload_bytes = BASE64_NO_PAD
            .decode(payload_b64)
            .context("Failed to decode JWT payload")?;

        let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
            .with_context(|| {
                format!(
                    "Failed to parse JWT claims from payload. Payload length: {} bytes, payload as string: '{}'",
                    payload_bytes.len(),
                    String::from_utf8_lossy(&payload_bytes)
                )
            })?;

        Ok(claims)
    }

    /// Extracts component/provider name from JWT claims
    #[cfg(test)]
    pub fn extract_name(claims: &JwtClaims) -> Option<String> {
        claims.wascap.as_ref().and_then(|w| w.name.clone())
    }

    /// Checks if the JWT is for a component
    #[cfg(test)]
    pub fn is_component(claims: &JwtClaims) -> bool {
        claims
            .wascap
            .as_ref()
            .map(|w| w.prov == Some(false) || (w.prov.is_none() && w.component.is_some()))
            .unwrap_or(false)
    }

    /// Checks if the JWT is for a provider
    #[cfg(test)]
    pub fn is_provider(claims: &JwtClaims) -> bool {
        claims
            .wascap
            .as_ref()
            .map(|w| w.prov == Some(true) || (w.prov.is_none() && w.provider.is_some()))
            .unwrap_or(false)
    }
}

impl JwtValidationResult {
    /// Returns true if the token is valid
    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Returns validation errors
    pub fn errors(&self) -> &[String] {
        &self.validation_errors
    }

    /// Returns the subject ID if present
    pub fn subject_id(&self) -> Option<&str> {
        self.claims.sub.as_deref()
    }

    /// Returns the component/provider name if present
    #[cfg(test)]
    pub fn name(&self) -> Option<&str> {
        self.claims.wascap.as_ref().and_then(|w| w.name.as_deref())
    }

    /// Returns true if this is a component JWT
    #[cfg(test)]
    pub fn is_component(&self) -> bool {
        JwtValidator::is_component(&self.claims)
    }

    /// Returns true if this is a provider JWT
    #[cfg(test)]
    pub fn is_provider(&self) -> bool {
        JwtValidator::is_provider(&self.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_jwt_payload(exp: Option<i64>, sub: Option<&str>) -> String {
        let mut claims = serde_json::json!({
            "iss": "wasmcloud",
            "aud": "wasmcloud"
        });

        if let Some(exp) = exp {
            claims["exp"] = serde_json::json!(exp);
        }

        if let Some(sub) = sub {
            claims["sub"] = serde_json::json!(sub);
        }

        let payload = BASE64_NO_PAD.encode(serde_json::to_string(&claims).unwrap());
        format!("header.{}.signature", payload)
    }

    #[test]
    fn test_empty_token() {
        let validator = JwtValidator::default();
        let result = validator.validate_token("").unwrap();

        assert!(!result.is_valid());
        assert_eq!(result.errors().len(), 1);
        assert_eq!(result.errors()[0], "Token is empty");
    }

    #[test]
    fn test_invalid_format() {
        let validator = JwtValidator::default();
        let result = validator.validate_token("invalid.token").unwrap();

        assert!(!result.is_valid());
        assert!(result.errors()[0].contains("Invalid JWT format"));
    }

    #[test]
    fn test_expired_token() {
        let validator = JwtValidator::new(true, 10);
        let expired_time = chrono::Utc::now().timestamp() - 3600; // 1 hour ago
        let token = create_test_jwt_payload(Some(expired_time), Some("test-component"));

        let result = validator.validate_token(&token).unwrap();
        assert!(!result.is_valid());
        assert!(result.errors().iter().any(|e| e.contains("expired")));
    }

    #[test]
    fn test_valid_token() {
        let validator = JwtValidator::new(false, 300); // Don't enforce expiration
        let token = create_test_jwt_payload(None, Some("test-component"));

        let result = validator.validate_token(&token).unwrap();
        assert!(result.is_valid());
        assert_eq!(result.subject_id(), Some("test-component"));
    }

    #[test]
    fn test_future_token() {
        let validator = JwtValidator::default();
        let future_time = chrono::Utc::now().timestamp() + 3600; // 1 hour from now

        let claims = serde_json::json!({
            "iss": "wasmcloud",
            "aud": "wasmcloud",
            "sub": "test-component",
            "nbf": future_time
        });

        let payload = BASE64_NO_PAD.encode(serde_json::to_string(&claims).unwrap());
        let token = format!("header.{}.signature", payload);

        let result = validator.validate_token(&token).unwrap();
        assert!(!result.is_valid());
        assert!(result.errors().iter().any(|e| e.contains("not yet valid")));
    }

    #[test]
    fn test_missing_subject() {
        let validator = JwtValidator::new(false, 300); // Don't enforce expiration
        let token = create_test_jwt_payload(None, None);

        let result = validator.validate_token(&token).unwrap();
        assert!(!result.is_valid());
        assert!(result
            .errors()
            .iter()
            .any(|e| e.contains("missing subject")));
    }

    #[test]
    fn test_component_detection() {
        let claims = JwtClaims {
            sub: Some("test-component".to_string()),
            wascap: Some(WascapClaims {
                hash: None,
                name: Some("My Component".to_string()),
                caps: None,
                tags: None,
                rev: None,
                ver: None,
                prov: Some(false),
                provider: None,
                component: Some(ComponentInfo {
                    rev: Some(1),
                    ver: Some("1.0.0".to_string()),
                }),
            }),
            ..Default::default()
        };

        assert!(JwtValidator::is_component(&claims));
        assert!(!JwtValidator::is_provider(&claims));
        assert_eq!(
            JwtValidator::extract_name(&claims),
            Some("My Component".to_string())
        );
    }

    #[test]
    fn test_provider_detection() {
        let claims = JwtClaims {
            sub: Some("test-provider".to_string()),
            wascap: Some(WascapClaims {
                hash: None,
                name: Some("My Provider".to_string()),
                caps: None,
                tags: None,
                rev: None,
                ver: None,
                prov: Some(true),
                provider: Some(ProviderInfo {
                    vendor: Some("Acme Corp".to_string()),
                    service: Some("http-server".to_string()),
                }),
                component: None,
            }),
            ..Default::default()
        };

        assert!(!JwtValidator::is_component(&claims));
        assert!(JwtValidator::is_provider(&claims));
        assert_eq!(
            JwtValidator::extract_name(&claims),
            Some("My Provider".to_string())
        );
    }

    #[test]
    fn test_real_jwt_payload() {
        let validator = JwtValidator::new(false, 300); // Don't enforce expiration for this test

        // This is the actual payload that was failing to parse
        let payload = r#"{
            "jti":"lncoposvGtxNfajm8ZqBIH",
            "iat":1714506509,
            "iss":"ADVIWF6Z3BFZNWUXJYT5NEAZZ2YX4T6NRKI3YOR3HKOSQQN7IVDGWSNO",
            "sub":"MBFFVNGFK3IA2ZXXG5DQXQNYM6TNG45PHJMJIJFVFI6YKS3XTXL3DRRK",
            "wascap":{
                "name":"http-hello-world",
                "hash":"285DC0570ABDEA89661E37921DCF86BCCDEC99D4050AE1335243BACB834597BB",
                "tags":["wasmcloud.com/experimental"],
                "rev":0,
                "ver":"0.1.0",
                "prov":false
            },
            "wascap_revision":3
        }"#;

        let payload_b64 = BASE64_NO_PAD.encode(payload);
        let token = format!("header.{}.signature", payload_b64);

        let result = validator.validate_token(&token).unwrap();
        assert!(result.is_valid());
        assert_eq!(
            result.subject_id(),
            Some("MBFFVNGFK3IA2ZXXG5DQXQNYM6TNG45PHJMJIJFVFI6YKS3XTXL3DRRK")
        );
        assert_eq!(result.name(), Some("http-hello-world"));
        assert!(result.is_component()); // prov: false means it's a component
        assert!(!result.is_provider());

        // Verify the wascap claims are parsed correctly
        let wascap = result.claims.wascap.as_ref().unwrap();
        assert_eq!(wascap.name, Some("http-hello-world".to_string()));
        assert_eq!(
            wascap.hash,
            Some("285DC0570ABDEA89661E37921DCF86BCCDEC99D4050AE1335243BACB834597BB".to_string())
        );
        assert_eq!(wascap.rev, Some(0));
        assert_eq!(wascap.ver, Some("0.1.0".to_string()));
        assert_eq!(wascap.prov, Some(false));
        assert_eq!(
            wascap.tags,
            Some(vec!["wasmcloud.com/experimental".to_string()])
        );

        // Verify top-level wascap_revision
        assert_eq!(result.claims.wascap_revision, Some(3));
    }
}
