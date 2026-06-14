#!/usr/bin/env bash
set -euo pipefail

yes=0
install_codex=1
install_docker=1
build_agent_image=1
run_doctor=1
agent_image_ready=0
librarian_home="${LIBRARIAN_HOME:-$HOME/Librarian}"
install_bin=""
bind="${LIBRARIAN_ADMIN_BIND:-127.0.0.1:17377}"

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
    --install-bin)
      install_bin="${2:?--install-bin requires a value}"
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
  python3 \
  util-linux-extra

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y
fi

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

cargo_bin="$(command -v cargo || true)"
if [[ -z "$cargo_bin" && -x "$HOME/.cargo/bin/cargo" ]]; then
  cargo_bin="$HOME/.cargo/bin/cargo"
fi
if [[ -z "$cargo_bin" ]]; then
  echo "cargo was installed but is not available in this shell. Run: source ~/.cargo/env" >&2
  exit 1
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
"$cargo_bin" build --release

if [[ -z "$install_bin" ]]; then
  install_bin="$librarian_home/.app/bin/librarian"
fi
mkdir -p "$(dirname "$install_bin")"
install_tmp="$(dirname "$install_bin")/.librarian.new.$$"
cp "$repo_root/target/release/librarian" "$install_tmp"
chmod +x "$install_tmp"
mv -f "$install_tmp" "$install_bin"

bin="$install_bin"
version_file="$librarian_home/.app/version.json"
installed_version="$("$bin" --version 2>/dev/null | awk '{print $2}')"
git_ref="${LIBRARIAN_INSTALL_REF:-$(git -C "$repo_root" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)}"
git_commit="$(git -C "$repo_root" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
installed_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
python3 - "$version_file" "$installed_version" "$git_ref" "$git_commit" "$installed_at" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps({
    "version": sys.argv[2] or "unknown",
    "git_ref": sys.argv[3] or "unknown",
    "git_commit": sys.argv[4] or "unknown",
    "installed_at": sys.argv[5],
}, indent=2) + "\n")
PY
link_bin="${LIBRARIAN_LINK_BIN:-$HOME/.local/bin/librarian}"
system_link_bin="${LIBRARIAN_SYSTEM_LINK_BIN:-/usr/local/bin/librarian}"
mkdir -p "$(dirname "$link_bin")"
ln -sfn "$bin" "$link_bin"
if [[ -n "$system_link_bin" ]]; then
  if [[ "${EUID}" -eq 0 ]]; then
    ln -sfn "$bin" "$system_link_bin" || true
  else
    "${sudo_cmd[@]}" ln -sfn "$bin" "$system_link_bin" || true
  fi
fi
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
  export PATH="$HOME/.local/bin:$PATH"
fi
if [[ -f "$HOME/.profile" ]] && ! grep -q 'HOME/.local/bin' "$HOME/.profile"; then
  cat >> "$HOME/.profile" <<'PROFILE'

# Added by Librarian installer: user-local command shims.
if [ -d "$HOME/.local/bin" ] ; then
    PATH="$HOME/.local/bin:$PATH"
fi
PROFILE
fi
if [[ -f "$HOME/.bashrc" ]] && ! grep -q 'HOME/.local/bin' "$HOME/.bashrc"; then
  cat >> "$HOME/.bashrc" <<'BASHRC'

# Added by Librarian installer: user-local command shims.
if [ -d "$HOME/.local/bin" ] ; then
    PATH="$HOME/.local/bin:$PATH"
fi
BASHRC
fi
"$bin" --home "$librarian_home" setup --yes --runtime host --skip-doctor
"$bin" --home "$librarian_home" config show >/dev/null

python3 - "$librarian_home/.cfg/config.toml" "$bind" <<'PY'
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

if [[ "$build_agent_image" -eq 1 ]]; then
  if docker info >/dev/null 2>&1; then
    "$bin" --home "$librarian_home" runtime build-agent-image
    agent_image_ready=1
  elif command -v sg >/dev/null 2>&1 && getent group docker >/dev/null 2>&1; then
    if sg docker -c "\"$bin\" --home \"$librarian_home\" runtime build-agent-image"; then
      agent_image_ready=1
    fi
  else
    agent_image_ready=0
  fi
fi

docker_ready=0
if docker info >/dev/null 2>&1; then
  docker_ready=1
fi

codex_profile_ready=0
if [[ -d "$librarian_home/codex-home" ]]; then
  codex_profile_ready=1
fi
if [[ -d "$librarian_home/.cfg/codex-home" ]]; then
  codex_profile_ready=1
fi

if [[ "$agent_image_ready" -eq 0 && "$docker_ready" -eq 1 ]]; then
  if docker image inspect librarian-agent:latest >/dev/null 2>&1; then
    agent_image_ready=1
  fi
fi

if [[ "$run_doctor" -eq 1 ]]; then
  "$bin" --home "$librarian_home" doctor || true
fi

if [[ "$docker_ready" -eq 0 ]]; then
  next_title="Activate Docker access"
  next_body="Open a new Ubuntu shell, or run the image build through a fresh docker group session."
  next_command="cd \"$repo_root\" && sg docker -c '\"$bin\" --home \"$librarian_home\" runtime build-agent-image'"
elif [[ "$agent_image_ready" -eq 0 ]]; then
  next_title="Build the agent image"
  next_body="Docker is reachable, but the Librarian agent image is not ready yet."
  next_command="\"$bin\" --home \"$librarian_home\" runtime build-agent-image"
elif [[ "$codex_profile_ready" -eq 0 ]]; then
  next_title="Sign in Codex for Librarian"
  next_body="Create the portable Codex profile that will later be mounted into agent containers."
  next_command="export CODEX_HOME=\"$librarian_home/.cfg/codex-home\" && codex"
elif ! grep -q "mount_host_home = true" "$librarian_home/.cfg/config.toml" 2>/dev/null; then
  next_title="Enable Codex container access"
  next_body="Codex is signed in, but agent containers cannot read that profile until you explicitly allow the mount."
  next_command="$bin --home \"$librarian_home\" auth codex --enable-container-mount --codex-home \"$librarian_home/.cfg/codex-home\""
else
  next_title="Start the admin UI"
  next_body="The basic setup is ready enough to open the web interface."
  next_command="\"$bin\" --home \"$librarian_home\" admin --bind \"$bind\""
fi

cat <<EOF

+------------------------------------------------------------+
| Librarian bootstrap complete                               |
+------------------------------------------------------------+

State root:
  $librarian_home

Installed binary:
  $bin

Shell command:
  $system_link_bin
  $link_bin

NEXT STEP: $next_title
  $next_body
  $next_command

Useful commands:
  Doctor:
    $bin --home "$librarian_home" doctor

  Admin UI:
    $bin --home "$librarian_home" admin --bind "$bind"
    librarian --home "$librarian_home" admin --bind "$bind"

  Browser URL from Windows/host:
    http://127.0.0.1:17377

  Codex auth:
    CODEX_HOME="$librarian_home/.cfg/codex-home" codex
    $bin --home "$librarian_home" auth codex --enable-container-mount --codex-home "$librarian_home/.cfg/codex-home"

Notes:
  $librarian_home is the only install folder.
  .app stores the installed binary, temporary source checkout, and runtime artifacts.
  .cfg stores config and auth profiles.
  .mdb stores SQLite databases and machine data.
  Library and Projects are user-visible project memory/work folders.
EOF
