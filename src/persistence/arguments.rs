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

macro_rules! impl_arguments_for_tuple {
    () => {
        impl Arguments for () {
            fn get(&self, _index: usize) -> Argument<'_> {
                panic!("Index out of bounds")
            }

            fn len(&self) -> usize {
                0
            }
        }
    };
    ($($T:ident)+) => {
        impl<$($T,)+> Arguments for ($($T,)+)
        where
            $($T: AsArgument,)+
        {
            #[allow(non_snake_case, unused_assignments)]
            fn get(&self, index: usize) -> Argument<'_> {
                let ($($T,)+) = self;
                let mut i: usize = 0;
                $(
                    if index == i { return $T.as_argument(); }
                    i += 1;
                )+
                panic!("Index out of bounds")
            }

            #[allow(non_snake_case)]
            fn len(&self) -> usize {
                let ($($T,)+) = self;
                let mut i: usize = 0;
                $(
                    let _ = $T;
                    i += 1;
                )+
                i
            }
        }
    };
}

impl_arguments_for_tuple! {}
impl_arguments_for_tuple! { A }
impl_arguments_for_tuple! { A B }
impl_arguments_for_tuple! { A B C }
impl_arguments_for_tuple! { A B C D }
impl_arguments_for_tuple! { A B C D E }

#[cfg(test)]
mod tests {

    use super::{Argument, Arguments};

    #[test]
    fn unit_type_is_empty_arguments() {
        assert_eq!(().len(), 0);
    }

    #[test]
    fn tuple_element_access() {
        let args = (1, 2, 3, 4, 5);
        assert_eq!(args.len(), 5);
        assert_eq!(Argument::I64(1), args.get(0));
        assert_eq!(Argument::I64(2), args.get(1));
        assert_eq!(Argument::I64(3), args.get(2));
        assert_eq!(Argument::I64(4), args.get(3));
        assert_eq!(Argument::I64(5), args.get(4));
    }
}
