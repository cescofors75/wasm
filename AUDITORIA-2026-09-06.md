# Auditoría de RayDrone — 6 de septiembre de 2026

> Documento histórico anterior a las correcciones. Estado actual y pruebas: [CORRECCIONES-2026-09-07.md](CORRECCIONES-2026-09-07.md).

La revisión detecta **11 hallazgos: 2 de prioridad alta (P1) y 9 de prioridad media (P2)**. Los dos primeros afectan a la validez de las mediciones y a la estabilidad del audio. Que las suites existentes pasen no basta para dar el proyecto por validado.

Se ha revisado el código propio del motor WASM, AudioWorklet, interfaz y traducción, SDK, laboratorio, implementaciones CPU/WebGPU, VST/standalone, scripts de compilación y generación de resultados. También se ha leído `../core/src/lib.rs`, dependencia local situada **fuera de este repositorio**. Los binarios, WAV, figuras y cachés de compilación no se han tratado como código fuente.

No se ha modificado el código de producción ni el WASM distribuido. Se conserva la auditoría anterior como documento histórico.

## Hallazgos

### 1. P1 — La raíz cuadrada introduce un suelo artificial en el RMS

**Ubicación:** [core/src/lib.rs:22](C:/DEV/projects/test/core/src/lib.rs:22), consumo en [raydrone.rs:1334](C:/DEV/projects/test/wasm/raydrone.rs:1334).

`sqrtf()` inicia Newton en 1 para todo valor entre 0 y 1 y ejecuta únicamente siete iteraciones. Para entradas positivas pequeñas termina cerca de `1/128 = 0.0078125`, independientemente de la raíz real. Afecta a `lab_rms()`, al cálculo de energía y a las probabilidades de importance/smart.

**Reproducción confirmada en WASM:** una fuente acotada por ±`1e-6`, estimada con un rayo y ventana Hann, tiene un error RMS necesariamente menor o igual que `2e-6`. El motor devuelve **`0.0078125`**. La propia suite muestra resultados de laboratorio cerca de ese valor y los acepta.

**Impacto:** mesetas de error y pendientes de convergencia pueden reflejar el error numérico de la función, no el estimador. No se pueden validar las conclusiones experimentales actuales solo con esas pruebas; los CSV históricos necesitan regeneración y comparación, no se presupone que todos fueran producidos por esta versión.

**Corrección:** usar una raíz con precisión comprobada en todo el rango de energía, verificar error relativo/absoluto y regenerar los experimentos afectados. Añadir una comprobación de que escalar la fuente escala proporcionalmente el RMS.

### 2. P1 — La envolvente permite una actualización inestable y acaba en NaN

**Ubicación:** [raydrone.rs:2000](C:/DEV/projects/test/wasm/raydrone.rs:2000), límites admitidos en `set_modulation()` y [sdk/raydrone-sdk.js:28](C:/DEV/projects/test/wasm/sdk/raydrone-sdk.js:28).

La actualización aplica `MOD_ENV += (blk - MOD_ENV) * n / (tiempo * sampleRate)`. Con attack/release de 1 ms, 48 kHz y bloques de 128, el coeficiente es aproximadamente **2.667**. Esa recurrencia diverge. El ABI y el SDK aceptan expresamente 1 ms, aunque los mínimos de la interfaz sean mayores.

**Reproducción confirmada:** `set_modulation(2, 2, .25, 1, .001, .001)` durante 400 bloques produce `top36_pitch_mean() = NaN`; la energía del último bloque queda en aproximadamente `6.92e-29`. El limitador puede ocultar el problema convirtiendo valores no finitos en silencio.

**Corrección:** calcular un coeficiente estable para la duración del bloque —por ejemplo, equivalente al filtro por muestra— y restablecer/sanear el estado de la envolvente. Probar los extremos admitidos por el SDK, distintas frecuencias de muestreo y tamaños de bloque.

### 3. P2 — Cambiar de archivo deja el transporte y el modo directo desincronizados

**Ubicación:** [index.html:809](C:/DEV/projects/test/wasm/index.html:809), [index.html:897](C:/DEV/projects/test/wasm/index.html:897), [raydrone.rs:559](C:/DEV/projects/test/wasm/raydrone.rs:559).

