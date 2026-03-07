/// Constructs a coproduct type from a list of [`Effect`](crate::Effect) types.
///
/// `Effects![A, B, C]` expands to `Coprod!(A, B, C)` from `frunk_core`.
///
/// # Example
///
/// ```ignore
/// type MyEffects = Effects![Log, Ask];
/// ```
#[macro_export]
macro_rules! Effects {
    [$($effect:ty),*] => {
        ::frunk_core::Coprod!($($effect),*)
    };
}

macro_rules! run {
    ($lt:lifetime, $effs:ty, $co:expr, $effect:pat => $handle:expr) => {{
        let mut co = ::std::pin::pin!($co);

        let mut yielded = co.as_mut().resume_with($crate::effect::Start);

        loop {
            match yielded {
                ::fauxgen::GeneratorState::Complete(value) => break Ok(value),

                ::fauxgen::GeneratorState::Yielded(effect) => {
                    let $effect = match effect {
                        ::frunk_core::coproduct::Coproduct::Inl(_) => unreachable!(),
                        ::frunk_core::coproduct::Coproduct::Inr(subeffect) => subeffect,
                    };

                    let resume: $crate::control::CoControl<$lt, $effs> = $handle;
                    match resume {
                        $crate::control::CoControl::Cancel => {
                            break Err($crate::control::Cancelled);
                        }
                        $crate::control::CoControl::Resume(r) => {
                            yielded = co
                                .as_mut()
                                .resume(::frunk_core::coproduct::Coproduct::Inr(r))
                        }
                    }
                }
            }
        }
    }};
}
