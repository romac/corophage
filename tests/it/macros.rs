use corophage::declare_effect;

declare_effect!(NoArg -> ());

declare_effect!(OneArg(i32) -> bool);

declare_effect!(TwoArgs(i32, String) -> String);

declare_effect!(WithLifetime<'a>(&'a str) -> bool);

declare_effect!(WithResumeLifetime(i32) -> &'r str);

declare_effect!(Generic<T: std::fmt::Debug>(T) -> T);

declare_effect!(NamedFields { path: String, recursive: bool } -> Vec<u8>);

declare_effect!(NamedWithLifetime<'a> { msg: &'a str } -> bool);

declare_effect!(NamedGeneric<T: std::fmt::Debug> { value: T } -> T);
