#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Release Version Bump Script
# =============================================================================
# Bumps the version across all project files and recomputes Nix hashes so
# `nix build` / `nix flake check` keep working after the change.
#
# Usage:
#   ./scripts/release-bump.sh patch                # 0.2.0 → 0.2.1
#   ./scripts/release-bump.sh minor                # 0.2.0 → 0.3.0
#   ./scripts/release-bump.sh major                # 0.2.0 → 1.0.0
#   ./scripts/release-bump.sh 1.2.3                # explicit version
#   ./scripts/release-bump.sh patch --dry-run      # preview only
#   ./scripts/release-bump.sh patch --only backend   # bump backend only
#   ./scripts/release-bump.sh minor --only frontend  # bump frontend only
#   ./scripts/release-bump.sh patch --git tag         # commit + tag
#   ./scripts/release-bump.sh patch --git push        # commit + tag + push
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
BUMP_BACKEND=true
BUMP_FRONTEND=true
ONLY_SCOPE=""
GIT_ACTION=""

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
  --only <scope>  Only bump "frontend" or "backend" (default: both)
  --git <action>  "tag" to commit + tag, "push" to commit + tag + push
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
        --only)
            [[ -n "${2:-}" ]] || die "--only requires an argument: frontend or backend"
            case "$2" in
                frontend)
                    BUMP_BACKEND=false
                    ONLY_SCOPE="frontend"
                    ;;
                backend)
                    BUMP_FRONTEND=false
                    ONLY_SCOPE="backend"
                    ;;
                *)
                    die "--only must be 'frontend' or 'backend' (got '$2')"
                    ;;
            esac
            shift 2
            ;;
        --git)
            [[ -n "${2:-}" ]] || die "--git requires an argument: tag or push"
            case "$2" in
                tag)  GIT_ACTION="tag"  ;;
                push) GIT_ACTION="push" ;;
                *)    die "--git must be 'tag' or 'push' (got '$2')" ;;
            esac
            shift 2
            ;;
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
if [[ -n "$ONLY_SCOPE" ]]; then
    echo -e "${BOLD}Scope:        ${CYAN}$ONLY_SCOPE only${NC}"
fi
echo ""

if $DRY_RUN; then
    warn "Dry-run mode — no files will be modified"
    echo ""
fi

# ── Verify prerequisites ─────────────────────────────────────────────────────

step "Checking prerequisites"

if $BUMP_BACKEND; then
    [[ -f "$CARGO_TOML" ]]   || die "Not found: $CARGO_TOML"
fi
if $BUMP_FRONTEND; then
    [[ -f "$PACKAGE_JSON" ]]  || die "Not found: $PACKAGE_JSON"
fi
[[ -f "$NIX_PACKAGE" ]]   || die "Not found: $NIX_PACKAGE"

ok "All required version files exist"

if ! $SKIP_NIX; then
    if ! command -v nix &>/dev/null; then
        warn "nix not found — will skip Nix hash recomputation (use --skip-nix to silence)"
        SKIP_NIX=true
    else
        ok "nix is available ($(nix --version 2>/dev/null | head -1))"
    fi
fi

if $BUMP_BACKEND; then
    command -v cargo &>/dev/null || die "cargo is required but not found"
    ok "cargo is available"
fi

if $BUMP_FRONTEND; then
    command -v node &>/dev/null || die "node is required but not found"
    ok "node is available"
fi

# ── Compute step counts based on scope ────────────────────────────────────────
# Steps: [backend Cargo.toml] [frontend package.json] [nix versions] [Cargo.lock] [pnpm-lock] [nix hashes]
TOTAL_STEPS=0
if $BUMP_BACKEND;  then TOTAL_STEPS=$((TOTAL_STEPS + 1)); fi  # Cargo.toml
if $BUMP_FRONTEND; then TOTAL_STEPS=$((TOTAL_STEPS + 1)); fi  # package.json
TOTAL_STEPS=$((TOTAL_STEPS + 1))                               # nix version strings
if $BUMP_BACKEND;  then TOTAL_STEPS=$((TOTAL_STEPS + 1)); fi  # Cargo.lock
if $BUMP_FRONTEND; then TOTAL_STEPS=$((TOTAL_STEPS + 1)); fi  # pnpm-lock
TOTAL_STEPS=$((TOTAL_STEPS + 1))                               # nix hashes
CURRENT_STEP=0
next_step() { CURRENT_STEP=$((CURRENT_STEP + 1)); }

