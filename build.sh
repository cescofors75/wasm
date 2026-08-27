#!/usr/bin/env bash
# Compila el motor RayDrone (Rust → WebAssembly). Sin dependencias externas:
# no usa wasm-bindgen ni crates, así que NO necesita acceso a crates.io.
set -e
cd "$(dirname "$0")"
core_tmp=libraydrone_core.tmp.rlib
wasm_tmp=raydrone.tmp.wasm
trap 'rm -f "$core_tmp" "$wasm_tmp"' EXIT

# El target de wasm hace falta una sola vez. Si ya está, esta línea no hace nada.
# (Necesita rustup; si lo instalaste con rustup, descargará el std de wasm offline-friendly.)
rustup target add wasm32-unknown-unknown 2>/dev/null || true

# 1) Kernel DSP compartido con el VST (no_std, sin deps) → rlib. Se compila con
#    el mismo rustc crudo, así que seguimos sin Cargo ni crates.io.
rustc --edition 2021 \
      --target wasm32-unknown-unknown \
      -O -C panic=abort -C lto=fat \
      --crate-name raydrone_core --crate-type=lib \
      ../core/src/lib.rs -o "$core_tmp"

# 2) Motor → wasm, enlazando el kernel por --extern.
rustc --edition 2021 \
      --target wasm32-unknown-unknown \
      -O -C panic=abort -C lto=fat \
      --extern raydrone_core="$core_tmp" \
      --crate-type=cdylib \
      raydrone.rs -o "$wasm_tmp"

mv "$core_tmp" libraydrone_core.rlib
mv "$wasm_tmp" raydrone.wasm

echo "✓ raydrone.wasm generado ($(wc -c < raydrone.wasm) bytes)"
echo "Ahora sirve la carpeta por HTTP, p.ej.:  python3 -m http.server 8080"
echo "y abre  http://localhost:8080/wasm/  (o el puerto que uses)"