`loadAudioBuffer()` silencia la densidad y cambia el botón a «Play», pero no pone `isPlaying = false` ni desactiva `DIRECT_ON`. Si se carga un archivo mientras suena Drone, el siguiente clic ejecuta `stop()` y hace falta otro para reproducir. Cambiar Carácter puede reactivar audio porque todavía encuentra `isPlaying = true`. En Original, una densidad cero no detiene la lectura directa y el archivo nuevo puede sonar pese al botón «Play».

**Evidencia:** ejecución de la función real con un decodificador simulado: después de cargar, `sendParams(false)`, `isPlaying === true`, botón `▶ Play`. La inspección del motor confirma que `set_sample()` no apaga `DIRECT_ON`. La prueba adicional con una muestra nueva silenciosa también encuentra salida de la cola anterior: pico `0.12876`; el reset no limpia los buffers de los efectos nuevos.

**Corrección:** hacer explícita y coherente la transición de transporte al sustituir una fuente; detener el modo directo y definir si se conservan o eliminan las colas. Cubrir carga durante Drone y Original, incluidos errores de decodificación.

### 4. P2 — Los identificadores de acordes no corresponden a las etiquetas

**Ubicación:** [index.html:312](C:/DEV/projects/test/wasm/index.html:312), [raydrone.rs:786](C:/DEV/projects/test/wasm/raydrone.rs:786), [core/src/lib.rs:293](C:/DEV/projects/test/core/src/lib.rs:293).

La interfaz y `render_demo.mjs` usan un catálogo distinto al del núcleo local. No es una diferencia de afinación menor: se elige otro acorde.

| Selección en la interfaz | ID | Ratios obtenidos del WASM |
| --- | --- | --- |
| Octavas | 1 | `1, 1.25, 1.5` — tríada mayor |
| Power pad | 2 | `1, 1.2, 1.5` — tríada menor |
| Tríada mayor | 3 | `1, 1.125, 1.5` — sus2 |
| Escala menor | 9 | `1` — unísono |

**Evidencia:** ratios leídos del registro de rayos del WASM real para cada preset.

**Corrección:** fijar un catálogo compartido y versionado de ID, nombre y ratios; probar todos los presets publicados, incluyendo el render de demostración.

### 5. P2 — Cutoff, resonancia y profundidad del filtro no implementan lo anunciado

**Ubicación:** [core/src/lib.rs:266](C:/DEV/projects/test/core/src/lib.rs:266), controles en [index.html:307](C:/DEV/projects/test/wasm/index.html:307).

El núcleo implementa un suavizado de primer orden con `alpha = cutoff / (cutoff + sampleRate)`. El parámetro no corresponde al corte en Hz mostrado. La resonancia solo multiplica toda la salida por `1 + resonance * 0.15`; no cambia los polos ni produce resonancia. El LFO presentado en octavas usa un factor lineal `1 + lfo/12`, sin transposición exponencial de frecuencia.

**Evidencia:** con cutoff de 1000 Hz, un tono de 220 Hz pierde **4.70 dB** respecto al bypass. Resonancia 1 frente a 0 cambia la ganancia aproximadamente **1.15 veces**, como predice el código.

**Corrección:** implementar el filtro y las unidades de modulación prometidas, o ajustar explícitamente el contrato de interfaz. Verificar respuesta en frecuencia, corte y resonancia; la prueba actual solo comprueba que un corte bajo atenúe algo.

### 6. P2 — El estimador redondeado no converge al objetivo triangular discreto

**Ubicación:** [raytrace_buffer.js:53](C:/DEV/projects/test/wasm/raytrace_buffer.js:53), [raytrace_buffer.js:81](C:/DEV/projects/test/wasm/raytrace_buffer.js:81); mismo patrón en `lab_target()`, `lab_estimate()` y el laboratorio JavaScript.

El objetivo usa pesos discretos `(A - |k|)/A²`, mientras que los rayos se obtienen redondeando una variable triangular continua. La masa de cada intervalo de redondeo no coincide con esos pesos. El defecto existe independientemente del error de `sqrtf` del hallazgo 1.

