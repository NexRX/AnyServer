{
  pkgs ? import <nixpkgs> {
    config = {
      allowUnfree = true;
      permittedInsecurePackages = [
        "dotnet-runtime-6.0.36"
      ];
    };
  },
}:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Playwright browsers with all dependencies
    playwright-driver.browsers

    # Node.js and package managers
    nodejs_20
    nodePackages.pnpm
    nodePackages.npm

    # java
    zulu17
    zulu21

    # C#
    dotnet-runtime # newer basically
    dotnet-runtime_6

    # Build tools
    git

    # Process management utilities
    killall
    lsof
    procps

    # Network utilities (for port checking)
    netcat

    # For test debugging
    gnused
    gnugrep
    coreutils
  ];

  shellHook = ''
    # Set Playwright browsers path to use Nix-provided browsers
    export PLAYWRIGHT_BROWSERS_PATH="${pkgs.playwright-driver.browsers}"
    export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true

    # Ensure pnpm uses local node_modules
    export PNPM_HOME="$PWD/.pnpm"

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  🚀 AnyServer Frontend Development Shell"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "  📦 Node.js:    $(node --version)"
    echo "  📦 pnpm:       $(pnpm --version)"
    echo "  🎭 Playwright: $PLAYWRIGHT_BROWSERS_PATH"
    echo ""
    echo "  Available commands:"
    echo "    pnpm install        - Install dependencies"
    echo "    pnpm dev            - Start development server"
    echo "    pnpm build          - Production build"
    echo "    pnpm test           - Run unit tests"
    echo "    pnpm test:e2e       - Run E2E tests"
    echo ""
    echo "  E2E Test Examples:"
    echo "    ./e2e/run-tests.sh -s auth           # Run auth tests"
    echo "    ./e2e/run-tests.sh -u                # Run in UI mode"
    echo "    ./e2e/run-tests.sh -w -s console     # Headed browser mode"
    echo "    ./e2e/run-tests.sh -t 'login'        # Filter by test name"
    echo ""
    echo "  📝 Make sure backend is built before running E2E tests:"
    echo "     cd ../backend && cargo build"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
  '';
}
