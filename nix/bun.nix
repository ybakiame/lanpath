{ pkgs }:
# 与 Windows CI 共用版本，固定归档摘要；不采用 26.05 自带的旧 Bun。
pkgs.bun.overrideAttrs {
  version = "1.4.2";
  src = pkgs.fetchurl {
    url = "https://github.com/oven-sh/bun/releases/download/bun-v1.4.2/bun-linux-x64-baseline.zip";
    hash = "sha256-xngEDxT+BEDrg503y9DOTAUaMtpygGrJfeamqra/co8=";
  };
}
