//! Standalone RayDrone desktop app.
//!
//! Unlike the plugin running inside a DAW (where the host grabs keystrokes), this
//! is a normal OS window, so the **computer keyboard plays the piano**: the keys
//! A W S E D F T G Y H U J K… (from C4) pitch the drone. Use a built-in scene or
//! load a WAV, then play.
//!
//! Run it with:
//!   cargo run --release --features standalone --bin raydrone-standalone
//!
//! Audio/MIDI device options are available via `--help`.

use nih_plug::prelude::*;

fn main() {
    nih_export_standalone::<raydrone::RayDrone>();
}
