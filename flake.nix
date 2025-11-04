{
  description = "Claudepod - CLI tool for managing Claude Code Docker instances";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
        };

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        claudepod = rustPlatform.buildRustPackage {
          pname = "claudepod";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            libiconv
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          # Skip tests during build
          doCheck = false;

          meta = with pkgs.lib; {
            description = "CLI tool for managing Claude Code Docker instances with declarative TOML configuration";
            homepage = "https://github.com/yourusername/claudepod";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "claudepod";
          };
        };

      in
      {
        packages = {
          default = claudepod;
          claudepod = claudepod;
        };

        apps = {
          default = flake-utils.lib.mkApp {
            drv = claudepod;
            name = "claudepod";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            rust-analyzer
            cargo-watch
            pkg-config
            libiconv
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      }
    );
}
