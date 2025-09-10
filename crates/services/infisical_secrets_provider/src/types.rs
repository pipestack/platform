use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Application information within the context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    /// Application name
    pub name: String,
    /// Application policy
    pub policy: String,
}

/// Context information for a secret request, including JWTs and application info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// The entity JWT
    pub entity_jwt: String,
    /// The host JWT
    pub host_jwt: String,
    /// Application information
    pub application: Application,
}

/// Request structure for retrieving a secret from the backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRequest {
    /// The key of the secret as addressed in the secret store
    pub key: String,
    /// The field within the secret (optional)
    pub field: Option<String>,
    /// The version of the secret (optional)
    pub version: Option<String>,
    /// Context for the request, including application information
    pub context: Context,
}

/// Response structure returned by the secrets backend
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecretResponse {
    /// The secret data (if successful)
    pub secret: Option<Secret>,
    /// Error message (if failed)
    pub error: Option<String>,
}

impl From<SecretResponse> for Bytes {
    fn from(resp: SecretResponse) -> Self {
        let encoded = serde_json::to_vec(&resp).unwrap();
        Bytes::from(encoded)
    }
}

/// Secret data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    /// Name of the secret
    pub name: String,
    /// Version of the secret
    pub version: String,
    /// String-based secret value
    // #[serde(rename = "secret_string")]
    pub string_secret: Option<String>,
    /// Binary secret value
    // #[serde(rename = "secret_binary")]
    pub binary_secret: Option<Vec<u8>>,
}

impl SecretResponse {
    /// Creates a successful response with a secret
    pub fn success(secret: Secret) -> Self {
        Self {
            secret: Some(secret),
            error: None,
        }
    }

    /// Creates an error response
    pub fn error<S: Into<String>>(message: S) -> Self {
        Self {
            secret: None,
            error: Some(message.into()),
        }
    }
}

impl Secret {
    /// Creates a new string-based secret
    pub fn new_string<N, V, Ver>(name: N, value: V, version: Ver) -> Self
    where
        N: Into<String>,
        V: Into<String>,
        Ver: Into<String>,
    {
        Self {
            name: name.into(),
            version: version.into(),
            string_secret: Some(value.into()),
            binary_secret: None,
        }
    }

    /// Creates a new binary secret
    pub fn new_binary<N, Ver>(name: N, value: Vec<u8>, version: Ver) -> Self
    where
        N: Into<String>,
        Ver: Into<String>,
    {
        Self {
            name: name.into(),
            version: version.into(),
            string_secret: None,
            binary_secret: Some(value),
        }
    }

    /// Returns true if this is a string secret
    pub fn is_string(&self) -> bool {
        self.string_secret.is_some()
    }

    /// Returns true if this is a binary secret
    pub fn is_binary(&self) -> bool {
        self.binary_secret.is_some()
    }

    /// Gets the secret value as a string (if it's a string secret)
    pub fn as_string(&self) -> Option<&str> {
        self.string_secret.as_deref()
    }

    /// Gets the secret value as bytes (if it's a binary secret)
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.binary_secret.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_response_success() {
        let secret = Secret::new_string("test_key", "test_value", "1.0");
        let response = SecretResponse::success(secret.clone());

        assert!(response.secret.is_some());
        assert!(response.error.is_none());
        assert_eq!(response.secret.unwrap().name, secret.name);
    }

    #[test]
    fn test_secret_response_error() {
        let response = SecretResponse::error("Something went wrong");

        assert!(response.secret.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap(), "Something went wrong");
    }

    #[test]
    fn test_string_secret() {
        let secret = Secret::new_string("api_key", "secret_value", "latest");

        assert!(secret.is_string());
        assert!(!secret.is_binary());
        assert_eq!(secret.as_string(), Some("secret_value"));
        assert_eq!(secret.as_bytes(), None);
    }

    #[test]
    fn test_binary_secret() {
        let data = vec![1, 2, 3, 4, 5];
        let secret = Secret::new_binary("binary_key", data.clone(), "1.0");

        assert!(!secret.is_string());
        assert!(secret.is_binary());
        assert_eq!(secret.as_string(), None);
        assert_eq!(secret.as_bytes(), Some(data.as_slice()));
    }

