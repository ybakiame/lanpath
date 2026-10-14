{ ... }:
let
  atRoot = command: ''
    set -euo pipefail
    cd "$(git rev-parse --show-toplevel)"
    ${command}
  '';
in {
  # 命令仅显式执行；进入 shell 不安装依赖、不构建 AAR、不启动代理。
  scripts = {
    lanpath-install.exec = atRoot "bun install --frozen-lockfile";
    lanpath-rust-targets.exec = atRoot "rustup target add aarch64-linux-android";
    lanpath-verify-frontend.exec = atRoot ''
      bun run --filter lanpath-app verify
      bun run --filter @lanpath/client-core verify
      bun run --filter @lanpath/ui verify
    '';
    lanpath-verify-rust.exec = atRoot ''
      cargo build --workspace
      bun run verify:rust
    '';
    lanpath-test-tools.exec = atRoot "bun test packages/client-core/scripts/runtime-queries.test.ts apps/client/scripts/package-portable.test.ts scripts/release-manifest.test.ts scripts/verify-release-signatures.test.ts";
    lanpath-build-aar.exec = atRoot "GOTOOLCHAIN=local bash apps/client/scripts/build-lanpath-core.sh";
    lanpath-build-android-debug.exec = atRoot ''
      cd apps/client
      bun run tauri android build --debug --target aarch64
    '';
    lanpath-build-android-release.exec = atRoot ''
      cd apps/client
      bun run android:build
    '';
  };
}
