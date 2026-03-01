use frunk_core::coproduct::{CNil, Coproduct};

pub trait Effect {
    type Resume<'r>: Sync + Send;
}

pub trait MapResume {
    type Output<'r>: Sync + Send;
}

impl MapResume for CNil {
    type Output<'r> = CNil;
}

impl<H: Effect, T: MapResume> MapResume for Coproduct<H, T> {
    type Output<'r> = Coproduct<H::Resume<'r>, <T as MapResume>::Output<'r>>;
}

pub trait Effects<'a>: MapResume + Send + Sync + 'a {}

impl<'a, E> Effects<'a> for E where E: MapResume + Send + Sync + 'a {}

pub type Resumes<'r, E> = <E as MapResume>::Output<'r>;

#[derive(Copy, Clone, Debug)]
pub struct Start;

impl Effect for Start {
    type Resume<'r> = Start;
}

pub type CanStart<Effs> = Coproduct<Start, Effs>;
