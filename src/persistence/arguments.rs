use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq)]
pub enum Argument<'a> {
    I64(i64),
    Text(Cow<'a, str>),
    Blob(Cow<'a, [u8]>),
}

impl<'a> From<&'a i64> for Argument<'static> {
    fn from(value: &'a i64) -> Self {
        Argument::I64(*value)
    }
}

impl<'a> From<&'a [u8]> for Argument<'a> {
    fn from(value: &'a [u8]) -> Self {
        Argument::Blob(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for Argument<'a> {
    fn from(value: &'a String) -> Self {
        Argument::Text(Cow::Borrowed(value.as_str()))
    }
}

pub trait Arguments {
    fn get(&self, index: usize) -> Argument<'_>;
    fn len(&self) -> usize;
}

impl Arguments for (i64, &[u8], &String, &String, i64) {
    fn get(&self, index: usize) -> Argument<'_> {
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

impl Arguments for i64 {
    fn get(&self, index: usize) -> Argument<'_> {
        match index {
            0 => Argument::I64(*self),
            _ => panic!("Index out of bounds"),
        }
    }

    fn len(&self) -> usize {
        1
    }
}

impl Arguments for &'_ [u8] {
    fn get(&self, index: usize) -> Argument<'_> {
        match index {
            0 => Argument::Blob(Cow::Borrowed(*self)),
            _ => panic!("Index out of bounds"),
        }
    }

    fn len(&self) -> usize {
        1
    }
}

impl Arguments for () {
    fn get(&self, _index: usize) -> Argument<'_> {
        panic!("Index out of bounds")
    }

    fn len(&self) -> usize {
        0
    }
}
