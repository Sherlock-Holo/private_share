use futures_util::stream::{self, Iter};

pub trait IterExt {
    type Iter;

    fn into_stream(self) -> Iter<Self::Iter>;
}

impl<T: IntoIterator> IterExt for T {
    type Iter = T::IntoIter;

    fn into_stream(self) -> Iter<Self::Iter> {
        stream::iter(self.into_iter())
    }
}
