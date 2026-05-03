use std::borrow::Cow;

/// An argument passed to a query over the [`Persistence`] trait.
///
/// The persistence implementation has no knowledge about the nature of the query at compile time.
/// In particulary it does not know anything about the parameter types of the queries. Therfore we
/// represent [`Argument`] is an enumeration so it contains runtime information about the type of
/// the argument in addition to its value.
#[derive(Debug, PartialEq, Eq)]
pub enum Argument<'a> {
    I64(i64),
    Text(Cow<'a, str>),
    Blob(Cow<'a, [u8]>),
}

/// Borrow `self` as an [`Argument`]. The produced [`Argument`] borrows from `self` for the
/// lifetime of the borrow, which is what callers in the [`Arguments`] trait need.
pub trait AsArgument {
    fn as_argument(&self) -> Argument<'_>;
}

impl AsArgument for i64 {
    fn as_argument(&self) -> Argument<'_> {
        Argument::I64(*self)
    }
}

impl AsArgument for &[u8] {
    fn as_argument(&self) -> Argument<'_> {
        Argument::Blob(Cow::Borrowed(*self))
    }
}

impl AsArgument for &String {
    fn as_argument(&self) -> Argument<'_> {
        Argument::Text(Cow::Borrowed(self.as_str()))
    }
}

/// A collection of arguments. We use a generic trait, rather than a `Vec` or similar in order to
/// be able to implement it directly for structs and pass them without having the need to allocate
/// an intermediate representation.
pub trait Arguments {
    fn get(&self, index: usize) -> Argument<'_>;
    fn len(&self) -> usize;
}

impl<A, B, C, D, E> Arguments for (A, B, C, D, E)
where
    A: AsArgument,
    B: AsArgument,
    C: AsArgument,
    D: AsArgument,
    E: AsArgument,
{
    fn get(&self, index: usize) -> Argument<'_> {
        match index {
            0 => self.0.as_argument(),
            1 => self.1.as_argument(),
            2 => self.2.as_argument(),
            3 => self.3.as_argument(),
            4 => self.4.as_argument(),
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
