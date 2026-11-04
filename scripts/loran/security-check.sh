#!/bin/bash
# Loran security check script

set -e

echo "🔍 Loran security check..."
echo

# Check database file permissions
echo "1. Checking database file permissions..."
find . -name "*.db" -type f | while read -r db; do
    perm=$(stat -c "%a" "$db" 2>/dev/null || stat -f "%A" "$db" 2>/dev/null)
    if [ "$perm" != "600" ]; then
        echo "  ⚠️  $db permission is $perm, should be 600"
        echo "     Fix: chmod 600 $db"
    else
        echo "  ✅ $db"
    fi
done

# Check for default keys in config
echo
echo "2. Checking for default keys..."
if grep -r "default-.*-key\|change-in-production" config*.toml 2>/dev/null; then
    echo "  ⚠️  Default key found, please change it!"
else
    echo "  ✅ No default keys found"
fi

# Check key length
echo
echo "3. Checking loran_shared_key length..."
if [ -f "config.toml" ]; then
    key=$(grep "loran_shared_key" config.toml | cut -d'"' -f2)
    if [ ${#key} -lt 16 ]; then
        echo "  ⚠️  Key length ${#key}, recommend at least 16 characters"
    else
        echo "  ✅ Key length ${#key}"
    fi
fi

# Check TLS certificates
echo
echo "4. Checking TLS certificates..."
if [ -f "config.toml" ]; then
    cert=$(grep "cert =" config.toml | cut -d'"' -f2 | head -1)
    if [ -n "$cert" ] && [ -f "$cert" ]; then
        expiry=$(openssl x509 -enddate -noout -in "$cert" 2>/dev/null | cut -d= -f2)
        echo "  ✅ Certificate exists: $cert"
        echo "     Expiry: $expiry"
    else
        echo "  ⚠️  Not configured or certificate not found"
    fi
fi

# Check dependency vulnerabilities
echo
echo "5. Checking dependency vulnerabilities..."
if command -v cargo-audit &> /dev/null; then
    cargo audit
else
    echo "  ⚠️  cargo-audit not installed, skipping"
    echo "     Install: cargo install cargo-audit"
fi

echo
echo "✅ Security check completed"
