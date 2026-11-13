//! `crastu_audio::Hysteresis` on its own (it is also covered through both gates). Kept here
//! because `crastu-audio` is at its size limit.

use crastu_audio::{GateEvent, Hysteresis};

#[test]
fn opens_only_on_opens_and_holds_on_holds() {
    let mut h = Hysteresis::new(1);
    // `holds` alone does not start speech.
    assert_eq!(h.step(false, true), GateEvent::Silence);
    assert_eq!(h.step(true, true), GateEvent::SpeechStart);
    assert_eq!(h.step(false, true), GateEvent::Speech);
}

#[test]
fn the_hangover_counts_frames_that_do_not_hold() {
    let mut h = Hysteresis::new(2);
    h.step(true, true);
    assert_eq!(h.step(false, false), GateEvent::Speech);
    assert_eq!(h.step(false, false), GateEvent::Speech);
    assert_eq!(h.step(false, false), GateEvent::SpeechEnd);
    assert_eq!(h.step(false, false), GateEvent::Silence);
}

#[test]
fn a_holding_frame_restarts_the_hangover_and_reset_closes() {
    let mut h = Hysteresis::new(1);
    h.step(true, true);
    h.step(false, false);
    assert_eq!(h.step(false, true), GateEvent::Speech);
    assert_eq!(h.step(false, false), GateEvent::Speech);
    h.reset();
    assert_eq!(h.step(false, false), GateEvent::Silence);
}
