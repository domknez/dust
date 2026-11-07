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
        glib-networking # TLS for the login window's webview
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

      mkDust = pkgs:
        let
          inherit (pkgs) lib;
          linux = pkgs.stdenv.hostPlatform.isLinux;
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          # Nix manages updates: the in-app updater can't write to the store, so it
          # doesn't check by default (still switchable in Settings). The unit test that
          # pins the default follows.
          postPatch = ''
            substituteInPlace src/settings.rs \
              --replace-fail 'get("auto_update") != Some("false")' 'get("auto_update") == Some("true")' \
              --replace-fail 'p.report_listens && !p.shuffle && p.auto_update &&' 'p.report_listens && !p.shuffle && !p.auto_update &&'
          '';

          nativeBuildInputs = [ pkgs.pkg-config ] ++ lib.optionals linux [ pkgs.wrapGAppsHook3 ];
          buildInputs = lib.optionals linux (linuxLibs pkgs);

          # Network tests are #[ignore]d; the rest run offline.
          doCheck = true;

          # Desktop entry and icons (the entry's `Icon=dust` is looked up in hicolor).
          postInstall = lib.optionalString linux ''
            install -Dm644 assets/linux/dust.desktop $out/share/applications/dust.desktop
            for size in 16 24 32 48 64 128 256 512; do
              install -Dm644 assets/dust-icon-pack/png/dust-$size.png \
                $out/share/icons/hicolor/''${size}x''${size}/apps/dust.png
            done
            install -Dm644 assets/dust-icon-pack/svg/dust-full.svg \
              $out/share/icons/hicolor/scalable/apps/dust.svg
          '';

          preFixup = lib.optionalString linux ''
            gappsWrapperArgs+=(--prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath (runtimeLibs pkgs)})
          '';

          meta = {
            description = cargoToml.package.description;
            license = lib.licenses.mit;
            homepage = cargoToml.package.repository;
            mainProgram = "dust";
            platforms = lib.platforms.linux ++ lib.platforms.darwin;
          };
        };
    in
    {
      packages = forAll (pkgs: { default = mkDust pkgs; });

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

      # `nix flake check` builds the package (running the unit tests) and checks what
      # was installed: binary, desktop entry, icons, and that every library resolves.
      checks = forAll (pkgs:
        let
          dust = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        in
        {
          package = dust;
        } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          install-layout = pkgs.runCommand "dust-install-layout" { nativeBuildInputs = [ pkgs.desktop-file-utils pkgs.file pkgs.glibc.bin ]; } ''
            fail() { echo "FAIL: $1" >&2; exit 1; }

            [ -x ${dust}/bin/dust ] || fail "bin/dust missing or not executable"
            desktop-file-validate ${dust}/share/applications/dust.desktop || fail "invalid dust.desktop"
            grep -q '^Exec=dust$' ${dust}/share/applications/dust.desktop || fail "unexpected Exec in dust.desktop"

            for size in 16 24 32 48 64 128 256 512; do
              icon=${dust}/share/icons/hicolor/''${size}x''${size}/apps/dust.png
              [ -f "$icon" ] || fail "missing $icon"
              file "$icon" | grep -q "''${size} x ''${size}" || fail "$icon is not ''${size}x''${size}"
            done
            [ -f ${dust}/share/icons/hicolor/scalable/apps/dust.svg ] || fail "missing scalable icon"

            # The wrapper puts the dlopen()ed GUI libraries on the library path.
            grep -q 'LD_LIBRARY_PATH' ${dust}/bin/dust || fail "bin/dust does not set LD_LIBRARY_PATH"
            grep -q 'libxkbcommon' ${dust}/bin/dust || fail "libxkbcommon not on the wrapper's library path"

            # No shared library left unresolved.
            libs=$(ldd ${dust}/bin/.dust-wrapped) || fail "ldd failed"
            echo "$libs" | grep -q libasound || fail "ldd output looks wrong: $libs"
            if echo "$libs" | grep -q 'not found'; then
              echo "$libs" >&2
              fail "unresolved shared libraries"
            fi

            touch $out
          '';
        });

      # `imports = [ dust.nixosModules.default ];` installs dust system-wide.
      nixosModules.default = { pkgs, ... }: {
        environment.systemPackages = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
      };

      overlays.default = final: _prev: { dust = self.packages.${final.stdenv.hostPlatform.system}.default; };
    };
}
