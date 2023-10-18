use std::os::raw::c_uint;

pub type Keysym = u32;
pub type ModMask = c_uint;

#[derive(Debug)]
pub struct KeyEvent {
    pub keysym: Keysym,
    pub modmask: ModMask,
}

impl TryFrom<String> for KeyEvent {
    type Error = ();

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let split: Vec<&str> = value.split(',').collect();
        if split.len() == 2 {
            Ok(KeyEvent {
                keysym: split.get(0).unwrap_or(&"").parse().map_err(|_| ())?,
                modmask: split.get(1).unwrap_or(&"").parse().map_err(|_| ())?,
            })
        } else {
            Err(())
        }
    }
}
