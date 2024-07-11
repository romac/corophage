use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine};
use frunk::coproduct::{CNil, CoprodInjector, CoprodUninjector, Coproduct};
use frunk::{HCons, HNil};

// pub mod coproduct;

pub enum Never {}

struct Start;

impl Effect for Start {
    type Resume = Start;
}

pub trait Effects: MapResume {}

impl<E> Effects for E where E: MapResume {}

#[macro_export]
macro_rules! Effects {
    [$($effect:ty),*] => {
        ::frunk::Coprod!($($effect),*)
    };
}

pub trait Effect {
    type Resume;
}

pub trait MapResume {
    type Output;
}

impl MapResume for CNil {
    type Output = CNil;
}

impl<H: Effect, T: MapResume> MapResume for Coproduct<H, T> {
    type Output = Coproduct<H::Resume, <T as MapResume>::Output>;
}

pub type Resumes<E> = <E as MapResume>::Output;

type CanStart<Effs> = Coproduct<Start, Effs>;

pub struct Program<'a, Effs, Return>
where
    Effs: Effects,
{
    co: ScopedCoroutine<'a, Resumes<CanStart<Effs>>, CanStart<Effs>, Return, DefaultStack>,
}

impl<'a, Effs, Return> Program<'a, Effs, Return>
where
    Effs: Effects,
{
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Yielder<'_, Effs>) -> Return + 'a,
    {
        Self {
            co: ScopedCoroutine::new(|yielder, _start| f(Yielder::new(yielder))),
        }
    }

    fn resume(
        &mut self,
        resume: Resumes<CanStart<Effs>>,
    ) -> CoroutineResult<CanStart<Effs>, Return> {
        self.co.resume(resume)
    }

    fn resume_with<R, Index>(&mut self, resume: R) -> CoroutineResult<CanStart<Effs>, Return>
    where
        Resumes<CanStart<Effs>>: CoprodInjector<R, Index>,
    {
        self.resume(Resumes::<CanStart<Effs>>::inject(resume))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cancelled;

pub trait CoproductFoldableMut<F, R> {
    fn fold_mut(self, f: &mut F) -> R;
}

impl<R> CoproductFoldableMut<CNil, R> for CNil {
    fn fold_mut(self, _: &mut CNil) -> R {
        match self {}
    }
}

impl<R> CoproductFoldableMut<HNil, R> for CNil {
    fn fold_mut(self, _: &mut HNil) -> R {
        unreachable!()
    }
}

impl<F, R, FTail, CH, CTail> CoproductFoldableMut<HCons<F, FTail>, R> for Coproduct<CH, CTail>
where
    F: FnMut(CH) -> R,
    CTail: CoproductFoldableMut<FTail, R>,
{
    fn fold_mut(self, f: &mut HCons<F, FTail>) -> R {
        let f_head = &mut f.head;
        let f_tail = &mut f.tail;

        match self {
            Coproduct::Inl(r) => (f_head)(r),
            Coproduct::Inr(rest) => rest.fold_mut(f_tail),
        }
    }
}

pub fn handle<Effs, Return, F>(
    mut co: Program<Effs, Return>,
    handler: &mut F,
) -> Result<Return, Cancelled>
where
    Effs: Effects + CoproductFoldableMut<F, CoControl<Effs>>,
{
    let mut yielded = co.resume_with(Start);

    loop {
        match yielded {
            CoroutineResult::Return(value) => break Ok(value),

            CoroutineResult::Yield(effect) => {
                let effect = match effect {
                    Coproduct::Inl(_) => unreachable!(),
                    Coproduct::Inr(subeffect) => subeffect,
                };

                let resume: CoControl<Effs> = effect.fold_mut(handler);
                match resume {
                    CoControl::Cancel => break Err(Cancelled),
                    CoControl::Resume(r) => yielded = co.resume(Coproduct::Inr(r)),
                }
            }
        }
    }
}

pub struct Yielder<'a, Effs>
where
    Effs: MapResume,
{
    yielder: &'a corosensei::Yielder<Resumes<CanStart<Effs>>, CanStart<Effs>>,
}

impl<'a, Effs> Yielder<'a, Effs>
where
    Effs: MapResume,
{
    fn new(yielder: &'a corosensei::Yielder<Resumes<CanStart<Effs>>, CanStart<Effs>>) -> Self {
        Self { yielder }
    }

    pub fn yield_<E, Index>(&self, effect: E) -> E::Resume
    where
        E: Effect,
        Effs: CoprodInjector<E, Index>,
        <Effs as MapResume>::Output: CoprodUninjector<E::Resume, Index>,
    {
        let resume = self.yielder.suspend(Coproduct::Inr(Effs::inject(effect)));
        match resume {
            Coproduct::Inl(_) => unreachable!(),
            Coproduct::Inr(value) => match value.uninject() {
                Ok(value) => value,
                Err(_) => unreachable!(),
            },
        }
    }
}

pub enum CoControl<Effs>
where
    Effs: Effects,
{
    Cancel,
    Resume(Resumes<Effs>),
}

impl<Effs> CoControl<Effs>
where
    Effs: Effects,
{
    pub fn cancel() -> Self {
        Self::Cancel
    }

    pub fn resume<R, Index>(r: R) -> Self
    where
        Resumes<Effs>: CoprodInjector<R, Index>,
    {
        Self::Resume(Resumes::<Effs>::inject(r))
    }
}

#[cfg(test)]
pub mod tests {
    use std::cell::RefCell;
    use std::marker::PhantomData;

    use frunk::hlist;

    use super::*;

    pub struct Cancel;

    impl Effect for Cancel {
        type Resume = Never;
    }

    pub struct Log<'a>(pub &'a str);

    impl<'a> Effect for Log<'a> {
        type Resume = ();
    }

    pub struct FileRead(pub String);

    impl Effect for FileRead {
        type Resume = String;
    }

    #[derive(Default)]
    pub struct GetState<S> {
        _marker: PhantomData<S>,
    }

    impl<S> Effect for GetState<S> {
        type Resume = S;
    }

    pub struct SetState<S>(pub S);

    impl<S> Effect for SetState<S> {
        type Resume = ((), ());
    }

    pub type CoEffs = Effects![Cancel, Log<'static>, FileRead, GetState<u64>, SetState<u64>];

    pub fn co() -> Program<'static, CoEffs, ()> {
        Program::new(move |yielder| {
            println!("Logging...");
            let () = yielder.yield_(Log("Hello, world!"));

            println!("Reading file...");
            let text = yielder.yield_(FileRead("example.txt".to_string()));
            println!("Read file: {text}");

            let state = yielder.yield_(GetState::default());
            println!("State: {state}");
            yielder.yield_(SetState(state * 2));
            let state = yielder.yield_(GetState::default());
            println!("State: {state}");

            println!("Cancelling...");
            yielder.yield_(Cancel);
            println!("Cancelled!");
        })
    }

    #[test]
    fn it_works() {
        #[derive(Debug, PartialEq, Eq)]
        struct State {
            x: u64,
        }

        let state = RefCell::new(State { x: 42 });

        fn cancel(_c: Cancel) -> CoControl<CoEffs> {
            CoControl::cancel()
        }

        fn log(Log(msg): Log<'_>) -> CoControl<CoEffs> {
            println!("LOG: {msg}");
            CoControl::resume(())
        }

        fn file_read(FileRead(file): FileRead) -> CoControl<CoEffs> {
            println!("Reading file: {file}");
            CoControl::resume("file content".to_string())
        }

        let result = handle(
            co(),
            &mut hlist![
                cancel,
                log,
                file_read,
                |_g: GetState<u64>| CoControl::resume(state.borrow().x),
                |SetState(x)| {
                    state.borrow_mut().x = x;
                    CoControl::resume(((), ()))
                },
            ],
        );

        assert_eq!(result, Err(Cancelled));
        assert_eq!(state.into_inner(), State { x: 84 });
    }
}
