# From Pixels to Grains: Variance-Reduced Monte Carlo as a Unifying Framework for Granular Synthesis

**Target venue:** DAFx (International Conference on Digital Audio Effects) — short/long paper.
**Status:** working draft / outline.

---

## Abstract

*(~150 words — draft)*

Asynchronous granular synthesis scatters short, windowed excerpts of a source
signal in time. We formulate an offline, synchronized grain cloud as a **Monte
Carlo estimator of a transport integral** over the time axis of the source
buffer, using the same class of numerical estimator used for the *rendering equation* in
computer graphics, where each pixel integrates incident light by tracing N
random rays. Under this view a grain is a ray, the playback focus is a pixel,
and the grain density N controls estimator variance: the texture converges to a
deterministic target with error proportional to 1/√N. We show that the
variance-reduction techniques of physically-based rendering — **stratified
sampling, quasi-Monte Carlo (QMC), and importance sampling** — can reduce
signal-domain error in this offline formulation. We further show that *source-energy reverse tracing*,
a tempting "improvement", is a **biased** estimator of a different target, which
explains its distinct timbral character. We provide an open, dependency-free
Rust/WebAssembly implementation and an in-browser convergence laboratory. A
synthetic experiment is reproducible from source; real-audio pilot data are
retained as archival evidence pending regeneration with the corrected estimator.

---

## 1. Introduction

- Granular synthesis (Gabor → Roads → Truax) as established practice; usually
  framed perceptually ("clouds of sound"), rarely with an estimation-theoretic
  lens.
- Computer graphics solved the *same shape of problem* — estimating a high-
  dimensional integral by random sampling — and built 30 years of variance-
  reduction theory around it.
- **Contribution:**
  1. A formal mapping: granular synthesis ≡ Monte Carlo estimation of a temporal
     transport integral (Sec. 3).
    2. Direct transfer of three graphics variance-reduction methods, with measured
      convergence in an offline laboratory (Sec. 4–5).
  3. An honest treatment of *reverse tracing* as a biased estimator (Sec. 5.3).
  4. A reproducible open implementation + browser-based Convergence Lab (Sec. 6).

---

## 2. Background

