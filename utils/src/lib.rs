// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! This crate contains utility traits, functions, and macros used by other crates of Winterfell
//! STARK prover and verifier.

use core::{convert::TryInto, mem, slice};

mod iterators;

mod errors;
pub use errors::DeserializationError;

#[cfg(test)]
mod tests;

#[cfg(feature = "concurrent")]
use rayon::prelude::*;

// SERIALIZABLE
// ================================================================================================

/// Defines how to serialize `Self` into bytes.
pub trait Serializable: Sized {
    // REQUIRED METHODS
    // --------------------------------------------------------------------------------------------
    /// Serializes `self` into bytes and appends the bytes at the end of the `target` vector.
    fn write_into<W: ByteWriter>(&self, target: &mut W);

    // PROVIDED METHODS
    // --------------------------------------------------------------------------------------------

    /// Serializes `self` into a vector of bytes.
    fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.get_size_hint());
        self.write_into(&mut result);
        result
    }

    /// Serializes all elements of the `source` and appends the resulting bytes at the end of
    /// the `target` vector.
    ///
    /// This method does not write any metadata (e.g. number of serialized elements) into the
    /// `target`.
    fn write_batch_into<W: ByteWriter>(source: &[Self], target: &mut W) {
        for item in source {
            item.write_into(target);
        }
    }

    /// Serializes all individual elements contained in the `source` and appends the resulting
    /// bytes at the end of the `target` vector.
    ///
    /// This method does not write any metadata (e.g. number of serialized elements) into the
    /// `target`.
    fn write_array_batch_into<W: ByteWriter, const N: usize>(source: &[[Self; N]], target: &mut W) {
        let source = flatten_slice_elements(source);
        Self::write_batch_into(source, target);
    }

    /// Returns an estimate of how many bytes are needed to represent self.
    ///
    /// The default implementation returns zero.
    fn get_size_hint(&self) -> usize {
        0
    }
}

impl Serializable for () {
    fn write_into<W: ByteWriter>(&self, _target: &mut W) {}
}

// BYTE READER
// ================================================================================================

/// Defines how primitive values are to be read from `Self`.
pub trait ByteReader {
    /// Returns a single byte read from `self` at the specified position.
    ///
    /// After the byte is read, `pos` is incremented by one.
    ///
    /// # Errors
    /// Returns a `DeserializationError` error if `pos` is out of bounds.
    fn read_u8(&self, pos: &mut usize) -> Result<u8, DeserializationError>;

    /// Returns a u16 value read from `self` in little-endian byte order starting at the specified
    /// position.
    ///
    /// After the value is read, `pos` is incremented by two.
    ///
    /// # Errors
    /// Returns an error if a u16 value could not be read from `self`.
    fn read_u16(&self, pos: &mut usize) -> Result<u16, DeserializationError>;

    /// Returns a u32 value read from `self` in little-endian byte order starting at the specified
    /// position.
    ///
    /// After the value is read, `pos` is incremented by four.
    ///
    /// # Errors
    /// Returns an error if a u32 value could not be read from `self`.
    fn read_u32(&self, pos: &mut usize) -> Result<u32, DeserializationError>;

    /// Returns a u64 value read from `self` in little-endian byte order starting at the specified
    /// position.
    ///
    /// After the value is read, `pos` is incremented by eight.
    ///
    /// # Errors
    /// Returns an error if a u64 value could not be read from `self`.
    fn read_u64(&self, pos: &mut usize) -> Result<u64, DeserializationError>;

    /// Returns a byte vector of the specified length read from `self` starting at the specified
    /// position.
    ///
    /// After the vector is read, `pos` is incremented by the length of the vector.
    ///
    /// # Errors
    /// Returns an error if a vector of the specified length could not be read from `self`.
    fn read_u8_vec(&self, pos: &mut usize, len: usize) -> Result<Vec<u8>, DeserializationError>;
}

