#![allow(dead_code)]

use std::ops::DerefMut;

pub trait Logger {
    fn info(&mut self, value: &str);
    fn warn(&mut self, value: &str);
    fn error(&mut self, value: &str);
}
impl Logger for Box<dyn Logger> {
    fn info(&mut self, value: &str) {
        self.deref_mut().info(value)
    }
    fn warn(&mut self, value: &str) {
        self.deref_mut().warn(value)
    }
    fn error(&mut self, value: &str) {
        self.deref_mut().error(value)
    }
}
impl<T: Logger> Logger for &mut [T] {
    fn info(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.info(value));
    }
    fn warn(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.warn(value));
    }
    fn error(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.error(value));
    }
}
impl<T: Logger> Logger for Vec<T> {
    fn info(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.info(value));
    }
    fn warn(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.warn(value));
    }
    fn error(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.error(value));
    }
}
impl<T: Logger> Logger for Box<[T]> {
    fn info(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.info(value));
    }
    fn warn(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.warn(value));
    }
    fn error(&mut self, value: &str) {
        self.iter_mut().for_each(|x| x.error(value));
    }
}

pub struct ColorlessPrintlnLogger;
impl Logger for ColorlessPrintlnLogger {
    fn info(&mut self, value: &str) {
        println!("{value}");
    }
    fn warn(&mut self, value: &str) {
        println!("{value}");
    }
    fn error(&mut self, value: &str) {
        println!("{value}");
    }
}
