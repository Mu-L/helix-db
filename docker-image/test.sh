#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: docker-image/test.sh --platform PLATFORM --image IMAGE\n' >&2
}

platform=""
image=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform)
      platform="${2:-}"
      shift 2
      ;;
    --image)
      image="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage
      exit 2
      ;;
  esac
done

case "$platform" in
  linux/amd64|linux/arm64) ;;
  *)
    usage
    exit 2
    ;;
esac
if [[ -z "$image" ]]; then
  usage
  exit 2
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
temp_dir=$(mktemp -d)
trap 'rm -rf "$temp_dir"' EXIT
archive="$temp_dir/helixdb-image.tar"

python3 -m unittest discover -s "$script_dir/tests" -p 'test_*.py'
docker image inspect "$image" >/dev/null
docker image save --output "$archive" "$image"
python3 "$script_dir/image_archive.py" inspect "$archive" --expected-image "$image"
python3 "$script_dir/image_archive.py" scan "$archive"
"$script_dir/tests/smoke.sh" --platform "$platform" --image "$image"
"$script_dir/tests/compose-smoke.sh" --platform "$platform" --image "$image"
