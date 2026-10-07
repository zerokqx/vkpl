{
  description = "VKPL — VK Music terminal player";

  # 26.11 dropped x86_64-darwin; 26.05 retains every declared release target.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let name = builtins.baseNameOf path;
              in name != "target" && name != ".direnv" && name != "result";
          };
          vkpl = pkgs.rustPlatform.buildRustPackage {
            pname = "vkpl";
            version = "0.1.0";
            inherit src;

            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "-p" "vkpl" ];

            nativeBuildInputs = [ pkgs.makeWrapper pkgs.pkg-config ];

            postInstall = ''
              wrapProgram $out/bin/vkpl
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.mpv pkgs.ffmpeg ]}
            '';

            meta = with pkgs.lib; {
              description = "VK Music terminal player";
              homepage = "https://vk.ru/music";
              license = licenses.mit;
              mainProgram = "vkpl";
              platforms = platforms.unix;
            };
          };
        in {
          default = vkpl;
          inherit vkpl;
        });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.vkpl}/bin/vkpl";
        };
      });

      devShells = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              pkg-config
              mpv
              ffmpeg
              lua5_4
            ] ++ pkgs.lib.optionals stdenv.isLinux [ chromium ];
          };
        });
    };
}
