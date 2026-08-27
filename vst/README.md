# RayDrone VST

A **VST3 / CLAP** build of [RayDrone](../README.md). A continuous cloud of
stochastic grains ("rays") is cast around a focal point inside a "scene"; the
drone is not stored in the scene — it **emerges** from the convergence of N rays.

It works three ways:

- **Ambient effect on a track** — *Live input* captures the track audio as the
  scene and the rays reshape it into a drifting texture (set the **Mix**).
- **Instrument** — built-in scenes (Pad/Choir/Bell/Noise) or a loaded WAV.
- **Playable** — incoming **MIDI**, the **on-screen piano**, or your **computer
  keyboard** pitch the drone; hold notes to play chords.

Both recursions of the original are present: the **autoevolution** control
feedback *and* the **recursive ray bounces** (Russian roulette).

Built with [nih-plug](https://github.com/robbert-vdh/nih-plug) (Rust). The DSP
is a per-instance port of `wasm/raydrone.rs` in [`src/engine.rs`](src/engine.rs).

## Controls

| Knob | Optical analogy | Effect |
|---|---|---|
| **Density** | Number of rays (N) | Grains per second. More → smoother, richer render |
| **Aperture** | Depth of field | Width of the temporal dispersion cone (ms). Narrow = tonal, wide = drone |
| **Focus** | Camera focal point | Position in the scene the rays are cast around |
| **Reverb** | — | Freeverb-lite wet mix |
| **Evolve** | Recursive feedback | Autoevolution: the output envelope feeds back into focus & aperture so the drone drifts and breathes on its own |
| **Shimmer** | Octave transport | Probability a grain reads an octave up → airy, bright sheen |
| **Bounce** | Recursive transport depth | How many times a dying grain relaunches a child grain (Russian-roulette bounces — recursive rays) |
| **Reflect** | Path survival probability | Probability each bounce survives → tail energy & length (the Neumann series) |
| **Mix** | — | Dry/Wet: blend of the original signal and the rendered drone |
| **Master** | — | Output level |

Plus a **Bypass** toggle (header) and a **Live input** source mode (process the
track audio instead of a loaded scene).

### Sound source (the "scene")

The plugin starts **already making sound** with a built-in **Pad** scene — no
file needed. Pick a different built-in (**Pad / Choir / Bell / Noise**) for an
instant tonal or textural source, or load your own audio with **Load WAV…**. The
choice (built-in name or WAV path) is saved with the DAW project and recalled on
reload.

> The **Load WAV…** dialog opens on a background thread on purpose: opening a
> native file dialog directly inside the plugin UI loop crashes some hosts
> (Reaper on macOS in particular). If you ever built an earlier version that
> crashed on load, this is the fix.

### Presets (the classic "Simple mode")

Three one-click character presets move Density + Aperture + Reverb + Evolve +
Shimmer together (Focus and Master are left to you):

- **Tonal** — narrow aperture, coherent, almost pitched.
- **Drone** — wide aperture, dense, evolving atmosphere.
- **Shimmer** — bright, octave-sprinkled, reverberant and drifting.

## The visualizer

The top panel renders the methodology live: a focal "camera" at the apex casts
**rays** down onto the scene timeline. The translucent **dispersion cone** is the
depth of field (aperture); each ray lands where a grain is actually fired, newest
rays brightest, colored from accent (near focus) to cyan (far). The whole field
glows with the output level and **drifts with Evolve** — you can see the
autoevolution sweeping the focus and breathing the aperture.

Right below it, the **Output** panel shows what you actually *hear*: an
oscilloscope over the rendered signal's spectrum (post-mix, post-reverb,
post-soft-clip) — the convergence of the rays, not just where they land.

## What it keeps vs. drops

**Keeps** the core methodology: continuous grain cloud, golden-ratio
quasi-Monte Carlo sampling, triangular dispersion (depth of field), Catmull-Rom
interpolation, per-grain micro-detune, equal-power stereo spread, Freeverb-lite,
DC blocker and soft clip.

**Drops** (to stay focused): chromatic aberration, ambient foci, microtonal
scales, inverse tracing and the Convergence Lab. Those live in the full WASM
build; recursive bounces and autoevolution are included here.

## Build

Requires a recent stable Rust toolchain.

```sh
cd vst
cargo xtask bundle raydrone --release
```

The bundle is written to `target/bundled/`:

- `RayDrone.vst3` — copy to your VST3 folder
  (Linux: `~/.vst3`, macOS: `~/Library/Audio/Plug-Ins/VST3`,
  Windows: `%COMMONPROGRAMFILES%\VST3`).
- `RayDrone.clap` — copy to your CLAP folder
  (Linux: `~/.clap`, macOS: `~/Library/Audio/Plug-Ins/CLAP`,
  Windows: `%COMMONPROGRAMFILES%\CLAP`).

A plain `cargo build --release` also produces the raw shared library under
`target/release/`, but the `xtask bundle` step is what creates the proper
`.vst3` / `.clap` plugin folders.

### macOS (tested target)

```sh
cd vst
cargo xtask bundle raydrone --release
```

