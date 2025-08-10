#!/usr/bin/env bash

# Remote bootstrap for EC2 (or any Linux host)
# - Installs Docker + compose plugin + buildx
# - Ensures netcat and python3 are available
# - Pre-pulls images used by the benchmark

set -euo pipefail

log() { printf "[%s] %s\n" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }

require_root() {
  if [ "${EUID:-$(id -u)}" -ne 0 ]; then
    log "Re-running with sudo..."
    exec sudo -E bash "$0" "$@"
  fi
}

require_root "$@"

log "Detecting distro/package manager"
PKG=""
if command -v apt-get >/dev/null 2>&1; then
  PKG=apt
elif command -v dnf >/dev/null 2>&1; then
  PKG=dnf
elif command -v yum >/dev/null 2>&1; then
  PKG=yum
else
  log "Unsupported distro (need apt, dnf or yum)" && exit 1
fi

log "Installing base packages and Docker"
case "$PKG" in
  apt)
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -y
    # Use distro docker to keep it simple and reproducible
    apt-get install -y ca-certificates curl gnupg lsb-release jq python3 python3-pip netcat-openbsd
    # Docker (Official Repo)
    install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
    chmod a+r /etc/apt/keyrings/docker.gpg
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null
    apt-get update
    apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
    ;;
  dnf)
    dnf -y update -x kernel* || true
    dnf install -y jq python3 python3-pip curl coreutils nmap-ncat
    dnf install -y docker docker-compose-plugin || true
    ;;
  yum)
    yum install -y jq python3 python3-pip curl nmap-ncat
    # Amazon Linux 2 path
    amazon-linux-extras enable docker || true
    yum install -y docker
    # compose v2 via plugin if available
    yum install -y docker-compose-plugin || true
    ;;
esac

log "Enabling and starting Docker"
systemctl enable docker || true
systemctl start docker

# Allow the default user to use docker without sudo
DEFAULT_USER=${SUDO_USER:-$(logname 2>/dev/null || echo ec2-user)}
if id "$DEFAULT_USER" >/dev/null 2>&1; then
  usermod -aG docker "$DEFAULT_USER" || true
fi

# Provide docker-compose shim if only 'docker compose' exists
if ! command -v docker-compose >/dev/null 2>&1; then
  if command -v docker >/dev/null 2>&1; then
    log "Creating docker-compose shim to 'docker compose'"
    cat >/usr/local/bin/docker-compose <<'EOF'
#!/usr/bin/env bash
exec docker compose "$@"
EOF
    chmod +x /usr/local/bin/docker-compose
  fi
fi

log "Validating Docker engine"
timeout 30s bash -c 'docker version >/dev/null'

log "Setting up buildx builder"
if ! docker buildx inspect >/dev/null 2>&1; then
  docker buildx create --use --name bifrost-builder >/dev/null 2>&1 || true
fi

log "Pre-pulling benchmark images"
timeout 300s docker pull --platform linux/amd64 memcached:1.6-alpine || true
timeout 300s docker pull --platform linux/amd64 studiosol/mcrouter:latest || true
timeout 300s docker pull --platform linux/amd64 redislabs/memtier_benchmark:latest || true

log "Bootstrap complete. You may need to re-login for docker group changes to take effect."

