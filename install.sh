#!/bin/sh
# Install or upgrade `ta` from this repository's public GitHub Releases. It
# downloads with the GitHub CLI when `gh` is installed and signed in, and with
# curl otherwise.
#
#   gh api repos/renderedstep/terminal/contents/install.sh \
#     -H 'Accept: application/vnd.github.raw' | sh
#
# or, without gh:
#
#   curl -fsSL https://raw.githubusercontent.com/renderedstep/terminal/main/install.sh | sh
#
# Environment:
#   TA_VERSION      a release tag such as v0.1.0 (default: the latest release)
#   TA_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   TA_RELEASE_REPO owner/name to install from (default: renderedstep/terminal)
set -eu

repo="${TA_RELEASE_REPO:-renderedstep/terminal}"
install_dir="${TA_INSTALL_DIR:-$HOME/.local/bin}"
version="${TA_VERSION:-}"

say() { printf '%s\n' "$*"; }
fail() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  use_gh=1
else
  use_gh=0
  command -v curl >/dev/null 2>&1 ||
    fail "need either the GitHub CLI (gh) signed in, or curl, to download the release"
fi
command -v tar >/dev/null 2>&1 || fail "tar is required"

case "$(uname -s)" in
  Linux) os=unknown-linux-gnu ;;
  Darwin) os=apple-darwin ;;
  *) fail "unsupported system $(uname -s); on Windows use install.ps1" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) fail "unsupported machine $(uname -m)" ;;
esac
target="$arch-$os"
asset="ta-$target.tar.gz"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

if [ -z "$version" ]; then
  if [ "$use_gh" = 1 ]; then
    version="$(gh release view --repo "$repo" --json tagName --jq .tagName)"
  else
    # /releases/latest redirects to /releases/tag/<tag>.
    version="$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
      "https://github.com/$repo/releases/latest")" || version=
    case "$version" in
      */releases/tag/*) version="${version##*/}" ;;
      *) version= ;;
    esac
  fi
  [ -n "$version" ] || fail "could not find a release in $repo"
fi
say "Downloading ta $version for $target"
if [ "$use_gh" = 1 ]; then
  gh release download "$version" --repo "$repo" --dir "$tmp" \
    --pattern "$asset" --pattern SHA256SUMS
else
  base="https://github.com/$repo/releases/download/$version"
  curl -fsSL -o "$tmp/$asset" "$base/$asset" &&
    curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"
fi || fail "could not download $asset from $repo $version"

expected="$(awk -v a="$asset" '{ n=$2; sub(/^\*/, "", n) } n == a { print $1 }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || fail "SHA256SUMS does not list $asset"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$asset" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')"
else
  fail "need sha256sum or shasum to verify the download"
fi
[ "$actual" = "$expected" ] || fail "checksum mismatch for $asset; nothing was installed"

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$install_dir"
# Replace by rename so a running `ta` keeps its old file.
cp "$tmp/ta" "$install_dir/.ta.new"
chmod 755 "$install_dir/.ta.new"
mv -f "$install_dir/.ta.new" "$install_dir/ta"
say "Installed ta $version to $install_dir/ta"

case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) say "Add $install_dir to your PATH, for example in ~/.profile:"
     say "  export PATH=\"$install_dir:\$PATH\"" ;;
esac
