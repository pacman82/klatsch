use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq)]
pub enum Parameter<'a> {
    I64(i64),
    Text(Cow<'a, str>),
    Blob(Cow<'a, [u8]>),
}

impl<'a> From<&'a i64> for Parameter<'static> {
    fn from(value: &'a i64) -> Self {
        Parameter::I64(*value)
    }
}

impl<'a> From<&'a [u8]> for Parameter<'a> {
    fn from(value: &'a [u8]) -> Self {
        Parameter::Blob(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for Parameter<'a> {
    fn from(value: &'a String) -> Self {
        Parameter::Text(Cow::Borrowed(value.as_str()))
    }
}

pub trait Parameters {
    fn get(&self, index: usize) -> Parameter<'_>;
    fn len(&self) -> usize;
}

impl Parameters for (i64, &[u8], &String, &String, i64) {
    fn get(&self, index: usize) -> Parameter<'_> {
        match index {
            0 => (&self.0).into(),
            1 => self.1.into(),
            2 => self.2.into(),
            3 => self.3.into(),
            4 => (&self.4).into(),
            _ => panic!("Index out of bounds"),
        }
    }

    fn len(&self) -> usize {
        5
    }
}

impl Parameters for i64 {
    fn get(&self, index: usize) -> Parameter<'_> {
        match index {
            0 => Parameter::I64(*self),
            _ => panic!("Index out of bounds"),
        }
    }

    fn len(&self) -> usize {
        1
    }
}

impl Parameters for &'_ [u8] {
    fn get(&self, index: usize) -> Parameter<'_> {
        match index {
            0 => Parameter::Blob(Cow::Borrowed(*self)),
            _ => panic!("Index out of bounds"),
        }
    }

    fn len(&self) -> usize {
        1
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
