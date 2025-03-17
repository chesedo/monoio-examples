#!/usr/bin/env bash
# HTTP Server Benchmarking Script for monoio vs hyper comparison with remote server

set -e

# Configuration
SERVER_HOST="server-machine-ip"  # Replace with your server machine's IP
SERVER_USER="username"           # Replace with SSH username for server machine
SERVER_PATH="/path/to/project"   # Path to the project on server machine
SERVER_PORT="8080"
BASE_URL="http://${SERVER_HOST}:${SERVER_PORT}"
DURATION=30                      # Duration in seconds
CONNECTIONS=(10 50 100 250 500 1000) # Number of concurrent connections to test
THREADS=4                        # Number of threads for wrk
ENDPOINTS=("/" "/health")
TIMEOUT="2s"                     # Timeout for wrk

# Check for required tools
command -v wrk >/dev/null 2>&1 || { echo "Error: wrk is required but not installed. Install it with your package manager"; exit 1; }
command -v bc >/dev/null 2>&1 || { echo "Error: bc is required but not installed. Install it with your package manager"; exit 1; }
command -v ssh >/dev/null 2>&1 || { echo "Error: ssh is required but not installed. Install it with your package manager"; exit 1; }

# Output directory
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTPUT_DIR="benchmark_results_${TIMESTAMP}"
mkdir -p "$OUTPUT_DIR"

# Log file
LOG_FILE="${OUTPUT_DIR}/benchmark.log"
CSV_FILE="${OUTPUT_DIR}/results.csv"

# Initialize CSV file with header
echo "implementation,endpoint,connections,threads,duration,requests,requests_per_sec,latency_avg_ms,latency_p50_ms,latency_p90_ms,latency_p99_ms,transfer_per_sec,socket_errors" > "$CSV_FILE"

# Function to kill any process using the server port on the remote server
kill_remote_process_on_port() {
    local pid=$(ssh ${SERVER_USER}@${SERVER_HOST} "lsof -i:${SERVER_PORT} -t" 2>/dev/null)
    if [ ! -z "$pid" ]; then
        echo "Killing process $pid using port $SERVER_PORT on remote server" | tee -a "$LOG_FILE"
        ssh ${SERVER_USER}@${SERVER_HOST} "kill -9 $pid" 2>/dev/null || true
        sleep 2
    fi
}

# Function to run a single benchmark
run_benchmark() {
    local implementation=$1
    local endpoint=$2
    local connections=$3

    echo "==================================" | tee -a "$LOG_FILE"
    echo "Benchmarking $implementation - $endpoint with $connections connections" | tee -a "$LOG_FILE"
    echo "==================================" | tee -a "$LOG_FILE"

    # Run the benchmark with timeout
    wrk_output=$(wrk --latency --timeout $TIMEOUT -t$THREADS -c$connections -d${DURATION}s "${BASE_URL}${endpoint}" 2>&1)

    # Log raw output
    echo "$wrk_output" >> "${OUTPUT_DIR}/${implementation}_${endpoint//\//_}_c${connections}.txt"
    echo "$wrk_output" | tee -a "$LOG_FILE"

    # Extract metrics
    requests=$(echo "$wrk_output" | grep "requests in" | awk '{print $1}')
    req_per_sec=$(echo "$wrk_output" | grep "Requests/sec:" | awk '{print $2}')
    latency_avg=$(echo "$wrk_output" | grep "Thread Stats" -A 2 | grep "Latency" | awk '{print $2}')

    # Extract percentile data - we expect latency flag to always provide this
    latency_p50=$(echo "$wrk_output" | grep -A 4 "Latency Distribution" | grep "50%" | awk '{print $2}' || echo "0")
    latency_p90=$(echo "$wrk_output" | grep -A 4 "Latency Distribution" | grep "90%" | awk '{print $2}' || echo "0")
    latency_p99=$(echo "$wrk_output" | grep -A 4 "Latency Distribution" | grep "99%" | awk '{print $2}' || echo "0")

    transfer=$(echo "$wrk_output" | grep "Transfer/sec:" | awk '{print $2}')

    # Extract socket errors if any
    socket_errors="0"
    if echo "$wrk_output" | grep -q "Socket errors:"; then
        socket_errors=$(echo "$wrk_output" | grep "Socket errors:" | sed 's/.*Socket errors: //')
    fi

    # Convert latency values to milliseconds for consistency
    latency_avg_ms=$(convert_to_ms "$latency_avg")
    latency_p50_ms=$(convert_to_ms "$latency_p50")
    latency_p90_ms=$(convert_to_ms "$latency_p90")
    latency_p99_ms=$(convert_to_ms "$latency_p99")

    # Add to CSV
    echo "$implementation,$endpoint,$connections,$THREADS,$DURATION,$requests,$req_per_sec,$latency_avg_ms,$latency_p50_ms,$latency_p90_ms,$latency_p99_ms,$transfer,\"$socket_errors\"" >> "$CSV_FILE"

    echo "" | tee -a "$LOG_FILE"
    echo "Sleeping for 5 seconds to allow system recovery..." | tee -a "$LOG_FILE"
    sleep 5
}

