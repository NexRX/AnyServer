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

# Check for available scripts in a package.json (call from the dir containing it)
has_script() {
    node -e "const p = require('./package.json'); process.exit(p.scripts && p.scripts['$1'] ? 0 : 1)" 2>/dev/null
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


# Detect package manager
if [ -f "pnpm-lock.yaml" ]; then
    PKG_MGR="pnpm"
elif [ -f "yarn.lock" ]; then
    PKG_MGR="yarn"
elif [ -f "bun.lockb" ]; then
    PKG_MGR="bun"
else
    PKG_MGR="npm"
fi

run_check "Frontend install dependencies" bash -c "cd '$FRONTEND_DIR' && $PKG_MGR install --frozen-lockfile 2>/dev/null || $PKG_MGR install"

if has_script "typecheck"; then
    run_check "Frontend type check" $PKG_MGR run typecheck
elif has_script "type-check"; then
    run_check "Frontend type check" $PKG_MGR run type-check
elif has_script "tsc"; then
    run_check "Frontend type check" $PKG_MGR run tsc
elif command -v tsc &>/dev/null; then
    run_check "Frontend type check (tsc)" tsc --noEmit
else
    skip_check "Frontend type check" "no typecheck script found"
fi

if has_script "lint"; then
    run_check "Frontend lint" $PKG_MGR run lint
elif command -v eslint &>/dev/null; then
    run_check "Frontend lint (eslint)" eslint .
else
    skip_check "Frontend lint" "no lint script found"
fi

if has_script "format:check"; then
    run_check "Frontend format check" $PKG_MGR run format:check
elif has_script "fmt:check"; then
    run_check "Frontend format check" $PKG_MGR run fmt:check
elif command -v prettier &>/dev/null; then
    run_check "Frontend format check (prettier)" prettier --check .
else
    skip_check "Frontend format check" "no format check script found"
fi

if has_script "test"; then
    run_check "Frontend unit tests" $PKG_MGR run test
else
    skip_check "Frontend unit tests" "no test script found"
fi

if has_script "build"; then
    run_check "Frontend build" $PKG_MGR run build
else
    skip_check "Frontend build" "no build script found"
fi

# =============================================================================
# End-to-End Tests
# =============================================================================
section "End-to-End Tests"
cd "$FRONTEND_DIR"

# The e2e tests are driven from the frontend directory via pnpm test:e2e.
# Check for that script first (the canonical way per the README), then
# fall back to detecting playwright/cypress configs.
if has_script "test:e2e"; then
    run_check "E2E tests" $PKG_MGR run test:e2e
elif has_script "e2e"; then
    run_check "E2E tests" $PKG_MGR run e2e
elif [ -f "playwright.config.ts" ] || [ -f "playwright.config.js" ]; then
    run_check "Playwright E2E tests" npx playwright test
elif [ -f "cypress.config.ts" ] || [ -f "cypress.config.js" ]; then
    run_check "Cypress E2E tests" npx cypress run
else
    skip_check "E2E tests" "no e2e test script or configuration found"
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
     if [ -f "Dockerfile" ]; then
         run_check "Docker build" docker build -t release-check .
     fi
     if [ -f "docker-compose.yml" ] || [ -f "docker-compose.yaml" ] || [ -f "compose.yml" ] || [ -f "compose.yaml" ]; then
         run_check "Docker Compose config validation" docker compose config --quiet
     fi
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
TODO_COUNT=$(grep -r --include="*.rs" --include="*.ts" --include="*.tsx" --include="*.js" --include="*.jsx" --include="*.py" -c 'TODO\|FIXME\|HACK\|XXX' . 2>/dev/null | awk -F: '{s+=$2} END {print s+0}')
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

# Check if version numbers are consistent (Cargo.toml, package.json, etc.)
echo -n "  ▶ Version consistency check ... "
VERSIONS=""
if [ -f "$BACKEND_DIR/Cargo.toml" ]; then
    CARGO_VERSION=$(grep -m1 '^version' "$BACKEND_DIR/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/' 2>/dev/null || echo "")
    if [ -n "$CARGO_VERSION" ]; then
        VERSIONS="$VERSIONS cargo:$CARGO_VERSION"
    fi
fi
if [ -f "$FRONTEND_DIR/package.json" ]; then
    PKG_VERSION=$(node -e "console.log(require('$FRONTEND_DIR/package.json').version || '')" 2>/dev/null || echo "")
    if [ -n "$PKG_VERSION" ]; then
        VERSIONS="$VERSIONS package:$PKG_VERSION"
    fi
fi
if [ -n "$VERSIONS" ]; then
    UNIQUE_VERSIONS=$(echo "$VERSIONS" | tr ' ' '\n' | grep -v '^$' | sed 's/.*://' | sort -u | wc -l)
    if [ "$UNIQUE_VERSIONS" -le 1 ]; then
        echo -e "${GREEN}OK${NC} ($VERSIONS)"
    else
        echo -e "${YELLOW}WARNING${NC} - versions may differ: $VERSIONS"
    fi
else
    echo -e "${YELLOW}SKIPPED${NC} (no version files found)"
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
