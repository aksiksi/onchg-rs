{
  description = "A tool that allows you to keep blocks in sync across different files in your codebase.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nix-pre-commit = {
      url = "github:jmgilman/nix-pre-commit";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, nix-pre-commit, ... }: let
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
          cargoSha256 = "sha256-t34KF87WPSDUBynLRZbmexEWvqYrddD9+YlhgpyWxWo=";
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

    shellHook = forAllSystems (system:
      (nix-pre-commit.lib.${system}.mkConfig {
        pkgs = nixpkgs.legacyPackages.${system};
        config = {
          repos = [
            {
              repo = "local";
              hooks = [
                {
                  id = "onchg";
                  language = "system";
                  entry = "${self.packages.${system}.default}/bin/onchg repo";
                  types = [ "text" ];
                  pass_filenames = false;
                }
              ];
            }
          ];
        };
      }).shellHook
    );

    # Development shell
    # nix develop
    devShells = forAllSystems (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        default = pkgs.mkShell {
          buildInputs = [ pkgs.cargo pkgs.libgit2 pkgs.rust-analyzer pkgs.rustc pkgs.rustfmt ];
        };
      }
    );
  };
}

