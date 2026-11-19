{ ... }:
let
  atRoot = command: ''
    set -euo pipefail
    cd "$(git rev-parse --show-toplevel)"
    ${command}
  '';
in {
  # Commands run explicitly; entering the shell never builds or starts the VPN.
  scripts = {
    lanpath-install.exec = atRoot "bun install --frozen-lockfile";
    lanpath-rust-targets.exec = atRoot "rustup target add aarch64-linux-android";
    lanpath-verify-rust.exec = atRoot ''
      cargo build --workspace
      bash scripts/check-rust-gates.sh
    '';
    lanpath-test-tools.exec = atRoot "bun test apps/gpui/tools";
    lanpath-build-aar.exec = atRoot "GOTOOLCHAIN=local bash mobile/lanpath-core/build-android.sh";
    lanpath-build-android-debug.exec = atRoot ''
      cd mobile/android
      ./gradlew :vpn:assembleDebug
    '';
    lanpath-build-android-release.exec = atRoot ''
      cd mobile/android
      ./gradlew :vpn:assembleRelease
    '';
  };
}
