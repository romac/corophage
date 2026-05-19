// Regression test for the dyn-handler escape hatch added for
// rust-lang/rust#100013. Twenty-three effects, stateful handler capturing
// `&Arc<…>`, driven through `tokio::spawn` — the exact shape that breaks
// the default hlist dispatch. The test passes by virtue of compiling.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use corophage::dyn_dispatch::{CoControl, EffectHandler, resume};
use corophage::match_effect;
use corophage::prelude::*;

macro_rules! decl_effect {
    ($name:ident) => {
        #[effect(u64)]
        pub struct $name(pub u64);
    };
}

decl_effect!(E01);
decl_effect!(E02);
decl_effect!(E03);
decl_effect!(E04);
decl_effect!(E05);
decl_effect!(E06);
decl_effect!(E07);
decl_effect!(E08);
decl_effect!(E09);
decl_effect!(E10);
decl_effect!(E11);
decl_effect!(E12);
decl_effect!(E13);
decl_effect!(E14);
decl_effect!(E15);
decl_effect!(E16);
decl_effect!(E17);
decl_effect!(E18);
decl_effect!(E19);
decl_effect!(E20);
decl_effect!(E21);
decl_effect!(E22);
decl_effect!(E23);

type Effs = Effects![
    E01, E02, E03, E04, E05, E06, E07, E08, E09, E10, E11, E12, E13, E14, E15, E16, E17, E18, E19,
    E20, E21, E22, E23
];

#[derive(Default)]
struct State {
    counter: u64,
}

struct Outer {
    bump: u64,
}

impl Outer {
    async fn bump(&self, n: u64) -> u64 {
        n + self.bump
    }
}

struct MyHandler {
    outer: Arc<Outer>,
}

impl<'a> EffectHandler<'a, Effs, State> for MyHandler {
    fn handle<'h>(
        &'h self,
        state: &'h mut State,
        effect: Effs,
    ) -> Pin<Box<dyn Future<Output = CoControl<'a, Effs>> + Send + 'h>>
    where
        Self: 'h,
        Effs: 'h,
        State: 'h,
    {
        Box::pin(async move {
            match_effect!(effect => {
                E01(n) => resume::<_, E01, _>(self.outer.bump(n).await),
                E02(n) => resume::<_, E02, _>(self.outer.bump(n).await),
                E03(n) => resume::<_, E03, _>(self.outer.bump(n).await),
                E04(n) => resume::<_, E04, _>(self.outer.bump(n).await),
                E05(n) => resume::<_, E05, _>(self.outer.bump(n).await),
                E06(n) => resume::<_, E06, _>(self.outer.bump(n).await),
                E07(n) => resume::<_, E07, _>(self.outer.bump(n).await),
                E08(n) => resume::<_, E08, _>(self.outer.bump(n).await),
                E09(n) => resume::<_, E09, _>(self.outer.bump(n).await),
                E10(n) => resume::<_, E10, _>(self.outer.bump(n).await),
                E11(n) => resume::<_, E11, _>(self.outer.bump(n).await),
                E12(n) => resume::<_, E12, _>(self.outer.bump(n).await),
                E13(n) => resume::<_, E13, _>(self.outer.bump(n).await),
                E14(n) => resume::<_, E14, _>(self.outer.bump(n).await),
                E15(n) => resume::<_, E15, _>(self.outer.bump(n).await),
                E16(n) => resume::<_, E16, _>(self.outer.bump(n).await),
                E17(n) => resume::<_, E17, _>(self.outer.bump(n).await),
                E18(n) => resume::<_, E18, _>(self.outer.bump(n).await),
                E19(n) => resume::<_, E19, _>(self.outer.bump(n).await),
                E20(n) => resume::<_, E20, _>(self.outer.bump(n).await),
                E21(n) => resume::<_, E21, _>(self.outer.bump(n).await),
                E22(n) => resume::<_, E22, _>(self.outer.bump(n).await),
                E23(n) => {
                    state.counter += n;
                    resume::<_, E23, _>(self.outer.bump(n).await)
                },
            })
        })
    }
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn dyn_handler_with_23_effects_under_tokio_spawn() {
    let outer = Arc::new(Outer { bump: 100 });
    let handler = MyHandler { outer };

    let handle = tokio::spawn(async move {
        let mut state = State::default();
        let prog = Program::new_send::<Effs, _>(|y: Yielder<'_, Effs>| async move {
            let v01 = y.yield_(E01(1)).await;
            let v23 = y.yield_(E23(23)).await;
            (v01, v23)
        });
        let result = prog.run_dyn_stateful(&handler, &mut state).await;
        (result, state.counter)
    });

    let (result, counter) = handle.await.unwrap();
    assert_eq!(result, Ok((101, 123)));
    assert_eq!(counter, 23);
}
