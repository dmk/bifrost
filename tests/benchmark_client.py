#!/usr/bin/env python3
import socket
import sys
import random
import time

def send_command(sock, command):
    """Send a command and receive the full response"""
    sock.sendall(command.encode('ascii'))

    response = b""
    while True:
        data = sock.recv(4096)
        if not data:
            break
        response += data

        # Check for different response endings
        if (b"END\r\n" in response or
            b"STORED\r\n" in response or
            b"NOT_FOUND\r\n" in response or
            b"ERROR\r\n" in response):
            break

    return response.decode('ascii', errors='ignore').strip()

def realistic_benchmark(host, port):
    """Simulate realistic memcached usage patterns"""
    try:
        # Create socket connection
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect((host, int(port)))

        # Simulate realistic workload
        operations = []

        # 1. Try to get some popular keys (cache misses/hits)
        popular_keys = ["user:123", "session:abc", "product:456", "cache:popular"]
        for key in popular_keys:
            operations.append(f"get {key}\r\n")

        # 2. Set some data (realistic sizes)
        set_ops = [
            "set user:789 0 3600 4\r\ntest\r\n",  # User data
            "set session:xyz 0 1800 12\r\nsessiondata1\r\n",  # Session
            "set product:999 0 7200 8\r\nproduct1\r\n",  # Product info
        ]
        operations.extend(set_ops)

        # 3. Get the data we just set
        for key in ["user:789", "session:xyz", "product:999"]:
            operations.append(f"get {key}\r\n")

        # 4. Multi-get operation (common pattern)
        operations.append("get user:123 user:789 product:456 product:999\r\n")

        # 5. Random gets (simulating varied access patterns)
        for i in range(3):
            random_key = f"random:{random.randint(1, 1000)}"
            operations.append(f"get {random_key}\r\n")

        # Execute all operations
        results = []
        for cmd in operations:
            response = send_command(sock, cmd)
            results.append(len(response))  # Track response sizes

        sock.close()

        # Print some stats for verification
        print(f"Completed {len(operations)} operations, avg response size: {sum(results)/len(results):.1f} bytes")

        return 0

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1

def simple_get(host, port, key="test"):
    """Simple GET operation for comparison"""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect((host, int(port)))

        response = send_command(sock, f"get {key}\r\n")
        sock.close()

        print(f"Response: {response[:100]}..." if len(response) > 100 else f"Response: {response}")
        return 0

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1

if __name__ == "__main__":
    if len(sys.argv) < 3 or len(sys.argv) > 4:
        print("Usage: python3 benchmark_client.py <host> <port> [simple]", file=sys.stderr)
        print("  Add 'simple' for basic GET test, omit for realistic workload", file=sys.stderr)
        sys.exit(1)

    host = sys.argv[1]
    port = sys.argv[2]
    mode = sys.argv[3] if len(sys.argv) == 4 else "complex"

    if mode == "simple":
        sys.exit(simple_get(host, port))
    else:
        sys.exit(realistic_benchmark(host, port))