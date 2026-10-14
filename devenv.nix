{ ... }:
{
  imports = [
    ./nix/tools.nix
    ./nix/desktop.nix
    ./nix/android.nix
    ./nix/commands.nix
  ];
  # Husky 是唯一提交检查入口；不配置 devenv git hooks。
  enterShell = ''
    echo "Lanpath dev shell: go=$(go version | cut -d' ' -f3) bun=$(bun --version) ndk=$(basename "$ANDROID_NDK_ROOT")"
    echo "Android build-tools: $ANDROID_BUILD_TOOLS"
  '';
}
