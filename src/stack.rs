//! Various sets of utilities to implement Forth stacks using Rust data types.
//!
//! A Forth stack is no more than a contiguous slice of memory owned by the
//! forth interpreter. These abstractions allow us to actually use different
//! implementations in the future without changing the public API.

use core::{marker::PhantomData, mem::MaybeUninit};

use crate::{error::Error, types::Address};

/// Storage for a forth stack of a given size and a given data type.
/// Alignment of the type must be respected.
pub struct StackStorage<const SIZE: usize, T: Sized>([MaybeUninit<T>; SIZE]);

impl<const SIZE: usize, T: Sized + Default + Copy> Default for StackStorage<SIZE, T> {
    fn default() -> Self {
        Self(unsafe { MaybeUninit::uninit().assume_init() })
    }
}

impl<const SIZE: usize, T: Sized + Default + Copy> StackStorage<SIZE, T> {
    pub const fn new() -> Self {
        Self(unsafe { MaybeUninit::uninit().assume_init() })
    }
}

/// Mutable reference to a StackStorage instance. Used by the Forth interpreter during runtime to
/// access the stack data.
pub struct Stack<'stack, T: Sized, const UP: bool = false> {
    storage: &'stack mut [MaybeUninit<T>],
}

impl<'stack, T: Sized, const UP: bool> Stack<'stack, T, UP> {
    /// Creates a new stack reference to the given stack storage. This is used by the core of the
    /// interpreter, and its type is independent of the actual size of the storage.
    pub fn new_with<const SIZE: usize>(storage: &'stack mut StackStorage<SIZE, T>) -> Self {
        Self {
            storage: &mut storage.0,
        }
    }

    pub fn properties(&mut self) -> StackProperties<'stack, T, UP> {
        let top = self.storage.as_ptr_range().end as Address;
        let bottom = self.storage.as_ptr_range().start as Address;
        let ptr = if UP { bottom } else { top };
        StackProperties {
            top,
            ptr,
            bottom,
            _pd: PhantomData,
        }
    }
}

/// Properties of a stack
#[repr(C)]
pub struct StackProperties<'a, T: Sized, const UP: bool = false> {
    pub ptr: Address,
    pub top: Address,
    pub bottom: Address,

    // Retain ownership of the stack data.
    _pd: PhantomData<&'a mut [T]>,
}

impl<'a, T: Sized> StackProperties<'a, T, true> {
    pub fn push(&mut self, v: T) -> Result<(), Error> {
        let new_ptr = self.ptr + core::mem::size_of::<T>() as Address;
        if new_ptr >= self.top {
            return Err(Error::StackOverflow);
        }

        unsafe { *(self.ptr as *mut T) = v };
        self.ptr = new_ptr;

        Ok(())
    }

    pub fn pop(&mut self) -> Result<T, Error> {
        let new_ptr = self.ptr - core::mem::size_of::<T>() as Address;
        if new_ptr < self.bottom {
            return Err(Error::StackUnderflow);
        }

        self.ptr = new_ptr;
        let v = unsafe { core::ptr::read(self.ptr as *mut T) };

        Ok(v)
    }
}

impl<'a, T: Sized> StackProperties<'a, T, false> {
    pub fn push(&mut self, v: T) -> Result<(), Error> {
        let new_ptr = self.ptr - core::mem::size_of::<T>() as Address;
        if new_ptr < self.bottom {
            return Err(Error::StackOverflow);
        }

        self.ptr = new_ptr;
        unsafe { *(self.ptr as *mut T) = v };

        Ok(())
    }

    pub fn pop(&mut self) -> Result<T, Error> {
        let new_ptr = self.ptr + core::mem::size_of::<T>() as Address;
        if new_ptr > self.top {
            return Err(Error::StackUnderflow);
        }

        let v = unsafe { core::ptr::read(self.ptr as *mut T) };
        self.ptr = new_ptr;

        Ok(v)
    }
}
