{
  inputs = {
    crane.url = "github:ipetkov/crane";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    crane,
    fenix,
    flake-utils,
    nixpkgs,
  }:
    flake-utils.lib.eachDefaultSystem
    (
      system: let
        rust-toolchain = fenix.packages.${system}.stable.toolchain;
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;
        # core は kana-table.tsv を include_str! するので、Cargo の source に足す。
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            (craneLib.fileset.commonCargoSources ./sekken-rs)
            ./sekken-rs/core/kana-table.tsv
          ];
        };
        commonArgs = {
          pname = "sekken";
          version = "0.2.0";
          inherit src;
          strictDeps = true;
          cargoToml = ./sekken-rs/Cargo.toml;
          cargoLock = ./sekken-rs/Cargo.lock;
          postUnpack = ''
            cd $sourceRoot/sekken-rs
            sourceRoot="."
          '';
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        sekken = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
            # テストは checks.sekken-test で走らせる。
            doCheck = false;
            meta.mainProgram = "sekken";
          });
        sekken-test = craneLib.cargoTest (commonArgs // {inherit cargoArtifacts;});
        sekken-clippy = craneLib.cargoClippy (commonArgs
          // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });
        sekken-el = pkgs.emacsPackages.trivialBuild {
          pname = "sekken";
          version = "0.2.0";
          src = ./lisp;
          # sekken-kana.el は自身の場所から ../share/kana-table.tsv を読む。
          # share/ の実体は symlink なので、core にある本体を写す。
          postInstall = ''
            mkdir -p $out/share/emacs/share
            cp ${./sekken-rs/core/kana-table.tsv} $out/share/emacs/share/kana-table.tsv
          '';
        };
      in {
        packages = {
          default = sekken;
          inherit sekken sekken-el;
        };

        checks = {
          inherit sekken sekken-test sekken-clippy;
          ert = pkgs.runCommand "sekken-ert" {
            nativeBuildInputs = [pkgs.emacs-nox];
          } ''
            cp -r ${./lisp} lisp
            mkdir share
            cp ${./sekken-rs/core/kana-table.tsv} share/kana-table.tsv
            chmod -R u+w lisp
            cd lisp
            bash test/run.sh
            touch $out
          '';
        };

        devShells.default = with pkgs;
          mkShell {
            buildInputs = [
              rust-toolchain
              emacs-nox
            ];
          };
      }
    );
}
