//! Platforms without a permission model of this kind: nothing is known, nothing can be asked.

use crate::{Permission, Status};

pub fn status(_: Permission) -> Status {
    Status::Unknown
}

pub fn request(_: Permission, done: Box<dyn FnOnce(Status) + Send + 'static>) {
    done(Status::Unknown)
}

pub fn open_settings(_: Permission) -> Result<(), String> {
    Err("opening the permission settings is only supported on macOS".into())
}