This drops `RayDrone.vst3` and `RayDrone.clap` in `target/bundled/`. Copy them to:

- `~/Library/Audio/Plug-Ins/VST3/`
- `~/Library/Audio/Plug-Ins/CLAP/`

Then rescan in your DAW (Reaper, Waveform, Bitwig, Ableton Live 10.1+…).

#### Which DAW on macOS? (free options)

⚠️ **GarageBand and Logic do _not_ load VST3 or CLAP** — they only accept Apple
**Audio Units (AU)**. This plugin is VST3 + CLAP, so it will _not_ appear in
GarageBand. To use it you need a host that loads VST3. Free options for Mac:

- **Reaper** — <https://reaper.fm>. Not strictly free, but the full version runs
  forever on an unlimited evaluation. Best all-round choice for loading and
  testing VST3 instruments. **Recommended.**
- **Tracktion Waveform Free** — <https://www.tracktion.com/products/waveform-free>.
  A genuinely free, full DAW that loads VST3.
- **Carla** — <https://kx.studio/Applications:Carla>. A free, lightweight plugin
  _host_ (not a full DAW); the fastest way to just open the plugin and hear it.

In any of them: add RayDrone on an instrument track, click **Load WAV…**, and the
drone plays continuously (no MIDI notes needed).

> Want it in GarageBand/Logic anyway? That requires an **AU** build, which
> nih-plug doesn't produce directly — it can be wrapped from the CLAP with
> [clap-wrapper](https://github.com/free-audio/clap-wrapper) (CMake + Xcode).
> Ask and it can be scaffolded.

**Universal binary (Apple Silicon + Intel)** — build a fat plugin so it runs on
both architectures:

```sh
rustup target add x86_64-apple-darwin aarch64-apple-darwin
cargo xtask bundle-universal raydrone --release
```

Notes for macOS:

- The GUI uses OpenGL via `baseview`; no extra system packages are needed
  (unlike Linux, which needs the X11/GL `-dev` headers to build).
- The plugin is **unsigned**. If Gatekeeper blocks it, clear the quarantine flag:
  `xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/RayDrone.vst3`.
- It's a **stereo effect that also takes MIDI**: drop it on a track with audio
  and choose *Source ▸ Live input*, or use a built-in scene / WAV and play it
  from a MIDI keyboard, the on-screen piano, or your computer keys.

## Standalone app — play with your laptop keyboard

Inside a DAW the host grabs your computer keystrokes for its own shortcuts, so
the **computer keyboard usually can't reach a plugin** (especially on macOS).
The fix is the **standalone app**: a normal OS window where the keyboard works.

```sh
cd vst
cargo run --release --features standalone --bin raydrone-standalone
```

This opens RayDrone as a desktop application. The computer keyboard plays the
piano (a row from C4): `A W S E D F T G Y H U J K …` — `A`=C, `W`=C#, `S`=D,
`E`=D#, `D`=E, `F`=F … hold keys to play chords. The on-screen piano (mouse) and
MIDI also work.

- On macOS it builds **without JACK** (the `jack/dynamic_loading` feature loads
  it at runtime if present, otherwise it uses **CoreAudio**). Audio/MIDI device
  options: add `--help`, e.g. `--backend coreaudio`.
- Pick a built-in scene or load a WAV, then play. To process external audio,
  run with the right input device and choose *Source ▸ Live input*.

> Inside the DAW, play it with **MIDI** or the **on-screen piano** instead — the
> computer keyboard is a standalone-only convenience because of how hosts route
> keys.

## Download & share (Windows + macOS + Linux)

A VST3 is a **compiled binary per operating system** — there is no single file
that works everywhere. Each friend needs the build for *their* OS. You don't have
to compile on every machine: the repo ships a GitHub Actions workflow
([`.github/workflows/build-vst.yml`](../.github/workflows/build-vst.yml)) that
builds all three automatically.

**To publish a shareable release (recommended):**

```sh
git tag v0.1.0
git push origin v0.1.0
```

GitHub then builds Windows, macOS (universal Intel+ARM) and Linux and attaches
three zips to a public **Release**:

- `raydrone-windows.zip`
- `raydrone-macos.zip`
- `raydrone-linux.zip`

Your friends just open the **Releases** page of the repo (no login needed),
download the zip for their OS, unzip, and copy `RayDrone.vst3` into their VST3
folder:

| OS | VST3 folder |
|---|---|
| Windows | `C:\Program Files\Common Files\VST3\` |
| macOS | `~/Library/Audio/Plug-Ins/VST3/` |
| Linux | `~/.vst3/` |

Then rescan plugins in their DAW. (Each zip also contains a `RayDrone.clap` for
CLAP hosts.)

> **macOS note for your friends:** the build is unsigned, so Gatekeeper may block
> it. Run once:
> `xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/RayDrone.vst3`
> And remember GarageBand/Logic won't see it — use a VST3 host (Reaper, Waveform
> Free, Carla).

You can also grab builds **without** cutting a release: go to the repo's
**Actions** tab → the latest "Build VST plugin" run → download the artifacts
(this requires a GitHub login).

## License

MIT — same as the parent project.
