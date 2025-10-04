{
  description = "Shader-sense language server build";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
        };

      in
      {
        packages = {
          shader-language-server = pkgs.rustPlatform.buildRustPackage rec {
            pname = "shader-language-server";
            version = "0.1.0";

            src = ./.;

            cargoLock = {
              lockFile = "${src}/Cargo.lock";
              outputHashes = {
                "hassle-rs-0.11.0" = "sha256-u5hyCKDOssK+ur+NIVQULQWgvWDn7aafR58CKijqP3s=";
              };
            };

            nativeBuildInputs = with pkgs; [
              rustToolchain
              pkg-config
              cmake
              python3
            ];

            buildInputs =
              with pkgs;
              [

                glslang
                spirv-tools
                spirv-headers

              ]
              ++ lib.optionals stdenv.isDarwin [
                darwin.apple_sdk.frameworks.Security
                darwin.apple_sdk.frameworks.SystemConfiguration
              ];

            cargoBuildFlags = [
              "--package"
              "shader_language_server"
            ];

            cargoTestFlags = [
              "--package"
              "shader_language_server"
            ];

            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

            NIX_CFLAGS_COMPILE = [
              "-I${pkgs.glslang}/include"
              "-I${pkgs.spirv-tools}/include"
            ];

            doCheck = false;
          };

          default = self.packages.${system}.shader-language-server;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            cmake
            python3
            glslang
            spirv-tools
            spirv-headers
            rust-analyzer
          ];

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

          NIX_CFLAGS_COMPILE = [
            "-I${pkgs.glslang}/include"
            "-I${pkgs.spirv-tools}/include"
          ];

          shellHook = ''
            echo "Shader-sense development environment"
            echo "Run 'cargo build --package shader-language-server' to build"
          '';
        };
      }
    );
}
