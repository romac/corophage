#[derive(Debug)]
pub struct Token(u64);

#[macro_export]
macro_rules! unique_token {
    () => {{
        use ::std::hash::{Hash, Hasher};

        struct PlaceHolder;
        let id = ::std::any::TypeId::of::<PlaceHolder>();
        let mut hasher = ::std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut hasher);
        Token(hasher.finish())
    }};
}
