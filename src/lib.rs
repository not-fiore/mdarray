//! # Multidimensional array for Rust
//!
//! ## Overview
//!
//! The mdarray crate provides a multidimensional array for Rust. Its main target
//! is for numeric types, however generic types are supported as well. The purpose
//! is to provide a generic container type that is simple and flexible to use,
//! with interworking to other crates for e.g. BLAS/LAPACK functionality.
//!
//! Here are the main features of mdarray:
//!
//! - Dense array type, where the rank is known at compile time.
//! - Subarrays (views) can be created with arbitrary shapes and strides.
//! - Standard Rust mechanisms are used for e.g. slices, indexing and iteration.
//!
//! The design is inspired from the Rust ndarray, nalgebra and bitvec crates,
//! the proposed C++ mdarray and mdspan types, and multidimensional arrays in
//! Julia and Matlab.
//!
//! ## Array types
//!
//! The base type for multidimensional arrays is `Array<B>`, where the generic
//! parameter is a buffer for the array storage. The following variants exist:
//!
//! - `Array<GridBuffer>` is a dense array that owns the storage, similar to
//!   the Rust `Vec` type.
//! - `Array<ViewBuffer>` and `Array<ViewBufferMut>` are arrays that refer to a
//!   parent array. They are used for example when creating a view of a larger
//!   array without duplicating elements.
//! - `Array<SpanBuffer>` is used as a generic array reference, similar to the
//!   Rust `slice` type. It consists of a pointer to an internal structure that
//!   holds the storage and the layout. Arrays and array views automatically
//!   dereference to an array span.
//!
//! The array layout describes how elements are stored in memory. The layout
//! is parameterized by the rank (i.e. the number of dimensions) and the storage
//! format. It contains the shape (i.e. the size in each dimension), and the
//! strides per dimension if needed.
//!
//! The storage format is `Dense` if elements are stored contiguously without gaps.
//! In this case, the strides are calculated from the shape and not stored as
//! part of the layout. The format is `General` if each dimension can have
//! arbitrary stride except for the innermost one, which has unit stride. It is
//! compatible with the BLAS/LAPACK general matrix storage.
//!
//! The format is `Flat` if the innermost dimension can have arbitrary stride
//! and the other dimensions must follow in order, allowing for linear indexing.
//! The format is `Strided` if all dimensions can have arbitrary strides.
//!
//! The array elements are stored in column-major order, also known as Fortran
//! order where the innermost dimension is the innermost one.
//!
//! The following type aliases are provided:
//!
//! - `Grid<T, const N: usize, A = Global>` for a dense array
//! - `Span<T, const N: usize, F = Dense>` for an array span
//! - `View<T, const N: usize, F = Dense>` for an array view
//! - `View<T, const N: usize, F = Dense>` for a mutable array view
//!
//! Prefer using `Span` instead of array views for function parameters, since
//! they can refer to either an owned array or an array view. Array views
//! are useful for example when lifetimes need to be maintained in function
//! return types.
//!
//! ## Indexing and views
//!
//! Scalar indexing is done using the normal square-bracket index operator and
//! an array of `usize` per dimension as index.
//!
//! If the array layout type supports linear indexing, a scalar `usize` can also
//! be used as index. If the array layout type supports slice indexing, a range
//! can be used as index to select a slice.
//!
//! An array view can be created with the `view` and `view_mut` methods and a
//! tuple of indices per dimension as argument. Each index can be either a range
//! or `usize`. The resulting storage format depends on both the format inferred
//! from the indices and the input format.
//!
//! ## Iteration
//!
//! If the array layout type supports linear indexing, an iterator can be created
//! with the `iter`, `iter_mut` and `into_iter` methods like `Vec` and `slice`.
//!
//! For multidimensional arrays, indexing over a single dimension is done
//! with the `outer_iter`/`outer_iter_mut`, `inner_iter`/`inner_iter_mut` and
//! `axis_iter`/`axis_iter_mut` methods. The iterators give array views of
//! the remaining dimensions.
//!
//! If linear indexing is possible but the array layout type is not known, the
//! `reformat`, `reformat_mut` and `into_format` methods can be used to change
//! format with runtime checking. Alternatively, the `flatten`, `flatten_mut`
//! and `into_flattened` methods can be used to change to a one-dimensional
//! array.
//!
//! ## Operators
//!
//! Arithmetic, logical, negation, comparison and compound assignment operators
//! are supported for arrays. For arithmetic, logical and negation operators,
//! at most one parameter can be passed by value. If all parametes are passed by
//! reference, a new array is allocated for the result. For comparison operators,
//! the parameters are always passed by reference.
//!
//! Scalar parameters must be passed using the `fill` function to wrap the value
//! in a `Fill` struct. If the type does not implement the `Copy` trait, the
//! parameter must be passed by reference.
//!
//! Note that for complex calculations it can be more efficient to use iterators
//! and element-wise operations to reduce memory accesses.
//!
//! ## Example
//!
//! This example implements matrix multiplication and addition `C = A * B + C`.
//! The matrices use column-major ordering, and the inner loop runs over one column
//! in `A` and `C`. By using iterators the array bounds checking is avoided, and
//! the compiler is able to vectorize the inner loop.
//!
//! ```
//! use mdarray::{Grid, Span, View};
//!
//! fn matmul(a: &Span<f64, 2>, b: &Span<f64, 2>, c: &mut Span<f64, 2>) {
//!     assert!(c.shape() == [a.size(0), b.size(1)] && a.size(1) == b.size(0), "shape mismatch");
//!
//!     for (mut cj, bj) in c.outer_iter_mut().zip(b.outer_iter()) {
//!         for (ak, bkj) in a.outer_iter().zip(bj.iter()) {
//!             for (cij, aik) in cj.iter_mut().zip(ak.iter()) {
//!                 *cij = aik.mul_add(*bkj, *cij);
//!             }
//!         }
//!     }
//! }
//!
//! let a = View::from(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
//! let b = View::from(&[[0.0, 1.0], [1.0, 1.0]]);
//!
//! let mut c = Grid::from_elem([3, 2], 0.0);
//!
//! matmul(&a, &b, &mut c);
//!
//! println!("{c:?}");
//! # assert!(c == View::from(&[[4.0, 5.0, 6.0], [5.0, 7.0, 9.0]]));
//! ```
//!
//! This will produce the result `[[4.0, 5.0, 6.0], [5.0, 7.0, 9.0]]`.

