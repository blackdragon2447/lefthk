pub mod context;

use crate::child::Children;
use crate::config::{command, Keybind};
use crate::errors::{self, Error, LeftError};
use crate::ipc::CommandPipe;
use crate::keyevent::KeyEvent;
use crate::keypipe::KeyPipe;
use crate::mode::Mode;
use crate::xkeysym_lookup;
use crate::xwrap::XWrap;
use x11_dl::xlib;
use xdg::BaseDirectories;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Status {
    Reload,
    Kill,
    Continue,
}

pub struct Worker {
    keybinds: Vec<Keybind>,
    base_directory: BaseDirectories,

    mode: Mode,

    pub xwrap: Option<XWrap>,
    pub children: Children,
    pub status: Status,

    /// "Chord Context": Holds the relevant data for chording
    pub chord_ctx: context::Chord,
}

impl Worker {
    pub fn new(keybinds: Vec<Keybind>, base_directory: BaseDirectories, mode: Mode) -> Self {
        match mode {
            Mode::Xlib => 
                Self {
                    status: Status::Continue,
                    keybinds,
                    base_directory,
                    mode,
                    xwrap: Some(XWrap::new()),
                    children: Children::default(),
                    chord_ctx: context::Chord::new(),
                },
                Mode::Pipe =>
                    Self {
                        status: Status::Continue,
                        keybinds,
                        base_directory,
                        mode,
                        xwrap: None,
                        children: Children::default(),
                        chord_ctx: context::Chord::new(),
                    }
        }
    }

    pub async fn event_loop(mut self) -> Status {
        let mut pipe = self.get_pipe().await;

        match self.mode {
            Mode::Xlib => {
                self.xwrap.as_ref().unwrap().grab_keys(&self.keybinds);
                while self.status == Status::Continue {
                    self.xwrap.as_ref().unwrap().flush();

                    self.evaluate_chord_xlib();

                    tokio::select! {
                        _ = self.children.wait_readable() => {
                            self.children.reap();
                        }
                        _ = self.xwrap.as_mut().unwrap().wait_readable() => {
                            let event_in_queue = self.xwrap.as_ref().unwrap().queue_len();
                            for _ in 0..event_in_queue {
                                let xlib_event = self.xwrap.as_ref().unwrap().get_next_event();
                                self.handle_event(&xlib_event);
                            }
                        }
                        Some(command) = pipe.get_next_command() => {
                            errors::log_on_error!(command.execute(&mut self));
                        }
                    };
                }
            }, 
            Mode::Pipe => {
                let mut key_pipe = self.get_keyevent_pipe().await;

                while self.status == Status::Continue {
                    self.evaluate_chord_pipe();

                    tokio::select! {
                        _ = self.children.wait_readable() => {
                            self.children.reap();
                        }
                        Some(event) = key_pipe.get_next_event() => {
                            let KeyEvent{modmask: mask, keysym: key} = event; 

                            if let Some(keybind) = self.get_keybind((mask, key)) {
                                if let Ok(command) = command::denormalize(&keybind.command) {
                                    errors::log_on_error!(command.execute(&mut self));
                                }
                            } else {
                                errors::log_on_error!(Err(LeftError::CommandNotFound));
                            }
                        }
                        Some(command) = pipe.get_next_command() => {
                            errors::log_on_error!(command.execute(&mut self));
                        }
                    };
                }

            },
        }

        self.status
    }

    async fn get_pipe(&self) -> CommandPipe {
        let pipe_name = CommandPipe::pipe_name();
        let pipe_file = errors::exit_on_error!(self.base_directory.place_runtime_file(pipe_name));
        errors::exit_on_error!(CommandPipe::new(pipe_file).await)
    }

    async fn get_keyevent_pipe(&self) -> KeyPipe {
        let pipe_name = KeyPipe::pipe_name();
        let pipe_file = errors::exit_on_error!(self.base_directory.place_runtime_file(pipe_name));
        errors::exit_on_error!(KeyPipe::new(pipe_file).await)
    }

    fn handle_event(&mut self, xlib_event: &xlib::XEvent) {
        let error = match xlib_event.get_type() {
            xlib::KeyPress => self.handle_key_press(&xlib::XKeyEvent::from(xlib_event)),
            xlib::MappingNotify => {
                self.handle_mapping_notify(&mut xlib::XMappingEvent::from(xlib_event))
            }
            _ => return,
        };
        errors::log_on_error!(error);
    }

    fn handle_key_press(&mut self, event: &xlib::XKeyEvent) -> Error {
        let key = self.xwrap.as_ref().unwrap().keycode_to_keysym(event.keycode);
        let mask = xkeysym_lookup::clean_mask(event.state);
        if let Some(keybind) = self.get_keybind((mask, key)) {
            if let Ok(command) = command::denormalize(&keybind.command) {
                return command.execute(self);
            }
        } else {
            return Err(LeftError::CommandNotFound);
        }
        Ok(())
    }

    fn get_keybind(&self, mask_key_pair: (u32, u32)) -> Option<Keybind> {
        let keybinds = if let Some(keybinds) = &self.chord_ctx.keybinds {
            keybinds
        } else {
            &self.keybinds
        };
        keybinds
            .iter()
            .find(|keybind| {
                if let Some(key) = xkeysym_lookup::into_keysym(&keybind.key) {
                    let mask = xkeysym_lookup::into_modmask(&keybind.modifier);
                    return mask_key_pair == (mask, key);
                }
                false
            })
            .cloned()
    }

    fn handle_mapping_notify(&self, event: &mut xlib::XMappingEvent) -> Error {
        if event.request == xlib::MappingModifier || event.request == xlib::MappingKeyboard {
            return self.xwrap.as_ref().unwrap().refresh_keyboard(event);
        }
        Ok(())
    }
}
