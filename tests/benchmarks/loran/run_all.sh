#!/bin/bash
# Actrix complete performance benchmark suite

set -e

RESULTS_DIR="benchmarks/results/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "🚀 Actrix performance benchmarks"
echo "Results directory: $RESULTS_DIR"
echo

# Check if service is running
if ! curl -k -s https://localhost:8443/signer/health > /dev/null 2>&1; then
    echo "❌ Service is not running, please start actrix first"
    exit 1
fi

echo "✅ Service is running normally"
echo

# === 1. Health Check benchmark ===
echo "1️⃣  Health Check benchmark..."
if command -v wrk &> /dev/null; then
    wrk -t4 -c100 -d30s --latency \
        https://localhost:8443/signer/health \
        > "$RESULTS_DIR/wrk_health.txt" 2>&1

    echo "  ✅ Done"
    grep "Requests/sec" "$RESULTS_DIR/wrk_health.txt"
    grep "Latency" "$RESULTS_DIR/wrk_health.txt" | head -3
else
    echo "  ⚠️  wrk not installed, skipping"
fi
echo

# === 2. Metrics Endpoint benchmark ===
echo "2️⃣  Metrics Endpoint benchmark..."
if command -v wrk &> /dev/null; then
    wrk -t4 -c100 -d30s --latency \
        https://localhost:8443/metrics \
        > "$RESULTS_DIR/wrk_metrics.txt" 2>&1

    echo "  ✅ Done"
    grep "Requests/sec" "$RESULTS_DIR/wrk_metrics.txt"
else
    echo "  ⚠️  wrk not installed, skipping"
fi
echo

# === 3. Signer gRPC benchmark ===
echo "3️⃣  Signer gRPC benchmark..."
if command -v ghz &> /dev/null; then
    cd crates/signer
    ghz --insecure \
        --proto proto/keyserver.proto \
        --call signer.v1.KeyServer/HealthCheck \
        -d '{"service":"signer"}' \
        -c 50 -n 10000 \
        localhost:50052 \
        > "$RESULTS_DIR/ghz_signer_health.txt" 2>&1
    cd ../..

    echo "  ✅ Done"
    grep "Requests/sec" "$RESULTS_DIR/ghz_signer_health.txt" || true
else
    echo "  ⚠️  ghz not installed, skipping"
    echo "     Install: go install github.com/bojand/ghz/cmd/ghz@latest"
fi
echo

# === 4. K6 load test ===
echo "4️⃣  K6 load test..."
if command -v k6 &> /dev/null; then
    if [ -f "benchmarks/k6/load_test.js" ]; then
        k6 run benchmarks/k6/load_test.js \
            --out json="$RESULTS_DIR/k6_load.json"
        echo "  ✅ Done"
    else
        echo "  ⚠️  K6 script not found"
    fi
else
    echo "  ⚠️  k6 not installed, skipping"
    echo "     Install: https://k6.io/docs/getting-started/installation/"
fi
echo

# === 5. Generate summary report ===
echo "5️⃣  Generating summary report..."
cat > "$RESULTS_DIR/summary.txt" <<EOF
Actrix performance benchmark report
=====================
Test time: $(date)
Git Commit: $(git rev-parse --short HEAD 2>/dev/null || echo "N/A")

EOF

if [ -f "$RESULTS_DIR/wrk_health.txt" ]; then
    echo "Health Check performance:" >> "$RESULTS_DIR/summary.txt"
    grep "Requests/sec" "$RESULTS_DIR/wrk_health.txt" >> "$RESULTS_DIR/summary.txt"
    grep "99%" "$RESULTS_DIR/wrk_health.txt" >> "$RESULTS_DIR/summary.txt"
    echo "" >> "$RESULTS_DIR/summary.txt"
fi

if [ -f "$RESULTS_DIR/wrk_metrics.txt" ]; then
    echo "Metrics Endpoint performance:" >> "$RESULTS_DIR/summary.txt"
    grep "Requests/sec" "$RESULTS_DIR/wrk_metrics.txt" >> "$RESULTS_DIR/summary.txt"
    echo "" >> "$RESULTS_DIR/summary.txt"
fi

echo "  ✅ Done"
echo

# === Display summary ===
echo "📊 Test summary:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
cat "$RESULTS_DIR/summary.txt"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "✅ All tests completed"
echo "Detailed results: $RESULTS_DIR/"
echo
echo "View summary: cat $RESULTS_DIR/summary.txt"
