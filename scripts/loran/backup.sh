#!/bin/bash
# Loran data backup script

set -e

BACKUP_DIR="${BACKUP_DIR:-./backup}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_PATH="$BACKUP_DIR/$TIMESTAMP"

echo "🔄 Starting Loran data backup..."
echo "Backup directory: $BACKUP_PATH"
echo

# Create backup directory
mkdir -p "$BACKUP_PATH"

# Backup config files
echo "1. Backing up config files..."
if [ -f "config.toml" ]; then
    cp config.toml "$BACKUP_PATH/config.toml"
    echo "  ✅ config.toml"
fi

# Backup database files
echo
echo "2. Backing up databases..."
find . -name "*.db" -type f | while read -r db; do
    db_name=$(basename "$db")
    # Use SQLite VACUUM INTO to create a clean backup copy
    if command -v sqlite3 &> /dev/null; then
        sqlite3 "$db" "VACUUM INTO '$BACKUP_PATH/$db_name'"
        echo "  ✅ $db_name (VACUUM)"
    else
        cp "$db" "$BACKUP_PATH/$db_name"
        echo "  ✅ $db_name (copy)"
    fi
done

# Backup logs (last 7 days)
echo
echo "3. Backing up recent logs..."
if [ -d "logs" ]; then
    mkdir -p "$BACKUP_PATH/logs"
    find logs -name "*.log" -mtime -7 -exec cp {} "$BACKUP_PATH/logs/" \;
    echo "  ✅ Logs from last 7 days"
fi

# Compress backup
echo
echo "4. Compressing backup..."
cd "$BACKUP_DIR"
tar -czf "${TIMESTAMP}.tar.gz" "$TIMESTAMP"
rm -rf "$TIMESTAMP"
echo "  ✅ ${TIMESTAMP}.tar.gz"

# Clean old backups (keep last 30 days)
echo
echo "5. Cleaning old backups..."
find . -name "*.tar.gz" -mtime +30 -delete
count=$(find . -name "*.tar.gz" | wc -l)
echo "  ✅ Kept $count backup files"

echo
echo "✅ Backup completed: $BACKUP_DIR/${TIMESTAMP}.tar.gz"
echo "Restore command: tar -xzf $BACKUP_DIR/${TIMESTAMP}.tar.gz"
