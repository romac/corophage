use std::rc::Rc;

use corophage::prelude::*;

fn main() {
    let mut state = Rc::new(0);
    // Even without effects, the runner retains the mutable state borrow.
    let _future = Program::new_send::<Effects![], _>(|_| async {})
        .run_stateful(&mut state);
}
