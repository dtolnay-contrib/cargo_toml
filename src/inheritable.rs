use crate::OptionalFile;
use serde::{Serialize, Deserialize};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Inheritable<T> {
    Set(T),
    Inherited { workspace: bool },
}

impl<T> Inheritable<T> {
    pub fn as_ref(&self) -> Inheritable<&T> {
        match self {
            Self::Set(t) => Inheritable::Set(t),
            Self::Inherited{..} => Inheritable::Inherited{workspace:true},
        }
    }

    pub fn get(&self) -> Option<&T> {
        match self {
            Self::Set(t) => Some(t),
            Self::Inherited{..} => None,
        }
    }

    pub fn as_mut(&mut self) -> Inheritable<&mut T> {
        match self {
            Self::Set(t) => Inheritable::Set(t),
            Self::Inherited{..} => Inheritable::Inherited{workspace:true},
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Set(t) => Some(t),
            Self::Inherited{..} => None,
        }
    }

    #[track_caller]
    pub fn unwrap(self) -> T {
        match self {
            Self::Set(t) => t,
            Self::Inherited{..} => panic!("inherited workspace value"),
        }
    }
}

impl<T: Default> Default for Inheritable<T> {
    fn default() -> Self {
        Self::Set(T::default())
    }
}

impl<T> Inheritable<Vec<T>> {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Inherited{..} => false,
            Self::Set(v) => v.is_empty(),
        }
    }
}

impl Inheritable<OptionalFile> {
    pub fn is_default(&self) -> bool {
        match self {
            Self::Inherited{..} => false,
            Self::Set(v) => v.is_default(),
        }
    }
}

impl<T> From<Option<T>> for Inheritable<T> {
    fn from(val: Option<T>) -> Self {
        match val {
            Some(val) => Self::Set(val),
            None => Self::Inherited{workspace:true},
        }
    }
}

impl<T> From<Inheritable<T>> for Option<T> {
    fn from(val: Inheritable<T>) -> Self {
        match val {
            Inheritable::Inherited{..} => None,
            Inheritable::Set(val) => Some(val),
        }
    }
}