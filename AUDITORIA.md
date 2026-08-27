# Auditoría técnica de RayDrone WASM

Fecha: 18 de julio de 2026.

## Resultado

El motor compila en local, la suite funcional completa pasa y la página carga
sin errores de consola. La ruta `Original · Ray direct` y la granular atraviesan
el mismo DSP de materiales, filtro, modulación y espacio.

## Corregido en esta revisión

- **Contaminación entre efectos:** el feedback de Delay se aplicaba al buffer
  compartido aunque Delay estuviera al 0 % cuando Chorus o Flanger estaban
  activos. Era una fuente de resonancia no solicitada. El feedback ahora solo
  existe si Delay está audible y hay una prueba de regresión dedicada.
- **DSP desperdiciado en bypass:** Reverb recorría cuatro trayectorias por
  muestra y Delay/Chorus/Flanger calculaban tres taps y dos LFO aunque todos los
  wet fueran cero. Los bypasses conservan historial directo, pero evitan esos
  cálculos.
- **Basura en el AudioWorklet:** las vistas de salida WASM, los arrays de rayos
  y la constelación de focos se reutilizan. Se evita crear dos vistas por quantum
  y varios arrays en cada envío de telemetría.
- **Lógica de entornos:** los niveles ya no son el mismo tablero con paneles
  ocultos. Básico usa materiales y cuatro macros; Medio muestra una tarea cada
  vez y solo parámetros primarios; Profesional mantiene toda la profundidad en
  seis áreas de trabajo.
- **Responsive:** el header móvil desbordaba horizontalmente y los títulos de
  panel con un único `span` desaparecían. Ambos defectos están corregidos.

## Arquitectura de interacción

- **Básico:** Material + Carácter + Movimiento + Espacio + Volumen, conservando
  Zoom, medidor y señal de salida para poder escuchar y ver el resultado. Las
  macros escriben en los parámetros reales; no existe un motor simplificado
  paralelo.
- **Medio:** Material, Espacio y Rayos. En Espacio se ven ocho controles
  primarios en lugar de la matriz de parámetros completa.
- **Profesional:** Material, Espacio, Rayos, Afinación, Analizar y Directo. Solo
  el inspector de la tarea activa está visible.
- Cambiar de entorno conserva la escena y el sonido.

## Riesgos y siguientes mejoras

1. **Comparación FX aún incompleta:** la sección convencional vs RayDrone es una
   explicación de modelos y solo RayDrone aporta telemetría real. Para cumplir
   el A/B completo faltan una ruta convencional audible equivalente, conmutación
   nivelada y medición aislada de CPU, memoria y latencia.
2. **Cambio de modelo sin crossfade:** entrar o salir de `Ray direct` reinicia el
   estado de voces deliberadamente. Un crossfade corto evitaría cualquier click
   en material muy transitorio.
3. **Memoria fija:** el sample y los buffers DSP son estáticos para garantizar
   tiempo real sin asignaciones. Es robusto, pero reserva el máximo también para
   audios cortos; una futura arena dimensionada al cargar reduciría memoria en
   móvil.
4. **Mantenibilidad web:** `index.html` concentra interfaz, audio, visualización
   y laboratorio. El siguiente refactor debería separar estado/acciones,
   componentes de interfaz y render gráfico sin cambiar el protocolo del SDK.
5. **Benchmark reproducible:** la telemetría de CPU del AudioWorklet es útil para
   diagnóstico, pero no sustituye una batería repetible por navegador, sample
   rate y tamaño de bloque.

## Verificación

- `build.ps1`: genera `raydrone.wasm` correctamente.
- `node test_engine.mjs`: todas las pruebas pasan, incluida la regresión de
  feedback oculto y el A/B audible de materiales/FX.
- `node --check processor.js`: correcto.
- Scripts inline de `index.html`: parsean correctamente.
- Navegador: Básico/Medio/Profesional, materiales y macros comprobados; sin
  errores de consola y sin overflow horizontal a 375 px.
