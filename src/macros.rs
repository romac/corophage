#[macro_export]
macro_rules! Effects {
    [$($effect:ty),*] => {
        ::frunk::Coprod!($($effect),*)
    };
}
