#!/usr/bin/env bash
set -euo pipefail

# Build the pinned sing-box/libbox Android arm64 binding inside devenv shell.
# Usage: bash mobile/lanpath-core/build-android.sh [OUTPUT_AAR]
# Requires SagerNet gomobile v0.1.12; upstream gomobile cannot link this core.
# Keep with_gvisor for mixed stacks and existing stack: gvisor configurations.
# The resulting library is GPL-3.0 and must not be committed.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

OUTPUT_AAR="${1:-$REPO_ROOT/mobile/android/vpn/libs/lanpathcore.aar}"

GOPATH="${GOPATH:-$HOME/go-work}"
GOMOBILE_TAG="v0.1.12"
: "${ANDROID_HOME:?请先从仓库根进入 devenv shell（SDK 由项目 devenv 提供）}"
SING_BOX_VERSION="v1.15.0-alpha.5"
SING_BOX_VERSION_NUM="${SING_BOX_VERSION#v}"

GO_BIN="$(command -v go)"
[[ -x "$GO_BIN" ]] || { echo "error: 请先从仓库根进入 devenv shell" >&2; exit 1; }

: "${JAVA_HOME:?请设置 JAVA_HOME（JDK 17 或 21）}"
: "${ANDROID_NDK_HOME:?请设置 ANDROID_NDK_HOME（NDK 路径）}"
[[ -d "$ANDROID_HOME" ]] || { echo "error: ANDROID_HOME=$ANDROID_HOME 不存在" >&2; exit 1; }
[[ -d "$ANDROID_NDK_HOME" ]] || { echo "error: ANDROID_NDK_HOME=$ANDROID_NDK_HOME 不存在" >&2; exit 1; }
command -v javac >/dev/null 2>&1 || { echo "error: javac 未在 PATH（需 JDK 17 或 21）" >&2; exit 1; }

JAVA_MAJOR="$(javac -version 2>&1 | sed -E 's/^javac ([0-9]+).*/\1/')"
case "$JAVA_MAJOR" in
    17|21) ;;
    *)
        echo "error: JDK 版本不受支持（当前 $JAVA_MAJOR），需要 17 或 21" >&2
        exit 1
        ;;
esac

export GOPATH
export ANDROID_HOME ANDROID_NDK_HOME
export PATH="$JAVA_HOME/bin:$PATH"

echo "==> 输出: $OUTPUT_AAR"
echo "==> Go: $("$GO_BIN" version)"
echo "==> JDK: $(javac -version 2>&1)"

GO_VERSION="$("$GO_BIN" env GOVERSION)"
GOMOBILE_BIN_DIR="$REPO_ROOT/.devenv/state/gomobile/$GOMOBILE_TAG/$GO_VERSION/bin"
export PATH="$GOMOBILE_BIN_DIR:$PATH"
if [[ ! -x "$GOMOBILE_BIN_DIR/gomobile" || ! -x "$GOMOBILE_BIN_DIR/gobind" ]]; then
    echo "==> 安装 sagernet/gomobile@$GOMOBILE_TAG（$GO_VERSION）..."
    GOBIN="$GOMOBILE_BIN_DIR" "$GO_BIN" install "github.com/sagernet/gomobile/cmd/gomobile@$GOMOBILE_TAG"
    GOBIN="$GOMOBILE_BIN_DIR" "$GO_BIN" install "github.com/sagernet/gomobile/cmd/gobind@$GOMOBILE_TAG"
fi
[[ -x "$GOMOBILE_BIN_DIR/gomobile" ]] || { echo "error: gomobile 安装失败" >&2; exit 1; }
[[ -x "$GOMOBILE_BIN_DIR/gobind" ]] || { echo "error: gobind 安装失败" >&2; exit 1; }
"$GOMOBILE_BIN_DIR/gomobile" init

echo "==> 构建 lanpathcore.aar（libbox，Android arm64）..."
mkdir -p "$(dirname "$OUTPUT_AAR")"
cd "$REPO_ROOT/mobile/lanpath-core"
"$GOMOBILE_BIN_DIR/gomobile" bind -v \
    -target android/arm64 \
    -androidapi 33 \
    -javapkg com.ybakiame.lanpath.core \
    -trimpath \
    -buildvcs=false \
    -ldflags "-X github.com/sagernet/sing-box/constant.Version=$SING_BOX_VERSION_NUM -X runtime.godebugDefault=multipathtcp=0,tlssha1=1 -s -w -buildid= -checklinkname=0" \
    -tags "with_gvisor,with_quic,with_wireguard,with_utls,with_naive_outbound,with_clash_api,with_usbip,with_openvpn,with_openconnect,badlinkname,tfogo_checklinkname0,with_tailscale,ts_omit_logtail,ts_omit_ssh,ts_omit_drive,ts_omit_taildrop,ts_omit_webclient,ts_omit_doctor,ts_omit_capture,ts_omit_kube,ts_omit_aws,ts_omit_synology,ts_omit_bird" \
    -o "$OUTPUT_AAR" \
    github.com/sagernet/sing-box/experimental/libbox

if [[ ! -f "$OUTPUT_AAR" ]]; then
    echo "error: 构建完成但未找到 $OUTPUT_AAR" >&2
    exit 1
fi

echo "==> 完成: $(du -h "$OUTPUT_AAR" | cut -f1)"
echo "    lanpathcore.aar 为 GPL-3.0 本地构建产物，不入库。"
