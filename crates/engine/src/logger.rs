//! Logging utilities

//! Provides sub microsecond logging thread
//! https://markrbest.github.io/fast-logging-in-rust/
use std::thread;

pub use lockfree::channel::mpsc::{create, Sender};
pub use log;

#[macro_export]
macro_rules! info {
    ($tx:expr,$($arg:tt)*) => {
        let _ = $tx.send($crate::logger::Log::new(move || ::log::info!($($arg)*)));
    }
}

#[macro_export]
macro_rules! debug {
    ($tx:ident,$($arg:tt)*) => {
        let _ = $tx.send($crate::logger::Log::new(move || ::log::debug!($($arg)*)));
    }
}

#[macro_export]
macro_rules! error {
    ($tx:ident,$($arg:tt)*) => {
        let _ = $tx.send($crate::logger::Log::new(move || ::log::error!($($arg)*)));
    }
}

pub(crate) use debug;
pub(crate) use error;
pub(crate) use info;

// struct for wrapping the closure so it can be serialized
pub struct Log {
    data: Box<dyn Fn() + Send + 'static>,
}

impl Log {
    pub fn new<T>(data: T) -> Log
    where
        T: Fn() + Send + 'static,
    {
        return Log {
            data: Box::new(data),
        };
    }
    fn invoke(self) {
        (self.data)()
    }
}

/// Initialize low latency logger service  
///
/// Returns a handle for submitting logs
pub fn init() -> Sender<Log> {
    // create async thread to execute logging closure
    let (tx, mut rx) = create::<Log>();
    thread::spawn(move || {
        let core_ids = core_affinity::get_core_ids().unwrap();
        core_affinity::set_for_current(*core_ids.last().unwrap());

        // internal loop here
        loop {
            if let Ok(log_fn) = rx.recv() {
                log_fn.invoke()
            }
        }
    });

    tx
}
