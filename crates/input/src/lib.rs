#![forbid(unsafe_code)]

pub mod capture;
pub mod injection;
pub mod edge;

pub use capture::InputCapture;
pub use injection::InputInjector;
