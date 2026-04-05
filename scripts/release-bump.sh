#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Release Version Bump Script
# =============================================================================
# Bumps the version across all project files and recomputes Nix hashes so
# `nix build` / `nix flake check` keep working after the change.
#
# Usage:
#   ./scripts/release-bump.sh patch          # 0.2.0 → 0.2.1
#   ./scripts/release-bump.sh minor          # 0.2.0 → 0.3.0
#   ./scripts/release-bump.sh major          # 0.2.0 → 1.0.0
#   ./scripts/release-bump.sh 1.2.3          # explicit version
#   ./scripts/release-bump.sh patch --dry-run # preview only
#
# Run from the repository root:  ./scripts/release-bump.sh <bump|version>
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$REPO_DIR/backend/Cargo.toml"
PACKAGE_JSON="$REPO_DIR/frontend/package.json"
NIX_PACKAGE="$REPO_DIR/nix/package.nix"

DRY_RUN=false
SKIP_NIX=false
SKIP_COMMIT=false

# ── Helpers ───────────────────────────────────────────────────────────────────

info()  { echo -e "${BLUE}ℹ${NC}  $*"; }
ok()    { echo -e "${GREEN}✔${NC}  $*"; }
warn()  { echo -e "${YELLOW}⚠${NC}  $*"; }
err()   { echo -e "${RED}✘${NC}  $*" >&2; }
step()  { echo -e "\n${BOLD}${CYAN}── $* ──${NC}\n"; }

die() { err "$@"; exit 1; }

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] <patch|minor|major|X.Y.Z>

Bump the project version across all files and update Nix hashes.

Arguments:
  patch           Bump the patch version   (0.2.0 → 0.2.1)
  minor           Bump the minor version   (0.2.0 → 0.3.0)
  major           Bump the major version   (0.2.0 → 1.0.0)
  X.Y.Z           Set an explicit version  (must be valid semver)

Options:
  --dry-run       Show what would change without modifying files
  --skip-nix      Skip Nix hash recomputation (version-only bump)
  --skip-commit   Do not create a git commit at the end
  -h, --help      Show this help message
EOF
    exit 0
}

# ── Argument parsing ──────────────────────────────────────────────────────────

BUMP_ARG=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)    DRY_RUN=true;    shift ;;
        --skip-nix)   SKIP_NIX=true;   shift ;;
        --skip-commit) SKIP_COMMIT=true; shift ;;
        -h|--help)    usage ;;
        -*)           die "Unknown option: $1" ;;
        *)
            [[ -n "$BUMP_ARG" ]] && die "Only one bump argument allowed (got '$BUMP_ARG' and '$1')"
            BUMP_ARG="$1"
            shift
            ;;
    esac
done

[[ -z "$BUMP_ARG" ]] && { usage; }

# ── Read current version from Cargo.toml ──────────────────────────────────────

read_current_version() {
    grep -m1 '^version' "$CARGO_TOML" | sed 's/.*"\(.*\)".*/\1/'
}

CURRENT_VERSION="$(read_current_version)"
[[ -z "$CURRENT_VERSION" ]] && die "Could not read current version from $CARGO_TOML"

IFS='.' read -r CUR_MAJOR CUR_MINOR CUR_PATCH <<< "$CURRENT_VERSION"

# ── Compute new version ──────────────────────────────────────────────────────

