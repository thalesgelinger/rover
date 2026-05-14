#!/bin/sh
set -e

repo="thalesgelinger/rover"
base_url="https://github.com/$repo/releases/download"
rover_home="${ROVER_HOME:-$HOME/.rover}"
bin_dir="$rover_home/bin"
no_modify_path="${ROVER_NO_MODIFY_PATH:-}"

say() {
  printf '%s\n' "$1"
}

err() {
  say "error: $1" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || err "$1 is required"
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os_part="apple-darwin" ;;
    Linux) os_part="unknown-linux-gnu" ;;
    *) err "unsupported OS: $os" ;;
  esac

  case "$arch" in
    x86_64|amd64) arch_part="x86_64" ;;
    arm64|aarch64) arch_part="aarch64" ;;
    *) err "unsupported arch: $arch" ;;
  esac

  printf '%s-%s' "$arch_part" "$os_part"
}

latest_version() {
  need curl
  curl -fsSL "https://api.github.com/repos/$repo/releases" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' \
    | head -n 1
}

append_path_sh() {
  profile="$1"
  mkdir -p "$(dirname "$profile")"
  touch "$profile"

  if grep -q 'ROVER_HOME' "$profile" 2>/dev/null; then
    return 0
  fi

  cat >> "$profile" <<EOF

# Rover
export ROVER_HOME="\$HOME/.rover"
export PATH="\$ROVER_HOME/bin:\$PATH"
EOF
  say "Updated PATH in $profile"
}

append_path_fish() {
  profile="$HOME/.config/fish/config.fish"
  mkdir -p "$(dirname "$profile")"
  touch "$profile"

  if grep -q 'ROVER_HOME' "$profile" 2>/dev/null; then
    return 0
  fi

  cat >> "$profile" <<EOF

# Rover
set -gx ROVER_HOME "\$HOME/.rover"
fish_add_path "\$ROVER_HOME/bin"
EOF
  say "Updated PATH in $profile"
}

setup_path() {
  case ":$PATH:" in
    *":$bin_dir:"*) return 0 ;;
  esac

  if [ "$no_modify_path" = "1" ]; then
    say "Add Rover to PATH: export PATH=\"$bin_dir:\$PATH\""
    return 0
  fi

  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name" in
    zsh) append_path_sh "$HOME/.zshrc" ;;
    bash)
      if [ -f "$HOME/.bashrc" ] || [ ! -f "$HOME/.bash_profile" ]; then
        append_path_sh "$HOME/.bashrc"
      else
        append_path_sh "$HOME/.bash_profile"
      fi
      ;;
    fish) append_path_fish ;;
    *)
      say "Add Rover to PATH: export PATH=\"$bin_dir:\$PATH\""
      return 0
      ;;
  esac

  say "Restart your shell or run: export PATH=\"$bin_dir:\$PATH\""
}

verify_checksum() {
  checksum_archive="$1"
  sums="$2"

  expected="$(sed -n "s/^\([a-fA-F0-9]*\) .*$(basename "$checksum_archive")$/\1/p" "$sums" | head -n 1)"
  [ -n "$expected" ] || err "checksum missing for $(basename "$checksum_archive")"

  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$checksum_archive" | cut -d ' ' -f 1)"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$checksum_archive" | cut -d ' ' -f 1)"
  else
    say "No sha256 tool found; skipping checksum verification"
    return 0
  fi

  [ "$actual" = "$expected" ] || err "checksum mismatch"
}

main() {
  need curl
  need tar

  target="$(detect_target)"
  version="${ROVER_VERSION:-$(latest_version)}"
  [ -n "$version" ] || err "could not resolve latest release"

  asset="rover-$version-$target"
  archive="$asset.tar.gz"
  tmp="$(mktemp -d)"

  trap 'rm -rf "$tmp"' EXIT INT TERM

  say "Installing Rover $version for $target"
  curl -fL "$base_url/$version/$archive" -o "$tmp/$archive"
  curl -fL "$base_url/$version/SHA256SUMS" -o "$tmp/SHA256SUMS"
  verify_checksum "$tmp/$archive" "$tmp/SHA256SUMS"

  tar -xzf "$tmp/$archive" -C "$tmp"
  mkdir -p "$bin_dir"
  cp "$tmp/$asset/rover" "$bin_dir/rover"
  if [ -d "$tmp/$asset/runtimes" ]; then
    rm -rf "$bin_dir/runtimes"
    cp -R "$tmp/$asset/runtimes" "$bin_dir/runtimes"
  fi
  case "$target" in
    *apple-darwin)
      [ -f "$bin_dir/runtimes/macos/rover-macos-host" ] || err "macOS runtime missing from archive"
      [ -s "$bin_dir/runtimes/macos/librover_macos.dylib" ] || err "macOS dylib missing from archive"
      [ -s "$bin_dir/runtimes/ios/librover_ios.a" ] || err "iOS runtime missing from archive"
      [ -s "$bin_dir/runtimes/ios/liblua5.4.a" ] || err "iOS Lua runtime missing from archive"
      ;;
  esac
  chmod 755 "$bin_dir/rover"
  if [ -f "$bin_dir/runtimes/macos/rover-macos-host" ]; then
    chmod 755 "$bin_dir/runtimes/macos/rover-macos-host"
  fi

  setup_path
  say "Rover installed: $bin_dir/rover"
  say "Run: rover --help"
}

main "$@"
