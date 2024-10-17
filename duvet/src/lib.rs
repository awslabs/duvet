macro_rules! ensure {
    ($cond:expr) => {
        ensure!($cond, ());
    };
    ($cond:expr, $otherwise:expr) => {
        if !($cond) {
            return $otherwise;
        }
    };
}

pub mod comment;
//mod citation;
//mod error;
mod ietf;
mod manifest;
pub mod report;
