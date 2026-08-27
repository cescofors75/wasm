# RayDrone SDK v1

RayDrone usa materiales sonoros como unidad creativa: una superficie define cómo
se comportan los rayos antes y durante sus reflexiones. El espacio se sintetiza
con trayectorias estéreo de distinta longitud, pérdida y absorción de material;
no usa convolución ni una reverb algorítmica clásica. El SDK publica
paquetes declarativos; el audio sigue ejecutándose dentro del WASM/AudioWorklet.
Así un paquete externo no puede bloquear el hilo de audio.

## Usar el SDK

Carga `sdk/raydrone-sdk.js` después de crear el `AudioWorkletNode` y pasa
`node.port` a cada llamada:

```js
RayDroneSDK.registerMaterial('cristal-metal', {
  base: RayDroneSDK.Material.CRYSTAL,
  amount: 0.82,
  modulation: { mode: 1, target: 1, rate: 0.18, depth: 0.32, attack: 0.05, release: 0.7 },
  effects: { delayWet: 0.18, delayTime: 0.47, delayFeedback: 0.42,
             chorusWet: 0.12, chorusRate: 0.16, chorusDepth: 0.009, reverbWet: 0.46 }
});

RayDroneSDK.registerScene('camara-de-cristal', {
  material: 'cristal-metal',
  params: { focus: 2.4, aperture: 0.18, grainMs: 340, grainRate: 145, gain: 0.28, master: 1 }
});

RayDroneSDK.applyScene(node.port, 'camara-de-cristal');
```

## Materiales integrados

| Constante | Comportamiento |
| --- | --- |
| `VACUUM` | No modifica la energía del rayo. |
| `METAL` | Realza transitorios y armónicos altos. |
| `WOOD` | Resonador cálido que concentra medios. |
| `CRYSTAL` | Lectura limpia con granos más largos. |
| `WATER` | Movimiento filtrado y suave. |
| `PLASMA` | Saturación espectral controlada y microdesafinación. |

Los materiales de terceros se componen sobre uno de estos núcleos mediante
modulación y efectos. Para crear un algoritmo DSP nativo nuevo hay que añadirlo
al motor Rust y asignarle un identificador de material estable; no se ejecuta JS
arbitrario dentro del AudioWorklet.
