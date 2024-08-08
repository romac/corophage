use frunk::coproduct::CNil;
use frunk::Coproduct;

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

pub trait Effects: MapResume {}

impl<E> Effects for E where E: MapResume {}

pub type Resumes<E> = <E as MapResume>::Output;

pub struct Start;

impl Effect for Start {
    type Resume = Start;
}

pub type CanStart<Effs> = Coproduct<Start, Effs>;
