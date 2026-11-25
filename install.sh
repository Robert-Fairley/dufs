#!/bin/sh
set -eu

usage() {
  printf 'Usage: %s [install-directory [filename]]\n' "$0"
  printf 'Install the local release binary (default: /usr/local/bin/dufs).\n'
}

parse_version() {
  printf '%s\n' "$1" | awk '
    NR == 1 && NF == 2 && $2 ~ /^[0-9]+[.][0-9]+[.][0-9]+([-+][0-9A-Za-z.+-]+)?$/ { value = $2 }
    END { if (NR == 1) print value }
  '
}

if [ "$#" -gt 2 ]; then
  usage >&2
  exit 1
fi

case "${1-}" in
  -h|--help)
    usage
    exit 0
    ;;
  -*)
    usage >&2
    exit 1
    ;;
esac

if [ "$#" -eq 1 ] && [ -z "$1" ]; then
  printf 'Install directory cannot be empty.\n' >&2
  exit 1
fi

directory=${1:-/usr/local/bin}
filename=${2:-dufs}
if [ "$#" -eq 2 ] && [ -z "$2" ]; then
  printf 'Install filename cannot be empty.\n' >&2
  exit 1
fi
case "$filename" in
  .|..|*/*) printf 'Install filename must be a filename, not a path.\n' >&2; exit 1 ;;
esac
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
binary="$project_dir/target/release/dufs"
destination="$directory/$filename"

if [ ! -f "$binary" ]; then
  while :; do
    printf 'No local build found. Bump the patch version and build a release binary? [y/n] '
    if ! IFS= read -r answer; then
      printf '\nInstallation cancelled.\n' >&2
      exit 1
    fi
    case "$answer" in
      y|Y|yes|YES)
        sh "$project_dir/release.sh"
        if [ ! -f "$binary" ]; then
          printf 'Build completed, but %s was not found.\n' "$binary" >&2
          exit 1
        fi
        break
        ;;
      n|N|no|NO)
        printf 'Installation cancelled.\n'
        exit 1
        ;;
      *)
        printf 'Please answer y or n.\n' >&2
        ;;
    esac
  done
fi

if ! version_output=$("$binary" --version 2>&1); then
  printf 'Cannot determine local release version:\n%s\n' "$version_output" >&2
  exit 1
fi
version=$(parse_version "$version_output")
if [ -z "$version" ]; then
  printf 'Unrecognized local release version: %s\n' "$version_output" >&2
  exit 1
fi

if [ -L "$destination" ] || { [ -e "$destination" ] && [ ! -f "$destination" ]; }; then
  printf 'Destination is not a regular file: %s\n' "$destination" >&2
  exit 1
fi
if [ -e "$destination" ]; then
  existing_version=
  if existing_output=$("$destination" --version 2>&1); then
    existing_version=$(parse_version "$existing_output")
  fi
  if [ "$existing_version" = "$version" ]; then
    printf 'Destination already has version %s; refusing to overwrite %s.\n' "$version" "$destination" >&2
    exit 1
  fi
  if [ -n "$existing_version" ]; then
    printf 'Destination has version %s; local release is %s.\n' "$existing_version" "$version"
  else
    printf 'Cannot determine the version of %s; it may be the same version.\n' "$destination" >&2
  fi
  printf 'Replace %s? [y/N] ' "$destination"
  if ! IFS= read -r answer; then
    printf '\nInstallation cancelled.\n' >&2
    exit 1
  fi
  case "$answer" in
    y|Y|yes|YES) ;;
    *) printf 'Installation cancelled.\n'; exit 1 ;;
  esac
fi

mkdir -p -- "$directory"
install -m 755 -- "$binary" "$destination"
printf 'Installed version %s to %s\n' "$version" "$destination"
