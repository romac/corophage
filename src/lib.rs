mod coproduct;
use coproduct::CoproductFoldableMut;

mod effect;
use effect::{Effects, Resumes, Start};

mod coroutine;
mod macros;

pub use coroutine::{Co, Yielder};
pub use effect::Effect;

use frunk::coproduct::CoprodInjector;
use genawaiter::GeneratorState;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cancelled;

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

pub fn run<Effs, Return, F>(mut co: Co<Effs, Return>, handler: &mut F) -> Result<Return, Cancelled>
where
    Effs: Effects + CoproductFoldableMut<F, CoControl<Effs>>,
{
    use frunk::Coproduct;

    let mut yielded = co.resume_with(Start);

    loop {
        match yielded {
            GeneratorState::Complete(value) => break Ok(value),

            GeneratorState::Yielded(effect) => {
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
