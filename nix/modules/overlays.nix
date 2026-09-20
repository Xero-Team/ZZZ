{ inputs, ... }:
{
  flake.overlays.default =
    final: _:
    let
      mkZZZ = import ../toolchain.nix { inherit inputs; };
    in
    {
      zed-editor = mkZZZ final;
    };
}
