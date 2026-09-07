# RayDrone · WebAssembly

RayDrone es un instrumento granular: el motor DSP está escrito en Rust y se
ejecuta dentro de un `AudioWorklet` mediante WebAssembly. La interfaz principal
abre en **Básico** y concentra los controles esenciales: material, carácter,
movimiento, espacio y volumen. Los modos Medio y Profesional despliegan el
control detallado sin alterar la escena actual.

## Ejecutar

```powershell
.\build.ps1
python -m http.server 8080
```

Abre `http://localhost:8080/`. AudioWorklet requiere HTTP local o HTTPS.

## Estructura

| Archivo | Función |
| --- | --- |
| `index.html` | Interfaz del instrumento y visualización. |
| `raydrone.rs` | Motor granular Rust `no_std`. |
| `processor.js` | Puente entre Web Audio y WASM. |
| `lab-worker.js` | Cálculos de convergencia fuera de la UI. |
| `core/` | Núcleo DSP compartido y versionado con WASM y VST. |
| `build.ps1` / `build.sh` | Compilación del núcleo y del WASM; intermedios en `.build/`. |

## Verificación

```powershell
node test_all.mjs
```

Requiere Node.js 24, Rust y el target `wasm32-unknown-unknown`
(`rustup target add wasm32-unknown-unknown`). El comando recompila desde el
código del repositorio, ejecuta las tres suites originales y las regresiones
de `tests/`. No depende de un directorio `core` externo.

Las regresiones cubren precisión numérica, envolvente, acordes, filtro,
convergencia, cambios de fuente, inicialización, telemetría, SDK y liberación
de recursos GPU. El motor del VST también se ejecuta bajo WASM para comprobar
su DSP sin requerir un SDK nativo; esto no sustituye las pruebas del plugin:

```powershell
cargo test --manifest-path vst/Cargo.toml --locked --lib
```

En Windows, estas últimas necesitan Visual Studio Build Tools con C++ y Windows
SDK. [CI](.github/workflows/test.yml) comprueba WASM desde un checkout limpio
en Linux y las pruebas del VST en Windows; no publica releases automáticamente.

## Test perceptual Top-36

La configuración musical base es 2.000 rayos simulados, selección a 100 Hz,
un Top-36 e inertia inicial de 25 %. Las sustituciones usan crossfade de 12 ms
para conservar continuidad. 8.000 rayos
se conserva como modo de comparación/estrés. Para renderizar el A/B reproducible:

La población se mantiene llena mientras Drone está activo: los 2.000 candidatos
se actualizan en O(N) por bloque y sólo el Top-36 recorre el DSP muestra a
muestra. La densidad y duración granular ya no reducen la población simulada.

```powershell
node perceptual_test.mjs 20 ab
```

Genera el barrido 2K con inertia `0 / 0,25 / 0,50`, además de
`renders/profile-inertia.csv`. La inercia Top-36 es continua entre 0 y 1 y no
cambia el número de voces; sólo añade histéresis a candidatos ya seleccionados.

8K queda fuera del experimento musical normal. Su comparación de estrés se
genera explícitamente con `node perceptual_test.mjs 20 stress`.

RayRunner se mantiene ahora en el repositorio independiente
`C:\Users\cesco\Desktop\RayRunner`.
