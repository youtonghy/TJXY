#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
    printf '%s\n' 'usage: scripts/package-release.sh <version> <target> <asset-suffix>' >&2
    exit 64
fi

version="$1"
target="$2"
asset_suffix="$3"
target_dir="${CARGO_TARGET_DIR:-target}"
bundle="tjxy-${version}-${asset_suffix}"
output_dir="dist/release"
stage_dir="${output_dir}/${bundle}"
binary_dir="${target_dir}/${target}/dist"

case "$version" in
    *[!0-9A-Za-z.v-]* | '')
        printf '%s\n' 'release version contains unsupported characters' >&2
        exit 64
        ;;
esac

for path in "$binary_dir/tjxy-server" "$binary_dir/tjxy-tui" admin/dist/index.html; do
    if [ ! -f "$path" ]; then
        printf 'missing release input: %s\n' "$path" >&2
        exit 1
    fi
done

rm -rf "$stage_dir"
mkdir -p "$stage_dir/admin"
install -m 755 "$binary_dir/tjxy-server" "$stage_dir/tjxy-server"
install -m 755 "$binary_dir/tjxy-tui" "$stage_dir/tjxy"
cp -R admin/dist "$stage_dir/admin/dist"
install -m 644 README.md LICENSE .env.example "$stage_dir"

mkdir -p "$output_dir"
tar -C "$output_dir" -czf "${output_dir}/${bundle}.tar.gz" "$bundle"
printf '%s\n' "${output_dir}/${bundle}.tar.gz"
