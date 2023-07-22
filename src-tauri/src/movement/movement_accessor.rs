use parking_lot::Mutex;
use tauri::Window;

//use crate::platform::PlatformAccessor;
use super::MovementCoordinator;

pub struct MovementAccessor {
    coordinator: Mutex<MovementCoordinator>,
}

impl MovementAccessor {
    pub fn new(window: Window /*platform: &'a PlatformAccessor<'a>*/) -> Self {
        Self {
            coordinator: Mutex::new(MovementCoordinator::new(window /*platform*/)),
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
