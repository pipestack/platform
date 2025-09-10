#!/bin/bash

# Infisical Secrets Provider Deployment Script
# This script helps deploy the Infisical secrets provider in different environments

set -euo pipefail

# Script configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../../.." && pwd)"
SERVICE_DIR="${PROJECT_ROOT}/crates/services/infisical_secrets_provider"

# Default values
ENVIRONMENT="development"
BUILD_MODE="debug"
COMPOSE_PROFILE=""
HEALTH_CHECK_TIMEOUT=60

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Show usage information
show_help() {
    cat << EOF
Infisical Secrets Provider Deployment Script

USAGE:
    $0 [COMMAND] [OPTIONS]

COMMANDS:
    build           Build the service binary
    docker-build    Build Docker image
    deploy          Deploy using Docker Compose
    start           Start the service (local binary)
    stop            Stop the service
    status          Check service status
    logs            View service logs
    test            Run tests
    clean           Clean build artifacts
    help            Show this help message

OPTIONS:
    -e, --env ENV           Environment (development, staging, production) [default: development]
    -m, --mode MODE         Build mode (debug, release) [default: debug]
    -p, --profile PROFILE   Docker Compose profile (monitoring, wasmcloud)
    -t, --timeout SECONDS  Health check timeout [default: 60]
    -h, --help              Show this help message

EXAMPLES:
    $0 build --mode release
    $0 deploy --env production --profile monitoring
    $0 start --env development
    $0 logs --follow
    $0 stop

ENVIRONMENT VARIABLES:
    Required:
        INFISICAL_CLIENT_ID
        INFISICAL_CLIENT_SECRET  
        INFISICAL_PROJECT_ID

    Optional (with defaults):
        INFISICAL_BASE_URL
        INFISICAL_ENVIRONMENT
        NATS_URL
        BACKEND_NAME
        RUST_LOG

EOF
}

# Parse command line arguments
parse_args() {
    COMMAND=""
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            build|docker-build|deploy|start|stop|status|logs|test|clean|help)
                COMMAND="$1"
                shift
                ;;
            -e|--env)
                ENVIRONMENT="$2"
                shift 2
                ;;
            -m|--mode)
                BUILD_MODE="$2"
                shift 2
                ;;
            -p|--profile)
                COMPOSE_PROFILE="$2"
                shift 2
                ;;
            -t|--timeout)
                HEALTH_CHECK_TIMEOUT="$2"
                shift 2
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                print_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    if [[ -z "$COMMAND" ]]; then
        print_error "No command specified"
        show_help
        exit 1
    fi
}

