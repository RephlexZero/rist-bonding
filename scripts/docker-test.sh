#!/bin/bash
# Docker-based testing script for RIST bonding
# 
# For VS Code development, consider using the devcontainer instead:
# - Install the Remote-Containers extension
# - Open the project in VS Code
# - Select "Reopen in Container"
# - Use .devcontainer/dev-helper.sh for common tasks

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "\n${BLUE}=== $1 ===${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Function to check if Docker is running
check_docker() {
    if ! docker info >/dev/null 2>&1; then
        print_error "Docker is not running or not accessible"
        exit 1
    fi
    print_success "Docker is running"
}

# Function to build Docker images
build_images() {
    print_header "Building Docker images"
    
    if docker-compose build; then
        print_success "Docker images built successfully"
    else
        print_error "Failed to build Docker images"
        exit 1
    fi
}

# Function to run all tests
run_tests() {
    print_header "Running tests in Docker container"
    
    if docker-compose run --rm rist-bonding-test; then
        print_success "All tests passed!"
    else
        print_error "Some tests failed"
        exit 1
    fi
}

# Function to run development container
run_dev() {
    print_header "Starting development container"
    
    print_warning "Starting interactive development container..."
    print_warning "Container has network capabilities for testing namespaces"
    print_warning "Use 'exit' to stop the container"
    
    docker-compose run --rm rist-bonding-dev
}

# Function to run specific test
run_specific_test() {
    local test_name="$1"
    
    print_header "Running specific test: $test_name"
    
    docker-compose run --rm rist-bonding-dev bash -c "
        /usr/local/bin/setup-network-test.sh
        cargo test $test_name
    "
}

# Function to clean up Docker resources
cleanup() {
    print_header "Cleaning up Docker resources"
    
    docker-compose down -v --remove-orphans
    docker system prune -f
    
    print_success "Cleanup completed"
}

# Function to show network status inside container
show_network_status() {
    print_header "Showing network status in container"
    
    docker-compose run --rm rist-bonding-dev bash -c "
        /usr/local/bin/setup-network-test.sh
        echo
        echo 'Network interfaces:'
        ip addr show
        echo
        echo 'Network namespaces:'
        ip netns list
        echo
        echo 'Routing table:'
        ip route
    "
}

# Function to run benchmark tests
run_benchmarks() {
    print_header "Running benchmark tests"
    
    docker-compose run --rm rist-bonding-dev bash -c "
        /usr/local/bin/setup-network-test.sh
        cargo bench
    "
}

# Function to run multi-container network test
run_multi_container_test() {
    print_header "Running multi-container network test"
    
    print_warning "Starting multi-container setup..."
    
    # Start both containers
    docker-compose up -d rist-bonding-dev rist-bonding-node2
    
    # Run network test between containers
    docker-compose exec rist-bonding-dev bash -c "
        echo 'Testing container-to-container connectivity...'
        ping -c 3 rist-bonding-node2 || echo 'Direct ping failed (expected)'
        
        echo 'Container 1 IP:'
        hostname -I
    "
    
    docker-compose exec rist-bonding-node2 bash -c "
        echo 'Container 2 IP:'
        hostname -I
        
        echo 'Setting up network simulation on node 2...'
        /usr/local/bin/setup-network-test.sh
    "
    
    # Stop containers
    docker-compose down
    
    print_success "Multi-container test completed"
}

# Function to show usage
show_usage() {
    echo "Usage: $0 [COMMAND]"
    echo
    echo "Commands:"
    echo "  test              Run all tests in Docker container"
    echo "  dev               Start interactive development container"
    echo "  build             Build Docker images"
    echo "  clean             Clean up Docker resources"
    echo "  network           Show network status inside container"
    echo "  bench             Run benchmark tests"
    echo "  multi             Run multi-container network test"
    echo "  run <test_name>   Run specific test"
    echo "  help              Show this help message"
    echo
    echo "Examples:"
    echo "  $0 test                                    # Run all tests"
    echo "  $0 run test_weighted_distribution_basic    # Run specific test"
    echo "  $0 dev                                     # Start development container"
}

# Main script logic
main() {
    case "${1:-help}" in
        "test")
            check_docker
            build_images
            run_tests
            ;;
        "dev")
            check_docker
            build_images
            run_dev
            ;;
        "build")
            check_docker
            build_images
            ;;
        "clean")
            check_docker
            cleanup
            ;;
        "network")
            check_docker
            show_network_status
            ;;
        "bench")
            check_docker
            build_images
            run_benchmarks
            ;;
        "multi")
            check_docker
            build_images
            run_multi_container_test
            ;;
        "run")
            if [[ $# -lt 2 ]]; then
                print_error "Test name required for 'run' command"
                show_usage
                exit 1
            fi
            check_docker
            build_images
            run_specific_test "$2"
            ;;
        "help"|*)
            show_usage
            ;;
    esac
}

main "$@"