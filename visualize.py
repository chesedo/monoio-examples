#!/usr/bin/env python3
"""
Visualize HTTP server benchmark results comparing monoio and hyper implementations.
"""

import sys
import os
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np

# Check if a CSV file path is provided
if len(sys.argv) < 2:
    print("Usage: python visualize.py <path_to_results.csv>")
    sys.exit(1)

csv_file = sys.argv[1]
if not os.path.exists(csv_file):
    print(f"Error: File {csv_file} not found")
    sys.exit(1)

# Create output directory for plots
output_dir = os.path.join(os.path.dirname(csv_file), "plots")
os.makedirs(output_dir, exist_ok=True)

# Read the CSV file
print(f"Reading benchmark data from {csv_file}...")
df = pd.read_csv(csv_file)

# Set the plot style
sns.set(style="whitegrid")
plt.rcParams.update({
    'figure.figsize': (12, 8),
    'font.size': 12,
    'axes.labelsize': 14,
    'axes.titlesize': 16
})

# Process socket errors - extract read errors
def extract_read_errors(error_str):
    if pd.isna(error_str) or error_str == "0":
        return 0
    try:
        # Extract the read error count
        read_part = error_str.split("read ")[1].split(",")[0]
        return int(read_part)
    except (IndexError, ValueError):
        return 0

df['read_errors'] = df['socket_errors'].apply(extract_read_errors)

# Calculate error rate
df['error_rate'] = df['read_errors'] / (df['requests'] + df['read_errors']) * 100

# Process the data for each endpoint
for endpoint in df['endpoint'].unique():
    endpoint_df = df[df['endpoint'] == endpoint]

    endpoint_name = endpoint.strip('/') if endpoint != '/' else 'root'
    print(f"\nGenerating plots for endpoint: {endpoint}")

    # Group by implementation and connections
    grouped = endpoint_df.groupby(['implementation', 'connections'])

    # Prepare data for bar chart (requests per second)
    implementations = endpoint_df['implementation'].unique()
    conn_values = sorted(endpoint_df['connections'].unique())

    # Requests per second bar chart
    plt.figure(figsize=(14, 8))
    bar_width = 0.35
    index = np.arange(len(conn_values))

    for i, impl in enumerate(implementations):
        impl_data = endpoint_df[endpoint_df['implementation'] == impl]
        reqs_per_sec = [impl_data[impl_data['connections'] == conn]['requests_per_sec'].mean()
                         for conn in conn_values]

        bars = plt.bar(index + i*bar_width, reqs_per_sec, bar_width,
                       label=impl, alpha=0.8)

        # Add annotation for error rate if present
        for j, conn in enumerate(conn_values):
            error_rate = impl_data[impl_data['connections'] == conn]['error_rate'].mean()
            if error_rate > 0:
                plt.text(index[j] + i*bar_width, reqs_per_sec[j] + 5000,
                         f"Err: {error_rate:.1f}%",
                         ha='center', va='bottom', fontsize=9, color='red',
                         rotation=45)

    plt.xlabel('Concurrent Connections')
    plt.ylabel('Requests per Second')
    plt.title(f'HTTP Server Throughput - Endpoint: {endpoint}')
    plt.xticks(index + bar_width/2, conn_values)
    plt.legend()
    plt.grid(True, linestyle='--', alpha=0.7)

    # Add some space at the top for annotations
    plt.ylim(0, plt.ylim()[1] * 1.15)

    # Add implementation comparison text
    for conn in conn_values:
        monoio_rps = endpoint_df[(endpoint_df['implementation'] == 'monoio-http') &
                                (endpoint_df['connections'] == conn)]['requests_per_sec'].mean()
        hyper_rps = endpoint_df[(endpoint_df['implementation'] == 'hyper-http') &
                               (endpoint_df['connections'] == conn)]['requests_per_sec'].mean()

        if monoio_rps > 0 and hyper_rps > 0:
            ratio = hyper_rps / monoio_rps
            plt.figtext(0.5, 0.01, f"At {conn} connections, hyper-http is {ratio:.2f}x faster than monoio-http",
                       ha='center', fontsize=10, bbox={"facecolor":"orange", "alpha":0.2, "pad":5})

    plt.tight_layout(rect=[0, 0.05, 1, 0.95])
    plt.savefig(os.path.join(output_dir, f"{endpoint_name}_throughput.png"))
    print(f"  Saved throughput chart: {endpoint_name}_throughput.png")

    # Latency comparison line charts
    plt.figure(figsize=(14, 8))

    # Define latency metrics to plot
    latency_metrics = [
        ('latency_avg_ms', 'Average'),
        ('latency_p50_ms', '50th Percentile'),
        ('latency_p90_ms', '90th Percentile'),
        ('latency_p99_ms', '99th Percentile')
    ]

    # Create a 2x2 grid for the latency plots
    fig, axes = plt.subplots(2, 2, figsize=(16, 12))
    axes = axes.flatten()

    for i, (metric, title) in enumerate(latency_metrics):
        ax = axes[i]

        for impl in implementations:
            impl_data = endpoint_df[endpoint_df['implementation'] == impl]
            latency_data = [impl_data[impl_data['connections'] == conn][metric].mean()
                           for conn in conn_values]

            ax.plot(conn_values, latency_data, 'o-', linewidth=2, label=impl)

        ax.set_title(f'{title} Latency - {endpoint}')
        ax.set_xlabel('Concurrent Connections')
        ax.set_ylabel('Latency (ms)')
        ax.legend()
        ax.grid(True, linestyle='--', alpha=0.7)

        # Use logarithmic scale if values vary greatly
        if max(endpoint_df[metric]) / (min(endpoint_df[metric]) + 0.001) > 10:
            ax.set_yscale('log')

    plt.tight_layout()
    plt.savefig(os.path.join(output_dir, f"{endpoint_name}_latency.png"))
    print(f"  Saved latency chart: {endpoint_name}_latency.png")

    # Create a combined metrics bar chart (single metric view)
    plt.figure(figsize=(14, 8))

    for i, impl in enumerate(implementations):
        impl_data = endpoint_df[endpoint_df['implementation'] == impl]
        metrics = [
            impl_data['requests_per_sec'].mean() / 1000,  # Scale down for visibility
            impl_data['latency_avg_ms'].mean() * 10,      # Scale up for visibility
            impl_data['latency_p99_ms'].mean() * 10,      # Scale up for visibility
            impl_data['error_rate'].mean()
        ]

        plt.bar(
            np.arange(4) + i*0.35,
            metrics,
            0.35,
            label=impl
        )

    plt.xticks(np.arange(4) + 0.35/2, [
        'Reqs/sec (thousands)',
        'Avg Latency (ms) × 10',
        'p99 Latency (ms) × 10',
        'Error Rate (%)'
    ])
    plt.title(f'Performance Metrics Comparison - {endpoint}')
    plt.legend()
    plt.tight_layout()
    plt.savefig(os.path.join(output_dir, f"{endpoint_name}_metrics.png"))
    print(f"  Saved combined metrics chart: {endpoint_name}_metrics.png")

print("\nVisualization complete. All charts saved to:", output_dir)
