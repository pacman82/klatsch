use std::borrow::Cow;

pub enum Parameter<'a> {
    I64(i64),
    Text(Cow<'a, str>),
    Blob(Cow<'a, [u8]>),
}

impl<'a> Parameter<'a> {
    pub fn borrowed(&self) -> Parameter<'_> {
        match self {
            Self::I64(value) => Parameter::I64(*value),
            Self::Text(value) => Parameter::Text(Cow::Borrowed(value.as_ref())),
            Self::Blob(value) => Parameter::Blob(Cow::Borrowed(value.as_ref())),
        }
    }
}

pub trait Parameters {
    fn get(&self, index: usize) -> Parameter<'_>;
    fn len(&self) -> usize;
}

pub trait AsParameters {
    fn as_params(&self) -> impl Parameters;
}

impl Parameters for Parameter<'_> {
    fn get(&self, index: usize) -> Parameter<'_> {
        if index == 0 {
            self.borrowed()
        } else {
            panic!("Index out of bounds")
        }
    }

    fn len(&self) -> usize {
        1
    }
}

impl Parameters
    for (
        Parameter<'_>,
        Parameter<'_>,
        Parameter<'_>,
        Parameter<'_>,
        Parameter<'_>,
    )
{
    fn get(&self, index: usize) -> Parameter<'_> {
        match index {
            0 => self.0.borrowed(),
            1 => self.1.borrowed(),
            2 => self.2.borrowed(),
            3 => self.3.borrowed(),
            4 => self.4.borrowed(),
            _ => panic!("Index out of bounds"),
        }
    }

    fn len(&self) -> usize {
        5
    }
}

impl From<i64> for Parameter<'static> {
    fn from(value: i64) -> Self {
        Parameter::I64(value)
    }
}

impl AsParameters for i64 {
    fn as_params(&self) -> impl Parameters {
        Parameter::I64(*self)
    }
}

impl<'a> AsParameters for &'a [u8] {
    fn as_params(&self) -> impl Parameters {
        Parameter::Blob(Cow::Borrowed(*self))
    }
}

impl Parameters for () {
    fn get(&self, _index: usize) -> Parameter<'_> {
        panic!("Index out of bounds")
    }

    fn len(&self) -> usize {
        0
    }
}

impl AsParameters for () {
    fn as_params(&self) -> impl Parameters {
        ()
    }
}

impl AsParameters for (i64, &[u8], &String, &String, i64) {
    fn as_params(&self) -> impl Parameters {
        (
            Parameter::I64(self.0),
            Parameter::Blob(Cow::Borrowed(self.1)),
            Parameter::Text(Cow::Borrowed(self.2.as_str())),
            Parameter::Text(Cow::Borrowed(self.3.as_str())),
            Parameter::I64(self.4),
        )
    }
}
