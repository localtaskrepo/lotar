#!/bin/bash
# Test runner script for LoTaR development

set -e

echo "🚀 Running LoTaR Test Suite"
echo "=========================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Build the project first
echo "Building project..."
if cargo build; then
    print_status "Build successful"
else
    print_error "Build failed"
    exit 1
fi

# Run unit tests
echo ""
echo "Running unit tests..."
if cargo test --lib; then
    print_status "Unit tests passed"
else
    print_error "Unit tests failed"
    exit 1
fi

# Run integration tests
echo ""
echo "Running integration tests..."
if cargo test --test '*'; then
    print_status "Integration tests passed"
else
    print_error "Integration tests failed"
    exit 1
fi

# Run specific test suites with better output
echo ""
echo "Running test suites individually..."

echo "  📁 Storage tests..."
if cargo test --test storage_test; then
    print_status "Storage tests passed"
else
    print_error "Storage tests failed"
fi

echo "  🔍 Scanner tests..."
if cargo test --test scanner_test; then
    print_status "Scanner tests passed"
else
    print_error "Scanner tests failed"
fi

echo "  ⌨️  CLI tests..."
if cargo test --test cli_integration_test; then
    print_status "CLI tests passed"
else
    print_warning "CLI tests failed (expected for incomplete implementation)"
fi

# Code quality checks
echo ""
echo "Running code quality checks..."

echo "  🎨 Format check..."
if cargo fmt --check; then
    print_status "Code formatting is good"
else
    print_warning "Code needs formatting (run: cargo fmt)"
fi

echo "  📎 Clippy check..."
if cargo clippy -- -D warnings; then
    print_status "No clippy warnings"
else
    print_warning "Clippy found issues"
fi

# Test coverage (if tarpaulin is available)
echo ""
echo "Checking test coverage..."
if command -v cargo-tarpaulin &> /dev/null; then
    if cargo tarpaulin --out Stdout --ignore-tests; then
        print_status "Coverage report generated"
    else
        print_warning "Coverage report failed"
    fi
else
    print_warning "Install cargo-tarpaulin for coverage reports: cargo install cargo-tarpaulin"
fi

echo ""
echo "🎉 Test suite completed!"
echo ""
echo "Next steps:"
echo "  1. Fix any failing tests"
echo "  2. Implement missing features flagged by tests"
echo "  3. Run 'cargo fmt' if formatting is needed"
echo "  4. Address clippy warnings if any"
