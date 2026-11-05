{pkgs, ...}: {
  packages = [
    pkgs.git
    pkgs.hyperfine
    pkgs.perf
    pkgs.python3
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
  };

  scripts.turing.exec = ''
    python turing.py $@
  '';
}
