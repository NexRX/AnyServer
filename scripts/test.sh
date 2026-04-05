#!/usr/bin/env bash
set -euo pipefail

cd backend && cargo test --test integration

cd ../frontend
pnpm test
if [ -f /etc/NIXOS ]; then
  nix-shell --run "pnpm test:e2e --workers 6"
else
  pnpm test:e2e --workers 6
fi

echo "✅ All Test suites completed successfully"
