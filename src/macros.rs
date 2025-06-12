#[macro_export]
macro_rules! Effects {
    [$($effect:ty),*] => {
        ::frunk_core::Coprod!($($effect),*)
    };
}
