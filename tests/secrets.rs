// SPDX-License-Identifier: GPL-3.0-or-later

//! Nothing that holds typed text can be printed: not by a `{:?}` left in a message, not by an
//! error that quotes its input. The check is at compile time of the test: it asks whether the type
//! has the trait.

#![allow(dead_code)]

use nyra_installer::backend::{Answers, Secret};
use nyra_installer::state::{Effect, State};
use std::fmt::{Debug, Display};

macro_rules! implements {
    ($t:ty, $tr:path) => {{
        trait No {
            const YES: bool = false;
        }
        impl<T: ?Sized> No for T {}
        struct Probe<T: ?Sized>(std::marker::PhantomData<T>);
        impl<T: ?Sized + $tr> Probe<T> {
            const YES: bool = true;
        }
        <Probe<$t>>::YES
    }};
}

#[test]
fn the_probe_sees_a_trait_when_there_is_one() {
    assert!(implements!(String, Debug));
    assert!(implements!(String, Display));
    assert!(!implements!(std::process::Child, Display));
}

#[test]
fn nothing_that_holds_typed_text_can_be_printed() {
    assert!(!implements!(Secret, Debug));
    assert!(!implements!(Secret, Display));
    assert!(!implements!(Answers, Debug));
    assert!(!implements!(Answers, Display));
    assert!(!implements!(State, Debug));
    assert!(!implements!(State, Display));
    // An effect can carry the answers.
    assert!(!implements!(Effect, Debug));
    assert!(!implements!(Effect, Display));
    assert!(!implements!(Box<Answers>, Debug));
}