    #[test]
    fn test_secret_request_serialization() {
        let request = SecretRequest {
            key: "test_secret".to_string(),
            field: None,
            version: Some("1.0".to_string()),
            context: Context {
                entity_jwt: "test.entity.jwt".to_string(),
                host_jwt: "test.host.jwt".to_string(),
                application: Application {
                    name: "test-app".to_string(),
                    policy: "{}".to_string(),
                },
            },
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        let deserialized: SecretRequest =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(request.key, deserialized.key);
        assert_eq!(request.field, deserialized.field);
        assert_eq!(request.version, deserialized.version);
        assert_eq!(request.context.entity_jwt, deserialized.context.entity_jwt);
        assert_eq!(request.context.host_jwt, deserialized.context.host_jwt);
        assert_eq!(
            request.context.application.name,
            deserialized.context.application.name
        );
    }

    #[test]
    fn test_secret_response_serialization() {
        let secret = Secret::new_string("test_key", "test_value", "1.0");
        let response = SecretResponse::success(secret);

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        let deserialized: SecretResponse =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert!(deserialized.secret.is_some());
        assert!(deserialized.error.is_none());

        let secret = deserialized.secret.unwrap();
        assert_eq!(secret.name, "test_key");
        assert_eq!(secret.as_string(), Some("test_value"));
        assert_eq!(secret.version, "1.0");
    }

    #[test]
    fn test_actual_payload_deserialization() {
        let payload = r#"{
            "key":"api_password",
            "field":null,
            "version":null,
            "context":{
                "entity_jwt":"eyJ0eXAiOiJqd3QiLCJhbGciOiJFZDI1NTE5In0.eyJqdGkiOiJsbmNvcG9zdkd0eE5mYWptOFpxQklIIiwiaWF0IjoxNzE0NTA2NTA5LCJpc3MiOiJBRFZJV0Y2WjNCRlpOV1VYSllUNU5FQVpaMllYNFQ2TlJLSTNZT1IzSEtPU1FRTjdJVkRHV1NOTyIsInN1YiI6Ik1CRkZWTkdGSzNJQTJaWFhHNURRWFFOWU02VE5HNDVQSEpNSklKRlZGSTZZS1MzWFRYTDNEUlJLIiwid2FzY2FwIjp7Im5hbWUiOiJodHRwLWhlbGxvLXdvcmxkIiwiaGFzaCI6IjI4NERDMDU3MEFCREVBODk2NjFFMzc5MjFEQ0Y4NkJDQ0RFQzk5RDQwNTBBRTEzMzUyNDNCQUNCODM0NTk3QkIiLCJ0YWdzIjpbIndhc21jbG91ZC5jb20vZXhwZXJpbWVudGFsIl0sInJldiI6MCwidmVyIjoiMC4xLjAiLCJwcm92IjpmYWxzZX0sIndhc2NhcF9yZXZpc2lvbiI6M30.SadmUTbEWISlEiITqIWPVdefBz3BdWq8WerXxW8LwaLBpbQ9eP6nykaEBPAzWYhsLaALoFTGNtxg_MjoLiL4Cg",
                "host_jwt":"eyJ0eXAiOiJqd3QiLCJhbGciOiJFZDI1NTE5In0.eyJqdGkiOiJKeHlyNTdSTmV3Z1NNcVJ1T1hjZmlaIiwiaWF0IjoxNzU3MzEwNDE1LCJpc3MiOiJBQ0xZSzJWSVRBWjRFRkcyQzJKUFE2Q1RXQldQQU9SVko3TFBETTVSVkg1VEFUSFk0V0hMREdKQiIsInN1YiI6Ik5DV1c0S0tNSVBVSDI3Rk9XSU1VWkpRS0VDRkdGQTQ1TkJOSEw1QklYWDRWVTRGTlg2QVBaU0hLIiwid2FzY2FwIjp7Im5hbWUiOiJxdWlldC1kYXduLTYzNzkiLCJsYWJlbHMiOnsic2VsZl9zaWduZWQiOiJ0cnVlIn19LCJ3YXNjYXBfcmV2aXNpb24iOjN9.SXnI_-DvH1mLJHDCHo4KEWLqVxyh_SrM5nijy0asxquTe5nj9xFYvuo8ZfbtYRs5H03v1sx6AMhC9cv7Sa2TDA",
                "application":{
                    "name":"rust-hello-world",
                    "policy":"{\"type\":\"properties.secret.wasmcloud.dev/v1alpha1\",\"properties\":{}}"
                }
            }
        }"#;

        let deserialized: SecretRequest =
            serde_json::from_str(payload).expect("Failed to deserialize actual payload");

        assert_eq!(deserialized.key, "api_password");
        assert_eq!(deserialized.field, None);
        assert_eq!(deserialized.version, None);
        assert_eq!(deserialized.context.application.name, "rust-hello-world");
        assert_eq!(
            deserialized.context.application.policy,
            "{\"type\":\"properties.secret.wasmcloud.dev/v1alpha1\",\"properties\":{}}"
        );
        assert!(!deserialized.context.entity_jwt.is_empty());
        assert!(!deserialized.context.host_jwt.is_empty());
    }
}