# ── Step: Bump version in Cargo.toml ──────────────────────────────────────────

if $BUMP_BACKEND; then
    next_step
    step "Step $CURRENT_STEP/$TOTAL_STEPS — Bump backend/Cargo.toml"

    if $DRY_RUN; then
        info "Would replace version = \"$CURRENT_VERSION\" → \"$NEW_VERSION\" in Cargo.toml"
    else
        sed -i "0,/^version = \"$CURRENT_VERSION\"/s//version = \"$NEW_VERSION\"/" "$CARGO_TOML"

        VERIFY="$(read_current_version)"
        [[ "$VERIFY" == "$NEW_VERSION" ]] || die "Cargo.toml version update failed (got '$VERIFY')"
        ok "backend/Cargo.toml → $NEW_VERSION"
    fi
else
    info "Skipping backend/Cargo.toml (--only frontend)"
fi

# ── Step: Bump version in package.json ────────────────────────────────────────

if $BUMP_FRONTEND; then
    next_step
    step "Step $CURRENT_STEP/$TOTAL_STEPS — Bump frontend/package.json"

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
else
    info "Skipping frontend/package.json (--only backend)"
fi

# ── Step: Bump version strings in nix/package.nix ────────────────────────────

next_step
step "Step $CURRENT_STEP/$TOTAL_STEPS — Bump nix/package.nix version strings"

if $DRY_RUN; then
    info "Would replace all version = \"$CURRENT_VERSION\" → \"$NEW_VERSION\" in package.nix"
else
    sed -i "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/g" "$NIX_PACKAGE"

    COUNT="$(grep -c "version = \"$NEW_VERSION\"" "$NIX_PACKAGE" || true)"
    [[ "$COUNT" -ge 2 ]] || die "Expected at least 2 version strings in package.nix (found $COUNT)"
    ok "nix/package.nix → $NEW_VERSION ($COUNT occurrences)"
fi

# ── Step: Update Cargo.lock ───────────────────────────────────────────────────

if $BUMP_BACKEND; then
    next_step
    step "Step $CURRENT_STEP/$TOTAL_STEPS — Update Cargo.lock"

    if $DRY_RUN; then
        info "Would run 'cargo check' in backend/ to refresh Cargo.lock"
    else
        (cd "$REPO_DIR/backend" && cargo check --quiet 2>&1) || die "cargo check failed"
        ok "Cargo.lock updated"
    fi
else
    info "Skipping Cargo.lock (--only frontend)"
fi

# ── Step: Update pnpm-lock.yaml ──────────────────────────────────────────────

if $BUMP_FRONTEND; then
    next_step
    step "Step $CURRENT_STEP/$TOTAL_STEPS — Update pnpm-lock.yaml"

    if $DRY_RUN; then
        info "Would run 'pnpm install' in frontend/ to refresh lockfile"
    else
        (cd "$REPO_DIR/frontend" && pnpm install --no-frozen-lockfile 2>&1 | tail -1) \
            || die "pnpm install failed"
        ok "pnpm-lock.yaml updated"
    fi
else
    info "Skipping pnpm-lock.yaml (--only backend)"
fi

# ── Step 6: Recompute Nix hashes ─────────────────────────────────────────────

