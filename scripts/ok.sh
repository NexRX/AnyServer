#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Release Readiness Check Script
# =============================================================================
# This script verifies that everything in the project is in good shape
# before cutting a release.
#
# Run from the repository root:  ./scripts/ok.sh
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

FAILED=0
PASSED=0
SKIPPED=0

# Always resolve REPO_DIR to the repo root (parent of this script's directory)
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_DIR="$REPO_DIR/backend"
FRONTEND_DIR="$REPO_DIR/frontend"

section() {
    echo ""
    echo -e "${BLUE}══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}══════════════════════════════════════════════════════════════${NC}"
    echo ""
}

run_check() {
    local name="$1"
    shift
    echo -n "  ▶ $name ... "
    if output=$("$@" 2>&1); then
        echo -e "${GREEN}PASSED${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAILED${NC}"
        echo "$output" | head -50 | sed 's/^/    /'
        FAILED=$((FAILED + 1))
    fi
}

skip_check() {
    local name="$1"
    local reason="$2"
    echo -e "  ▶ $name ... ${YELLOW}SKIPPED${NC} ($reason)"
    SKIPPED=$((SKIPPED + 1))
}

# =============================================================================
# Git Checks
# =============================================================================
section "Git & Repository Checks"
cd "$REPO_DIR"

run_check "No uncommitted changes" git diff --quiet HEAD

run_check "No untracked files" bash -c '[ -z "$(git ls-files --others --exclude-standard)" ]'

run_check "On main/master branch" bash -c '
    branch=$(git rev-parse --abbrev-ref HEAD)
    if [ "$branch" = "main" ] || [ "$branch" = "master" ]; then
        exit 0
    else
        echo "Currently on branch: $branch"
        exit 1
    fi
'

# =============================================================================
# Backend (Rust) Checks
# =============================================================================
section "Backend (Rust) Checks"
cd "$BACKEND_DIR"

run_check "Cargo check" cargo check --all-targets --all-features

run_check "Cargo test" cargo test --all-features

run_check "Cargo clippy" cargo clippy --all-targets --all-features -- -D warnings

run_check "Cargo fmt check" cargo fmt --all -- --check

run_check "Cargo audit (dependencies)" bash -c '
    if command -v cargo-audit &>/dev/null; then
        cargo audit
    else
        echo "cargo-audit not installed, installing..."
        cargo install cargo-audit && cargo audit
    fi
'

run_check "No unused dependencies" bash -c '
    if command -v cargo-udeps &>/dev/null; then
        cargo +nightly udeps --all-targets
    else
        echo "cargo-udeps not installed, skipping"
        exit 0
    fi
'

run_check "Cargo doc (no warnings)" bash -c 'RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features'

# =============================================================================
# Frontend Checks
# =============================================================================
section "Frontend Checks"
cd "$FRONTEND_DIR"

run_check "Frontend install dependencies" bash -c "cd '$FRONTEND_DIR' && pnpm install --frozen-lockfile 2>/dev/null || pnpm install"

run_check "Frontend unit tests" pnpm run test

run_check "Frontend build" pnpm run build

# =============================================================================
# End-to-End Tests
# =============================================================================
section "End-to-End Tests"
cd "$FRONTEND_DIR"

# Use nix-shell with frontend/shell.nix when available so that Playwright
# browsers and all system dependencies are provided reproducibly.
if command -v nix-shell &>/dev/null; then
    run_check "E2E tests (nix-shell)" nix-shell "$FRONTEND_DIR/shell.nix" --run "pnpm test:e2e"
else
    run_check "E2E tests" pnpm run test:e2e
fi

# =============================================================================
# Nix Flake Checks
# =============================================================================
section "Nix Flake Checks"
cd "$REPO_DIR"

if command -v nix &>/dev/null; then
    run_check "Nix flake check" nix flake check

    run_check "Nix flake build" nix build

    run_check "Nix flake lock is up to date" bash -c '
        nix flake lock --no-update-lock-file 2>&1
    '
else
    skip_check "Nix flake checks" "nix not found"
fi

# =============================================================================
# Docker Checks
# =============================================================================
section "Docker Checks"
cd "$REPO_DIR"

if command -v docker &>/dev/null; then
    run_check "Docker build" docker build -t release-check .
    run_check "Docker Compose config validation" docker compose config --quiet
else
    skip_check "Docker checks" "docker not found"
fi

# =============================================================================
# Miscellaneous Checks
# =============================================================================
section "Miscellaneous Checks"
cd "$REPO_DIR"

# Check for TODO/FIXME in code (warning only)
echo -n "  ▶ Checking for TODOs/FIXMEs ... "
TODO_COUNT=$(grep -r --include="*.rs" --include="*.ts" --include="*.tsx" --include="*.js" --include="*.jsx" -c 'TODO\|FIXME\|HACK\|XXX' . 2>/dev/null | awk -F: '{s+=$2} END {print s+0}')
if [ "$TODO_COUNT" -gt 0 ]; then
    echo -e "${YELLOW}WARNING${NC} ($TODO_COUNT occurrences found)"
else
    echo -e "${GREEN}CLEAN${NC}"
fi

# Check for .env files that shouldn't be committed
run_check "No .env files committed" bash -c '
    env_files=$(git ls-files "*.env" ".env*" 2>/dev/null | grep -v ".env.example" | grep -v ".env.template" | grep -v ".env.sample" || true)
    if [ -n "$env_files" ]; then
        echo "Found committed .env files:"
        echo "$env_files"
        exit 1
    fi
'

# Check if version numbers are consistent (Cargo.toml & package.json)
echo -n "  ▶ Version consistency check ... "
CARGO_VERSION=$(grep -m1 '^version' "$BACKEND_DIR/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/' 2>/dev/null || echo "")
PKG_VERSION=$(node -e "console.log(require('$FRONTEND_DIR/package.json').version || '')" 2>/dev/null || echo "")
if [ -n "$CARGO_VERSION" ] && [ -n "$PKG_VERSION" ]; then
    if [ "$CARGO_VERSION" = "$PKG_VERSION" ]; then
        echo -e "${GREEN}OK${NC} (cargo:$CARGO_VERSION package:$PKG_VERSION)"
    else
        echo -e "${YELLOW}WARNING${NC} - versions differ: cargo:$CARGO_VERSION package:$PKG_VERSION"
    fi
else
    echo -e "${YELLOW}SKIPPED${NC} (could not read one or both version files)"
fi

# =============================================================================
# Summary
# =============================================================================
section "Summary"

TOTAL=$((PASSED + FAILED + SKIPPED))
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
echo -e "  Total:   $TOTAL"
echo ""

if [ "$FAILED" -gt 0 ]; then
    echo -e "${RED}✘ Release check FAILED — $FAILED check(s) did not pass.${NC}"
    echo -e "${RED}  Please fix the issues above before releasing.${NC}"
    exit 1
else
    echo -e "${GREEN}✔ All checks passed! Ready for release.${NC}"
    exit 0
fi
