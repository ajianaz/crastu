//! The start, continue and hangover logic shared by every gate (the energy gate here, the Silero
//! gate in `crastu-audio-silero`). A gate decides, per frame, whether the frame is strong enough
//! to start speech (`opens`) and whether it is strong enough to keep speech going (`holds`);
//! this turns that into events.

use crate::gate::GateEvent;

/// Turns "strong enough to start" and "strong enough to continue" per frame into [`GateEvent`]s.
#[derive(Clone, Debug)]
pub struct Hysteresis {
    hangover_frames: u32,
    open: bool,
    quiet_frames: u32,
}

impl Hysteresis {
    /// `hangover_frames`: frames below the continue level that still count as speech.
    pub fn new(hangover_frames: u32) -> Self {
        Hysteresis {
            hangover_frames,
            open: false,
            quiet_frames: 0,
        }
    }

    /// One frame: `opens` starts speech while closed, `holds` keeps it going while open.
    pub fn step(&mut self, opens: bool, holds: bool) -> GateEvent {
        if !self.open {
            if opens {
                self.open = true;
                self.quiet_frames = 0;
                return GateEvent::SpeechStart;
            }
            return GateEvent::Silence;
        }
        if holds {
            self.quiet_frames = 0;
            return GateEvent::Speech;
        }
        self.quiet_frames += 1;
        if self.quiet_frames > self.hangover_frames {
            self.open = false;
            self.quiet_frames = 0;
            GateEvent::SpeechEnd
        } else {
            GateEvent::Speech
        }
    }

    /// Closes the gate and forgets the hangover.
    pub fn reset(&mut self) {
        self.open = false;
        self.quiet_frames = 0;
    }
}