**Reproducción confirmada con la referencia JavaScript:** fuente `[0,1,0]`, foco 1, apertura 1, ventana `[1]`: objetivo **1**, estimación QMC con 262144 rayos **0.749996**. El límite teórico de esa estimación es 0.75, no 1.

**Impacto:** sesgo persistente con aperturas pequeñas; las comparaciones entre random/QMC/stratified y importance no usan exactamente la misma distribución objetivo.

**Corrección:** muestrear la distribución discreta del objetivo o calcular el objetivo con las masas integradas de los intervalos de redondeo. Probar aperturas 1 y 2, además de aperturas amplias.

### 7. P2 — El bypass del VST pierde NoteOff y deja notas retenidas

**Ubicación:** [vst/src/lib.rs:401](C:/DEV/projects/test/wasm/vst/src/lib.rs:401), [vst/src/lib.rs:363](C:/DEV/projects/test/wasm/vst/src/lib.rs:363).

**Hallazgo estático:** `process()` retorna en bypass antes del bucle que consume los eventos MIDI. Una nota activada antes del bypass y liberada durante él conserva `midi_held[note] = true` al volver. Además, `reset()` limpia el motor pero no la máscara MIDI del plugin, que se vuelve a copiar al motor en el bloque siguiente.

**Corrección:** mantener el estado MIDI al día durante bypass y limpiar notas en el reset del host. Comprobar NoteOn → bypass → NoteOff → salir de bypass y el reset del transporte en un DAW. No se ha ejecutado esta secuencia en un host nativo en esta auditoría.

### 8. P2 — Un fallo temporal de addModule impide reintentar el arranque

**Ubicación:** [index.html:740](C:/DEV/projects/test/wasm/index.html:740).

Se asigna `wasmBytes` antes de completar `audioWorklet.addModule()`, pero ambos pasos están bajo `if (!wasmBytes)`. Si el módulo falla, el siguiente intento omite su registro e intenta construir un procesador inexistente. Hace falta recargar la página.

**Reproducción confirmada con la función real y un AudioContext simulado:** primer intento «temporary module failure»; segundo intento «processor not registered»; `addModule` se invoca solo una vez.

**Corrección:** separar los estados de descarga, registro e instancia; compartir una promesa de inicialización y limpiar el estado fallido para permitir reintentos. También evita carreras entre carga automática y manual.

### 9. P2 — Reiniciar contadores del motor produce miles de millones de granos/s

**Ubicación:** [processor.js:225](C:/DEV/projects/test/wasm/processor.js:225), [raydrone.rs:377](C:/DEV/projects/test/wasm/raydrone.rs:377).

El motor pone `SPAWN_COUNT` y `SLOG_W` a cero al cargar una fuente o entrar en Original. El worklet conserva `lastSpawn` y `lastW`, y convierte la diferencia negativa a `u32`, interpretándola como un desbordamiento de contador.

**Reproducción confirmada ejecutando la clase real del procesador con WASM:** después de 16 bloques de Drone y 16 de Original, la telemetría comunica aproximadamente **20 132 656 792 granos/s**. El suavizado mantiene el valor erróneo durante más mensajes. El registro de rayos también interpreta el reset como datos nuevos y puede mostrar entradas antiguas.

**Corrección:** sincronizar los cursores y reiniciar el suavizado cuando el motor reinicia su estado, o usar contadores monotónicos/una generación explícita del protocolo.

### 10. P2 — El laboratorio mezcla resultados de una fuente con el audio de otra

**Ubicación:** [index.html:809](C:/DEV/projects/test/wasm/index.html:809), [index.html:2053](C:/DEV/projects/test/wasm/index.html:2053), [index.html:2149](C:/DEV/projects/test/wasm/index.html:2149).

**Hallazgo estático:** cargar un archivo no invalida `labTarget`, `labCurves`, `labF0` o `labA`, ni cancela una medición en curso. El worker conserva su copia de la fuente anterior, pero Noisy/Clean llaman a `labEstimate()` sobre el `sampleData` actual. Se puede medir A, cargar B y comparar el objetivo de A con una estimación de B. Un resultado del worker anterior también puede habilitar los botones después del cambio de fuente.

