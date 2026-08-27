---
name: RayDrone Signal Field
description: A monochrome acoustic instrument built from signal, measurement and inversion.
colors:
  field-black: "#000000"
  signal-white: "#ffffff"
  hairline-white: "rgba(255,255,255,0.34)"
typography:
  display:
    fontFamily: "Bahnschrift, DIN Alternate, Arial Narrow, sans-serif"
    fontSize: "clamp(1.65rem, 2.6vw, 3rem)"
    fontWeight: 500
    lineHeight: 0.9
    letterSpacing: "0.035em"
  body:
    fontFamily: "Bahnschrift, DIN Alternate, Arial Narrow, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.45
  data:
    fontFamily: "Cascadia Mono, SFMono-Regular, Consolas, monospace"
    fontSize: "0.68rem"
    fontWeight: 500
    lineHeight: 1.35
rounded:
  square: "0px"
spacing:
  hairline: "1px"
  xs: "8px"
  sm: "10px"
  md: "16px"
  lg: "20px"
components:
  button-primary:
    backgroundColor: "{colors.signal-white}"
    textColor: "{colors.field-black}"
    rounded: "{rounded.square}"
    padding: "9px 12px"
    height: "52px"
  button-secondary:
    backgroundColor: "{colors.field-black}"
    textColor: "{colors.signal-white}"
    rounded: "{rounded.square}"
    padding: "9px 12px"
    height: "44px"
  panel:
    backgroundColor: "{colors.field-black}"
    textColor: "{colors.signal-white}"
    rounded: "{rounded.square}"
    padding: "16px"
---

# Design System: RayDrone Signal Field

## Overview

**Creative North Star: "The Data-Sublime Instrument"**

RayDrone behaves like a live acoustic field made operable. Its visual world comes from signal laboratories, spectral plots, barcode density and tabular measurement—not from conventional synthesizer panels. The waveform is the dominant object; navigation and controls read as calibrated parts of that field.

Expression never obscures operation. A musician finds Load, Play, Material and the four macros immediately, while a DSP specialist can read state and enter the advanced mathematical area without forcing that density onto the main instrument.

**Key Characteristics:**

- Pure black and white with state expressed by inversion.
- Hairline frames, barcode density and sine lattices used functionally.
- One dominant signal field followed by material specimens and parameter bays.
- Mathematical analysis is a separate advanced destination.
- Dense information remains aligned, tabular and keyboard-operable.

## Colors

The palette is binary: Field Black is the physical ground, Signal White carries content and action, and Hairline White creates low-density structure.

**The Binary Field Rule.** Interface chrome uses only pure black and pure white; no gray ramps, gradients, glow or colored accents.

**The Inversion Rule.** Active, selected and primary states invert foreground and background instead of introducing a new color.

## Typography

**Display Font:** Bahnschrift with DIN Alternate and Arial Narrow fallbacks

**Body Font:** Bahnschrift with local condensed grotesk fallbacks
**Label/Mono Font:** Cascadia Mono with SFMono-Regular and Consolas fallbacks

**Character:** Narrow technical lettering gives actions and headings the cadence of a calibrated instrument. Tabular monospace is reserved for measurements, values, engine state and status.

### Hierarchy

- **Display** (500, `clamp(1.65rem, 2.6vw, 3rem)`, 0.9): RayDrone identity only.
- **Headline** (500, `clamp(1.4rem, 2.2vw, 2.3rem)`, 1.05): task-level statements.
- **Title** (600, `0.72rem`, 1.2): panel and station labels.
- **Body** (400, `15px`, 1.45): instructions and explanatory copy, capped near 72ch.
- **Data** (500, `0.68rem`, 1.35): values, telemetry and status.

**The Measurement Rule.** Values align tabularly and never compete with larger action labels.

## Layout

The desktop surface uses a compact sticky command header, a dominant signal-and-transport stage, and a specimen/parameter workbench. The grid has twelve columns, 10px gutters and a 1680px maximum working width. Basic places the signal stage beside its macro console above 1100px. Medium and Professional use full-width signal staging followed by the material strip and aligned parameter bays.

At 1100px the instrument becomes a single-column control flow. At 760px the header gains two rows, material specimens become a two-column grid, all targets remain at least 42–44px, and mathematical analysis stacks vertically. The interface must have no horizontal overflow at 375px.

## Elevation & Depth

The system is fully flat and uses no shadows. Depth is communicated by containment, line weight, filled inversion and the relative density of barcode fields.

**The Flat Signal Rule.** A control may invert or strengthen its frame in response to state; it never lifts off the surface.

## Shapes

All structural shapes are rectilinear with square corners. One-pixel rules form panels, tracks and dividers. Circular geometry belongs only to signal data or necessary native control affordances, never to decorative containers.

## Components

### Buttons

- **Shape:** square, framed and at least 44px high.
- **Primary:** Signal White ground with Field Black text; used for Load and Play.
- **Hover / Focus:** full inversion for hover; 2px external Signal White outline for keyboard focus.
- **Secondary:** Field Black ground with Signal White text and a 1px frame.

### Inputs / Fields

- **Style:** black ground, white frame, square geometry and locally available system controls.
- **Range:** barcode track with a 14px rectangular white thumb.
- **Checkbox:** 42×24px framed field; checked state fills with signal bars.
- **Focus:** 2px external white outline without glow.

### Cards / Containers

Containers are framed stations, not cards. They use no radius, shadow or independent accent. Internal padding is 16px desktop and 12px mobile.

### Navigation

Básico, Medio and Profesional form one segmented control. Selection uses full inversion. Language is the only compact disclosure in the command header; it expands without covering the environment control.

### Material Specimen

Six specimens share one continuous framed strip. Each carries a material name, a short audible description and a barcode signature. Hover and active states invert the full specimen. Mobile uses two columns without changing semantic order.

### Mathematical Analysis Hatch

The advanced destination begins as a full-width white hatch after the instrument. It remains closed by default, uses a native `details` disclosure, and reveals Convergence Lab plus the model comparison only in Professional.

## Do's and Don'ts

### Do:

- **Do** make the live signal the largest visual object in the first viewport.
- **Do** preserve familiar buttons, inputs, keyboard focus and readable labels.
- **Do** use barcode texture to communicate activity, progress or selection.
- **Do** reduce information density deliberately across breakpoints.

### Don't:

- **Don't** use rounded cards, glass, glow, gradients or neon.
- **Don't** imitate a hardware synth with decorative knobs.
- **Don't** hide primary instrument controls behind tabs or dropdown navigation.
- **Don't** place mathematical analysis inside the main creative flow.
