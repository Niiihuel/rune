{
  description = "RUNE: Rust UI, No Electron. A native chat client built on limn.";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      each = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      # The same shell as `nix-shell`, so both entry points stay in step.
      devShells = each (pkgs: {
        default = import ./shell.nix { inherit pkgs; };
        session = import ./shell.nix { inherit pkgs; withLogin = true; };
        # The separate renderer is isolated before reading untrusted PDFs.
        documents = import ./shell.nix { inherit pkgs; withLogin = true; withDocs = true; };
      });
    };
}
