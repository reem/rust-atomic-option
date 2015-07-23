#![cfg_attr(test, deny(warnings))]
#![deny(missing_docs)]

//! # atomic-option
//!
//! An atomic, nullable, owned pointer.
//!

use std::mem;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

const NULL: usize = 0;

pub struct AtomicOption<T: Send> {
    // Contains the address of a Box<T>, or 0 to indicate None
    inner: AtomicUsize,
    phantom: PhantomData<Option<T>>
}

impl<T: Send> AtomicOption<T> {
    #[inline]
    pub fn new(data: Box<T>) -> AtomicOption<T> {
        AtomicOption {
            inner: AtomicUsize::new(into_raw(data)),
            phantom: PhantomData
        }
    }

    #[inline]
    pub fn empty() -> AtomicOption<T> {
        AtomicOption {
            inner: AtomicUsize::new(NULL),
            phantom: PhantomData
        }
    }

    #[inline]
    pub fn take(&self, ordering: Ordering) -> Option<Box<T>> {
        self.replace(None, ordering)
    }

    #[inline]
    pub fn swap(&self, new: Box<T>, ordering: Ordering) -> Option<Box<T>> {
        self.replace(Some(new), ordering)
    }

    #[inline]
    pub fn replace(&self, new: Option<Box<T>>, ordering: Ordering) -> Option<Box<T>> {
        let raw_new = new.map(into_raw).unwrap_or(0);

        match self.inner.swap(raw_new, ordering) {
            NULL => None,
            old => Some(unsafe { from_raw(old) })
        }
    }

    #[inline]
    pub fn try_store(&self, new: Box<T>, ordering: Ordering) -> Option<Box<T>> {
        let raw_new = into_raw(new);

        match self.inner.compare_and_swap(NULL, raw_new, ordering) {
            NULL => None,
            _ => Some(unsafe { from_raw(raw_new) })
        }
    }
}

impl<T: Send> From<Option<Box<T>>> for AtomicOption<T> {
    #[inline]
    fn from(opt: Option<Box<T>>) -> AtomicOption<T> {
        match opt {
            Some(data) => AtomicOption::new(data),
            None => AtomicOption::empty()
        }
    }
}

impl<T: Send> Drop for AtomicOption<T> {
    fn drop(&mut self) {
        let _ = self.take(Ordering::SeqCst);
    }
}

#[inline(always)]
fn into_raw<T>(data: Box<T>) -> usize {
    unsafe { mem::transmute(data) }
}

#[inline(always)]
unsafe fn from_raw<T>(data: usize) -> Box<T> {
    mem::transmute(data)
}

