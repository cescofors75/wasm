# paper/ — hacia un artículo (DAFx)

Esta carpeta es para convertir RayDrone en un **artículo científico**. La idea,
en una frase: *la síntesis granular asíncrona es un estimador Monte Carlo de una
integral de transporte sobre el eje temporal, igual que un píxel en la ecuación
de rendering — y las técnicas de reducción de varianza de los gráficos transfieren
y mejoran la convergencia de forma medible.*

- **`raydrone-dafx.md`** — borrador/esqueleto del paper (en inglés, con notas 💡
  en español que borraremos antes de enviar).

## Tus dudas, respondidas en corto

**¿Esto es publicable de verdad o me estoy flipando?**
Es publicable. No porque "suene bonito", sino porque la analogía es *medible*: el
Convergence Lab ya muestra el 1/√N y que las estrategias mejoran la pendiente. Eso
es una contribución concreta y reproducible. Lo que NO hay que afirmar es que
simulas física acústica (no lo haces) — y como lo decimos nosotros en la sección
de límites, jugamos limpio.

**¿No es "solo" síntesis granular renombrada?**
No. La síntesis granular existe; lo nuevo es el **marco** (verla como estimación
MC) y el **resultado** (que el toolkit de varianza de gráficos transfiere y se
mide). Ese cambio de lente es exactamente el tipo de aportación que valora DAFx.

**¿Y si un revisor me dice que la analogía es superficial?**
Por eso la Sección 3 pone las DOS fórmulas (`g[n]` y `ĝ_N[n]`): no es una metáfora,
es la misma clase de estimador en la formulación offline sincronizada. La Sección
5.3 (trazado inverso sesgado) distingue converger al objetivo de cambiarlo.

## Dónde enviarlo
- **DAFx** — encaje natural (efectos/síntesis digital). *(primera opción)*
- **SMC** (Sound and Music Computing), **ICMC** (computer music).
- *Computer Music Journal* (MIT Press) para versión extendida.

## Qué falta (checklist)
- [x] **Export CSV del Convergence Lab** (botón ⬇ CSV en la página) — los runs
      1–4 de `data/` salieron de ahí.
- [x] Correr el Lab sobre samples distintos y tabular pendientes → `RESULTS.md`.
- [ ] Fig. 1: diagrama píxel/rayo ↔ foco/grano (ya tenemos el texto "genesis").
- [ ] Fig. 2: distribuciones de puntos de las 4 estrategias sobre la apertura.
- [x] Fig. 3: gráfica log-log de convergencia con la línea ideal 1/√N
      (`figures/run1…run4*.png`, vía `make_figures.py`).
- [x] Fig. 4: Reverse (se aplana) vs Importance (converge) → sesgo. Hecho con el
      run 5 sintético (`node paper/make_bias_run.mjs` →
      `figures/run5-bias-ap91-foc2.0.png`). Ojo al matiz honesto: en música real
      (runs 1–4, históricos) reverse ≈ random en pendiente; los dos
      regímenes están redactados en el §5.3.
- [ ] Redactar Sec. 3 (núcleo matemático) en limpio.
- [ ] Pasar a plantilla LaTeX de DAFx cuando el contenido esté cerrado.
- [ ] Más trials (16–32) en los runs 1–4 y confianza sobre la pendiente ajustada
      (el NaN del estimador importance a N grande ya está arreglado; se puede
      subir N sin miedo).

> Sin prisa y con rigor. El código y el Lab ya son nuestra "sección de
> reproducibilidad"; el resto es contar bien lo que ya hicimos.