impl ByteReader for [u8] {
    fn read_u8(&self, pos: &mut usize) -> Result<u8, DeserializationError> {
        if *pos >= self.len() {
            return Err(DeserializationError::UnexpectedEOF);
        }
        let result = self[*pos];

        *pos += 1;
        Ok(result)
    }

    fn read_u16(&self, pos: &mut usize) -> Result<u16, DeserializationError> {
        let end_pos = *pos + 2;
        if end_pos > self.len() {
            return Err(DeserializationError::UnexpectedEOF);
        }

        let result = u16::from_le_bytes(
            self[*pos..end_pos]
                .try_into()
                .map_err(|err| DeserializationError::UnknownError(format!("{}", err)))?,
        );

        *pos = end_pos;
        Ok(result)
    }

    fn read_u32(&self, pos: &mut usize) -> Result<u32, DeserializationError> {
        let end_pos = *pos + 4;
        if end_pos > self.len() {
            return Err(DeserializationError::UnexpectedEOF);
        }

        let result = u32::from_le_bytes(
            self[*pos..end_pos]
                .try_into()
                .map_err(|err| DeserializationError::UnknownError(format!("{}", err)))?,
        );

        *pos = end_pos;
        Ok(result)
    }

    fn read_u64(&self, pos: &mut usize) -> Result<u64, DeserializationError> {
        let end_pos = *pos + 8;
        if end_pos > self.len() {
            return Err(DeserializationError::UnexpectedEOF);
        }

        let result = u64::from_le_bytes(
            self[*pos..end_pos]
                .try_into()
                .map_err(|err| DeserializationError::UnknownError(format!("{}", err)))?,
        );

        *pos = end_pos;
        Ok(result)
    }

    fn read_u8_vec(&self, pos: &mut usize, len: usize) -> Result<Vec<u8>, DeserializationError> {
        let end_pos = *pos + len as usize;
        if end_pos > self.len() {
            return Err(DeserializationError::UnexpectedEOF);
        }
        let result = self[*pos..end_pos].to_vec();
        *pos = end_pos;
        Ok(result)
    }
}

impl ByteReader for Vec<u8> {
    fn read_u8(&self, pos: &mut usize) -> Result<u8, DeserializationError> {
        self.as_slice().read_u8(pos)
    }

    fn read_u16(&self, pos: &mut usize) -> Result<u16, DeserializationError> {
        self.as_slice().read_u16(pos)
    }

    fn read_u32(&self, pos: &mut usize) -> Result<u32, DeserializationError> {
        self.as_slice().read_u32(pos)
    }

    fn read_u64(&self, pos: &mut usize) -> Result<u64, DeserializationError> {
        self.as_slice().read_u64(pos)
    }

    fn read_u8_vec(&self, pos: &mut usize, len: usize) -> Result<Vec<u8>, DeserializationError> {
        self.as_slice().read_u8_vec(pos, len)
    }
}

// BYTE WRITER
// ================================================================================================

/// Defines how primitive values are to be written into `Self`.
pub trait ByteWriter: Sized {
    // REQUIRED METHODS
    // --------------------------------------------------------------------------------------------

    /// Writes a single byte into `self`.
    ///
    /// # Panics
    /// Panics if the byte could not be written into `self`.
    fn write_u8(&mut self, value: u8);

    /// Writes a sequence of bytes into `self`.
    ///
    /// # Panics
    /// Panics if the sequence of bytes could not be written into `self`.
    fn write_u8_slice(&mut self, values: &[u8]);

    // PROVIDED METHODS
    // --------------------------------------------------------------------------------------------

    /// Writes a u16 value in little-endian byte order into `self`.
    ///
    /// # Panics
    /// Panics if the value could not be written into `self`.
    fn write_u16(&mut self, value: u16) {
        self.write_u8_slice(&value.to_le_bytes());
    }

    /// Writes a u32 value in little-endian byte order into `self`.
    ///
    /// # Panics
    /// Panics if the value could not be written into `self`.
    fn write_u32(&mut self, value: u32) {
        self.write_u8_slice(&value.to_le_bytes());
    }

