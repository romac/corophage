use std::future::Future;
use std::pin::Pin;

use frunk::coproduct::{CoprodInjector, CoprodUninjector, Coproduct};
use genawaiter::sync as gen;
use genawaiter::GeneratorState;

use crate::effect::CanStart;
use crate::effect::Effect;
use crate::effect::Effects;
use crate::effect::MapResume;
use crate::effect::Resumes;

type PinBoxFuture<A> = Pin<Box<dyn Future<Output = A> + 'static + Send>>;
type Gen<Effs, Return> = gen::Gen<CanStart<Effs>, Resumes<CanStart<Effs>>, PinBoxFuture<Return>>;

pub struct Co<Effs, Return>
where
    Effs: Effects,
{
    gen: Gen<Effs, Return>,
}

impl<Effs, Return> Co<Effs, Return>
where
    Effs: Effects,
{
    pub fn new<F>(f: impl FnOnce(Yielder<Effs>) -> F) -> Self
    where
        F: Future<Output = Return> + Send + 'static,
    {
        Self {
            gen: Gen::new_boxed(|co| f(Yielder::new(co))),
        }
    }

    pub(crate) fn resume(
        &mut self,
        resume: Resumes<CanStart<Effs>>,
    ) -> GeneratorState<CanStart<Effs>, Return> {
        self.gen.resume_with(resume)
    }

    pub(crate) fn resume_with<R, Index>(
        &mut self,
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
    co: gen::Co<CanStart<Effs>, Resumes<CanStart<Effs>>>,
}

impl<Effs> Yielder<Effs>
where
    Effs: MapResume,
{
    fn new(co: gen::Co<CanStart<Effs>, Resumes<CanStart<Effs>>>) -> Self {
        Self { co }
    }

    pub async fn yield_<E, Index>(&self, effect: E) -> E::Resume
    where
        E: Effect,
        Effs: CoprodInjector<E, Index>,
        <Effs as MapResume>::Output: CoprodUninjector<E::Resume, Index>,
    {
        let resume = self.co.yield_(Coproduct::Inr(Effs::inject(effect))).await;

        match resume {
            Coproduct::Inl(_) => unreachable!(),
            Coproduct::Inr(value) => match value.uninject() {
                Ok(value) => value,
                Err(_) => unreachable!(),
            },
        }
    }
}
