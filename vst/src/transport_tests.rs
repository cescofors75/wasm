use super::*;
use std::collections::VecDeque;

struct Context {
    events: VecDeque<NoteEvent<()>>,
}
impl ProcessContext<RayDrone> for Context {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Vst3
    }
    fn execute_background(&self, _: ()) {}
    fn execute_gui(&self, _: ()) {}
    fn transport(&self) -> &Transport {
        panic!("this processor does not read transport")
    }
    fn next_event(&mut self) -> Option<NoteEvent<()>> {
        self.events.pop_front()
    }
    fn send_event(&mut self, _: NoteEvent<()>) {}
    fn set_latency_samples(&self, _: u32) {}
    fn set_current_voice_capacity(&self, _: u32) {}
}
fn process(plugin: &mut RayDrone, events: Vec<NoteEvent<()>>) -> [f32; 8] {
    let mut l = [0.25; 8];
    let mut r = [-0.25; 8];
    let mut buffer = Buffer::default();
    // These slices outlive the callback and both have the declared length.
    unsafe {
        buffer.set_slices(8, |channels| {
            channels.push(&mut l);
            channels.push(&mut r);
        });
    }
    let mut aux = AuxiliaryBuffers {
        inputs: &mut [],
        outputs: &mut [],
    };
    let mut context = Context {
        events: events.into(),
    };
    plugin.process(&mut buffer, &mut aux, &mut context);
    assert!(context.events.is_empty());
    l
}
fn bypass(plugin: &mut RayDrone, on: bool) {
    Arc::get_mut(&mut plugin.params).unwrap().bypass = BoolParam::new("Bypass", on);
}
fn note_on(note: u8) -> NoteEvent<()> {
    NoteEvent::NoteOn {
        timing: 0,
        voice_id: None,
        channel: 0,
        note,
        velocity: 1.0,
    }
}
fn note_off(note: u8) -> NoteEvent<()> {
    NoteEvent::NoteOff {
        timing: 4,
        voice_id: None,
        channel: 0,
        note,
        velocity: 0.0,
    }
}

#[test]
fn bypass_preserves_audio_but_consumes_note_off() {
    let mut plugin = RayDrone::default();
    process(&mut plugin, vec![note_on(60)]);
    assert!(plugin.midi_held[60]);
    bypass(&mut plugin, true);
    assert_eq!(process(&mut plugin, vec![note_off(60)]), [0.25; 8]);
    bypass(&mut plugin, false);
    process(&mut plugin, vec![]);
    assert!(!plugin.midi_held[60]);
}
#[test]
fn bypass_consumes_all_events_including_choke_and_invalid_note() {
    let mut plugin = RayDrone::default();
    bypass(&mut plugin, true);
    process(
        &mut plugin,
        vec![
            note_on(60),
            note_on(64),
            note_on(255),
            NoteEvent::Choke {
                timing: 2,
                voice_id: None,
                channel: 0,
                note: 60,
            },
            note_off(64),
        ],
    );
    assert!(plugin.midi_held.iter().all(|held| !held));
}
#[test]
fn host_reset_clears_midi_mask_before_next_block() {
    let mut plugin = RayDrone::default();
    process(&mut plugin, vec![note_on(60)]);
    plugin.reset();
    process(&mut plugin, vec![]);
    assert!(plugin.midi_held.iter().all(|held| !held));
}
