#!/usr/bin/env bash
set -euo pipefail

UNSIGNED_APK="${1:-}"
if [[ -z "$UNSIGNED_APK" || ! -f "$UNSIGNED_APK" ]]; then
  echo "用法: $0 <unsigned.apk>"
  exit 1
fi

BUILD_TOOLS="${ANDROID_BUILD_TOOLS:-$ANDROID_HOME/build-tools/36.0.0}"
KEYSTORE="${ANDROID_DEBUG_KEYSTORE:-$HOME/.android/debug.keystore}"

ALIGNED_APK="${UNSIGNED_APK%-unsigned.apk}-aligned.apk"
SIGNED_APK="${UNSIGNED_APK%-unsigned.apk}-debug-signed.apk"

"$BUILD_TOOLS/zipalign" -v -p 4 "$UNSIGNED_APK" "$ALIGNED_APK"
"$BUILD_TOOLS/apksigner" sign \
  --ks "$KEYSTORE" \
  --ks-key-alias androiddebugkey \
  --ks-pass pass:android \
  --key-pass pass:android \
  --out "$SIGNED_APK" \
  "$ALIGNED_APK"

"$BUILD_TOOLS/apksigner" verify --verbose --print-certs "$SIGNED_APK"
echo "Signed: $SIGNED_APK"
