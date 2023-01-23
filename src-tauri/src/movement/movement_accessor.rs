use parking_lot::Mutex;
use tauri::Window;

//use crate::platform::PlatformAccessor;

use crate::platform::KeyManager;

use super::MovementCoordinator;

pub struct MovementAccessor /*<'a>*/ {
    coordinator: Mutex<MovementCoordinator /*<'a>*/>,
}

impl<'a> MovementAccessor /*<'a>*/ {
    pub fn new(key_manager: KeyManager /*platform: &'a PlatformAccessor<'a>*/) -> Self {
        Self {
            coordinator: Mutex::new(MovementCoordinator::new(key_manager /*platform*/)),
        }
    }

    pub fn schedule<F>(&self, func: F)
    where
        F: Fn(&mut MovementCoordinator),
    {
        let mut coordinator = self.coordinator.lock();
        func(&mut coordinator);
    }
}
