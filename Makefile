# Makefile for RIST Bonding Docker operations
.PHONY: help build test clean dev network bench multi

# Default target
help:
	@echo "RIST Bonding Docker Testing"
	@echo "Available targets:"
	@echo "  build     - Build Docker images"
	@echo "  test      - Run all tests in Docker"
	@echo "  clean     - Clean up Docker resources"
	@echo "  dev       - Start interactive development container"
	@echo "  network   - Show network status inside container"
	@echo "  bench     - Run benchmarks"
	@echo "  multi     - Run multi-container tests"

# Build Docker images
build:
	@./scripts/docker-test.sh build

# Run all tests
test:
	@./scripts/docker-test.sh test

# Start development container
dev:
	@./scripts/docker-test.sh dev

# Clean up Docker resources
clean:
	@./scripts/docker-test.sh clean

# Show network status
network:
	@./scripts/docker-test.sh network

# Run benchmarks
bench:
	@./scripts/docker-test.sh bench

# Run multi-container tests
multi:
	@./scripts/docker-test.sh multi

# Run specific test (usage: make run-test TEST=test_name)
run-test:
	@./scripts/docker-test.sh run $(TEST)

# Quick test cycle (build + test)
quick-test: build test

# Development cycle (build + dev)
dev-cycle: build dev

# CI simulation (all checks)
ci: build test bench

# Docker system cleanup
docker-cleanup:
	@echo "Cleaning up Docker system..."
	@docker system prune -f
	@docker volume prune -f
	@docker network prune -f