case "$BUMP_ARG" in
    patch)
        NEW_VERSION="$CUR_MAJOR.$CUR_MINOR.$((CUR_PATCH + 1))"
        ;;
    minor)
        NEW_VERSION="$CUR_MAJOR.$((CUR_MINOR + 1)).0"
        ;;
    major)
        NEW_VERSION="$((CUR_MAJOR + 1)).0.0"
        ;;
    *)
        if [[ "$BUMP_ARG" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            NEW_VERSION="$BUMP_ARG"
        else
            die "Invalid version or bump type: '$BUMP_ARG' (expected patch|minor|major|X.Y.Z)"
        fi
        ;;
esac

if [[ "$NEW_VERSION" == "$CURRENT_VERSION" ]]; then
    die "New version ($NEW_VERSION) is the same as the current version"
fi

echo ""
echo -e "${BOLD}Version bump: ${RED}$CURRENT_VERSION${NC} → ${GREEN}$NEW_VERSION${NC}"
echo ""

if $DRY_RUN; then
    warn "Dry-run mode — no files will be modified"
    echo ""
fi

# ── Verify prerequisites ─────────────────────────────────────────────────────

step "Checking prerequisites"

[[ -f "$CARGO_TOML" ]]   || die "Not found: $CARGO_TOML"
[[ -f "$PACKAGE_JSON" ]]  || die "Not found: $PACKAGE_JSON"
[[ -f "$NIX_PACKAGE" ]]   || die "Not found: $NIX_PACKAGE"

ok "All version files exist"

if ! $SKIP_NIX; then
    if ! command -v nix &>/dev/null; then
        warn "nix not found — will skip Nix hash recomputation (use --skip-nix to silence)"
        SKIP_NIX=true
    else
        ok "nix is available ($(nix --version 2>/dev/null | head -1))"
    fi
fi

command -v cargo &>/dev/null || die "cargo is required but not found"
ok "cargo is available"

command -v node &>/dev/null || die "node is required but not found"
ok "node is available"

# ── Step 1: Bump version in Cargo.toml ────────────────────────────────────────

step "Step 1/6 — Bump backend/Cargo.toml"

if $DRY_RUN; then
    info "Would replace version = \"$CURRENT_VERSION\" → \"$NEW_VERSION\" in Cargo.toml"
else
    sed -i "0,/^version = \"$CURRENT_VERSION\"/s//version = \"$NEW_VERSION\"/" "$CARGO_TOML"

    VERIFY="$(read_current_version)"
    [[ "$VERIFY" == "$NEW_VERSION" ]] || die "Cargo.toml version update failed (got '$VERIFY')"
    ok "backend/Cargo.toml → $NEW_VERSION"
fi

# ── Step 2: Bump version in package.json ──────────────────────────────────────

step "Step 2/6 — Bump frontend/package.json"

if $DRY_RUN; then
    info "Would replace \"version\": \"$CURRENT_VERSION\" → \"$NEW_VERSION\" in package.json"
else
    node -e "
        const fs = require('fs');
        const path = '${PACKAGE_JSON}';
        const pkg = JSON.parse(fs.readFileSync(path, 'utf8'));
        pkg.version = '${NEW_VERSION}';
        fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + '\n');
    "

    PKG_VER="$(node -e "console.log(require('$PACKAGE_JSON').version)")"
    [[ "$PKG_VER" == "$NEW_VERSION" ]] || die "package.json version update failed (got '$PKG_VER')"
    ok "frontend/package.json → $NEW_VERSION"
fi

# ── Step 3: Bump version strings in nix/package.nix ──────────────────────────

step "Step 3/6 — Bump nix/package.nix version strings"

if $DRY_RUN; then
    info "Would replace all version = \"$CURRENT_VERSION\" → \"$NEW_VERSION\" in package.nix"
else
    sed -i "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/g" "$NIX_PACKAGE"

    COUNT="$(grep -c "version = \"$NEW_VERSION\"" "$NIX_PACKAGE" || true)"
    [[ "$COUNT" -ge 2 ]] || die "Expected at least 2 version strings in package.nix (found $COUNT)"
    ok "nix/package.nix → $NEW_VERSION ($COUNT occurrences)"
fi

# ── Step 4: Update Cargo.lock ─────────────────────────────────────────────────

step "Step 4/6 — Update Cargo.lock"

if $DRY_RUN; then
    info "Would run 'cargo check' in backend/ to refresh Cargo.lock"
else
    (cd "$REPO_DIR/backend" && cargo check --quiet 2>&1) || die "cargo check failed"
    ok "Cargo.lock updated"
fi

# ── Step 5: Update pnpm-lock.yaml ────────────────────────────────────────────

step "Step 5/6 — Update pnpm-lock.yaml"

if $DRY_RUN; then
    info "Would run 'pnpm install' in frontend/ to refresh lockfile"
else
    (cd "$REPO_DIR/frontend" && pnpm install --no-frozen-lockfile 2>&1 | tail -1) \
        || die "pnpm install failed"
    ok "pnpm-lock.yaml updated"
fi

# ── Step 6: Recompute Nix hashes ─────────────────────────────────────────────

step "Step 6/6 — Recompute Nix hashes"

if $SKIP_NIX; then
    warn "Skipping Nix hash recomputation (--skip-nix or nix not available)"
    warn "You MUST update cargoHash and pnpmDeps.hash in nix/package.nix manually!"
elif $DRY_RUN; then
    info "Would recompute cargoHash and pnpmDeps hash via nix build"
