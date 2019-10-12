with import <nixpkgs> {};
mkShell {
  nativeBuildInputs = [ pkgconfig ];
  buildInputs = [ libical ];

  LIBCLANG_PATH = "${llvmPackages.libclang}/lib";
}
