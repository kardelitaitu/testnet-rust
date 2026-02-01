#!/bin/bash
# Auto-cleanup script for log rotation
# Keeps logs directory under 20MB by deleting old hourly log files

LOG_DIR="logs"
MAX_SIZE_MB=20

# Function to get directory size in MB
get_dir_size_mb() {
    du -sm "$LOG_DIR" 2>/dev/null | cut -f1
}

# Delete oldest log files until size is under limit
while true; do
    current_size=$(get_dir_size_mb)
    
    if [ -z "$current_size" ] || [ "$current_size" -lt "$MAX_SIZE_MB" ]; then
        break
    fi
    
    # Find and delete oldest log file
    oldest_file=$(find "$LOG_DIR" -name "app.*.log" -type f -printf '%T+ %p\n' | sort | head -n1 | cut -d' ' -f2-)
    
    if [ -z "$oldest_file" ]; then
        break
    fi
    
    echo "Deleting old log: $oldest_file (Current size: ${current_size}MB)"
    rm -f "$oldest_file"
done

echo "Log cleanup complete. Current size: $(get_dir_size_mb)MB / ${MAX_SIZE_MB}MB"