next_step
step "Step $CURRENT_STEP/$TOTAL_STEPS — Recompute Nix hashes"

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
    NIX_PACKAGE_BACKUP="$(mktemp)"
    cp "$NIX_PACKAGE" "$NIX_PACKAGE_BACKUP"

    # On any exit (error, signal, die, etc.) restore package.nix from backup
    # so a failed hash probe never leaves FAKE_HASH in the repo.
    cleanup() {
        local exit_code=$?
        if [[ $exit_code -ne 0 && -f "$NIX_PACKAGE_BACKUP" ]]; then
            warn "Restoring nix/package.nix from backup after failure"
            cp "$NIX_PACKAGE_BACKUP" "$NIX_PACKAGE"
        fi
        rm -f "$NIX_LOG" "$NIX_PACKAGE_BACKUP"
    }
    trap cleanup EXIT

    # Find the line numbers for each hash so we can target sed precisely
    PNPM_LINE="$(grep -n 'hash = "sha256-' "$NIX_PACKAGE" | head -1 | cut -d: -f1)"
    CARGO_LINE="$(grep -n 'cargoHash = "sha256-' "$NIX_PACKAGE" | head -1 | cut -d: -f1)"

    if $BUMP_FRONTEND; then
        [[ -n "$PNPM_LINE" ]]  || die "Could not find pnpmDeps hash line in package.nix"
    fi
    if $BUMP_BACKEND; then
        [[ -n "$CARGO_LINE" ]] || die "Could not find cargoHash line in package.nix"
    fi

    # Read the current hash values
    OLD_PNPM_HASH=""
    OLD_CARGO_HASH=""
    if $BUMP_FRONTEND && [[ -n "$PNPM_LINE" ]]; then
        OLD_PNPM_HASH="$(sed -n "${PNPM_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
        [[ -n "$OLD_PNPM_HASH" ]]  || die "Could not extract pnpmDeps hash from line $PNPM_LINE"
        info "pnpmDeps hash (line $PNPM_LINE):  $OLD_PNPM_HASH"
    fi
    if $BUMP_BACKEND && [[ -n "$CARGO_LINE" ]]; then
        OLD_CARGO_HASH="$(sed -n "${CARGO_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
        [[ -n "$OLD_CARGO_HASH" ]] || die "Could not extract cargoHash from line $CARGO_LINE"
        info "cargoHash     (line $CARGO_LINE): $OLD_CARGO_HASH"
    fi

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

    # Helper: stage only the files this script manages so unrelated changes
    # are never pulled into the flake evaluation.
    git_stage_managed() {
        local files=("$NIX_PACKAGE")
        $BUMP_BACKEND  && files+=("$CARGO_TOML" "$REPO_DIR/backend/Cargo.lock")
        $BUMP_FRONTEND && files+=("$PACKAGE_JSON" "$REPO_DIR/frontend/pnpm-lock.yaml")
        (cd "$REPO_DIR" && git add -- "${files[@]}")
    }

    # Helper: run nix build and capture stderr to the log file.
    # -L (print-build-logs) ensures hash-mismatch diagnostics appear on stderr
    # even when the failing derivation is an inner (non-top-level) build.
    nix_build_probe() {
        git_stage_managed
        (cd "$REPO_DIR" && nix build -L --no-link 2>"$NIX_LOG") || true
    }

    # ── 6a. pnpmDeps hash ──

    if $BUMP_FRONTEND; then
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
    else
        info "Skipping pnpmDeps hash (--only backend)"
    fi

    # ── 6b. cargoHash ──

    if $BUMP_BACKEND; then
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
    else
        info "Skipping cargoHash (--only frontend)"
    fi

    # ── 6c. Verify the full build passes ──

    info "Verifying nix build with updated hashes..."
    echo ""
    info "Final hashes in nix/package.nix:"
    if $BUMP_FRONTEND && [[ -n "$PNPM_LINE" ]]; then
        info "  pnpmDeps (line $PNPM_LINE): $(sed -n "${PNPM_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
    fi
    if $BUMP_BACKEND && [[ -n "$CARGO_LINE" ]]; then
        info "  cargoHash (line $CARGO_LINE): $(sed -n "${CARGO_LINE}p" "$NIX_PACKAGE" | grep -o 'sha256-[A-Za-z0-9+/]*=*')"
    fi
    echo ""

    git_stage_managed
    if (cd "$REPO_DIR" && nix build -L --no-link 2>"$NIX_LOG"); then
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

if [[ -n "$ONLY_SCOPE" ]]; then
    echo -e "  Scope:         ${CYAN}$ONLY_SCOPE only${NC}"
fi

if $DRY_RUN; then
    echo -e "  ${YELLOW}DRY RUN — no changes were made${NC}"
    echo ""
    echo -e "  Version:       ${RED}$CURRENT_VERSION${NC} → ${GREEN}$NEW_VERSION${NC}"
    echo ""
    echo "  Files that would be modified:"
    if $BUMP_BACKEND; then
        echo "    • backend/Cargo.toml"
        echo "    • backend/Cargo.lock"
    fi
    if $BUMP_FRONTEND; then
        echo "    • frontend/package.json"
        echo "    • frontend/pnpm-lock.yaml"
    fi
    echo "    • nix/package.nix"
    echo ""
    exit 0
fi

echo -e "  Version:       ${RED}$CURRENT_VERSION${NC} → ${GREEN}$NEW_VERSION${NC}"
echo ""
echo "  Updated files:"
if $BUMP_BACKEND; then
    echo -e "    ${GREEN}✔${NC} backend/Cargo.toml"
    echo -e "    ${GREEN}✔${NC} backend/Cargo.lock"
fi
if $BUMP_FRONTEND; then
    echo -e "    ${GREEN}✔${NC} frontend/package.json"
    echo -e "    ${GREEN}✔${NC} frontend/pnpm-lock.yaml"
fi
echo -e "    ${GREEN}✔${NC} nix/package.nix (versions)"
if ! $SKIP_NIX; then
    if $BUMP_BACKEND; then
        echo -e "    ${GREEN}✔${NC} nix/package.nix (cargoHash)"
    fi
    if $BUMP_FRONTEND; then
        echo -e "    ${GREEN}✔${NC} nix/package.nix (pnpmDeps hash)"
    fi
fi
echo ""

# ── Optional: git commit ──────────────────────────────────────────────────────

if $SKIP_COMMIT; then
    if [[ -n "$GIT_ACTION" ]]; then
        warn "--git $GIT_ACTION ignored because --skip-commit was specified"
    fi
    info "Skipping git commit (--skip-commit)"
    echo ""
    echo -e "  ${YELLOW}Remember to commit and tag:${NC}"
    echo "    git add -A"
    echo "    git commit -m \"chore: bump version to v$NEW_VERSION\""
    echo "    git tag v$NEW_VERSION"
    echo ""
else
    COMMIT_FILES_LIST=("$NIX_PACKAGE")
    $BUMP_BACKEND  && COMMIT_FILES_LIST+=("$CARGO_TOML" "$REPO_DIR/backend/Cargo.lock")
    $BUMP_FRONTEND && COMMIT_FILES_LIST+=("$PACKAGE_JSON" "$REPO_DIR/frontend/pnpm-lock.yaml")
    (cd "$REPO_DIR" && git add -- "${COMMIT_FILES_LIST[@]}")

    STAGED="$(cd "$REPO_DIR" && git diff --cached --name-only)"
    if [[ -z "$STAGED" ]]; then
        warn "Nothing to commit (all changes already committed?)"
    else
        COMMIT_FILES=""
        if $BUMP_BACKEND; then
            COMMIT_FILES="${COMMIT_FILES}
- backend/Cargo.toml
- backend/Cargo.lock"
        fi
        if $BUMP_FRONTEND; then
            COMMIT_FILES="${COMMIT_FILES}
- frontend/package.json
- frontend/pnpm-lock.yaml"
        fi
        COMMIT_FILES="${COMMIT_FILES}
- nix/package.nix (versions + hashes)"

        SCOPE_MSG=""
        if [[ -n "$ONLY_SCOPE" ]]; then
            SCOPE_MSG=" ($ONLY_SCOPE only)"
        fi

        (cd "$REPO_DIR" && git commit -m "chore: bump version to v${NEW_VERSION}${SCOPE_MSG}

Automated release bump $CURRENT_VERSION → $NEW_VERSION.

Files updated:${COMMIT_FILES}")

        ok "Committed version bump"

        if [[ "$GIT_ACTION" == "tag" || "$GIT_ACTION" == "push" ]]; then
            (cd "$REPO_DIR" && git tag "v$NEW_VERSION")
            ok "Tagged v$NEW_VERSION"
        fi

        if [[ "$GIT_ACTION" == "push" ]]; then
            (cd "$REPO_DIR" && git push && git push --tags)
            ok "Pushed commit and tags"
        fi

        if [[ -z "$GIT_ACTION" ]]; then
            echo ""
            echo -e "  ${YELLOW}Don't forget to tag and push:${NC}"
            echo "    git tag v$NEW_VERSION"
            echo "    git push && git push --tags"
        elif [[ "$GIT_ACTION" == "tag" ]]; then
            echo ""
            echo -e "  ${YELLOW}Don't forget to push:${NC}"
            echo "    git push && git push --tags"
        fi
    fi
fi

echo ""
echo -e "${GREEN}${BOLD}✔ Release bump complete!${NC}"
echo ""
