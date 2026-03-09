use corophage::coroutine::Co;
use corophage::prelude::*;

struct Foo;
struct Bar;

impl Effect for Foo {
    type Resume<'r> = ();
}

impl Effect for Bar {
    type Resume<'r> = ();
}

type CoEffs = Effects![Foo, Bar];

#[test]
fn same_resume() {
    fn foo(_: Foo) -> CoControl<'static, CoEffs> {
        CoControl::resume_for::<Foo, _>(())
    }

    fn bar(_: Bar) -> CoControl<'static, CoEffs> {
        CoControl::resume_for::<Bar, _>(())
    }

    pub fn co() -> Co<'static, CoEffs, ()> {
        Co::new(|y| async move {
            y.yield_(Foo).await;
            y.yield_(Bar).await;
        })
    }

    let _ = corophage::sync::run(co(), &mut hlist![foo, bar]);
}
