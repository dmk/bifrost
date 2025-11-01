#!/usr/bin/env bash

# Orchestrate a full benchmark run on a remote EC2 host via SSH.
# Requirements locally: ssh, scp/rsync, and AWS instance reachable.

# Find repo root from script location
cd "$(dirname "$0")/../.." || exit 1

set -euo pipefail

usage() {
  cat <<USAGE
Usage: $(basename "$0") --host ec2-user@IP [--key ~/.ssh/key.pem] [--repo /home/ec2-user/bifrost] [--timeout 5400]

Runs tests/benchmark/run_benchmark.sh on the remote host inside the repository,
then fetches tests/benchmark/benchmark_results/ back to ./tests/benchmark/benchmark_results/

Options:
  --host       SSH target in user@host form (required)
  --key        Path to SSH private key (default: SSH agent or default key)
  --repo       Remote repo directory (default: ~/bifrost)
  --branch     Git branch to checkout remotely (default: current HEAD branch name)
  --timeout    Max seconds to allow remote benchmark to run (default: 5400)
  --no-bootstrap  Skip installing Docker on remote (default: run bootstrap)

Examples:
  $(basename "$0") --host ec2-user@1.2.3.4 --key ~/.ssh/my.pem
USAGE
}

HOST=""
KEY_PATH=""
REMOTE_REPO="/root/bifrost"  # Default to absolute path for root user
BRANCH=""
TIMEOUT_SEC=5400
DO_BOOTSTRAP=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host) HOST="$2"; shift 2 ;;
    --key) KEY_PATH="$2"; shift 2 ;;
    --repo) REMOTE_REPO="$2"; shift 2 ;;
    --branch) BRANCH="$2"; shift 2 ;;
    --timeout) TIMEOUT_SEC="$2"; shift 2 ;;
    --no-bootstrap) DO_BOOTSTRAP=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1"; usage; exit 1 ;;
  esac
done

if [[ -z "$HOST" ]]; then
  echo "--host is required"; usage; exit 1
fi

if [[ -n "$KEY_PATH" ]]; then
  SSH=(ssh -o StrictHostKeyChecking=accept-new -o ServerAliveInterval=30 -o ServerAliveCountMax=6 -i "$KEY_PATH" "$HOST")
  SCP=(scp -o StrictHostKeyChecking=accept-new -i "$KEY_PATH")
  RSYNC=(rsync -az --delete -e "ssh -o StrictHostKeyChecking=accept-new -i $KEY_PATH")
else
  SSH=(ssh -o StrictHostKeyChecking=accept-new -o ServerAliveInterval=30 -o ServerAliveCountMax=6 "$HOST")
  SCP=(scp -o StrictHostKeyChecking=accept-new)
  RSYNC=(rsync -az --delete -e "ssh -o StrictHostKeyChecking=accept-new")
fi

# Determine current branch if not provided
if [[ -z "$BRANCH" ]]; then
  if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    BRANCH=$(git rev-parse --abbrev-ref HEAD)
  else
    BRANCH="main"
  fi
fi

echo "[local] Preparing remote directory: $REMOTE_REPO"
"${SSH[@]}" "mkdir -p $REMOTE_REPO"

if [[ $DO_BOOTSTRAP -eq 1 ]]; then
  echo "[local] Uploading and running bootstrap"
  "${SCP[@]}" tests/benchmark/remote_bootstrap.sh "$HOST:$REMOTE_REPO/"
  "${SSH[@]}" "bash $REMOTE_REPO/remote_bootstrap.sh"
fi

echo "[local] Syncing repo to remote"
# Prefer rsync if available remotely; fallback to tar over ssh
if "${SSH[@]}" 'command -v rsync >/dev/null 2>&1'; then
  echo "[local] Using rsync"
  "${RSYNC[@]}" --exclude '.git' --exclude 'target' --exclude 'tests/benchmark/benchmark_results' ./ "$HOST:$REMOTE_REPO/"
else
  echo "[local] rsync not found on remote; using tar stream"
  EXCLUDES=(
    "--exclude=.git"
    "--exclude=target"
    "--exclude=tests/benchmark/benchmark_results"
  )
  tar -czf - "${EXCLUDES[@]}" . | "${SSH[@]}" "mkdir -p $REMOTE_REPO && tar -xzf - -C $REMOTE_REPO"
fi

echo "[remote] Building and running benchmark (timeout ${TIMEOUT_SEC}s)"
REMOTE_CMD="set -euo pipefail; cd '$REMOTE_REPO/tests/benchmark'; timeout ${TIMEOUT_SEC}s ./run_benchmark.sh stress_test"
if ! "${SSH[@]}" "$REMOTE_CMD"; then
  echo "Remote benchmark command failed or timed out" >&2
  exit 2
fi

echo "[local] Fetching results back"
mkdir -p tests/benchmark/benchmark_results
"${RSYNC[@]}" "$HOST:$REMOTE_REPO/tests/benchmark/benchmark_results/" tests/benchmark/benchmark_results/

echo "[local] Done. Results in tests/benchmark/benchmark_results/"