**Corrección:** ligar resultados y mensajes a una generación de fuente, invalidar/cancelar al cambiar de audio y conservar un snapshot de los datos usados en el A/B. Probar tanto un cambio durante la medición como después de terminarla.

### 11. P2 — El repositorio no contiene la dependencia necesaria para recompilarlo

**Ubicación:** [build.ps1:36](C:/DEV/projects/test/wasm/build.ps1:36), [build.sh:20](C:/DEV/projects/test/wasm/build.sh:20), [vst/Cargo.toml:31](C:/DEV/projects/test/wasm/vst/Cargo.toml:31).

WASM y VST dependen de `../core`, fuera de la raíz Git `C:/DEV/projects/test/wasm`. Existe en este equipo y se ha podido usar para compilar, pero no se entrega como fuente ni submódulo de este repositorio ni se indica una revisión reproducible para obtenerla. Un clon aislado no puede seguir las instrucciones de compilación. El `.rlib` versionado no resuelve la dependencia de fuente de los scripts ni la dependencia Cargo.

El README del VST también promete `.github/workflows/build-vst.yml` y releases al crear un tag, pero ese workflow no existe en este checkout.

**Corrección:** incluir el núcleo o declarar cómo obtenerlo con una revisión fija; añadir una compilación de CI desde checkout limpio y corregir las instrucciones de distribución. El contrato de acordes incompatible muestra por qué importa versionar el núcleo junto con sus consumidores.

## Verificación realizada y límites

| Comprobación | Resultado |
| --- | --- |
| `node test_engine.mjs` sobre el WASM distribuido | Pasa |
| Recompilación aislada de núcleo y WASM con los comandos equivalentes a los scripts | Pasa; no se sustituyó el binario distribuido |
| Suite del motor sobre el WASM recién compilado | Pasa, con los mismos resultados mostrados por la suite |
| `node test_acoustic.mjs` | Pasa; backend CPU |
| `node test_raytrace_buffer.mjs` | Pasa; backend CPU |
| Sintaxis de processor, worker, SDK e i18n | Pasa |
| Parseo de los dos scripts inline de index.html | Pasa |
| Reproducciones específicas de esta auditoría | Salidas guardadas en `.audit/resultados.txt` |
| `cargo test --manifest-path vst/Cargo.toml --locked --offline` | No ejecuta tests: falta el checkout de `nih_plug_xtask` en la caché local |
| Tests del motor VST mediante rustc sin Cargo | No enlazan: `link.exe` no está disponible en este entorno |

Las reproducciones de interfaz usan las funciones originales con dependencias simuladas; no equivalen a pruebas en un navegador. No se han validado Safari/iOS, un DAW real, la ejecución WebGPU, límites de tiempo real en dispositivos físicos ni vulnerabilidades publicadas de dependencias externas. La revisión de seguridad del código propio no sustituye ese análisis de dependencias.

Se revisaron también el SDK y la gestión de recursos GPU: conviene añadir validación explícita del esquema de escenas —actualmente `params` se propaga sin una lista de campos admitidos— y liberación de buffers/dispositivos GPU en rutas de éxito y error. Son mejoras de robustez adicionales; no se presentan aquí como exploits o fugas medidas.

El repositorio versiona `vst/target`, `.rlib` y `__pycache__`; deberían excluirse los artefactos de compilación. Los binarios distribuidos deliberadamente necesitan una política separada de publicación y correspondencia con el código.

## Evidencias y siguiente orden de trabajo

Ejecutar desde la raíz: `node .audit/reproduce.mjs`. El script imprime reproducciones diagnósticas y no modifica archivos de producción; no es una suite de regresión que deba pasar con el código corregido.

- [Script de reproducción](.audit/reproduce.mjs).
- [Resultados capturados](.audit/resultados.txt).

Primero corregir la raíz cuadrada y la estabilidad de la envolvente. Después fijar el contrato del núcleo —acordes, filtro y distribución objetivo— y regenerar las mediciones. A continuación resolver transporte, inicialización, MIDI y generación de resultados del laboratorio. Finalmente validar la compilación desde un checkout limpio y completar pruebas de navegador, GPU y DAW.
