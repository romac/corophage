use frunk::coproduct::{CNil, Coproduct};

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

pub trait Effects: MapResume + 'static {}

impl<E> Effects for E where E: MapResume + 'static {}

pub type Resumes<E> = <E as MapResume>::Output;

#[derive(Copy, Clone, Debug)]
pub struct Start;

impl Effect for Start {
    type Resume = Start;
}

pub type CanStart<Effs> = Coproduct<Start, Effs>;
