with import <nixpkgs> {};
mkShell {
  buildInputs = [ (enableDebugging mosquitto) ];
}
