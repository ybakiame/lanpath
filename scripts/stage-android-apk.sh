#!/usr/bin/env bash
set -euo pipefail
variant=debug
if [[ "${RELEASE_BUILD:-false}" == true ]]; then variant=release; fi
suffix=-debug
if [[ "$variant" == release ]]; then suffix=; fi
mapfile -t apks < <(find apps/client/src-tauri/gen/android/app/build/outputs/apk -type f -name "*.apk" -path "*/$variant/*")
if [[ ${#apks[@]} != 1 ]]; then
  echo "Expected exactly one $variant arm64 APK, got ${#apks[@]}" >&2
  exit 1
fi
"$ANDROID_BUILD_TOOLS/apksigner" verify --verbose --print-certs "${apks[0]}"
version=$(bun -e 'console.log(require("./apps/client/package.json").version)')
mkdir -p dist
cp "${apks[0]}" "dist/Lanpath_${version}_android_arm64${suffix}.apk"