    /// Writes a u64 value in little-endian byte order into `self`.
    ///
    /// # Panics
    /// Panics if the value could not be written into `self`.
    fn write_u64(&mut self, value: u64) {
        self.write_u8_slice(&value.to_le_bytes());
    }

    /// Writes a single serializable value into `self`.
    ///
    /// # Panics
    /// Panics if the value could not be written into `self`.
    fn write<S: Serializable>(&mut self, value: S) {
        value.write_into(self)
    }

    /// Writes a sequence of serializable values into `self`.
    ///
    /// # Panics
    /// Panics if the values could not be written into `self`.
    fn write_slice<S: Serializable>(&mut self, values: &[S]) {
        S::write_batch_into(values, self)
    }

    /// Writes a table of serializable values into `self`.
    ///
    /// # Panics
    /// Panics if the values could not be written into `self`.
    fn write_table<S: Serializable, const N: usize>(&mut self, values: &[[S; N]]) {
        S::write_array_batch_into(values, self);
    }
}

impl ByteWriter for Vec<u8> {
    fn write_u8(&mut self, value: u8) {
        self.push(value);
    }

    fn write_u8_slice(&mut self, values: &[u8]) {
        self.extend_from_slice(values);
    }
}

// AS BYTES
// ================================================================================================

/// Defines a zero-copy representation of `Self` as a sequence of bytes.
pub trait AsBytes {
    /// Returns a byte representation of `self`.
    ///
    /// This method is intended to re-interpret the underlying memory as a sequence of bytes, and
    /// thus, should be zero-copy.
    fn as_bytes(&self) -> &[u8];
}

impl<const N: usize, const M: usize> AsBytes for [[u8; N]; M] {
    /// Flattens a two-dimensional array of bytes into a slice of bytes.
    fn as_bytes(&self) -> &[u8] {
        let p = self.as_ptr();
        let len = N * M;
        unsafe { slice::from_raw_parts(p as *const u8, len) }
    }
}

impl<const N: usize> AsBytes for [[u8; N]] {
    /// Flattens a slice of byte arrays into a slice of bytes.
    fn as_bytes(&self) -> &[u8] {
        let p = self.as_ptr();
        let len = self.len() * N;
        unsafe { slice::from_raw_parts(p as *const u8, len) }
    }
}

// VECTOR FUNCTIONS
// ================================================================================================

/// Returns a vector of the specified length with un-initialized memory.
///
/// This is usually faster than requesting a vector with initialized memory and is useful when we
/// overwrite all contents of the vector immediately after memory allocation.
///
/// # Safety
/// Using values from the returned vector before initializing them will lead to undefined behavior.
pub unsafe fn uninit_vector<T>(length: usize) -> Vec<T> {
    let mut vector = Vec::with_capacity(length);
    vector.set_len(length);
    vector
}

// GROUPING / UN-GROUPING FUNCTIONS
// ================================================================================================

/// Transmutes a vector of `n` elements into a vector of `n` / `N` elements, each of which is
/// an array of `N` elements.
///
/// This function just re-interprets the underlying memory and is thus zero-copy.
/// # Panics
/// Panics if `n` is not divisible by `N`.
///
/// # Example
/// ```
/// # use winter_utils::group_vector_elements;
/// let a = vec![0_u32, 1, 2, 3, 4, 5, 6, 7];
/// let b: Vec<[u32; 2]> = group_vector_elements(a);
///
/// assert_eq!(vec![[0, 1], [2, 3], [4, 5], [6, 7]], b);
/// ```
pub fn group_vector_elements<T, const N: usize>(source: Vec<T>) -> Vec<[T; N]> {
    assert_eq!(
        source.len() % N,
        0,
        "source length must be divisible by {}, but was {}",
        N,
        source.len()
    );
    let mut v = mem::ManuallyDrop::new(source);
    let p = v.as_mut_ptr();
    let len = v.len() / N;
    let cap = v.capacity() / N;
    unsafe { Vec::from_raw_parts(p as *mut [T; N], len, cap) }
}

