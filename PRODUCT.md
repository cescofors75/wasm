# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

RayDrone sirve a dos públicos: músicos y productores que quieren cargar audio y obtener un resultado sonoro con rapidez, y especialistas DSP que quieren estudiar y controlar el motor con más profundidad.

## Product Purpose

RayDrone es un instrumento granular de trazado acústico. Permite cargar una fuente de audio, transformarla mediante materiales, rayos, movimiento y espacio, y escuchar el resultado en tiempo real. El éxito consiste en que la primera experiencia sea inmediata y musical sin ocultar la capacidad técnica del motor.

## Positioning

El sonido se genera con un motor DSP escrito en Rust y ejecutado mediante WebAssembly dentro de un AudioWorklet. Su vocabulario creativo se organiza alrededor de materiales sonoros y trayectorias de rayos, no de una cadena convencional de efectos aislados.

## Operating Context

La interfaz se usa en navegador sobre HTTP local o HTTPS. El flujo principal consiste en cargar un WAV o MP3, iniciar la reproducción, elegir material y carácter, y ajustar movimiento, espacio y comportamiento de rayos. Los entornos Básico, Medio y Profesional exponen distintas profundidades sobre el mismo estado sonoro.

## Capabilities and Constraints

- DSP local en tiempo real mediante Rust, WebAssembly y AudioWorklet.
- Una misma escena y estado sonoro al cambiar entre Básico, Medio y Profesional.
- Materiales, control granular, geometría de rayos, modulación, espacio, afinación y exportación WAV.
- El análisis matemático, Convergence Lab y las comparativas técnicas pertenecen a una zona avanzada separada del flujo instrumental.
- Debe conservarse el funcionamiento sin dependencias de interfaz externas y la compatibilidad responsive.

## Brand Commitments

El producto se llama RayDrone. Su identidad combina instrumento sonoro experimental, laboratorio DSP contemporáneo y tecnología local verificable. La interfaz debe sentirse moderna y profesional, con una dirección “LABS 2026”, sin convertir la experiencia principal en documentación técnica.

## Evidence on Hand

- Motor y pruebas funcionales en `raydrone.rs` y `test_engine.mjs`.
- Interfaz funcional existente en `index.html`.
- Auditoría técnica en `AUDITORIA.md`.
- SDK de materiales y escenas en `SDK.md`.
- Resultados y figuras de investigación en `paper/`.

No hay testimonios, clientes, métricas comerciales ni claims externos que deban inventarse en la interfaz.

## Product Principles

1. Sonido útil antes que complejidad técnica.
2. La profundidad se revela de forma progresiva sin cambiar la escena.
3. Cada control debe tener una consecuencia audible y comprensible.
4. El análisis matemático demuestra el motor, pero no interrumpe el acto de tocar.
5. La interfaz debe comunicar precisión, experimentación y funcionamiento local.

## Accessibility & Inclusion

La interfaz debe ser operable con teclado, legible en pantallas pequeñas, respetar `prefers-reduced-motion` y mantener contraste suficiente en sus temas.
