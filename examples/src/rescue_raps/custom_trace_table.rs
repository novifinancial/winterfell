// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use core_utils::{collections::Vec, uninit_vector};
use winterfell::{
    math::{log2, FieldElement, StarkField},
    EvaluationFrame, Matrix, Trace, TraceInfo, TraceLayout,
};

// RAP TRACE TABLE
// ================================================================================================
/// A concrete implementation of the [Trace] trait supporting custom RAPs.
///
/// This implementation supports concurrent trace generation and should be sufficient for most use
/// cases. There are two ways to create a trace table trace.
///
/// First, you can use the [RapTraceTable::init()] function which takes a set of vectors as a
/// parameter, where each vector contains values for a given column of the trace. This approach
/// allows you to build an execution trace as you see fit, as long as it meets a basic set of
/// requirements. These requirements are:
///
/// 1. Lengths of all columns in the execution trace must be the same.
/// 2. The length of the columns must be some power of two.
///
/// The other approach is to use [RapTraceTable::new()] function, which takes trace width and
/// length as parameters. This function will allocate memory for the trace, but will not fill it
/// with data. To fill the execution trace, you can use the [fill()](RapTraceTable::fill) method,
/// which takes two closures as parameters:
///
/// 1. The first closure is responsible for initializing the first state of the computation
///    (the first row of the execution trace).
/// 2. The second closure receives the previous state of the execution trace as input, and must
///    update it to the next state of the computation.
///
/// You can also use [RapTraceTable::with_meta()] function to create a blank execution trace.
/// This function work just like [RapTraceTable::new()] function, but also takes a metadata
/// parameter which can be an arbitrary sequence of bytes up to 64KB in size.
///
/// # Concurrent trace generation
/// For computations which consist of many small independent computations, we can generate the
/// execution trace of the entire computation by building fragments of the trace in parallel,
/// and then joining these fragments together.
///
/// For this purpose, `RapTraceTable` struct exposes [fragments()](RapTraceTable::fragments)
/// method, which takes fragment length as a parameter, breaks the execution trace into equally
/// sized fragments, and returns an iterator over these fragments. You can then use fragment's
/// [fill()](RapTraceTableFragment::fill) method to fill all fragments with data in parallel.
/// The semantics of the fragment's [RapTraceTableFragment::fill()] method are identical to the
/// semantics of the [RapTraceTable::fill()] method.
pub struct RapTraceTable<B: StarkField> {
    layout: TraceLayout,
    trace: Matrix<B>,
    meta: Vec<u8>,
}

impl<B: StarkField> RapTraceTable<B> {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new execution trace of the specified width and length.
    ///
    /// This allocates all the required memory for the trace, but does not initialize it. It is
    /// expected that the trace will be filled using one of the data mutator methods.
    ///
    /// # Panics
    /// Panics if:
    /// * `width` is zero or greater than 255.
    /// * `length` is smaller than 8, greater than biggest multiplicative subgroup in the field
    ///   `B`, or is not a power of two.
    pub fn new(width: usize, length: usize) -> Self {
        Self::with_meta(width, length, vec![])
    }

    /// Creates a new execution trace of the specified width and length, and with the specified
    /// metadata.
    ///
    /// This allocates all the required memory for the trace, but does not initialize it. It is
    /// expected that the trace will be filled using one of the data mutator methods.
    ///
    /// # Panics
    /// Panics if:
    /// * `width` is zero or greater than 255.
    /// * `length` is smaller than 8, greater than the biggest multiplicative subgroup in the
    ///   field `B`, or is not a power of two.
    /// * Length of `meta` is greater than 65535;
    pub fn with_meta(width: usize, length: usize, meta: Vec<u8>) -> Self {
        assert!(
            width > 0,
            "execution trace must consist of at least one column"
        );
        assert!(
            width <= TraceInfo::MAX_TRACE_WIDTH,
            "execution trace width cannot be greater than {}, but was {}",
            TraceInfo::MAX_TRACE_WIDTH,
            width
        );
        assert!(
            length >= TraceInfo::MIN_TRACE_LENGTH,
            "execution trace must be at lest {} steps long, but was {}",
            TraceInfo::MIN_TRACE_LENGTH,
            length
        );
        assert!(
            length.is_power_of_two(),
            "execution trace length must be a power of 2"
        );
        assert!(
            log2(length) as u32 <= B::TWO_ADICITY,
            "execution trace length cannot exceed 2^{} steps, but was 2^{}",
            B::TWO_ADICITY,
            log2(length)
        );
        assert!(
            meta.len() <= TraceInfo::MAX_META_LENGTH,
            "number of metadata bytes cannot be greater than {}, but was {}",
            TraceInfo::MAX_META_LENGTH,
            meta.len()
        );

        let columns = unsafe { (0..width).map(|_| uninit_vector(length)).collect() };
        Self {
            layout: TraceLayout::new(width, [3], [5]),
            trace: Matrix::new(columns),
            meta,
        }
    }

