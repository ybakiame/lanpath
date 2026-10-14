{ pkgs, ... }:
let
  libraries = with pkgs; [
    webkitgtk_4_1 gtk3 glib glib-networking gdk-pixbuf pango cairo libsoup_3
    openssl alsa-lib libxkbcommon libX11 libXcursor libXrandr libXi dbus libGL
    nss nspr at-spi2-core libdrm mesa expat cups libpng librsvg fontconfig
    freetype harfbuzz libepoxy xz bzip2 zstd
  ];
in {
  packages = libraries;
  enterShell = ''
    export PKG_CONFIG_PATH="${pkgs.webkitgtk_4_1.dev}/lib/pkgconfig:${pkgs.gtk3.dev}/lib/pkgconfig:${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.glib.dev}/lib/pkgconfig:''${PKG_CONFIG_PATH-}"
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath libraries}:''${LD_LIBRARY_PATH-}"
  '';
}
