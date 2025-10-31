# Makefile for RIST Bonding Project
.PHONY: help build test clean dev network bench multi gstreamer verify

# Default target
help:
	@echo "RIST Bonding Project"
	@echo ""
	@echo "Quick Start:"
	@echo "  cargo build      # Builds everything automatically!"
	@echo ""
	@echo "Available make targets:"
	@echo "  build     - Build the Rust project (same as cargo build)"
	@echo "  verify    - Verify the build and plugins are working"
	@echo "  gstreamer - Explicitly build/rebuild GStreamer"
	@echo "  test      - Run all tests"
	@echo "  clean     - Clean build artifacts (keeps GStreamer)"
	@echo "  clean-all - Clean everything including GStreamer build"
	@echo ""
	@echo "Note: 'cargo build' automatically handles:"
	@echo "  - Git submodule initialization"
	@echo "  - GStreamer building (first time only)"
	@echo "  - Environment configuration"
	@echo ""
	@echo "Docker targets:"
	@echo "  docker-build - Build Docker images"
	@echo "  docker-test  - Run all tests in Docker"
	@echo "  dev          - Start interactive development container"
	@echo "  network      - Show network status inside container"
	@echo "  bench        - Run benchmarks"
	@echo "  multi        - Run multi-container tests"

# Build the Rust project (same as cargo build, GStreamer auto-built if needed)
build:
	@cargo build

# Build release version (same as cargo build --release)
build-release:
	@cargo build --release

# Build GStreamer explicitly
gstreamer:
	@./build_gstreamer.sh

# Verify the build
verify:
	@./verify-build.sh

# Run tests
test:
	@cargo test

# Clean Rust artifacts only
clean:
	@cargo clean

# Clean everything including GStreamer
clean-all: clean
	@rm -rf target/gstreamer
	@echo "==> Cleaned all build artifacts"

# Docker-related targets
docker-build:
	@./scripts/docker-test.sh build

docker-test:
	@./scripts/docker-test.sh test

dev:
	@./scripts/docker-test.sh dev

network:
	@./scripts/docker-test.sh network

bench:
	@./scripts/docker-test.sh bench

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