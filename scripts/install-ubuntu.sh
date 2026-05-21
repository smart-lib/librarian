#!/usr/bin/env bash
set -euo pipefail

repo_url="${LIBRARIAN_REPO_URL:-https://github.com/smart-lib/librarian.git}"
ref="${LIBRARIAN_REF:-main}"
install_dir="${LIBRARIAN_DIR:-$HOME/librarian}"
bootstrap_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ref)
      ref="${2:?--ref requires a value}"
      shift 2
      ;;
    --nightly)
      ref="${LIBRARIAN_NIGHTLY_REF:-develop}"
      shift
      ;;
    --dir)
      install_dir="${2:?--dir requires a value}"
      shift 2
      ;;
    --repo)
      repo_url="${2:?--repo requires a value}"
      shift 2
      ;;
    *)
      bootstrap_args+=("$1")
      shift
      ;;
  esac
done

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This starter is for Ubuntu/Linux hosts." >&2
  exit 1
fi

if ! command -v apt-get >/dev/null 2>&1; then
  echo "This starter currently supports Ubuntu/Debian systems with apt-get." >&2
  exit 1
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

export DEBIAN_FRONTEND=noninteractive
"${sudo_cmd[@]}" apt-get update
"${sudo_cmd[@]}" apt-get install -y ca-certificates curl git

if [[ -d "$install_dir/.git" ]]; then
  git -C "$install_dir" fetch --tags origin
else
  mkdir -p "$(dirname "$install_dir")"
  git clone "$repo_url" "$install_dir"
fi

git -C "$install_dir" checkout "$ref"
git -C "$install_dir" pull --ff-only origin "$ref" || true

exec "$install_dir/scripts/bootstrap-ubuntu.sh" --yes "${bootstrap_args[@]}"
