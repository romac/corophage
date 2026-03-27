use corophage::coroutine::Co;
use corophage::prelude::*;

struct Foo;
struct Bar;

impl Effect for Foo {
    type Resume<'r> = ();
}

impl CovariantResume for Foo {
    fn shorten_resume<'a: 'b, 'b>(resume: ()) {
        resume
    }
}

impl Effect for Bar {
    type Resume<'r> = ();
}

impl CovariantResume for Bar {
    fn shorten_resume<'a: 'b, 'b>(resume: ()) {
        resume
    }
}

type CoEffs = Effects![Foo, Bar];

#[test]
fn same_resume() {
    fn foo(_: Foo) -> Control<()> {
        Control::resume(())
    }

    fn bar(_: Bar) -> Control<()> {
        Control::resume(())
    }

    pub fn co() -> Co<'static, CoEffs, ()> {
        Co::new(|y| async move {
            y.yield_(Foo).await;
            y.yield_(Bar).await;
        })
    }

    let _ = corophage::sync::run(co(), &mut hlist![foo, bar]);
}