#![cfg_attr(feature = "nightly", feature(allocator_api))]
#![cfg_attr(feature = "nightly", feature(extern_types))]
#![cfg_attr(feature = "nightly", feature(hasher_prefixfree_extras))]
#![cfg_attr(feature = "nightly", feature(int_roundings))]
#![cfg_attr(feature = "nightly", feature(slice_range))]
#![warn(missing_docs)]

/// Module for array span and view indexing, and for array axis subarray types.
pub mod index {
    pub(crate) mod axis;
    pub(crate) mod span;
    pub(crate) mod view;

    pub use axis::Axis;
    pub use span::SpanIndex;
    pub use view::{DimIndex, Params, ViewIndex};
}

/// Module for array axis and flat array span iterators.
pub mod iter {
    pub(crate) mod sources;

    pub use sources::{AxisIter, AxisIterMut, FlatIter, FlatIterMut};
}

mod array;
mod buffer;
mod dim;
mod format;
mod grid;
mod layout;
mod mapping;
mod ops;
mod raw_span;
mod span;
mod view;

#[cfg(feature = "serde")]
mod serde;

#[cfg(not(feature = "nightly"))]
mod alloc {
    pub trait Allocator {}

    #[derive(Copy, Clone, Default, Debug)]
    pub struct Global;

    impl Allocator for Global {}
}

#[cfg(feature = "nightly")]
use std::alloc::Global;

#[cfg(not(feature = "nightly"))]
use alloc::Global;
use array::{GridArray, SpanArray, ViewArray, ViewArrayMut};

pub use array::Array;
pub use buffer::{Buffer, BufferMut, SizedBuffer, SizedBufferMut};
pub use buffer::{GridBuffer, SpanBuffer, ViewBuffer, ViewBufferMut};
pub use dim::{Const, Dim, Shape, Strides};
pub use format::{Dense, Flat, Format, General, Strided, Uniform, UnitStrided};
pub use layout::Layout;
pub use ops::{fill, step, Fill, StepRange};

/// Dense multidimensional array.
pub type Grid<T, const N: usize, A = Global> = GridArray<T, Const<N>, A>;

/// Multidimensional array span.
pub type Span<T, const N: usize, F = Dense> = SpanArray<T, Const<N>, F>;

/// Multidimensional array view.
pub type View<'a, T, const N: usize, F = Dense> = ViewArray<'a, T, Const<N>, F>;

/// Mutable multidimensional array view.
pub type ViewMut<'a, T, const N: usize, F = Dense> = ViewArrayMut<'a, T, Const<N>, F>;
