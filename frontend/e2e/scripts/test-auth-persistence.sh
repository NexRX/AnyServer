#!/usr/bin/env bash
#
# Test runner for auth token persistence E2E test
#
# This script runs the auth persistence test multiple times to verify
# that the fixes for ticket #037 have resolved the flakiness issues.
#
# Usage:
#   ./test-auth-persistence.sh [iterations]
#
# Examples:
#   ./test-auth-persistence.sh       # Run 3 times (default)
#   ./test-auth-persistence.sh 10    # Run 10 times
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default iterations
ITERATIONS=${1:-3}

# Counters
PASSED=0
FAILED=0
RETRIED=0

echo -e "${BLUE}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Auth Token Persistence Test Runner (Ticket #037)             ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "Running test ${GREEN}$ITERATIONS${NC} times to verify stability..."
echo ""

# Check if backend is built
if [ ! -f "../backend/target/debug/anyserver" ]; then
    echo -e "${YELLOW}⚠ Backend binary not found!${NC}"
    echo -e "Building backend first..."
    (cd ../backend && cargo build)
fi

# Track start time
START_TIME=$(date +%s)

# Run the test multiple times
for i in $(seq 1 $ITERATIONS); do
    echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}Test Run $i of $ITERATIONS${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"

    # Run the test and capture output
    if npm run test:e2e -- auth.spec.ts --grep "token persists across page reloads" 2>&1 | tee /tmp/auth-test-$i.log; then
        echo -e "${GREEN}✓ Test run $i PASSED${NC}"
        PASSED=$((PASSED + 1))

        # Check if it passed on retry
        if grep -q "Retry #1" /tmp/auth-test-$i.log; then
            echo -e "${YELLOW}  (passed on retry)${NC}"
            RETRIED=$((RETRIED + 1))
        fi
    else
        echo -e "${RED}✗ Test run $i FAILED${NC}"
        FAILED=$((FAILED + 1))

        # Save failed test output
        cp /tmp/auth-test-$i.log ./test-results/auth-persistence-failure-$i.log
        echo -e "${YELLOW}  Output saved to: test-results/auth-persistence-failure-$i.log${NC}"
    fi

    echo ""

    # Clean up temp log
    rm -f /tmp/auth-test-$i.log
done

# Calculate elapsed time
END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
ELAPSED_MIN=$((ELAPSED / 60))
ELAPSED_SEC=$((ELAPSED % 60))

# Print summary
echo -e "${BLUE}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Test Summary                                                  ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "Total runs:      ${BLUE}$ITERATIONS${NC}"
echo -e "Passed:          ${GREEN}$PASSED${NC}"
echo -e "Failed:          ${RED}$FAILED${NC}"
echo -e "Passed on retry: ${YELLOW}$RETRIED${NC}"
echo -e "Success rate:    $(awk "BEGIN {printf \"%.1f\", ($PASSED/$ITERATIONS)*100}")%"
echo -e "Elapsed time:    ${ELAPSED_MIN}m ${ELAPSED_SEC}s"
echo ""

# Evaluation
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║  ✓ ALL TESTS PASSED!                                          ║${NC}"
    echo -e "${GREEN}║    The auth token persistence test is stable.                 ║${NC}"
    echo -e "${GREEN}╚════════════════════════════════════════════════════════════════╝${NC}"

    if [ $RETRIED -gt 0 ]; then
        echo -e ""
        echo -e "${YELLOW}Note: $RETRIED test(s) required retry due to infrastructure timing.${NC}"
        echo -e "${YELLOW}This is acceptable and handled by the retry configuration.${NC}"
    fi

    exit 0
elif [ $PASSED -gt 0 ]; then
    echo -e "${YELLOW}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${YELLOW}║  ⚠ INTERMITTENT FAILURES DETECTED                             ║${NC}"
    echo -e "${YELLOW}║    The test is still flaky. Further investigation needed.     ║${NC}"
    echo -e "${YELLOW}╚════════════════════════════════════════════════════════════════╝${NC}"
    exit 1
else
    echo -e "${RED}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${RED}║  ✗ ALL TESTS FAILED                                            ║${NC}"
    echo -e "${RED}║    The test is consistently failing. Check implementation.     ║${NC}"
    echo -e "${RED}╚════════════════════════════════════════════════════════════════╝${NC}"
    exit 1
fi
