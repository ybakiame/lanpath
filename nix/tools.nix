{ pkgs, ... }:
{
  # Rust 仍由 rustup + rust-toolchain.toml 管理，不叠加第二套 Rust 工具链。
  # Android 使用项目 Gradle wrapper，不安装全局 Gradle。
  packages = [ (import ./bun.nix { inherit pkgs; }) ] ++ (with pkgs; [
    git curl wget unzip gnumake cmake ninja pkg-config perl
    rustup cargo-ndk go nodejs jdk17 llvmPackages.libclang clang
  ]);
  env.JAVA_HOME = pkgs.jdk17.home;
  env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
}
