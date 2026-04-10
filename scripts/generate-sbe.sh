#!/bin/bash
#
# SBE Code Generation Script
#
# Uses Docker to run the official SBE tool and generate Rust code.
#
# Usage:
#   ./scripts/generate-sbe.sh [OPTIONS]
#
# Options:
#   --clean    Clean generated code first
#   --help     Show this help message

set -e

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SCHEMA_DIR="${PROJECT_ROOT}/schemas"
OUTPUT_DIR="${PROJECT_ROOT}/mt-engine-sbe"
DOCKER_IMAGE="sbe-tool:1.37.1"
CLEAN=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --clean)
            CLEAN=true
            shift
            ;;
        --help|-h)
            head -15 "$0" | tail -12
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Clean
if [ "$CLEAN" = true ]; then
    echo "Cleaning $OUTPUT_DIR..."
    rm -rf "$OUTPUT_DIR"
fi

# Build Docker image if needed
if ! docker images | grep -q "$DOCKER_IMAGE"; then
    echo "Building Docker image..."
    docker build -f "${PROJECT_ROOT}/Dockerfile.sbe" -t "$DOCKER_IMAGE" "${PROJECT_ROOT}"
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Generate code
echo "Generating Rust code..."
docker run --rm \
    -v "${SCHEMA_DIR}:/sbe/schema:ro" \
    -v "${OUTPUT_DIR}:/sbe/output" \
    "$DOCKER_IMAGE" \
    -Dsbe.target.language=Rust \
    -Dsbe.output.dir=/sbe/output \
    -Dsbe.rust.crate.version=0.1.0 \
    -Dsbe.rust.directory.structure=true \
    -Dsbe.validation.stop.on.error=true \
    /sbe/schema/mt-engine/templates_FixBinary.xml

# Post-process: Inject rkyv traits into the generated files
echo "Injecting rkyv traits..."
# We target both enums and structs that already have Serialize/Deserialize
# Use -i '' for macOS sed compatibility
find "$OUTPUT_DIR/src" -name "*.rs" -exec sed -i '' \
    '/derive(Serialize, Deserialize))/a\
#[cfg_attr(feature = "rkyv", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]' {} +

echo ""
echo "Generated files:"
find "$OUTPUT_DIR" -type f \( -name "*.rs" -o -name "Cargo.toml" \) 2>/dev/null || echo "  (none found)"
