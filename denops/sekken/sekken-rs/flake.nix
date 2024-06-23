{
  description = "A devShell example";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in {
        devShells.default = with pkgs;
          mkShell {
            packages = [
              (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)

              binaryen
              capnproto
              capnproto-rust
              cargo-edit
              wasm-bindgen-cli
              wasm-pack

              duckdb
            ];
          };
      }
    );
}