/// Transmutes a slice of `n` elements into a slice of `n` / `N` elements, each of which is
/// an array of `N` elements.
///
/// This function just re-interprets the underlying memory and is thus zero-copy.
/// # Panics
/// Panics if `n` is not divisible by `N`.
///
/// # Example
/// ```
/// # use winter_utils::group_slice_elements;
/// let a = [0_u32, 1, 2, 3, 4, 5, 6, 7];
/// let b: &[[u32; 2]] = group_slice_elements(&a);
///
/// assert_eq!(&[[0, 1], [2, 3], [4, 5], [6, 7]], b);
/// ```
pub fn group_slice_elements<T, const N: usize>(source: &[T]) -> &[[T; N]] {
    assert_eq!(
        source.len() % N,
        0,
        "source length must be divisible by {}",
        N
    );
    let p = source.as_ptr();
    let len = source.len() / N;
    unsafe { slice::from_raw_parts(p as *const [T; N], len) }
}

/// Transmutes a slice of `n` arrays each of length `N`, into a slice of `N` * `n` elements.
///
/// This function just re-interprets the underlying memory and is thus zero-copy.
/// # Example
/// ```
/// # use winter_utils::flatten_slice_elements;
/// let a = vec![[1, 2, 3, 4], [5, 6, 7, 8]];
///
/// let b = flatten_slice_elements(&a);
/// assert_eq!(&[1, 2, 3, 4, 5, 6, 7, 8], b);
/// ```
pub fn flatten_slice_elements<T, const N: usize>(source: &[[T; N]]) -> &[T] {
    let p = source.as_ptr();
    let len = source.len() * N;
    unsafe { slice::from_raw_parts(p as *const T, len) }
}

/// Transmutes a vector of `n` arrays each of length `N`, into a vector of `N` * `n` elements.
///
/// This function just re-interprets the underlying memory and is thus zero-copy.
/// # Example
/// ```
/// # use winter_utils::flatten_vector_elements;
/// let a = vec![[1, 2, 3, 4], [5, 6, 7, 8]];
///
/// let b = flatten_vector_elements(a);
/// assert_eq!(vec![1, 2, 3, 4, 5, 6, 7, 8], b);
/// ```
pub fn flatten_vector_elements<T, const N: usize>(source: Vec<[T; N]>) -> Vec<T> {
    let v = mem::ManuallyDrop::new(source);
    let p = v.as_ptr();
    let len = v.len() * N;
    let cap = v.capacity() * N;
    unsafe { Vec::from_raw_parts(p as *mut T, len, cap) }
}

// TRANSPOSING
// ================================================================================================

/// Transposes a slice of `n` elements into a matrix with `N` columns and `n`/`N` rows.
///
/// # Panics
/// Panics if `n` is not divisible by `N`.
///
/// # Example
/// ```
/// # use winter_utils::transpose_slice;
/// let a = [0_u32, 1, 2, 3, 4, 5, 6, 7];
/// let b: Vec<[u32; 2]> = transpose_slice(&a);
///
/// assert_eq!(vec![[0, 4], [1, 5], [2, 6], [3, 7]], b);
/// ```
pub fn transpose_slice<T: Copy + Send + Sync, const N: usize>(source: &[T]) -> Vec<[T; N]> {
    let row_count = source.len() / N;
    assert_eq!(
        row_count * N,
        source.len(),
        "source length must be divisible by {}, but was {}",
        N,
        source.len()
    );

    let mut result = unsafe { group_vector_elements(uninit_vector(row_count * N)) };
    iter_mut!(result, 1024)
        .enumerate()
        .for_each(|(i, element)| {
            for j in 0..N {
                element[j] = source[i + j * row_count]
            }
        });
    result
}
