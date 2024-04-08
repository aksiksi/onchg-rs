{
  description = "A tool that allows you to keep blocks in sync across different files in your codebase.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs, ... }: let
    supportedSystems = [ "x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin" ];
    forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    pname = "onchg";
    owner = "aksiksi";
    version = "0.1.6";
  in {
    packages = forAllSystems (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        default = pkgs.rustPlatform.buildRustPackage {
          inherit pname;
          inherit version;
          src = ./.;
          cargoSha256 = "sha256-/YRvUALUTg+wYhPr21cS5HQj9+kLVdFHG9JKYOPMJJc";
          meta = {
            description = "A tool that allows you to keep blocks in sync across different files in your codebase.";
            homepage = "https://github.com/aksiksi/onchg-rs";
            license = nixpkgs.lib.licenses.mit;
            maintainers = [];
          };
          # Do not run tests; they rely on the filesystem.
          doCheck = false;
        };
      }
    );

    # Development shell
    # nix develop
    devShells = forAllSystems (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        default = pkgs.mkShell {
          buildInputs = [ pkgs.cargo pkgs.libgit2 ];
        };
      }
    );
  };
}

