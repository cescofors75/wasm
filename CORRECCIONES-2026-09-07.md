# Correcciones y validación — 7 de septiembre de 2026

Se han corregido los 11 hallazgos de la auditoría. El WASM distribuido se ha recompilado con el núcleo incluido en este repositorio.

| Hallazgo | Corrección | Cobertura |
|---|---|---|
| 1. RMS con suelo artificial | Raíz cuadrada con estimación por exponente y normalización de subnormales | Precisión en todo el rango f32 y RMS a distintas amplitudes |
| 2. Envelope inestable | Coeficiente compuesto por bloque y estado acotado | 15 combinaciones de frecuencia de muestreo y tamaño de bloque |
| 3. Cambio de muestra | Detención de reproducción y reinicio del transporte, voces y efectos | Motor, UI y navegador real en Original y Drone |
| 4. Acordes incorrectos | Catálogo compartido con los nueve identificadores de la interfaz | Los nueve acordes |
| 5. Filtro y LFO | Filtro de estado variable con corte en Hz, resonancia y modulación en octavas | Respuesta en frecuencia y conversión de octavas |
| 6. Importance sesgado | Inversa exacta de la distribución triangular discreta | Aperturas pequeñas en CPU y WASM; experimento sintético regenerado |
| 7. MIDI en bypass/reset | Consumo de eventos durante bypass y limpieza de notas en reset | Tres pruebas nativas añadidas; pendientes de ejecución |
| 8. Inicialización sin reintento | Promesa compartida y reintento tras fallo de addModule | Fallo/reintento y llamadas simultáneas |
| 9. Telemetría desbordada | Sincronización de cursores y descarte de registros antiguos | Reinicio de muestra y reproducción directa |
| 10. Laboratorio obsoleto | Cancelación del worker y generación que invalida resultados anteriores | Callbacks tardíos, decodificación concurrente y navegador real |
| 11. Build dependiente de carpeta externa | Núcleo local, scripts independientes y CI explícito | Compilación desde copia que contiene únicamente los fuentes necesarios |

Además, se validan los parámetros del SDK antes de enviar mensajes, se completan escenas parciales con valores predeterminados, se conservan snapshots de materiales y se liberan buffers/dispositivos WebGPU incluso si falla el cálculo. Los nuevos artefactos intermedios se guardan en `.build/`.

## Resultados comprobados

- `node test_all.mjs`: **84 pruebas nuevas aprobadas, 0 fallos**, además de las tres suites existentes (`test_engine.mjs`, `test_acoustic.mjs` y `test_raytrace_buffer.mjs`). El comando recompila el motor de producción y el puente de pruebas Rust/WASM.
- Navegador: interfaz básica/profesional, reproducción, reemplazo de audio, laboratorio completo y cancelación durante reemplazo; sin errores de página observados.
- WebGPU real: render de una muestra conocida con resultado 1; simulación acústica de 2048 rayos con 800 muestras finitas y sin error de validación GPU. Las rutas de liberación ante errores tienen seis pruebas automatizadas.
- Experimento sintético run5 regenerado, junto con su figura y el resumen. Runs 1–4 se conservan como históricos: falta el audio original para reproducirlos con el estimador corregido.

## Límites de validación

`cargo test --manifest-path vst/Cargo.toml --locked --lib` no pudo ejecutarse porque falta el enlazador MSVC (`link.exe`) y el SDK de Windows. El motor que usa el VST sí se prueba mediante WASM, pero esto no sustituye las pruebas del plugin nativo ni la carga en un DAW. El workflow añadido ejecutará las tres pruebas de transporte en Windows; no se ha ejecutado el CI remoto durante esta sesión.

El filtro y los acordes corregidos cambian el resultado sonoro de los ajustes afectados. No se ha realizado una evaluación perceptual formal.

La revisión automática rechazó la eliminación de las cachés históricas ya versionadas (`vst/target`, `paper/__pycache__` y la biblioteca intermedia raíz). Se conservaron y se restauraron los artefactos previos modificados por las pruebas. `.gitignore` evita añadir nuevos artefactos equivalentes, pero no elimina los existentes del índice.

La auditoría original y `.audit/resultados.txt` describen el estado anterior. `.audit/test-results.txt` contiene la ejecución final; `.audit/reproduce.mjs` ejecuta ahora la batería de regresión vigente.
