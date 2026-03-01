use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;

use fauxgen::__private::SyncGenerator;
use fauxgen::Generator;
use fauxgen::GeneratorState;
use fauxgen::GeneratorToken;
use frunk_core::coproduct::{CoprodInjector, CoprodUninjector, Coproduct};

use crate::effect::{CanStart, Effect, Effects, MapResume, Resumes, Start};
use crate::locality::{Local, Locality, Sendable};

type Gen<'a, Effs, Return, L> = SyncGenerator<
    <L as Locality>::PinBoxFuture<'a, Return>,
    CanStart<Effs>,
    Resumes<'a, CanStart<Effs>>,
>;

pub type Co<'a, Effs, Return> = GenericCo<'a, Effs, Return, Local>;
pub type CoSend<'a, Effs, Return> = GenericCo<'a, Effs, Return, Sendable>;

pub struct GenericCo<'a, Effs, Return, L: Locality = Local>
where
    Effs: Effects<'a>,
{
    generator: Gen<'a, Effs, Return, L>,
    _pin: PhantomPinned,
}

impl<'a, Effs, Return> Co<'a, Effs, Return>
where
    Effs: Effects<'a>,
{
    pub fn new<F>(f: impl FnOnce(Yielder<'a, Effs>) -> F + 'a) -> Self
    where
        F: Future<Output = Return>,
    {
        let token = fauxgen::__private::token();
        let marker = token.marker();

        let fut = Box::pin(async move {
            let token = fauxgen::__private::register_owned(token).await;

            let start = token.argument().await;
            debug_assert!(matches!(start, CanStart::Inl(Start)));

            f(Yielder::new(token)).await
        }) as Pin<Box<dyn Future<Output = Return> + 'a>>;

        let generator = fauxgen::__private::gen_sync(marker, fut);
        Self {
            generator,
            _pin: PhantomPinned,
        }
    }
}

impl<'a, Effs, Return> CoSend<'a, Effs, Return>
where
    Effs: Effects<'a>,
    for<'r> Resumes<'r, CanStart<Effs>>: Send + Sync,
{
    pub fn new<F>(f: impl FnOnce(Yielder<'a, Effs>) -> F + Send + 'a) -> Self
    where
        F: Future<Output = Return> + Send,
    {
        let token = fauxgen::__private::token();
        let marker = token.marker();

        let fut = Box::pin(async move {
            let token = fauxgen::__private::register_owned(token).await;

            let start = token.argument().await;
            debug_assert!(matches!(start, CanStart::Inl(Start)));

            f(Yielder::new(token)).await
        }) as Pin<Box<dyn Future<Output = Return> + Send + 'a>>;

        let generator = fauxgen::__private::gen_sync(marker, fut);
        Self {
            generator,
            _pin: PhantomPinned,
        }
    }
}

impl<'a, Effs, Return, L: Locality> GenericCo<'a, Effs, Return, L>
where
    Effs: Effects<'a>,
{
    pub(crate) fn resume(
        self: Pin<&mut Self>,
        resume: Resumes<'a, CanStart<Effs>>,
    ) -> GeneratorState<CanStart<Effs>, Return> {
        let mut g = unsafe { self.map_unchecked_mut(|s| &mut s.generator) };
        Generator::resume(g.as_mut(), resume)
    }

    pub(crate) fn resume_with<R, Index>(
        self: Pin<&mut Self>,
        resume: R,
    ) -> GeneratorState<CanStart<Effs>, Return>
    where
        Resumes<'a, CanStart<Effs>>: CoprodInjector<R, Index>,
    {
        self.resume(Resumes::<'a, CanStart<Effs>>::inject(resume))
    }
}

pub struct Yielder<'a, Effs>
where
    Effs: MapResume,
{
    token: GeneratorToken<CanStart<Effs>, Resumes<'a, CanStart<Effs>>>,
}

impl<'a, Effs> Yielder<'a, Effs>
where
    Effs: MapResume,
{
    fn new(token: GeneratorToken<CanStart<Effs>, Resumes<'a, CanStart<Effs>>>) -> Self {
        Self { token }
    }

    pub async fn yield_<E, Index>(&self, effect: E) -> E::Resume<'a>
    where
        E: Effect,
        Effs: CoprodInjector<E, Index>,
        <Effs as MapResume>::Output<'a>: CoprodUninjector<E::Resume<'a>, Index>,
    {
        let resume = self
            .token
            .yield_(Coproduct::Inr(Effs::inject(effect)))
            .await;

        match resume {
            Coproduct::Inl(_) => unreachable!(),
            Coproduct::Inr(value) => match value.uninject() {
                Ok(value) => value,
                Err(_) => unreachable!(),
            },
        }
    }
}
