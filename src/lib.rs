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

/// An atomic version of `Option<Box<T>>`, useful for moving owned objects
/// between threads in a wait-free manner.
pub struct AtomicOption<T> {
    // Contains the address of a Box<T>, or 0 to indicate None
    inner: AtomicUsize,
    phantom: PhantomData<Option<Box<T>>>
}

unsafe impl<T> Sync for AtomicOption<T> {}

impl<T> AtomicOption<T> {
    /// Create a new AtomicOption storing the specified data.
    ///
    /// ```
    /// # use std::sync::atomic::Ordering;
    /// # use atomic_option::AtomicOption;
    /// let opt = AtomicOption::new(Box::new(7));
    /// let value = opt.take(Ordering::SeqCst).unwrap();
    /// assert_eq!(value, Box::new(7));
    /// ```
    #[inline]
    pub fn new(data: Box<T>) -> AtomicOption<T> {
        AtomicOption {
            inner: AtomicUsize::new(into_raw(data)),
            phantom: PhantomData
        }
    }

    /// Create a new AtomicOption storing None.
    ///
    /// ```
    /// # use std::sync::atomic::Ordering;
    /// # use atomic_option::AtomicOption;
    /// let opt: AtomicOption<()> = AtomicOption::empty();
    /// let value = opt.take(Ordering::SeqCst);
    /// assert!(value.is_none());
    /// ```
    #[inline]
    pub fn empty() -> AtomicOption<T> {
        AtomicOption {
            inner: AtomicUsize::new(NULL),
            phantom: PhantomData
        }
    }

    /// Take the value out of the AtomicOption, if there is one.
    ///
    /// ```
    /// # use std::sync::atomic::Ordering;
    /// # use atomic_option::AtomicOption;
    /// let opt = AtomicOption::new(Box::new(178));
    /// let first_take = opt.take(Ordering::SeqCst);
    /// let second_take = opt.take(Ordering::SeqCst);
    ///
    /// assert_eq!(first_take, Some(Box::new(178)));
    /// assert!(second_take.is_none());
    /// ```
    #[inline]
    pub fn take(&self, ordering: Ordering) -> Option<Box<T>> {
        self.replace(None, ordering)
    }

    /// Swap the value in the AtomicOption with a new one, returning the
    /// old value if there was one.
    ///
    /// ```
    /// # use std::sync::atomic::Ordering;
    /// # use atomic_option::AtomicOption;
    /// let opt = AtomicOption::new(Box::new(1236));
    /// let old = opt.swap(Box::new(542), Ordering::SeqCst).unwrap();
    /// assert_eq!(old, Box::new(1236));
    ///
    /// let new = opt.take(Ordering::SeqCst).unwrap();
    /// assert_eq!(new, Box::new(542));
    /// ```
    #[inline]
    pub fn swap(&self, new: Box<T>, ordering: Ordering) -> Option<Box<T>> {
        self.replace(Some(new), ordering)
    }

    /// Replace the Option in the AtomicOption with a new one, returning the old option.
    ///
    /// ```
    /// # use std::sync::atomic::Ordering;
    /// # use atomic_option::AtomicOption;
    /// let opt = AtomicOption::empty();
    /// let old = opt.replace(Some(Box::new("hello")), Ordering::SeqCst);
    /// assert!(old.is_none());
    ///
    /// let new = opt.take(Ordering::SeqCst).unwrap();
    /// assert_eq!(new, Box::new("hello"));
    /// ```
    #[inline]
    pub fn replace(&self, new: Option<Box<T>>, ordering: Ordering) -> Option<Box<T>> {
        let raw_new = new.map(into_raw).unwrap_or(0);

        match self.inner.swap(raw_new, ordering) {
            NULL => None,
            old => Some(unsafe { from_raw(old) })
        }
    }

    /// Store the new value in the AtomicOption iff it currently contains a None.
    ///
    /// None is returned if the store succeeded, or Some is returned with the rejected
    /// data if the store fails.
    ///
    /// This operation is implemented as a single atomic `compare_and_swap`.
    ///
    /// ```
    /// # use std::sync::atomic::Ordering;
    /// # use atomic_option::AtomicOption;
    /// let opt = AtomicOption::empty();
    /// let stored = opt.try_store(Box::new("some data"), Ordering::SeqCst);
    /// assert!(stored.is_none());
    ///
    /// let stored2 = opt.try_store(Box::new("some more data"), Ordering::SeqCst);
    /// assert_eq!(stored2, Some(Box::new("some more data")));
    ///
    /// let value = opt.take(Ordering::SeqCst).unwrap();
    /// assert_eq!(value, Box::new("some data"));
    /// ```
    #[inline]
    pub fn try_store(&self, new: Box<T>, ordering: Ordering) -> Option<Box<T>> {
        let raw_new = into_raw(new);

        match self.inner.compare_and_swap(NULL, raw_new, ordering) {
            NULL => None,
            _ => Some(unsafe { from_raw(raw_new) })
        }
    }

    /// Execute a `compare_and_swap` loop until there is a value in the AtomicOption,
    /// then return it.
    ///
    /// ```
    /// # use std::sync::atomic::Ordering;
    /// # use std::sync::Arc;
    /// # use std::thread;
    /// # use atomic_option::AtomicOption;
    ///
    /// // We'll use an AtomicOption as a lightweight channel transferring data via a spinlock.
    /// let tx = Arc::new(AtomicOption::empty());
    /// let rx = tx.clone();
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(*rx.spinlock(Ordering::Acquire), 7);
    /// });
    ///
    /// tx.swap(Box::new(7), Ordering::Release);
    /// ```
    #[inline]
    pub fn spinlock(&self, ordering: Ordering) -> Box<T> {
        loop {
            match self.replace(None, ordering) {
                Some(v) => return v,
                None => {}
            }
        }
    }

    /// Get the raw value stored in the AtomicOption.
    ///
    /// ## Safety
    ///
    /// It is almost *never* safe to read from this pointer.
    pub fn load_raw(&self, ordering: Ordering) -> *const T {
        self.inner.load(ordering) as *const T
    }
}

impl<T> From<Option<Box<T>>> for AtomicOption<T> {
    #[inline]
    fn from(opt: Option<Box<T>>) -> AtomicOption<T> {
        match opt {
            Some(data) => AtomicOption::new(data),
            None => AtomicOption::empty()
        }
    }
}

impl<T> Drop for AtomicOption<T> {
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

