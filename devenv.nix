{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: {
  packages = [
    pkgs.git
    pkgs.hyperfine
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
  };

  env = {
    # MACHINE = "1RB1LC_1RC1RB_1RD0LE_1LA1LD_1RZ0LA";

    # Antihydra
    MACHINE = "1RB1RA_0LC1LE_1LD1LC_1LA0LB_1LF1RE_---0RA";
  };
}
