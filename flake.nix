{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;

      shellForSystem =
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        pkgs.mkShell {
          packages = [
            pkgs.probe-rs
            pkgs.flip-link
            pkgs.gcc-arm-embedded-14
          ];

        };

    in
    {
      devShell = forAllSystems shellForSystem;
    };
}
