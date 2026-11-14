{
  description = "Native Wayland price check (Rust daemon + Quickshell overlay)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
    in {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "gold";
          version = "0.1.0";

          # the code, plus the game data it reads at build (tests) and run time
          src = nixpkgs.lib.fileset.toSource {
            root = ./.;
            fileset = nixpkgs.lib.fileset.unions [
              ./native
              ./data/en
            ];
          };
          cargoRoot = "native";
          buildAndTestSubdir = "native";
          cargoLock.lockFile = ./native/Cargo.lock;
          cargoBuildFlags = [ "-p" "gold" ];

          nativeBuildInputs = [ pkgs.makeWrapper ];

          postInstall = ''
            mkdir -p $out/share/gold/data/en $out/share/gold/quickshell
            cp data/en/{client_strings.js,items.ndjson,stats.ndjson} $out/share/gold/data/en/
            cp native/quickshell/GoldOverlay.qml $out/share/gold/quickshell/
            wrapProgram $out/bin/gold --prefix PATH : ${nixpkgs.lib.makeBinPath [ pkgs.wl-clipboard ]}
          '';

          meta = {
            description = "Native Wayland price check for Hyprland";
            license = nixpkgs.lib.licenses.mit;
            mainProgram = "gold";
            platforms = nixpkgs.lib.platforms.linux;
          };
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc clippy rustfmt rust-analyzer socat ];
        };
      });
    };
}
