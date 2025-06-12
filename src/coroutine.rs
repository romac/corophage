use std::future::Future;
use std::pin::Pin;

use fauxgen::__private::SyncGenerator;
use fauxgen::Generator;
use fauxgen::GeneratorState;
use fauxgen::GeneratorToken;
use frunk::coproduct::{CoprodInjector, CoprodUninjector, Coproduct};

use crate::effect::{CanStart, Effect, Effects, MapResume, Resumes, Start};

type PinBoxFuture<A> = Pin<Box<dyn Future<Output = A>>>;

type Gen<Effs, Return> =
    SyncGenerator<PinBoxFuture<Return>, CanStart<Effs>, Resumes<CanStart<Effs>>>;

pub struct Co<Effs, Return>
where
    Effs: Effects,
{
    generator: Gen<Effs, Return>,
}

impl<Effs, Return> Co<Effs, Return>
where
    Effs: Effects,
{
    pub fn new<F>(f: impl FnOnce(Yielder<Effs>) -> F + 'static) -> Self
    where
        F: Future<Output = Return>,
    {
        let func = |token: GeneratorToken<_, _>| f(Yielder::new(token));

        let token = fauxgen::__private::token();
        let marker = token.marker();

        let fut = Box::pin(async move {
            let token = fauxgen::__private::register_owned(token).await;

            let start = token.argument().await;
            debug_assert!(matches!(start, CanStart::Inl(Start)));

            func(token).await
        }) as PinBoxFuture<Return>;

        let generator = fauxgen::__private::gen_sync(marker, fut);
        Self { generator }
    }

    pub(crate) fn resume(
        self: Pin<&mut Self>,
        resume: Resumes<CanStart<Effs>>,
    ) -> GeneratorState<CanStart<Effs>, Return> {
        let mut g = unsafe { self.map_unchecked_mut(|s| &mut s.generator) };
        g.as_mut().resume(resume)
    }

    pub(crate) fn resume_with<R, Index>(
        self: Pin<&mut Self>,
        resume: R,
    ) -> GeneratorState<CanStart<Effs>, Return>
    where
        Resumes<CanStart<Effs>>: CoprodInjector<R, Index>,
    {
        self.resume(Resumes::<CanStart<Effs>>::inject(resume))
    }
}

pub struct Yielder<Effs>
where
    Effs: MapResume,
{
    token: GeneratorToken<CanStart<Effs>, Resumes<CanStart<Effs>>>,
}

impl<Effs> Yielder<Effs>
where
    Effs: MapResume,
{
    fn new(token: GeneratorToken<CanStart<Effs>, Resumes<CanStart<Effs>>>) -> Self {
        Self { token }
    }

    pub async fn yield_<E, Index>(&self, effect: E) -> E::Resume
    where
        E: Effect,
        Effs: CoprodInjector<E, Index>,
        <Effs as MapResume>::Output: CoprodUninjector<E::Resume, Index>,
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
