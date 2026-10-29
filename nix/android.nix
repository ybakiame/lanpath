{ pkgs, config, lib, ... }:
let
  sdkRoot = config.env.ANDROID_HOME;
  ndk = config.env.ANDROID_NDK_ROOT;
  llvm = "${ndk}/toolchains/llvm/prebuilt/linux-x86_64";
in {
  android = {
    enable = true;
    platforms.version = [ "35" "36" ];
    buildTools.version = [ "36.0.0" "35.0.0" ];
    ndk = {
      enable = true;
      version = [ "28.0.13004108" ];
    };
    emulator.enable = false;
    systemImages.enable = false;
    sources.enable = false;
    googleAPIs.enable = false;
    googleTVAddOns.enable = false;
    extras = [ ];
    extraLicenses = [ ];
    android-studio.enable = false;
  };
  languages.java = {
    jdk.package = pkgs.jdk17;
    lsp.enable = false;
  };
  env = {
    ANDROID_SDK_ROOT = sdkRoot;
    ANDROID_NDK_HOME = ndk;
    NDK_HOME = ndk;
    ANDROID_JAR = "${sdkRoot}/platforms/android-35/android.jar";
    ANDROID_BUILD_TOOLS = "${sdkRoot}/build-tools/36.0.0";
    CC_aarch64_linux_android = "${llvm}/bin/aarch64-linux-android33-clang";
    AR_aarch64_linux_android = "${llvm}/bin/llvm-ar";
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${llvm}/bin/aarch64-linux-android33-clang";
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android = "--target=aarch64-linux-android33 --sysroot=${llvm}/sysroot -isystem ${llvm}/sysroot/usr/include";
  };
  enterShell = lib.mkAfter ''
    export PATH="$ANDROID_BUILD_TOOLS:$PATH"
    export ANDROID_USER_HOME="$HOME/.android"
    export ANDROID_AVD_HOME="$ANDROID_USER_HOME/avd"
    export ANDROID_DEBUG_KEYSTORE="$ANDROID_USER_HOME/debug.keystore"
  '';
}
