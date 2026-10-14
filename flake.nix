{
  description = "Lanpath 开发环境：Rust(mobile/desktop) + Android(NDK/SDK) + Go(lanpathcore) + 前端(Bun)";

  nixConfig = {
    substituters = [
      "https://cache.nixos.org"
      "https://devenv.cachix.org"
    ];
    extra-trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    devenv = {
      url = "github:cachix/devenv/v2.4.0";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    android-nixpkgs.url = "github:tadfisher/android-nixpkgs/stable";
  };

  outputs = { nixpkgs, devenv, ... }@inputs:
  let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
  in {
    devShells.${system}.default = devenv.lib.mkShell {
      inherit inputs pkgs;
      modules = [ ./devenv.nix ];
    };
  };
}
