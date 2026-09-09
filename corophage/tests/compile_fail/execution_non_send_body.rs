use corophage::prelude::*;

fn assert_send(_: impl Send) {}

fn main() {
    let local = std::rc::Rc::new(1);
    let future = Program::new(move |_: Yielder<'_, Effects![]>| async move {
        std::future::pending::<()>().await;
        *local
    }).run();
    assert_send(future);
}
