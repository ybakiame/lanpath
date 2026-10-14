{ pkgs, inputs, ... }:
let
  sdk = inputs.android-nixpkgs.sdk.${pkgs.stdenv.hostPlatform.system} (packages: with packages; [
    cmdline-tools-latest platform-tools
    platforms-android-35 platforms-android-36
    build-tools-35-0-0 build-tools-36-0-0 ndk-28-0-13004108
  ]);
  sdkRoot = "${sdk}/share/android-sdk";
  ndk = "${sdkRoot}/ndk/28.0.13004108";
  llvm = "${ndk}/toolchains/llvm/prebuilt/linux-x86_64";
in {
  packages = [ sdk ];
  env = {
    ANDROID_HOME = sdkRoot;
    ANDROID_SDK_ROOT = sdkRoot;
    ANDROID_NDK_ROOT = ndk;
    ANDROID_NDK_HOME = ndk;
    NDK_HOME = ndk;
    ANDROID_JAR = "${sdkRoot}/platforms/android-35/android.jar";
    ANDROID_BUILD_TOOLS = "${sdkRoot}/build-tools/36.0.0";
    CC_aarch64_linux_android = "${llvm}/bin/aarch64-linux-android33-clang";
    AR_aarch64_linux_android = "${llvm}/bin/llvm-ar";
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${llvm}/bin/aarch64-linux-android33-clang";
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android = "--target=aarch64-linux-android33 --sysroot=${llvm}/sysroot -isystem ${llvm}/sysroot/usr/include";
  };
  enterShell = ''
    export PATH="$ANDROID_BUILD_TOOLS:$PATH"
    export ANDROID_DEBUG_KEYSTORE="$HOME/.android/debug.keystore"
  '';
}