# Function to convert latency values to milliseconds
convert_to_ms() {
    local value=$1

    # Return 0 if value is empty or null
    if [ -z "$value" ]; then
        echo "0"
        return
    fi

    local unit=${value: -2}
    local number=${value%??}

    case "$unit" in
        "us")
            echo "scale=3; $number / 1000" | bc
            ;;
        "ms")
            echo "$number"
            ;;
        "s ")
            echo "scale=3; $number * 1000" | bc
            ;;
        *)
            echo "$number"
            ;;
    esac
}

# Function to run server and benchmarks
benchmark_implementation() {
    local implementation=$1
    local binary_name=$2

    echo "Starting $implementation server on remote machine..." | tee -a "$LOG_FILE"

    # Kill any existing process on the port
    kill_remote_process_on_port

    # Start the server in background on the remote server
    echo "Running $binary_name on remote server" | tee -a "$LOG_FILE"
    ssh -f ${SERVER_USER}@${SERVER_HOST} "cd ${SERVER_PATH} && ./target/release/$binary_name > /tmp/${binary_name}.log 2>&1 &"

    # Wait for server to start
    echo "Waiting for server to start..." | tee -a "$LOG_FILE"
    sleep 3

    # Test server is responding
    if ! curl -s "${BASE_URL}/health" > /dev/null; then
        echo "Server failed to start or respond properly" | tee -a "$LOG_FILE"
        kill_remote_process_on_port
        return 1
    fi

    echo "Server is running, beginning benchmarks..." | tee -a "$LOG_FILE"

    # Run benchmarks for each endpoint and connection count
    for endpoint in "${ENDPOINTS[@]}"; do
        for conn in "${CONNECTIONS[@]}"; do
            run_benchmark "$implementation" "$endpoint" "$conn"
        done
    done

    # Stop the server
    echo "Stopping $implementation server..." | tee -a "$LOG_FILE"
    kill_remote_process_on_port

    # Fetch server logs
    echo "Fetching server logs..." | tee -a "$LOG_FILE"
    scp ${SERVER_USER}@${SERVER_HOST}:/tmp/${binary_name}.log "${OUTPUT_DIR}/${binary_name}.log" || true
}

# Check SSH connectivity to server
check_server_connectivity() {
    echo "Checking connectivity to server ${SERVER_HOST}..." | tee -a "$LOG_FILE"
    if ! ssh -q -o BatchMode=yes -o ConnectTimeout=5 ${SERVER_USER}@${SERVER_HOST} exit; then
        echo "Cannot connect to server ${SERVER_HOST}. Please check SSH connectivity." | tee -a "$LOG_FILE"
        exit 1
    fi
    echo "Successfully connected to server ${SERVER_HOST}" | tee -a "$LOG_FILE"
}

# Main execution
echo "Starting benchmark suite at $(date)" | tee "$LOG_FILE"
echo "Results will be saved to $OUTPUT_DIR" | tee -a "$LOG_FILE"
echo "Server: ${SERVER_HOST}:${SERVER_PORT}" | tee -a "$LOG_FILE"

# Check connectivity
check_server_connectivity

# Make sure the port is free before starting
kill_remote_process_on_port

# Build the project on the remote server
echo "Building $implementation on remote server..." | tee -a "$LOG_FILE"
ssh ${SERVER_USER}@${SERVER_HOST} "cd ${SERVER_PATH} && cargo build --release" || {
    echo "Failed to build $implementation on server" | tee -a "$LOG_FILE"
    exit 1
}

# Benchmark monoio implementation
benchmark_implementation "monoio-http" "monoio-http"

# Benchmark hyper implementation
benchmark_implementation "hyper-http" "hyper-http"

echo "Benchmarking complete! Results are in $OUTPUT_DIR" | tee -a "$LOG_FILE"
echo "CSV data available at $CSV_FILE" | tee -a "$LOG_FILE"

# Generate simple report
echo "==================================" | tee -a "$LOG_FILE"
echo "Summary Report" | tee -a "$LOG_FILE"
echo "==================================" | tee -a "$LOG_FILE"

# Extract average requests per second for each implementation and endpoint
for endpoint in "${ENDPOINTS[@]}"; do
    echo "Endpoint: $endpoint" | tee -a "$LOG_FILE"
    monoio_avg=$(grep "monoio-http,$endpoint" "$CSV_FILE" | awk -F, '{sum+=$7} END {print sum/NR}')
    hyper_avg=$(grep "hyper-http,$endpoint" "$CSV_FILE" | awk -F, '{sum+=$7} END {print sum/NR}')
    echo "  monoio-http average reqs/sec: $monoio_avg" | tee -a "$LOG_FILE"
    echo "  hyper-http average reqs/sec: $hyper_avg" | tee -a "$LOG_FILE"

    # Calculate performance difference
    if [[ ! -z "$monoio_avg" && ! -z "$hyper_avg" && "$hyper_avg" != "0" ]]; then
        diff_percent=$(echo "scale=2; ($monoio_avg - $hyper_avg) / $hyper_avg * 100" | bc)
        echo "  monoio-http is $diff_percent% different from hyper-http" | tee -a "$LOG_FILE"
    fi
    echo "" | tee -a "$LOG_FILE"
done

echo "Detailed results can be found in $OUTPUT_DIR" | tee -a "$LOG_FILE"
