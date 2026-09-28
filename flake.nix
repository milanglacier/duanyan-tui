{
  description = "Duanyan (端砚): a fullscreen TUI Chinese input scratchpad powered by librime";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      librimeFile =
        pkgs:
        "${pkgs.librime}/lib/librime${
          if pkgs.stdenv.hostPlatform.isDarwin then ".1.dylib" else ".so.1"
        }";

      duanyan =
        {
          lib,
          rustPlatform,
          makeWrapper,
          symlinkJoin,
          librime,
          stdenv,
          # Rime data packages merged into the default shared data dir. Empty
          # means the shared data dir is auto-detected at runtime.
          rimeDataPackages ? [ ],
        }:
        let
          rimeData = symlinkJoin {
            name = "duanyan-rime-data";
            paths = rimeDataPackages;
          };
          libFile = "${librime}/lib/librime${if stdenv.hostPlatform.isDarwin then ".1.dylib" else ".so.1"}";
        in
        rustPlatform.buildRustPackage {
          pname = "duanyan";
          version = (builtins.fromTOML (builtins.readFile ./crates/duanyan/Cargo.toml)).package.version;
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./crates
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ makeWrapper ];
          # Integration tests need librime and rime-data, provided through env.
          preCheck = ''
            export DUANYAN_LIBRIME_PATH=${libFile}
          '';
          postInstall = ''
            wrapProgram $out/bin/duanyan \
              --set-default DUANYAN_LIBRIME_PATH ${libFile} ${lib.optionalString (rimeDataPackages != [ ]) "--set-default DUANYAN_RIME_SHARED_DIR ${rimeData}/share/rime-data"}
          '';
          meta = {
            description = "Fullscreen TUI Chinese input scratchpad powered by librime";
            mainProgram = "duanyan";
            license = lib.licenses.gpl3Plus;
            platforms = lib.platforms.linux ++ lib.platforms.darwin;
          };
        };
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.callPackage duanyan { };
        duanyan = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer
            cargo-nextest
            pkg-config
            librime
            rime-data
          ];
          DUANYAN_LIBRIME_PATH = librimeFile pkgs;
          DUANYAN_RIME_SHARED_DIR = "${pkgs.rime-data}/share/rime-data";
          RIME_INCLUDE_DIR = "${pkgs.librime}/include";
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });

      checks = forAllSystems (pkgs: {
        package = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
    };
}
