use std::ops::Deref;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct Ir<T>(Rc<T>);

impl<T> AsRef<T> for Ir<T> {
    fn as_ref(&self) -> &T {
        self.0.as_ref()
    }
}

impl<T> Deref for Ir<T> {
    type Target = <Rc<T> as Deref>::Target;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

// TODO: can this be a derive?
impl<T> Clone for Ir<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Ir<T> {
    pub fn new(inner: T) -> Self {
        Self(Rc::new(inner))
    }
}

impl<T> Ir<T> {
    pub fn same(this: &Self, other: &Self) -> bool {
        return Rc::ptr_eq(&this.0, &other.0);
    }
}