# Check if required environment variables are set
check_environment() {
    local missing_vars=()
    
    if [[ -z "${INFISICAL_CLIENT_ID:-}" ]]; then
        missing_vars+=("INFISICAL_CLIENT_ID")
    fi
    
    if [[ -z "${INFISICAL_CLIENT_SECRET:-}" ]]; then
        missing_vars+=("INFISICAL_CLIENT_SECRET")
    fi
    
    if [[ -z "${INFISICAL_PROJECT_ID:-}" ]]; then
        missing_vars+=("INFISICAL_PROJECT_ID")
    fi
    
    if [[ ${#missing_vars[@]} -gt 0 ]]; then
        print_error "Missing required environment variables:"
        for var in "${missing_vars[@]}"; do
            print_error "  - $var"
        done
        print_info "Please set these variables or create a .env file in the service directory"
        print_info "See .env.example for reference"
        exit 1
    fi
}

# Load environment file if it exists
load_env_file() {
    local env_file="${SERVICE_DIR}/.env"
    if [[ -f "$env_file" ]]; then
        print_info "Loading environment from $env_file"
        # Export variables from .env file
        set -a
        source "$env_file"
        set +a
    else
        print_warning "No .env file found at $env_file"
        print_info "Using environment variables from shell"
    fi
}

# Check if Docker is available
check_docker() {
    if ! command -v docker &> /dev/null; then
        print_error "Docker is required but not installed"
        exit 1
    fi
    
    if ! docker info &> /dev/null; then
        print_error "Docker daemon is not running"
        exit 1
    fi
}

# Check if Docker Compose is available
check_docker_compose() {
    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        print_error "Docker Compose is required but not installed"
        exit 1
    fi
}

# Build the service binary
build_service() {
    print_info "Building Infisical secrets provider..."
    
    cd "$PROJECT_ROOT"
    
    local cargo_flags=""
    if [[ "$BUILD_MODE" == "release" ]]; then
        cargo_flags="--release"
    fi
    
    cargo build $cargo_flags -p infisical_secrets_provider
    
    print_success "Build completed successfully"
}

# Build Docker image
docker_build() {
    print_info "Building Docker image for Infisical secrets provider..."
    
    check_docker
    
    cd "$PROJECT_ROOT"
    
    docker build \
        -f "${SERVICE_DIR}/Dockerfile" \
        -t infisical-secrets-provider:latest \
        -t "infisical-secrets-provider:${ENVIRONMENT}" \
        .
    
    print_success "Docker image built successfully"
}

# Deploy using Docker Compose
deploy_service() {
    print_info "Deploying Infisical secrets provider with Docker Compose..."
    
    check_docker
    check_docker_compose
    check_environment
    
    cd "$SERVICE_DIR"
    
    # Build compose command
    local compose_cmd="docker-compose"
    if docker compose version &> /dev/null; then
        compose_cmd="docker compose"
    fi
    
    # Add profile if specified
    local profile_args=""
    if [[ -n "$COMPOSE_PROFILE" ]]; then
        profile_args="--profile $COMPOSE_PROFILE"
    fi
    
    # Deploy services
    $compose_cmd $profile_args up -d --build
    
    print_info "Waiting for services to become healthy..."
    wait_for_health
    
    print_success "Deployment completed successfully"
    print_info "Services are running. Use '$0 logs' to view logs"
}

# Start the service locally
start_service() {
    print_info "Starting Infisical secrets provider locally..."
    
    check_environment
    load_env_file
    
    cd "$SERVICE_DIR"
    
    # Build if binary doesn't exist
    local binary_path
    if [[ "$BUILD_MODE" == "release" ]]; then
        binary_path="${PROJECT_ROOT}/target/release/infisical_secrets_provider"
    else
        binary_path="${PROJECT_ROOT}/target/debug/infisical_secrets_provider"
    fi
    
    if [[ ! -f "$binary_path" ]]; then
        print_info "Binary not found, building first..."
        build_service
    fi
    
    print_info "Starting service in $BUILD_MODE mode..."
    "$binary_path"
}

# Stop the service
stop_service() {
    print_info "Stopping Infisical secrets provider..."
    
    cd "$SERVICE_DIR"
    
    if [[ -f "docker-compose.yml" ]]; then
        local compose_cmd="docker-compose"
        if docker compose version &> /dev/null; then
            compose_cmd="docker compose"
        fi
        
        $compose_cmd down
    fi
    
    # Also try to stop local processes
    pkill -f "infisical_secrets_provider" || true
    
    print_success "Service stopped"
}

# Check service status
check_status() {
    print_info "Checking Infisical secrets provider status..."
    
    # Check Docker containers
    if command -v docker &> /dev/null; then
        print_info "Docker containers:"
        docker ps --filter "name=infisical" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" || true
    fi
    
    # Check local processes
    if pgrep -f "infisical_secrets_provider" &> /dev/null; then
        print_info "Local processes:"
        pgrep -fl "infisical_secrets_provider"
    else
        print_info "No local processes found"
    fi
    
    # Check NATS connectivity (if available)
    if command -v nats &> /dev/null; then
        print_info "NATS connectivity:"
        nats pub --count=1 test.connection "test" 2>/dev/null && print_success "NATS is accessible" || print_warning "NATS connection failed"
    fi
}

# View service logs
view_logs() {
    print_info "Viewing Infisical secrets provider logs..."
    
    cd "$SERVICE_DIR"
    
    local compose_cmd="docker-compose"
    if docker compose version &> /dev/null; then
        compose_cmd="docker compose"
    fi
    
    # Follow logs for Docker Compose deployment
    if [[ -f "docker-compose.yml" ]] && docker ps --filter "name=infisical-secrets-provider" --format "{{.Names}}" | grep -q "infisical-secrets-provider"; then
        $compose_cmd logs -f infisical-secrets-provider
    else
        print_warning "No Docker containers found. Showing recent system logs..."
        # Try to show recent logs from journald if available
        if command -v journalctl &> /dev/null; then
            journalctl -u infisical_secrets_provider -f --no-pager || print_warning "No systemd service found"
        else
            print_warning "Unable to display logs. Run the service directly to see output."
        fi
    fi
}

# Run tests
run_tests() {
    print_info "Running tests for Infisical secrets provider..."
    
    cd "$PROJECT_ROOT"
    
    # Unit tests
    print_info "Running unit tests..."
    cargo test -p infisical_secrets_provider
    
    # Integration tests (if environment is configured)
    if check_environment 2>/dev/null; then
        print_info "Running integration tests..."
        cargo test -p infisical_secrets_provider --test integration -- --ignored
    else
        print_warning "Skipping integration tests (missing environment configuration)"
    fi
    
    print_success "Tests completed"
}

# Clean build artifacts
clean_artifacts() {
    print_info "Cleaning build artifacts..."
    
    cd "$PROJECT_ROOT"
    
    # Clean Rust build artifacts
    cargo clean -p infisical_secrets_provider
    
    # Clean Docker images
    if command -v docker &> /dev/null; then
        print_info "Cleaning Docker images..."
        docker rmi infisical-secrets-provider:latest 2>/dev/null || true
        docker rmi "infisical-secrets-provider:${ENVIRONMENT}" 2>/dev/null || true
        
        # Clean dangling images
        docker image prune -f >/dev/null 2>&1 || true
    fi
    
    print_success "Cleanup completed"
}

# Wait for service to become healthy
wait_for_health() {
    local timeout=$HEALTH_CHECK_TIMEOUT
    local elapsed=0
    local interval=5
    
    print_info "Waiting for service health check (timeout: ${timeout}s)..."
    
    while [[ $elapsed -lt $timeout ]]; do
        if docker ps --filter "name=infisical-secrets-provider" --filter "health=healthy" --format "{{.Names}}" | grep -q "infisical-secrets-provider"; then
            print_success "Service is healthy"
            return 0
        fi
        
        if [[ $((elapsed % 15)) -eq 0 ]] && [[ $elapsed -gt 0 ]]; then
            print_info "Still waiting... (${elapsed}/${timeout}s)"
        fi
        
        sleep $interval
        elapsed=$((elapsed + interval))
    done
    
    print_error "Service health check timed out after ${timeout}s"
    print_info "Check service logs for details: $0 logs"
    return 1
}

# Main function
main() {
    print_info "Infisical Secrets Provider Deployment Script"
    print_info "Environment: $ENVIRONMENT, Build mode: $BUILD_MODE"
    
    case "$COMMAND" in
        build)
            build_service
            ;;
        docker-build)
            docker_build
            ;;
        deploy)
            deploy_service
            ;;
        start)
            start_service
            ;;
        stop)
            stop_service
            ;;
        status)
            check_status
            ;;
        logs)
            view_logs
            ;;
        test)
            run_tests
            ;;
        clean)
            clean_artifacts
            ;;
        help)
            show_help
            ;;
        *)
            print_error "Unknown command: $COMMAND"
            show_help
            exit 1
            ;;
    esac
}

# Parse arguments and run main function
parse_args "$@"
main