pub enum Never {}

pub trait Effect {
    type Injection;
}

pub struct Cancel;

impl Effect for Cancel {
    /// Resuming an effectful function after it has cancelled is impossible.
    type Injection = Never;
}

pub struct Log<'a>(&'a str);

impl<'a> Effect for Log<'a> {
    /// The logging handler does not provide any information back to the effectful function.
    type Injection = ();
}

pub struct FileRead(String);

impl Effect for FileRead {
    /// For this example, we pretend files are just strings, and the whole file is read at once.
    type Injection = String;
}
