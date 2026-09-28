//! Giving the terminal back. Gap 125.
//!
//! `run` itself needs a tty, so these drive the step list it hands over.

use std::cell::Cell;
use std::rc::Rc;

use typ_app::run::{Step, teardown};

fn failing() -> Step {
    Box::new(|| Err(std::io::Error::other("mouse capture would not turn off")))
}

fn counted(ran: &Rc<Cell<u32>>) -> Step {
    let ran = Rc::clone(ran);
    Box::new(move || {
        ran.set(ran.get() + 1);
        Ok(())
    })
}

#[test]
fn a_failed_step_does_not_skip_the_ones_after_it() {
    // A `?` here left raw mode and the alternate screen on because mouse
    // capture failed to turn off: a shell nobody can type into.
    let ran = Rc::new(Cell::new(0));
    let mut steps = [failing(), counted(&ran), counted(&ran)];

    let result = teardown(Ok(()), &mut steps);

    assert_eq!(ran.get(), 2, "leaving raw mode was skipped");
    assert!(
        result.is_err(),
        "a cleanup failure on a clean exit is still said"
    );
}

#[test]
fn the_loop_error_outranks_a_cleanup_error() {
    let ran = Rc::new(Cell::new(0));
    let mut steps = [failing(), counted(&ran)];

    let result = teardown(Err(anyhow::anyhow!("the loop failed")), &mut steps);

    let error = result.expect_err("an error went missing");
    assert!(
        error.to_string().contains("the loop failed"),
        "the cleanup swallowed the reason the editor stopped: {error:#}"
    );
    assert_eq!(ran.get(), 1);
}
