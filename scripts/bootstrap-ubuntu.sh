#!/usr/bin/env bash
set -euo pipefail

yes=0
install_codex=1
install_docker=1
build_agent_image=1
run_doctor=1
librarian_home="${LIBRARIAN_HOME:-$HOME/Librarian}"
bind="${LIBRARIAN_ADMIN_BIND:-0.0.0.0:17377}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes)
      yes=1
      shift
      ;;
    --home)
      librarian_home="${2:?--home requires a value}"
      shift 2
      ;;
    --bind)
      bind="${2:?--bind requires a value}"
      shift 2
      ;;
    --skip-codex)
      install_codex=0
      shift
      ;;
    --skip-docker)
      install_docker=0
      shift
      ;;
    --skip-agent-image)
      build_agent_image=0
      shift
      ;;
    --skip-doctor)
      run_doctor=0
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This bootstrap is for Ubuntu/Linux hosts." >&2
  exit 1
fi

if ! command -v apt-get >/dev/null 2>&1; then
  echo "This bootstrap currently supports Ubuntu/Debian systems with apt-get." >&2
  exit 1
fi

if [[ "$yes" -ne 1 ]]; then
  echo "This will install system packages, Rust, Node.js/npm, Codex CLI, and Docker if missing."
  echo "Librarian root: $librarian_home"
  read -r -p "Continue? [y/N] " answer
  case "$answer" in
    y|Y|yes|YES) ;;
    *) exit 0 ;;
  esac
fi

if [[ "${EUID}" -eq 0 ]]; then
  sudo_cmd=()
else
  if ! command -v sudo >/dev/null 2>&1; then
    echo "sudo is required for package installation." >&2
    exit 1
  fi
  sudo_cmd=(sudo)
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export DEBIAN_FRONTEND=noninteractive

node_major() {
  if command -v node >/dev/null 2>&1; then
    node -p "process.versions.node.split('.')[0]" 2>/dev/null || echo 0
  else
    echo 0
  fi
}

run_root_bash() {
  if [[ "${EUID}" -eq 0 ]]; then
    bash "$@"
  else
    sudo -E bash "$@"
  fi
}

"${sudo_cmd[@]}" apt-get update
"${sudo_cmd[@]}" apt-get install -y \
  ca-certificates \
  curl \
  build-essential \
  pkg-config \
  libssl-dev \
  python3

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y
fi

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

if [[ "$install_codex" -eq 1 ]]; then
  if [[ "$(node_major)" -lt 22 ]] || ! command -v npm >/dev/null 2>&1; then
    curl -fsSL https://deb.nodesource.com/setup_22.x | run_root_bash -
    "${sudo_cmd[@]}" apt-get install -y nodejs
  fi
  if ! command -v codex >/dev/null 2>&1; then
    "${sudo_cmd[@]}" npm install -g @openai/codex
  fi
fi

if [[ "$install_docker" -eq 1 ]]; then
  if ! command -v docker >/dev/null 2>&1; then
    "${sudo_cmd[@]}" apt-get install -y docker.io
  fi
  if command -v systemctl >/dev/null 2>&1; then
    "${sudo_cmd[@]}" systemctl enable --now docker || true
  else
    "${sudo_cmd[@]}" service docker start || true
  fi
  if [[ "${EUID}" -ne 0 ]]; then
    "${sudo_cmd[@]}" usermod -aG docker "$USER" || true
  fi
fi

cd "$repo_root"
cargo build --release

bin="$repo_root/target/release/librarian"
"$bin" --home "$librarian_home" setup --yes --runtime host --skip-doctor
"$bin" --home "$librarian_home" config show >/dev/null

if [[ "$bind" != "127.0.0.1:17377" ]]; then
  "$bin" --home "$librarian_home" config show >/dev/null
  python3 - "$librarian_home/config.toml" "$bind" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
bind = sys.argv[2]
text = path.read_text()
lines = text.splitlines()
for index, line in enumerate(lines):
    if line.startswith("bind = ") and index > 0 and lines[index - 1].strip() == "[admin]":
        lines[index] = f'bind = "{bind}"'
        break
else:
    lines.append("[admin]")
    lines.append(f'bind = "{bind}"')
path.write_text("\n".join(lines) + "\n")
PY
fi

if [[ "$build_agent_image" -eq 1 ]]; then
  if docker info >/dev/null 2>&1; then
    "$bin" --home "$librarian_home" runtime build-agent-image
  elif command -v sg >/dev/null 2>&1 && getent group docker >/dev/null 2>&1; then
    sg docker -c "\"$bin\" --home \"$librarian_home\" runtime build-agent-image" || {
      echo "Could not build the agent image in the current session. Log out/in or run: $bin --home $librarian_home runtime build-agent-image" >&2
    }
  else
    echo "Docker is installed but not reachable in this session. Run: $bin --home $librarian_home runtime build-agent-image" >&2
  fi
fi

if [[ "$run_doctor" -eq 1 ]]; then
  "$bin" --home "$librarian_home" doctor || true
fi

cat <<EOF

Librarian bootstrap complete.

Root:
  $librarian_home

Start the admin UI:
  $bin --home "$librarian_home" admin --bind "$bind"

Open from Windows/host:
  http://127.0.0.1:17377

Codex auth, if doctor reports a missing profile:
  export CODEX_HOME="$librarian_home/codex-home"
  codex
  $bin --home "$librarian_home" auth codex --enable-container-mount --codex-home "$librarian_home/codex-home"
EOF
