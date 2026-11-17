# Install with Nix

Nix packaging is maintained by @untypequicode; please tag them on Nix issues.

The repository is a flake (`flake.nix`): the package includes the desktop entry, the
icons and the libraries dust loads at run time (Wayland, X11, OpenGL, WebKitGTK for the
login window). Nix manages the updates, so dust doesn't check for them by itself (it can
still be switched on in Settings). Linux (x86_64, aarch64) and macOS build from the same
flake; the flakes experimental feature must be enabled.

**Try it without installing:**

```sh
nix run github:domknez/dust
```

**Install it for your user:**

```sh
nix profile install github:domknez/dust
nix profile upgrade dust   # later, to update
```

**On NixOS**, add the flake as an input of your system flake and install the package:

```nix
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.dust = {
    url = "github:domknez/dust";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, dust, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        dust.nixosModules.default # installs dust system-wide
      ];
    };
  };
}
```

Without the module, `environment.systemPackages = [ dust.packages.${pkgs.stdenv.hostPlatform.system}.default ];`
does the same.

Rebuild with `sudo nixos-rebuild switch --flake .#myhost`; `nix flake update dust` pulls
a newer dust. The same package works in Home Manager (`home.packages`). The flake also
has an overlay (`dust.overlays.default`, which adds `pkgs.dust`; note that nixpkgs
already has an unrelated `dust`, the disk usage tool, so prefer the package form above).

To pin a branch or version, use `github:domknez/dust/<branch-or-tag>`.

**Check the flake:** `nix flake check` builds dust, runs its unit tests, and verifies the
installed result: the binary, the desktop entry, every icon size and that no shared
library is missing.

**Develop:** `nix develop` gives a shell with Rust, clippy, rustfmt and every native
library; then `cargo run` / `cargo test` as usual.

To build without the in-app updater by hand: `cargo build --release --no-default-features --features login-window`.
