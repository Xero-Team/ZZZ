{ inputs, ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      mkZZZ = import ../toolchain.nix { inherit inputs; };
      zed-editor = mkZZZ pkgs;
    in
    {
      packages = {
        default = zed-editor;
        debug = zed-editor.override { profile = "dev"; };
      };
    };
}
