#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
source "$project_root/.infra/tools/cleanup-build-artifacts.sh"
"$project_root/.infra/tools/prepare-local-path-dependencies.sh"
"$project_root/.infra/tools/infra-tool.sh" rs-infra-coverage --project "$project_root" collect "$@"
mkdir -p "$project_root/target/llvm-cov"
# Keep macro collection from cleaning the runtime profile used by the report.
CARGO_TARGET_DIR="$project_root/target/llvm-cov-macros" \
    cargo llvm-cov --package qubit-ioc-macros --all-features --locked \
    --json --output-path "$project_root/target/llvm-cov/macros-coverage.json"
"$project_root/.infra/tools/check-macro-coverage.sh" \
    "$project_root/target/llvm-cov/macros-coverage.json"
"$project_root/.infra/tools/coverage-report.sh"