else
    # ── Fake-hash strategy ──
    #
    # 1. Replace each hash with a known-bad value using LINE-TARGETED sed
    #    (so we never accidentally touch the wrong hash).
    # 2. Stage everything so the flake can see the changes.
    # 3. Run `nix build` — it fails and prints the correct hash to stderr.
    # 4. Capture stderr to a TEMP FILE (not a variable — bash $() mangles
    #    the output and can cause wrong extraction).
    # 5. Parse the `got:` line from the temp file.
    # 6. Patch the correct hash back in via line-targeted sed.
    #
    # Order matters: fix pnpmDeps (inner derivation) before cargoHash (outer),
    # because the Rust build depends on the frontend being built first.

    FAKE_HASH="sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    NIX_LOG="$(mktemp)"
    trap 'rm -f "$NIX_LOG"' EXIT

    # Find the line numbers for each hash so we can target sed precisely
    PNPM_LINE="$(grep -n 'hash = "sha256-' "$NIX_PACKAGE" | head -1 | cut -d: -f1)"
    CARGO_LINE="$(grep -n 'cargoHash = "sha256-' "$NIX_PACKAGE" | head -1 | cut -d: -f1)"
    [[ -n "$PNPM_LINE" ]]  || die "Could not find pnpmDeps hash line in package.nix"
    [[ -n "$CARGO_LINE" ]] || die "Could not find cargoHash line in package.nix"

    # Read the current hash values
    OLD_PNPM_HASH="$(sed -n "${PNPM_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
    OLD_CARGO_HASH="$(sed -n "${CARGO_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
    [[ -n "$OLD_PNPM_HASH" ]]  || die "Could not extract pnpmDeps hash from line $PNPM_LINE"
    [[ -n "$OLD_CARGO_HASH" ]] || die "Could not extract cargoHash from line $CARGO_LINE"

    info "pnpmDeps hash (line $PNPM_LINE):  $OLD_PNPM_HASH"
    info "cargoHash     (line $CARGO_LINE): $OLD_CARGO_HASH"

    # Helper: replace the hash on a specific line
    replace_hash_on_line() {
        local lineno="$1" old_hash="$2" new_hash="$3" file="$4"
        sed -i "${lineno}s|${old_hash}|${new_hash}|" "$file"
        # Verify it stuck
        local check
        check="$(sed -n "${lineno}p" "$file" | grep -o 'sha256-[A-Za-z0-9+/]*=*' || true)"
        if [[ "$check" != "$new_hash" ]]; then
            die "sed replacement failed on line $lineno: expected '$new_hash', got '$check'"
        fi
    }

    # Helper: extract the `got:` hash from a nix build log file
    extract_got_hash() {
        grep 'got:' "$1" | head -1 | grep -o 'sha256-[A-Za-z0-9+/]*=*' || true
    }

    # Helper: run nix build and capture stderr to the log file
    nix_build_probe() {
        (cd "$REPO_DIR" && git add -A)
        (cd "$REPO_DIR" && nix build --no-link 2>"$NIX_LOG") || true
    }

    # ── 6a. pnpmDeps hash ──

    info "Probing pnpmDeps hash..."
    replace_hash_on_line "$PNPM_LINE" "$OLD_PNPM_HASH" "$FAKE_HASH" "$NIX_PACKAGE"
    nix_build_probe

    NEW_PNPM_HASH="$(extract_got_hash "$NIX_LOG")"

    if [[ -z "$NEW_PNPM_HASH" ]]; then
        # Build may have succeeded if the hash wasn't actually checked (unlikely),
        # or the output format changed.  Fall back to old hash.
        warn "Could not extract pnpmDeps hash from nix output — restoring old hash"
        NEW_PNPM_HASH="$OLD_PNPM_HASH"
    fi

    replace_hash_on_line "$PNPM_LINE" "$FAKE_HASH" "$NEW_PNPM_HASH" "$NIX_PACKAGE"

    if [[ "$NEW_PNPM_HASH" == "$OLD_PNPM_HASH" ]]; then
        ok "pnpmDeps hash unchanged: $NEW_PNPM_HASH"
    else
        ok "pnpmDeps hash updated:"
        info "  old: $OLD_PNPM_HASH"
        info "  new: $NEW_PNPM_HASH"
    fi

    # ── 6b. cargoHash ──

    info "Probing cargoHash..."

    # Re-read the cargo line number in case line numbers shifted (they shouldn't,
    # but be safe)
    CARGO_LINE="$(grep -n 'cargoHash = "sha256-' "$NIX_PACKAGE" | head -1 | cut -d: -f1)"
    [[ -n "$CARGO_LINE" ]] || die "Lost cargoHash line in package.nix"

    replace_hash_on_line "$CARGO_LINE" "$OLD_CARGO_HASH" "$FAKE_HASH" "$NIX_PACKAGE"
    nix_build_probe

    NEW_CARGO_HASH="$(extract_got_hash "$NIX_LOG")"

    if [[ -z "$NEW_CARGO_HASH" ]]; then
        # If the build SUCCEEDED, the old hash was still valid
        if grep -q 'error:' "$NIX_LOG" 2>/dev/null; then
            # Build failed but we couldn't parse the hash — dump the log
            warn "Could not extract cargoHash from nix output — restoring old hash"
            warn "Nix output was:"
            head -30 "$NIX_LOG" | sed 's/^/    /' >&2
        fi
        NEW_CARGO_HASH="$OLD_CARGO_HASH"
    fi

    replace_hash_on_line "$CARGO_LINE" "$FAKE_HASH" "$NEW_CARGO_HASH" "$NIX_PACKAGE"

    if [[ "$NEW_CARGO_HASH" == "$OLD_CARGO_HASH" ]]; then
        ok "cargoHash unchanged: $NEW_CARGO_HASH"
    else
        ok "cargoHash updated:"
        info "  old: $OLD_CARGO_HASH"
        info "  new: $NEW_CARGO_HASH"
    fi

    # ── 6c. Verify the full build passes ──

    info "Verifying nix build with updated hashes..."
    echo ""
    info "Final hashes in nix/package.nix:"
    info "  pnpmDeps (line $PNPM_LINE): $(sed -n "${PNPM_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
    info "  cargoHash (line $CARGO_LINE): $(sed -n "${CARGO_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
    echo ""

    (cd "$REPO_DIR" && git add -A)
    if (cd "$REPO_DIR" && nix build --no-link 2>"$NIX_LOG"); then
        ok "nix build succeeded!"
    else
        err "nix build FAILED after hash updates. Build log:"
        cat "$NIX_LOG" | sed 's/^/    /' >&2
        echo "" >&2
        err "The hashes in nix/package.nix may need manual correction."
        err "You can also try running:  nix build 2>&1 | grep got:"
        exit 1
    fi
fi

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo -e "${BOLD}${CYAN}══════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}${CYAN}  Summary${NC}"
echo -e "${BOLD}${CYAN}══════════════════════════════════════════════════════════════${NC}"
echo ""

if $DRY_RUN; then
    echo -e "  ${YELLOW}DRY RUN — no changes were made${NC}"
    echo ""
    echo -e "  Version:       ${RED}$CURRENT_VERSION${NC} → ${GREEN}$NEW_VERSION${NC}"
    echo ""
    echo "  Files that would be modified:"
    echo "    • backend/Cargo.toml"
    echo "    • backend/Cargo.lock"
    echo "    • frontend/package.json"
    echo "    • frontend/pnpm-lock.yaml"
    echo "    • nix/package.nix"
    echo ""
    exit 0
fi

echo -e "  Version:       ${RED}$CURRENT_VERSION${NC} → ${GREEN}$NEW_VERSION${NC}"
echo ""
echo "  Updated files:"
echo -e "    ${GREEN}✔${NC} backend/Cargo.toml"
echo -e "    ${GREEN}✔${NC} backend/Cargo.lock"
echo -e "    ${GREEN}✔${NC} frontend/package.json"
echo -e "    ${GREEN}✔${NC} frontend/pnpm-lock.yaml"
echo -e "    ${GREEN}✔${NC} nix/package.nix (versions)"
if ! $SKIP_NIX; then
    echo -e "    ${GREEN}✔${NC} nix/package.nix (cargoHash)"
    echo -e "    ${GREEN}✔${NC} nix/package.nix (pnpmDeps hash)"
fi
echo ""

# ── Optional: git commit ──────────────────────────────────────────────────────

if $SKIP_COMMIT; then
    info "Skipping git commit (--skip-commit)"
    echo ""
    echo -e "  ${YELLOW}Remember to commit and tag:${NC}"
    echo "    git add -A"
    echo "    git commit -m \"chore: bump version to v$NEW_VERSION\""
    echo "    git tag v$NEW_VERSION"
    echo ""
else
    (cd "$REPO_DIR" && git add -A)

    STAGED="$(cd "$REPO_DIR" && git diff --cached --name-only)"
    if [[ -z "$STAGED" ]]; then
        warn "Nothing to commit (all changes already committed?)"
    else
        (cd "$REPO_DIR" && git commit -m "chore: bump version to v$NEW_VERSION

Automated release bump $CURRENT_VERSION → $NEW_VERSION.

Files updated:
- backend/Cargo.toml
- backend/Cargo.lock
- frontend/package.json
- frontend/pnpm-lock.yaml
- nix/package.nix (versions + hashes)")

        ok "Committed version bump"
        echo ""
        echo -e "  ${YELLOW}Don't forget to tag and push:${NC}"
        echo "    git tag v$NEW_VERSION"
        echo "    git push && git push --tags"
    fi
fi

echo ""
echo -e "${GREEN}${BOLD}✔ Release bump complete!${NC}"
echo ""
