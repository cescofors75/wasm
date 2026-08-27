# Future directions — where (and where not) the idea generalizes

> Honest map of where RayDrone's core move transfers. The goal is **rigour, not
> hype**: a framework that "fits everything" distinguishes nothing. This file
> exists so the paper's *Generalization* section can be written with discipline,
> and so good directions are not lost.

## The transferable kernel

Strip RayDrone to its abstract structure and you get a recipe, not a sound:

1. The thing you want to produce is a **deterministic integral / expectation**
   over many contributions: `g = ∫ p(τ) · f(τ) dτ`.
2. You **estimate** it with `N` stochastic samples → unbiased, error `∝ 1/√N`.
3. You **reduce variance** with the graphics toolkit: stratification, QMC,
   importance sampling (reweighted `p/q`), Russian roulette.
4. Honest counterpoint: rejection **without** reweighting is *biased* — a
  different target, not a faster one. (The reproducible synthetic reverse curve illustrates it.)

**Two layers, only one is ours.**
- The *mathematics* (Monte Carlo + variance reduction) is one of the most
  universal patterns in science. It is **already deployed** almost everywhere.
  "It fits domain X" is usually not a discovery — X already uses it.
- The *creative move* — taking a **stored medium** and treating its *rendering*
  as stochastic transport over a **non-spatial axis**, exposed as an interactive,
  **measurable** instrument — is the part that is genuinely ours and might travel.

> ⚠️ Discipline: do **not** dilute the audio paper by claiming six domains. Finish
> one measurable result; devote *one honest paragraph* to generalization that
> cites where the physical version already exists.

## Verdict per domain

Legend — **Math already there?** (is Monte Carlo already standard in the field) ·
**Room for our move?** (does the interactive/measurable/axis-swap angle add
something) · **Verdict**.

### 🔊 Audio — spectral & video siblings  ·  *best next steps*
- **Spectral RayDrone**: sample FFT **frequency bins** instead of time offsets
  (stochastic spectral freeze). Focus = centre frequency, aperture = bandwidth,
  importance ∝ spectral energy (reweighted). Reuses ~90% of the engine and the
  Lab. **This is paper 2.**
- **Video textures**: Schödl et al. 2000 already do stochastic frame transitions;
  our estimator drops in directly (focus on the timeline, N video "rays"). A
  browser "RayDrone for video" would be a stunning demo, identical math.
- Verdict: **strongest real extensions.** Same kernel, open ground, we can ship an
  artifact.

### 🏥 Medicine — literal, but already owned by experts
- Radiotherapy dose = Monte Carlo of **photon/proton transport** — the *same*
  transport equation as light. PET/CT/SPECT reconstruct with MC + variance
  reduction. Drug-binding free energy uses importance/umbrella sampling.
- Math already there: **yes, 40 years deployed.** Room for our move: **low** —
  hard to out-contribute medical physicists.
- Verdict: **sibling, not a branch.** Our idea and radiotherapy share a parent
  (radiative transport). Use as a *framing* ("the physical instance lives in
  medicine"), not a project, unless paired with a domain expert.

### 🏛️ Architecture — the closest relative; our best citation
- Room acoustics and daylighting design **are** ray tracing + Monte Carlo. There
  is a formal **"acoustic rendering equation"** (Siltanen et al. 2007) — Kajiya
  applied to sound propagation in a room.
- Verdict: **best citation, not best branch.** It integrates over *spatial
  geometry*; we integrate over the *time axis of a buffer*. Citing it positions
  our temporal/instrumental variant as distinct and serious.

### 🔐 Security — conceptual fit, less crowded
- The strong piece: **importance sampling for rare events** — estimate the
  probability of a very unlikely failure (an exploit path, a crypto collision) by
  sampling *toward* the risk, reweighted (unbiased). Coverage-guided fuzzing is
  importance sampling over the input space.
- Verdict: **real room**, but it is sampling theory, not "granularity". A separate
  line if ever pursued; note the conceptual link only.

### 🧠 Psychology — inspiration, not technical transfer
- Real science exists: the **sampling hypothesis** of cognition (Vul, Griffiths;
  Sanborn & Chater) and the **Bayesian brain** / predictive coding — perception as
  inference by *sampling hypotheses and converging*, structurally echoing our
  drone settling as N grows.
- Verdict: **gold for the discussion / "why it moves us emotionally" angle**,
  dangerous as a technical claim. Keep it analogical and clearly labelled
  speculation.

## One-paragraph generalization (draft for the paper)

> The construction here is an instance of a general recipe: any quantity defined
> as an integral over a medium can be *rendered* by Monte Carlo estimation with
> variance reduction. The **physical** instance of this recipe is already mature
> in medicine (Monte Carlo radiotherapy dose, transport of photons/protons) and
> in architecture (room-acoustic and daylight ray tracing, the acoustic rendering
> equation of Siltanen et al.). Our contribution is to apply it not to physical
> propagation through space but to the **time axis of a recorded signal**, turning
> a deterministic transport integral into a playable, *measurable* instrument. The
> same move extends naturally along other axes of stored media — spectral bins,
> video frames — and connects, more speculatively, to sampling accounts of
> perception in cognitive science.

## Key references to chase

- Kajiya, J. *The Rendering Equation.* SIGGRAPH 1986.  *(origin)*
- Veach, E. *Robust Monte Carlo Methods for Light Transport.* PhD 1997.  *(MIS, IS)*
- Siltanen, Lokki, Kiminki, Savioja. *The room acoustic rendering equation.* JASA 2007.  *(closest sibling)*
- Schödl, Szeliski, Salesin, Essa. *Video textures.* SIGGRAPH 2000.  *(video extension)*
- Vul, Goodman, Griffiths, Tenenbaum. *One and done? …* Cognitive Science 2014.  *(sampling cognition)*
- Sanborn & Chater. *Bayesian brains without probabilities.* TiCS 2016.  *(psychology framing)*
- Roads, C. *Microsound.* 2001 · Truax 1988 · Gabor 1947.  *(granular lineage)*
- (Security) rare-event simulation / importance sampling; coverage-guided fuzzing.

---

*Bottom line: finish the audio paper. Spectral and video are the only branches
worth building soon; medicine/architecture are citations; psychology is a
discussion paragraph. Breadth in one honest paragraph beats six shallow claims.*
