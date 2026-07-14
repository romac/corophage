use corophage::coroutine::Co;
use corophage::prelude::*;

#[effect(())]
struct Ping;

fn subprogram() -> Effectful<'static, Effects![Ping], ()> {
    Program::new(|mut yielder| async move {
        yielder.yield_(Ping).await;
    })
}

fn main() {
    type Effs = Effects![Ping];

    let _: Co<'static, Effs, ()> = Co::new(|mut yielder| async move {
        let first = yielder.invoke(subprogram());
        let second = yielder.invoke(subprogram());
        let _ = (first.await, second.await);
    });
}