    // DATA MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Fill all rows in the execution trace.
    ///
    /// The rows are filled by executing the provided closures as follows:
    /// - `init` closure is used to initialize the first row of the trace; it receives a mutable
    ///   reference to the first state initialized to all zeros. The contents of the state are
    ///   copied into the first row of the trace after the closure returns.
    /// - `update` closure is used to populate all subsequent rows of the trace; it receives two
    ///   parameters:
    ///   - index of the last updated row (starting with 0).
    ///   - a mutable reference to the last updated state; the contents of the state are copied
    ///     into the next row of the trace after the closure returns.
    pub fn fill<I, U>(&mut self, init: I, update: U)
    where
        I: Fn(&mut [B]),
        U: Fn(usize, &mut [B]),
    {
        let mut state = vec![B::ZERO; self.main_trace_width()];
        init(&mut state);
        self.update_row(0, &state);

        for i in 0..self.length() - 1 {
            update(i, &mut state);
            self.update_row(i + 1, &state);
        }
    }

    /// Updates a single row in the execution trace with provided data.
    pub fn update_row(&mut self, step: usize, state: &[B]) {
        self.trace.update_row(step, state);
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the number of columns in this execution trace.
    pub fn width(&self) -> usize {
        self.main_trace_width()
    }

    /// Returns value of the cell in the specified column at the specified row of this trace.
    pub fn get(&self, column: usize, step: usize) -> B {
        self.trace.get(column, step)
    }

    /// Reads a single row from this execution trace into the provided target.
    pub fn read_row_into(&self, step: usize, target: &mut [B]) {
        self.trace.read_row_into(step, target);
    }
}

// TRACE TRAIT IMPLEMENTATION
// ================================================================================================

impl<B: StarkField> Trace for RapTraceTable<B> {
    type BaseField = B;

    fn layout(&self) -> &TraceLayout {
        &self.layout
    }

    fn length(&self) -> usize {
        self.trace.num_rows()
    }

    fn meta(&self) -> &[u8] {
        &self.meta
    }

    fn read_main_frame(&self, row_idx: usize, frame: &mut EvaluationFrame<Self::BaseField>) {
        let next_row_idx = (row_idx + 1) % self.length();
        self.trace.read_row_into(row_idx, frame.current_mut());
        self.trace.read_row_into(next_row_idx, frame.next_mut());
    }

    fn main_segment(&self) -> &Matrix<B> {
        &self.trace
    }

    fn build_aux_segment<E>(
        &mut self,
        aux_segments: &[Matrix<E>],
        rand_elements: &[E],
    ) -> Option<Matrix<E>>
    where
        E: FieldElement<BaseField = Self::BaseField>,
    {
        // We only have one auxiliary segment for this example
        if !aux_segments.is_empty() {
            return None;
        }

        let mut row = unsafe { uninit_vector(self.width()) };
        self.read_row_into(0, &mut row);
        let mut aux_columns = vec![vec![E::ZERO; self.length()]; self.aux_trace_width()];

        aux_columns[0][0] = rand_elements[0] * row[2].into() + rand_elements[1] * row[3].into();
        aux_columns[1][0] = rand_elements[0] * row[6].into() + rand_elements[1] * row[7].into();

        // Permutation argument column
        aux_columns[2][0] = E::ONE;

        for index in 1..self.length() {
            // At every last step before a new hash iteration,
            // copy the permuted values into the auxiliary columns
            if (index % super::CYCLE_LENGTH) == super::CYCLE_LENGTH - 1 {
                self.read_row_into(index, &mut row);

                aux_columns[0][index] =
                    rand_elements[0] * row[2].into() + rand_elements[1] * row[3].into();
                aux_columns[1][index] =
                    rand_elements[0] * row[6].into() + rand_elements[1] * row[7].into();
            }

            let num = aux_columns[0][index - 1] + rand_elements[2];
            let denom = aux_columns[1][index - 1] + rand_elements[2];
            aux_columns[2][index] = aux_columns[2][index - 1] * num * denom.inv();
        }

        Some(Matrix::new(aux_columns))
    }
}
