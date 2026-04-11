#!/bin/sh
set -eu

REPO="ichi0g0y/ghx"
BINARY="ghx"

main() {
  os=$(uname -s)
  arch=$(uname -m)

  case "$os" in
    Darwin) os_target="apple-darwin" ;;
    Linux)  os_target="unknown-linux-gnu" ;;
    *)
      echo "error: unsupported OS: $os" >&2
      echo "  Windows: https://github.com/$REPO/releases" >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64)   arch_target="x86_64" ;;
    arm64|aarch64)   arch_target="aarch64" ;;
    *)
      echo "error: unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  target="${arch_target}-${os_target}"

  tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')

  if [ -z "$tag" ]; then
    echo "error: failed to fetch latest release" >&2
    exit 1
  fi

  archive="${BINARY}-${tag}-${target}.tar.gz"
  url="https://github.com/$REPO/releases/download/${tag}/${archive}"

  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  echo "Downloading $BINARY $tag ($target)..."
  curl -fsSL "$url" -o "$tmpdir/$archive"
  tar xzf "$tmpdir/$archive" -C "$tmpdir"

  install_dir="/usr/local/bin"
  if [ ! -w "$install_dir" ] 2>/dev/null; then
    install_dir="$HOME/.local/bin"
    mkdir -p "$install_dir"
  fi

  mv "$tmpdir/$BINARY" "$install_dir/$BINARY"
  chmod +x "$install_dir/$BINARY"

  echo "$BINARY $tag installed to $install_dir/$BINARY"

  if ! echo "$PATH" | tr ':' '\n' | grep -qx "$install_dir"; then
    echo ""
    echo "NOTE: $install_dir is not in your PATH."
    echo "  Add it: export PATH=\"$install_dir:\$PATH\""
  fi
}

main
