use crate::{config::Keybind, worker::Worker};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chord {
    pub keybinds: Option<Vec<Keybind>>,
    pub elapsed: bool,
}

impl Chord {
    pub fn new() -> Self {
        Self {
            keybinds: None,
            elapsed: false,
        }
    }
}

impl Worker {
    pub fn evaluate_chord_xlib(&mut self) {
        if self.chord_ctx.elapsed {
            self.xwrap.as_ref().unwrap().grab_keys(&self.keybinds);
            self.chord_ctx.keybinds = None;
            self.chord_ctx.elapsed = false;
        }
    }

    pub fn evaluate_chord_pipe(&mut self) {
        if self.chord_ctx.elapsed {
            self.chord_ctx.keybinds = None;
            self.chord_ctx.elapsed = false;
        }
    }
}
