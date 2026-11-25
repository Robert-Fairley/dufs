#!/bin/sh
set -eu
set -f

usage() {
  printf 'Usage: %s [--bump patch|minor|major | --version MAJOR.MINOR.PATCH]\n' "$0"
}

bump=patch
override=
case "${1-}" in
  "")
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  --bump)
    [ "$#" -eq 2 ] || { usage >&2; exit 1; }
    bump=$2
    case "$bump" in patch|minor|major) ;; *) usage >&2; exit 1 ;; esac
    ;;
  --version)
    [ "$#" -eq 2 ] || { usage >&2; exit 1; }
    override=$2
    [ -n "$override" ] || { usage >&2; exit 1; }
    ;;
  *)
    usage >&2
    exit 1
    ;;
esac

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
manifest="$project_dir/Cargo.toml"
current=$(awk '
  /^\[package\][[:space:]]*$/ { package = 1; next }
  /^\[/ { package = 0 }
  package && /^version[[:space:]]*=/ {
    if ($0 !~ /^version[[:space:]]*=[[:space:]]*"[^"]+"[[:space:]]*$/) exit 1
    split($0, parts, "\"")
    print parts[2]
    count++
  }
  END { if (count != 1) exit 1 }
' "$manifest") || {
  printf 'Unable to read the package version from %s.\n' "$manifest" >&2
  exit 1
}

valid_version() {
  version=$1
  old_ifs=$IFS
  IFS=.
  set -- $version
  IFS=$old_ifs
  [ "$#" -eq 3 ] || return 1
  [ "$version" = "$1.$2.$3" ] || return 1
  for number do
    case "$number" in ''|*[!0-9]*|0[0-9]*) return 1 ;; esac
    [ "${#number}" -le 9 ] || return 1
  done
}

if ! valid_version "$current"; then
  printf 'Current version must be MAJOR.MINOR.PATCH with numeric components: %s\n' "$current" >&2
  exit 1
fi

old_ifs=$IFS
IFS=.
set -- $current
IFS=$old_ifs
major=$1 minor=$2 patch=$3

if [ -n "$override" ]; then
  if ! valid_version "$override"; then
    printf 'Version must be MAJOR.MINOR.PATCH with numeric components: %s\n' "$override" >&2
    exit 1
  fi
  old_ifs=$IFS
  IFS=.
  set -- $override
  IFS=$old_ifs
  if [ "$1" -lt "$major" ] ||
     { [ "$1" -eq "$major" ] && [ "$2" -lt "$minor" ]; } ||
     { [ "$1" -eq "$major" ] && [ "$2" -eq "$minor" ] && [ "$3" -le "$patch" ]; }; then
    printf 'Version %s must be greater than current version %s.\n' "$override" "$current" >&2
    exit 1
  fi
  next=$override
else
  case "$bump" in
    patch) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    major) major=$((major + 1)); minor=0; patch=0 ;;
  esac
  next="$major.$minor.$patch"
  if ! valid_version "$next"; then
    printf 'Version component is too large: %s\n' "$next" >&2
    exit 1
  fi
fi

temp=$(mktemp "$project_dir/.cargo-version.XXXXXX")
trap 'rm -f -- "$temp"' 0
trap 'exit 1' HUP INT TERM
cp -p "$manifest" "$temp"
awk -v version="$next" '
  /^\[package\][[:space:]]*$/ { package = 1; print; next }
  /^\[/ { package = 0 }
  package && /^version[[:space:]]*=/ { sub(/"[^"]*"/, "\"" version "\"") }
  { print }
' "$manifest" > "$temp"
mv -- "$temp" "$manifest"

printf 'Building release %s (previous version %s).\n' "$next" "$current"
if ! cargo build --release --manifest-path "$manifest" --target-dir "$project_dir/target"; then
  printf 'Release build failed. The package version was changed to %s; inspect Cargo.toml and Cargo.lock before retrying.\n' "$next" >&2
  exit 1
fi
