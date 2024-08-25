use std::ops::{Deref, DerefMut};

use config::parse_args;
use log::ColorlessPrintlnLogger;
use upload::upload;

#[cfg(not(any(feature = "ureq", feature = "minreq")))]
compile_error!("Either 'ureq' or 'minreq' feature must be enabled");
#[cfg(all(feature = "ureq", feature = "minreq"))]
compile_error!("Cannot enable both 'ureq' and 'minreq' features");

mod config;
mod hook;
mod log;
mod temp;
mod upload;

struct Defer<T, G, F: Fn(&mut T) -> G>(T, F);
impl<T, G, F: Fn(&mut T) -> G> Defer<T, G, F> {
    pub fn new(value: T, fun: F) -> Self {
        Self(value, fun)
    }
}
impl<T, G, F: Fn(&mut T) -> G> Deref for Defer<T, G, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T, G, F: Fn(&mut T) -> G> DerefMut for Defer<T, G, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl<T, G, F: Fn(&mut T) -> G> Drop for Defer<T, G, F> {
    fn drop(&mut self) {
        (self.1)(&mut self.0);
    }
}
impl<I, T: AsRef<I>, G, F: Fn(&mut T) -> G> AsRef<I> for Defer<T, G, F> {
    fn as_ref(&self) -> &I {
        self.0.as_ref()
    }
}
impl<I, T: AsMut<I>, G, F: Fn(&mut T) -> G> AsMut<I> for Defer<T, G, F> {
    fn as_mut(&mut self) -> &mut I {
        self.0.as_mut()
    }
}

fn main() {
    let config = Box::leak(Box::new(parse_args()));

    let mut logger = ColorlessPrintlnLogger;

    let mut first = true;

    loop {
        if first {
            first = false;
        } else {
            std::thread::sleep(config.delay);
        }

        upload(config, &mut logger);
    }
}
