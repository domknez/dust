{
  description = "dust: ultra-lightweight desktop music player for Deezer and Navidrome";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      cargoToml = fromTOML (builtins.readFile ./Cargo.toml);

      # Libraries the Linux build links against or loads at run time.
      linuxLibs = pkgs: with pkgs; [
        alsa-lib # cpal
        dbus # secret-service keyring
        gtk3 # login window (wry/tao)
        webkitgtk_4_1 # login window (wry)
        libsoup_3
        openssl
      ];

      # Loaded with dlopen by winit/glow, so they must be on the library path.
      runtimeLibs = pkgs: with pkgs; [
        wayland
        libxkbcommon
        libGL
        libx11
        libxcursor
        libxi
        libxrandr
      ];
    in
    {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.pkg-config ]
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.makeWrapper pkgs.wrapGAppsHook3 ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (linuxLibs pkgs);

          # Network tests are #[ignore]d; the rest run offline.
          doCheck = true;

          postInstall = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            install -Dm644 assets/linux/dust.desktop $out/share/applications/dust.desktop
          '';

          preFixup = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            gappsWrapperArgs+=(--prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)})
          '';

          meta = {
            description = cargoToml.package.description;
            license = pkgs.lib.licenses.mit;
            homepage = cargoToml.package.repository;
            mainProgram = "dust";
          };
        };
      });

      apps = forAll (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/dust";
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [ pkg-config cargo rustc clippy rustfmt rust-analyzer ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (linuxLibs pkgs);
          LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux (pkgs.lib.makeLibraryPath (runtimeLibs pkgs));
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });

      overlays.default = final: _prev: { dust = self.packages.${final.stdenv.hostPlatform.system}.default; };
    };
}
