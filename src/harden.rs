// SPDX-License-Identifier: GPL-3.0-or-later

//! The installer holds passwords and, for a moment, nothing else worth stealing; but a crash must
//! not write them to a file. No core dumps, and the process cannot be dumped or traced by
//! another one that is not root.

use rustix::process::{
    dumpable_behavior, getrlimit, set_dumpable_behavior, setrlimit, DumpableBehavior, Resource,
    Rlimit,
};

/// Sets the core file size limit to 0 and makes the process non-dumpable.
pub fn disable_core_dumps() -> Result<(), String> {
    let zero = Rlimit {
        current: Some(0),
        maximum: Some(0),
    };
    setrlimit(Resource::Core, zero).map_err(|e| format!("the core file limit: {e}"))?;
    set_dumpable_behavior(DumpableBehavior::NotDumpable).map_err(|e| format!("dumpable: {e}"))
}

/// Both protections are in place.
pub fn core_dumps_are_off() -> bool {
    let limit = getrlimit(Resource::Core);
    limit.current == Some(0)
        && limit.maximum == Some(0)
        && dumpable_behavior().is_ok_and(|d| d == DumpableBehavior::NotDumpable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_dumps_are_switched_off() {
        disable_core_dumps().unwrap();
        assert!(core_dumps_are_off());
    }
}
