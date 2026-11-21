#!/bin/sh
# Install a prebuilt Pitwall release for macOS. No sudo, source build, or sign-in.
set -eu
umask 077
repo=thearyanag/pitwall
fail() { printf 'Pitwall: %s\n' "$*" >&2; exit 1; }
[ "$(uname -s)" = Darwin ] || fail 'This release supports macOS only.'
major=$(sw_vers -productVersion | cut -d. -f1)
[ "$major" -ge 13 ] || fail 'macOS 13 or newer is required.'
case "$(uname -m)" in
  arm64|aarch64) arch=arm64 ;;
  x86_64) arch=x64
    if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = 1 ]; then arch=arm64; fi ;;
  *) fail 'Unsupported Mac architecture.' ;;
esac
for tool in curl tar shasum awk mktemp; do command -v "$tool" >/dev/null || fail "Missing required tool: $tool"; done
fetch() { curl --proto '=https' --tlsv1.2 -fLsS --retry 3 --connect-timeout 15 --max-time 600 "$@"; }
version=${PITWALL_VERSION:-}
if [ -z "$version" ]; then
  latest=$(fetch -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")
  case "$latest" in "https://github.com/$repo/releases/tag/"*) version=${latest##*/} ;; *) fail 'Could not resolve the latest release.' ;; esac
fi
case "$version" in v[0-9]*) ;; *) fail 'PITWALL_VERSION must be a release tag such as v0.2.0.' ;; esac
case "$version" in *[!A-Za-z0-9._-]*) fail 'Invalid release tag.' ;; esac
install_dir=${PITWALL_INSTALL_DIR:-$HOME/.local/share/pitwall}
bin_dir=${PITWALL_BIN_DIR:-$HOME/.local/bin}
case "$install_dir:$bin_dir" in *'
'*) fail 'Installation paths cannot contain newlines.' ;; esac
case "$install_dir" in /*) ;; *) fail 'PITWALL_INSTALL_DIR must be absolute.' ;; esac
case "$bin_dir" in /*) ;; *) fail 'PITWALL_BIN_DIR must be absolute.' ;; esac
if [ -e "$bin_dir/pitwall" ] || [ -L "$bin_dir/pitwall" ]; then
  [ -L "$bin_dir/pitwall" ] && [ "$(readlink "$bin_dir/pitwall")" = "$install_dir/pitwall" ] || fail "Refusing to replace an unrelated $bin_dir/pitwall. Choose PITWALL_BIN_DIR."
fi
if [ -d "$install_dir" ] && [ -n "$(ls -A "$install_dir")" ] && [ ! -f "$install_dir/.pitwall-install" ]; then
  fail "Refusing to use a nonempty unmanaged directory: $install_dir"
fi
if [ -e "$install_dir/current" ] && [ ! -L "$install_dir/current" ]; then fail 'The current release path must be a symlink.'; fi
mkdir -p "$install_dir/versions" "$install_dir/state" "$bin_dir"
printf '%s\n' 'pitwall-installer-v1' > "$install_dir/.pitwall-install"
staging=$(mktemp -d "$install_dir/.install.XXXXXX")
next_link=$install_dir/.current.$$
cleanup() { rm -rf "$staging"; rm -f "$next_link"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
asset=pitwall-darwin-$arch.tar.gz
base=https://github.com/$repo/releases/download/$version
printf 'Downloading Pitwall %s for macOS %s…\n' "$version" "$arch"
fetch "$base/SHA256SUMS" -o "$staging/SHA256SUMS"
fetch "$base/$asset" -o "$staging/$asset"
expected=$(awk -v file="$asset" '$2 == file {print $1}' "$staging/SHA256SUMS")
[ "${#expected}" -eq 64 ] || fail 'Release checksum is missing or ambiguous.'
actual=$(shasum -a 256 "$staging/$asset" | awk '{print $1}')
[ "$actual" = "$expected" ] || fail 'Checksum mismatch. The current installation was not replaced.'
tar -tzf "$staging/$asset" > "$staging/entries"
while IFS= read -r entry; do
  case "$entry" in pitwall|pitwall/|pitwall/*) ;; *) fail 'Unexpected release archive layout.' ;; esac
  case "/$entry/" in */../*|*/./*) fail 'Unsafe path in release archive.' ;; esac
done < "$staging/entries"
mkdir "$staging/unpacked"
tar -xzf "$staging/$asset" -C "$staging/unpacked"
release=$staging/unpacked/pitwall
[ "$(cat "$release/VERSION")" = "$version" ] || fail 'Release version does not match the requested tag.'
for file in sprout runtimes/node/bin/node scripts/pitwall-launcher.sh; do [ -x "$release/$file" ] || fail "Release is missing $file"; done
for file in runtime.sh board-runtime/adapter.mjs worker/listener.py assets/brand/pitwall-mark-on-dark.png; do [ -f "$release/$file" ] || fail "Release is missing $file"; done
[ "$("$release/sprout" --version)" = "Pitwall ${version#v}" ] || fail 'The downloaded binary could not run or has the wrong version.'
"$release/runtimes/node/bin/node" --version >/dev/null || fail 'The bundled Node runtime could not run.'
destination=$install_dir/versions/$version-darwin-$arch
if [ -e "$destination" ]; then
  [ -f "$destination/.archive-sha256" ] && [ "$(cat "$destination/.archive-sha256")" = "$actual" ] || fail 'An existing version directory has a different checksum; it was left untouched.'
else
  printf '%s\n' "$actual" > "$release/.archive-sha256"
  mv "$release" "$destination"
fi
if [ ! -f "$install_dir/config.env" ]; then
  printf '%s\n' '# Pitwall settings. This trusted shell file is kept across updates.' '# export SPROUT_TEAM="your-team"' '# export SPROUT_BOARD="General"' '# export SPROUT_PYTHON="/absolute/path/to/python3"' '# export SPROUT_CODEX="/absolute/path/to/codex"' > "$install_dir/config.env"
fi
cp "$destination/scripts/pitwall-launcher.sh" "$install_dir/.launcher.$$"
chmod 755 "$install_dir/.launcher.$$"
mv -f "$install_dir/.launcher.$$" "$install_dir/pitwall"
ln -s "versions/$version-darwin-$arch" "$next_link"
# BSD mv -h replaces the symlink, rather than following it into the old release.
mv -fh "$next_link" "$install_dir/current"
if [ ! -L "$bin_dir/pitwall" ]; then ln -s "$install_dir/pitwall" "$bin_dir/pitwall"; fi
printf '\nInstalled %s\nTry it: %s/pitwall --demo\n' "$version" "$bin_dir"
case ":$PATH:" in *":$bin_dir:"*) ;; *) printf 'For the short command, add %s to PATH in your shell configuration.\n' "$bin_dir" ;; esac
printf 'Connect: %s/pitwall login\nThen:    %s/pitwall --team YOUR_TEAM\n' "$bin_dir" "$bin_dir"
printf 'Restart existing Pitwall windows and workers to use this version. Settings and drafts are preserved.\n'
