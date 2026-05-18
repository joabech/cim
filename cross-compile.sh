#!/usr/bin/env bash

# Cross-compilation script for Code in Motion (cim)
# Builds release binaries for multiple target architectures

set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Target architectures to build for
TARGETS=(
    "x86_64-pc-windows-gnu"
    "aarch64-unknown-linux-gnu"
    "x86_64-unknown-linux-gnu"
)

# Force amd64 images on Apple Silicon — cross-rs images lack arm64 variants
export DOCKER_DEFAULT_PLATFORM=linux/amd64

# Global variables
CONTINUE_ON_ERROR=false
SELECTED_TARGETS=()
FAILED_TARGETS=()
SUCCESSFUL_TARGETS=()

usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Cross-compile Code in Motion (cim) for multiple target architectures.

OPTIONS:
    -c, --continue          Continue building all targets even if some fail
    -t, --target TARGET     Build only this target (can be repeated)
    -h, --help              Show this help message

TARGETS:
EOF
    for target in "${TARGETS[@]}"; do
        echo "    - $target"
    done
    echo
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

build_target() {
    local target="$1"
    local current="$2"
    local total="$3"
    
    log_info "Building target $current/$total: $target"
    
    # Capture both stdout and stderr to a temporary file
    local build_log=$(mktemp)
    if cargo build --release --target "$target" 2>&1 | tee "$build_log"; then
        log_success "Build completed for $target"
        SUCCESSFUL_TARGETS+=("$target")
        rm -f "$build_log"
        return 0
    else
        log_error "Build failed for $target"
        
        # Check for common error patterns and provide helpful suggestions
        if grep -q "toolchain.*may not be able to run on this system" "$build_log"; then
            echo
            log_warning "Cross detected a toolchain compatibility issue."
            log_info "This can happen after reinstalling Rust. Try running:"
            echo "    rustup toolchain add stable-x86_64-unknown-linux-gnu --profile minimal --force-non-host"
            echo
        elif grep -q "can't find crate for 'core'" "$build_log" || grep -q "can't find crate for 'std'" "$build_log"; then
            echo
            log_warning "Missing target support detected."
            log_info "Try installing the target with:"
            echo "    rustup target add $target"
            echo
        fi
        
        FAILED_TARGETS+=("$target")
        rm -f "$build_log"
        return 1
    fi
}

print_summary() {
    echo
    echo "=============================================="
    echo "           BUILD SUMMARY"
    echo "=============================================="
    
    if [ ${#SUCCESSFUL_TARGETS[@]} -gt 0 ]; then
        echo -e "${GREEN}Successful builds (${#SUCCESSFUL_TARGETS[@]}):${NC}"
        for target in "${SUCCESSFUL_TARGETS[@]}"; do
            echo -e "  ${GREEN}✓${NC} $target"
        done
    fi
    
    if [ ${#FAILED_TARGETS[@]} -gt 0 ]; then
        echo -e "${RED}Failed builds (${#FAILED_TARGETS[@]}):${NC}"
        for target in "${FAILED_TARGETS[@]}"; do
            echo -e "  ${RED}✗${NC} $target"
        done
    fi
    
    echo "=============================================="
    
    if [ ${#FAILED_TARGETS[@]} -eq 0 ]; then
        log_success "All builds completed successfully!"
        return 0
    else
        log_error "${#FAILED_TARGETS[@]} build(s) failed"
        return 1
    fi
}

main() {
    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            -c|--continue)
                CONTINUE_ON_ERROR=true
                shift
                ;;
            -t|--target)
                if [[ -z "${2:-}" ]]; then
                    log_error "--target requires an argument"
                    usage
                    exit 1
                fi
                local valid=false
                for t in "${TARGETS[@]}"; do
                    if [[ "$t" == "$2" ]]; then
                        valid=true
                        break
                    fi
                done
                if [[ "$valid" == false ]]; then
                    log_error "Unknown target: $2"
                    usage
                    exit 1
                fi
                SELECTED_TARGETS+=("$2")
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    # Determine which targets to build
    local build_targets=()
    if [[ ${#SELECTED_TARGETS[@]} -gt 0 ]]; then
        build_targets=("${SELECTED_TARGETS[@]}")
    else
        build_targets=("${TARGETS[@]}")
    fi

    # Check if cross is installed
    if ! command -v cross &> /dev/null; then
        log_error "cross command not found. Please install it with: cargo install cross"
        exit 1
    fi

    log_info "Starting cross-compilation for ${#build_targets[@]} target(s)"
    if [ "$CONTINUE_ON_ERROR" = true ]; then
        log_info "Continue mode: Will build all targets regardless of failures"
    else
        log_info "Fail-fast mode: Will stop on first failure"
    fi
    echo

    # Build each target
    local current=1
    for target in "${build_targets[@]}"; do
        if ! build_target "$target" "$current" "${#build_targets[@]}"; then
            if [ "$CONTINUE_ON_ERROR" = false ]; then
                log_error "Build failed for $target. Stopping due to fail-fast mode."
                exit 1
            fi
        fi
        current=$((current + 1))
        echo  # Add spacing between builds
    done
    
    # Print final summary
    if ! print_summary; then
        exit 1
    fi
}

# Run main function with all arguments
main "$@"