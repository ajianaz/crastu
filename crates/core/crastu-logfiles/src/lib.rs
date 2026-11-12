//! The file side of the flight recorder: session folders with a liveness lock, private files,
//! append-only line streams with a size cap, and retention. See `crastu-recorder` for the whole.

mod io;
mod retention;

pub use io::{LOCK_FILE, Session, Stream, create_session, open_append, write_private};
pub use retention::{Retention, is_session_name, prune};
