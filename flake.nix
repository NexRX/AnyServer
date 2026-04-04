{
  description = "AnyServer — a self-hosted panel for running any binary as a managed server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      pkgsFor = system: nixpkgs.legacyPackages.${system};
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          anyserver = pkgs.callPackage ./nix/package.nix { src = self; };
          default = self.packages.${system}.anyserver;
        }
      );

      nixosModules = {
        anyserver =
          { ... }:
          {
            imports = [ ./nix/module.nix ];
            nixpkgs.overlays = [ self.overlays.default ];
          };
        default = self.nixosModules.anyserver;
      };

      overlays = {
        default = _final: _prev: {
          anyserver = self.packages.${_prev.stdenv.hostPlatform.system}.anyserver;
        };
      };

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.anyserver ];
            packages = with pkgs; [
              nodejs_20
              pnpm
              rust-analyzer
              clippy
              rustfmt
              sqlx-cli
            ];
          };
        }
      );
    };
}
