# Infisical Secrets Provider for wasmCloud

A wasmCloud secrets backend implementation that integrates with [Infisical](https://infisical.com/) to provide secure secret management for wasmCloud components and providers.

## Overview

This service implements the wasmCloud secrets backend API to retrieve secrets from Infisical and provide them securely to wasmCloud applications. It uses x25519 encryption (xkeys) to ensure end-to-end encryption of secret data over NATS.

## Features

- 🔐 **Secure Communication**: Uses xkeys (x25519 keypairs) for end-to-end encryption
- 🔑 **Infisical Integration**: Connects to Infisical Cloud or self-hosted instances
- 🚀 **wasmCloud Compatible**: Implements the official wasmCloud secrets backend API
- ✅ **JWT Validation**: Validates component/provider JWT tokens
- 📊 **Comprehensive Logging**: Structured logging with tracing
- 🔄 **Automatic Retry**: Built-in error handling and connection management

## Architecture

```
┌─────────────────┐    NATS (encrypted)    ┌───────────────────────────┐
│   wasmCloud     │ ◄─────────────────────► │ Infisical Secrets         │
│   Host/Runtime  │                         │ Provider                  │
└─────────────────┘                         └─────────────┬─────────────┘
                                                          │
                                                          │ HTTPS
                                                          ▼
                                                 ┌─────────────────┐
                                                 │    Infisical    │
                                                 │    Instance     │
                                                 └─────────────────┘
```

## Quick Start

### 1. Setup Infisical

1. Create an Infisical project and environment
2. Create a Universal Auth machine identity in your project
3. Note down your client ID, client secret, and project ID

### 2. Configure Environment Variables

```bash
# Required
export INFISICAL_CLIENT_ID="your_client_id"
export INFISICAL_CLIENT_SECRET="your_client_secret"
export INFISICAL_PROJECT_ID="your_project_id"

# Optional
export INFISICAL_BASE_URL="https://app.infisical.com"  # or your self-hosted URL
export INFISICAL_ENVIRONMENT="prod"                    # or dev, staging, etc.
export NATS_URL="nats://localhost:4222"
export BACKEND_NAME="infisical"
```

### 3. Run the Service

```bash
cd crates/services/infisical_secrets_provider
cargo run
```

### 4. Configure wasmCloud Application

Create a wasmCloud application manifest with the Infisical secrets backend:

```yaml
apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: my-app
spec:
  policies:
    - name: infisical-secrets
      type: policy.secret.wasmcloud.dev/v1alpha1
      properties:
        backend: infisical
  components:
    - name: my-component
      type: component
      properties:
        image: wasmcloud.azurecr.io/my-component:latest
      traits:
        - type: secrets
          properties:
            policy: infisical-secrets
            secrets:
              - name: SECRET_API_KEY
                secret_ref:
                  backend: infisical
                  key: "API_KEY"
              - name: SECRET_DB_PASSWORD
                secret_ref:
                  backend: infisical
                  key: "DATABASE_PASSWORD"
```

## Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `INFISICAL_CLIENT_ID` | Yes | - | Universal Auth client ID |
| `INFISICAL_CLIENT_SECRET` | Yes | - | Universal Auth client secret |
| `INFISICAL_PROJECT_ID` | Yes | - | Infisical project ID |
| `INFISICAL_BASE_URL` | No | `https://app.infisical.com` | Infisical instance URL |
| `INFISICAL_ENVIRONMENT` | No | `prod` | Environment to fetch secrets from |
| `NATS_URL` | No | `nats://localhost:4222` | NATS server URL |
| `NATS_SUBJECT_PREFIX` | No | `wasmcloud.secrets` | NATS subject prefix |
| `BACKEND_NAME` | No | `infisical` | Backend identifier |
| `API_VERSION` | No | `v1alpha1` | API version |

### NATS Subjects

The service listens on the following NATS subjects:

- **Get Secret**: `wasmcloud.secrets.v1alpha1.infisical.get`
- **Server XKey**: `wasmcloud.secrets.v1alpha1.infisical.server_xkey`

## Security

### Encryption

All secret payloads are encrypted using x25519 keypairs (xkeys):
- Each request uses a unique host-generated xkey
- The server generates its own xkey pair on startup
- All communication over NATS is end-to-end encrypted

### JWT Validation

The service validates JWT tokens from components and providers:
- Checks token expiration and validity periods
- Validates token structure and required claims
- Supports both component and provider JWTs

### Best Practices

1. **Rotate Credentials**: Regularly rotate your Infisical Universal Auth credentials
2. **Network Security**: Use TLS for NATS connections in production
3. **Access Control**: Configure Infisical project permissions appropriately
4. **Monitoring**: Monitor logs for unauthorized access attempts

## Development

### Building

```bash
cargo build
```

### Testing

```bash
# Run unit tests
cargo test

# Run with logs
RUST_LOG=debug cargo test -- --nocapture
```

### Project Structure

```
src/
├── main.rs              # Entry point and configuration
├── backend.rs           # Main secrets backend implementation  
├── config.rs            # Configuration management
├── types.rs             # wasmCloud API types
├── encryption.rs        # xkey encryption/decryption
├── infisical_client.rs  # Infisical API wrapper
└── jwt.rs               # JWT validation utilities
```

## Troubleshooting

### Common Issues

1. **Authentication Failed**
   ```
   Error: Failed to authenticate with Infisical
   ```
   - Verify your `INFISICAL_CLIENT_ID` and `INFISICAL_CLIENT_SECRET`
   - Check that the Universal Auth identity has access to the project

2. **Secret Not Found**
   ```
   Error: Secret 'API_KEY' not found
   ```
   - Verify the secret exists in the specified project and environment
   - Check the secret name spelling

3. **NATS Connection Failed**
   ```
   Error: Failed to connect to NATS
   ```
   - Verify NATS server is running at the specified URL
   - Check network connectivity

4. **JWT Validation Failed**
   ```
   Error: JWT validation failed: Token has expired
   ```
   - Check system clock synchronization
   - Verify component/provider JWT is not expired

### Debug Logging

Enable debug logging for detailed troubleshooting:

```bash
RUST_LOG=infisical_secrets_provider=debug cargo run
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Submit a pull request

## License

This project is licensed under the same terms as the parent wasmCloud project.

## Related Documentation

- [wasmCloud Secrets Documentation](https://wasmcloud.com/docs/deployment/security/secrets/)
- [Infisical Documentation](https://infisical.com/docs)
- [Infisical Rust SDK](https://github.com/Infisical/rust-sdk)
- [NATS Documentation](https://docs.nats.io/)