### 2.1 The rendering equation and Monte Carlo integration
The pixel value is L(x,ω) = Lₑ + ∫_Ω f_r(x,ω,ω') L_i(x,ω') (ω'·n) dω', estimated by
tracing N rays and averaging. Error of a standard MC estimator falls as σ/√N.

### 2.2 Granular synthesis
Asynchronous grain clouds: grains of duration D, windowed by w(·), drawn around a
read position (the *focus*) within a spread (the *aperture*), at density N.

---

## 3. Granular synthesis as a temporal transport integral  ← **núcleo del paper**

Let `s[·]` be the source buffer, `f` the focus (samples), and `τ` an offset drawn
from a probability density `p(τ)` supported on the aperture `[-A, A]` (e.g. the
triangular kernel we use). Define the **target grain texture** as the windowed
expectation of shifted copies of the source:

```
g[n] = w[n] · E_{τ~p}[ s[f + τ + n] ]
     = w[n] · ∫ p(τ) · s[f + τ + n] dτ ,    n = 0 … D-1
```

A cloud of N grains is precisely the **Monte Carlo estimator** of this integral:

```
ĝ_N[n] = w[n] · (1/N) Σ_{i=1}^{N} s[f + τ_i + n] ,     τ_i ~ p
```

The estimator is unbiased by linearity of expectation (`E[ĝ_N] = g` for every
N), with per-sample variance σ²/N; the central limit theorem then gives the
convergence rate, so the RMS error over the grain falls as

```
‖ ĝ_N − g ‖ ∝ σ / √N .
```

**The correspondence (Table 1):**

| Computer graphics | RayDrone (audio) |
|---|---|
| Pixel | Playback focus `f` |
| Ray / light path | Grain |
| Hemisphere integral | Aperture integral over `τ` |
| Samples per pixel N | Grain density N |
| Pixel noise (variance) | Textural "grain"/roughness |
| Convergence ∝ 1/√N | Texture settling ∝ 1/√N |

This result applies to the synchronized finite estimator above. A live
asynchronous granular process additionally includes random launch times,
overlapping grains, stateful effects, and control-rate changes; its stochastic
process is not derived here.

---

## 4. Transferring variance reduction

For each method: one paragraph (what it is in graphics) + how it maps to grains.

### 4.1 Stratified sampling
Partition `[-A,A]` into N strata, draw one `τ` per stratum → removes clumping,
variance falls faster than pure random.

### 4.2 Quasi-Monte Carlo (QMC)
Golden-ratio additive recurrence (`τ_i = frac(τ_0 + i·φ)·2A − A`) with
Cranley–Patterson rotation → low-discrepancy coverage of the aperture.

### 4.3 Importance sampling
Draw `τ ∝ q(τ)` where `q` follows local source energy, and **reweight by
`p(τ)/q(τ)`**. This is the key: reweighting keeps the estimator *unbiased* while
concentrating samples where the integrand is large.

---

## 5. Evaluation: the Convergence Lab

### 5.1 Methodology
- Deterministic **target** = full weighted sum `g[n]` (all taps in the aperture).
- For N ∈ {1,2,4,…,4096}, compute `ĝ_N` and measure RMS error vs target.
- Average over T independent trials; fit the slope in log–log space (`log RMS`
  vs `log N`). Ideal random estimator → slope −0.5.
- Record the number of trials and estimator implementation used for every run.

### 5.2 Results (real audio)
The corrected implementation produces the expected qualitative behaviour on the
reproducible synthetic run:
- Random ≈ −0.5 (matches theory).
- Stratified / QMC / importance: steeper (faster convergence).

Runs 1–4 are archival pilot runs produced before the importance-normalization
and reverse-sampling corrections. They are useful for exploratory comparison but
are not final measurements of the current implementation. They must be
regenerated from provenance-recorded audio before submission.

### 5.3 Reverse tracing is biased — and that is the point
Sampling proportional to source energy *without* reweighting does **not**
estimate `g[n]`; it estimates a different, energy-weighted target. The estimator's
expected squared error decomposes into squared bias plus estimator variance. The
reported metric averages per-trial RMS, so it should not be identified directly
with the square root of expected MSE at small trial counts.

- **On archival musical-material runs** (runs 1–4), reverse tracked random in
  fitted slope (means −0.496 and −0.490). Because these runs used the earlier
  implementation and only four trials per point, no significance or noise-floor
  claim is made.
- **On strongly structured material** (run 5: a source whose aperture straddles a
  loud/near-silent boundary), the bias term dominates at large N and the curve
  approaches a floor: between N = 8192 and 32768 reverse's local slope is about
  −0.10 while random's is about −0.45. At N = 32768, reverse error is 3.22·10⁻³
  and reweighted importance error is 9.67·10⁻⁶, about 333× lower in this
  constructed condition.

We present the pair as a worked example of bias vs. variance, and argue reverse
tracing is a *timbral* choice (it renders the energy-weighted texture, which can
sound fuller), not a quality improvement.

---

## 6. Implementation

- `no_std` Rust, no crates, compiled to `wasm32-unknown-unknown`; runs in an
  AudioWorklet, mixing per sample (no per-grain Web Audio nodes, no scheduler
  jitter).
- Continuous grain cloud, equal-power panning, cubic (Catmull-Rom) interpolation,
  per-grain micro-detune, Freeverb-lite stereo reverb, output DC blocker.
- In-browser Convergence Lab reproduces all figures; performance diagnostics
  (audio-thread CPU, active voices, grains/s, latency).
- **Lab vs. live instrument — an honest distinction.** The Lab measures the
  *canonical* forms of each sampler: N-strata stratification, f64 golden-ratio
  Kronecker sequence with Cranley–Patterson rotation, and reweighted (`p/q`,
  unbiased) importance sampling. The real-time instrument ships cheaper
  *streaming* variants of the first two — an iterative f32 golden recurrence
  without rotation, and a fixed 17-stratum round-robin (which improves the
  constant but is asymptotically slope −0.5) — and its "smart rays" mode is
  precisely the **biased reverse** sampler of Sec. 5.3, not the unbiased
  importance sampler. All convergence figures characterise the canonical forms;
  no measured convergence claim is made here for the streaming variants.
- Open source (MIT license).

---

## 7. Discussion & limitations (honest)

- This is a **mathematical/algorithmic** correspondence, **not** a physical
  simulation of acoustic wave propagation. The integral is over the *time axis of
  one buffer*, not over a room's geometry.
- Terms like "chromatic aberration" and "bounces" are perceptual metaphors built
  on top of the estimator, not claims about physics.
- Evaluation is signal-domain (RMS convergence); a perceptual listening study is
  future work.
- Runs 1–4 lack source-audio provenance and predate estimator corrections; they
  are excluded from confirmatory claims until regenerated.
- Current CSVs contain means rather than per-trial observations. Confidence
  intervals and inferential comparisons require per-trial exports and more runs.

## 8. Future work
Multiple importance sampling (MIS) combining `p` and energy-`q`; temporal "BRDF"
kernels; adaptive N driven by a perceptual error metric; listening tests.

## 9. Conclusion
An offline synchronized granular texture can be formulated as Monte Carlo
transport on the time axis. In the evaluated synthetic condition, stratified,
QMC, and properly reweighted importance estimators reduce signal-domain error,
while unweighted energy sampling converges toward a different timbral target.

---

## References
- J. T. Kajiya, “The Rendering Equation,” *Proceedings of SIGGRAPH*, 1986.
- E. Veach, *Robust Monte Carlo Methods for Light Transport Simulation*, Ph.D.
  dissertation, Stanford University, 1997.
- C. Roads, *Microsound*. MIT Press, 2001.
- B. Truax, “Real-Time Granular Synthesis with a Digital Signal Processor,”
  *Computer Music Journal*, vol. 12, no. 2, 1988.
- D. Gabor, “Acoustical Quanta and the Theory of Hearing,” *Nature*, vol. 159,
  1947.
- R. Cranley and T. N. L. Patterson, “Randomization of Number Theoretic Methods
  for Multiple Integration,” *SIAM Journal on Numerical Analysis*, vol. 13,
  no. 6, 1976.
