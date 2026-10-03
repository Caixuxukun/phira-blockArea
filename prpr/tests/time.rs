use prpr::time::TimeManager;
use std::{cell::Cell, rc::Rc};

fn clock() -> (Rc<Cell<f64>>, TimeManager) {
    let now = Rc::new(Cell::new(10.));
    let source = Rc::clone(&now);
    let tm = TimeManager::manual(Box::new(move || source.get()));
    (now, tm)
}

#[test]
fn initial_and_repeated_activation_leave_running_clock_unchanged() {
    let (now, mut tm) = clock();
    now.set(12.);
    tm.resume();
    assert_eq!(tm.now(), 2.);
    assert_eq!(tm.start_time, 10.);
    now.set(13.);
    tm.resume();
    assert_eq!(tm.now(), 3.);
    assert!(!tm.paused());
}

#[test]
fn repeated_pause_preserves_first_pause_and_excludes_background_time() {
    let (now, mut tm) = clock();
    tm.speed = 2.;
    now.set(12.);
    tm.pause();
    now.set(15.);
    tm.pause();
    assert_eq!(tm.now(), 4.);
    now.set(20.);
    tm.resume();
    assert_eq!(tm.now(), 4.);
    now.set(21.);
    tm.resume();
    assert_eq!(tm.now(), 6.);
}

#[test]
fn reset_while_paused_makes_later_activation_harmless() {
    let (now, mut tm) = clock();
    tm.pause();
    now.set(15.);
    tm.reset();
    now.set(16.);
    tm.resume();
    assert_eq!(tm.now(), 1.);
    assert!(!tm.paused());
}
