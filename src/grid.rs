#[cfg(feature = "nightly")]
use std::alloc::{Allocator, Global};
use std::collections::TryReserveError;
use std::iter::FromIterator;
use std::mem;
use std::result::Result;

#[cfg(not(feature = "nightly"))]
use crate::alloc::{Allocator, Global};
use crate::array::{GridArray, SpanArray};
use crate::buffer::GridBuffer;
use crate::dim::{Const, Dim, Shape};
use crate::format::Format;
use crate::layout::{DenseLayout, Layout};

#[cfg(not(feature = "nightly"))]
macro_rules! vec_t {
    ($type:ty, $alloc:ty) => {
        Vec<$type>
    };
}

#[cfg(feature = "nightly")]
macro_rules! vec_t {
    ($type:ty, $alloc:ty) => {
        Vec<$type, $alloc>
    };
}

impl<T, D: Dim, A: Allocator> GridArray<T, D, A> {
    /// Returns a reference to the underlying allocator.
    #[cfg(feature = "nightly")]
    pub fn allocator(&self) -> &A {
        self.buffer.allocator()
    }

    /// Moves all elements from another array into the array along the outer dimension.
    /// # Panics
    /// Panics if the inner dimensions do not match.
    pub fn append(&mut self, other: &mut Self) {
        let new_shape = if self.is_empty() {
            other.shape()
        } else {
            let mut shape = self.shape();

            assert!(
                other.shape()[..D::RANK - 1] == shape[..D::RANK - 1],
                "inner dimensions mismatch"
            );

            shape[D::RANK - 1] += other.size(D::RANK - 1);
            shape
        };

        let mut src_guard = other.buffer.guard_mut();
        let mut dst_guard = self.buffer.guard_mut();

        dst_guard.append(&mut src_guard);

        src_guard.set_layout(Layout::default());
        dst_guard.set_layout(DenseLayout::new(new_shape));
    }

    /// Returns the number of elements the array can hold without reallocating.
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Clears the array, removing all values.
    pub fn clear(&mut self) {
        let mut guard = self.buffer.guard_mut();

        guard.clear();
        guard.set_layout(Layout::default());
    }

    /// Clones all elements in an array span and appends to the array along the outer dimension.
    /// # Panics
    /// Panics if the inner dimensions do not match.
    pub fn extend_from_span(&mut self, other: &SpanArray<T, D, impl Format>)
    where
        T: Clone,
    {
        let new_shape = if self.is_empty() {
            other.shape()
        } else {
            let mut shape = self.shape();

            assert!(
                other.shape()[..D::RANK - 1] == shape[..D::RANK - 1],
                "inner dimensions mismatch"
            );

            shape[D::RANK - 1] += other.size(D::RANK - 1);
            shape
        };

        let mut guard = self.buffer.guard_mut();

        guard.reserve(other.len());

        unsafe {
            #[cfg(not(feature = "nightly"))]
            extend_from_span::<_, _, A>(&mut guard, other);
            #[cfg(feature = "nightly")]
            extend_from_span(&mut guard, other);
        }

        guard.set_layout(DenseLayout::new(new_shape));
    }

    /// Creates an array from the given element with the specified allocator.
    #[cfg(feature = "nightly")]
    pub fn from_elem_in(shape: D::Shape, elem: &T, alloc: A) -> Self
    where
        T: Clone,
    {
        let len = shape[..].iter().fold(1usize, |acc, &x| acc.saturating_mul(x));
        let mut vec = Vec::<T, A>::with_capacity_in(len, alloc);

        unsafe {
            for i in 0..len {
                vec.as_mut_ptr().add(i).write(elem.clone());
                vec.set_len(i + 1);
            }

            Self::from_parts(vec, DenseLayout::new(shape))
        }
    }

    /// Creates an array with the results from the given function with the specified allocator.
    #[cfg(feature = "nightly")]
    pub fn from_fn_in(shape: D::Shape, mut f: impl FnMut(D::Shape) -> T, alloc: A) -> Self {
        let len = shape[..].iter().fold(1usize, |acc, &x| acc.saturating_mul(x));
        let mut vec = Vec::with_capacity_in(len, alloc);

        unsafe {
            from_fn::<T, D, A, D::Lower>(&mut vec, shape, D::Shape::default(), &mut f);

            Self::from_parts(vec, DenseLayout::new(shape))
        }
    }

    /// Creates an array from raw components of another array with the specified allocator.
    /// # Safety
    /// The pointer must be a valid allocation given the shape, capacity and allocator.
    #[cfg(feature = "nightly")]
    pub unsafe fn from_raw_parts_in(
        ptr: *mut T,
        shape: D::Shape,
        capacity: usize,
        alloc: A,
    ) -> Self {
        let layout = DenseLayout::new(shape);

        Self::from_parts(Vec::from_raw_parts_in(ptr, layout.len(), capacity, alloc), layout)
    }

    /// Converts the array into a one-dimensional array.
    pub fn into_flattened(self) -> GridArray<T, Const<1>, A> {
        self.into_vec().into()
    }

    /// Decomposes an array into its raw components including the allocator.
    #[cfg(feature = "nightly")]
    pub fn into_raw_parts_with_alloc(self) -> (*mut T, D::Shape, usize, A) {
        let (vec, layout) = self.buffer.into_parts();
        let (ptr, _, capacity, alloc) = vec.into_raw_parts_with_alloc();

        (ptr, layout.shape(), capacity, alloc)
    }

    /// Converts an array with a single element into the contained value.
    /// # Panics
    /// Panics if the array length is not equal to one.
    pub fn into_scalar(self) -> T {
        assert!(self.len() == 1, "invalid length");

        self.into_vec().pop().unwrap()
    }

    /// Converts the array into a reshaped array, which must have the same length.
    /// # Panics
    /// Panics if the array length is changed.
    pub fn into_shape<S: Shape>(self, shape: S) -> GridArray<T, S::Dim, A> {
        let (vec, layout) = self.buffer.into_parts();

        unsafe { GridArray::from_parts(vec, layout.reshape(shape)) }
    }

    /// Converts the array into a vector.
    pub fn into_vec(self) -> vec_t!(T, A) {
        let (vec, _) = self.buffer.into_parts();

        vec
    }

    /// Returns the array with the given closure applied to each element.
    pub fn map(mut self, mut f: impl FnMut(T) -> T) -> Self
    where
        T: Default,
    {
        map(&mut self, &mut f);

        self
    }

    /// Creates a new, empty array with the specified allocator.
    #[cfg(feature = "nightly")]
    pub fn new_in(alloc: A) -> Self {
        unsafe { Self::from_parts(Vec::new_in(alloc), Layout::default()) }
    }

    /// Reserves capacity for at least the additional number of elements in the array.
    pub fn reserve(&mut self, additional: usize) {
        self.buffer.guard_mut().reserve(additional);
    }

    /// Reserves the minimum capacity for the additional number of elements in the array.
    pub fn reserve_exact(&mut self, additional: usize) {
        self.buffer.guard_mut().reserve_exact(additional);
    }

    /// Resizes the array to the new shape, creating new elements with the given value.
    pub fn resize(&mut self, new_shape: D::Shape, value: &T)
    where
        T: Clone,
        A: Clone,
    {
        self.buffer.resize_with(new_shape, || value.clone());
    }

    /// Resizes the array to the new shape, creating new elements from the given closure.
    pub fn resize_with(&mut self, new_shape: D::Shape, f: impl FnMut() -> T)
    where
        A: Clone,
    {
        self.buffer.resize_with(new_shape, f);
    }

    /// Forces the array shape to the new shape.
    /// # Safety
    /// All elements within the array length must be initialized.
    pub unsafe fn set_shape(&mut self, new_shape: D::Shape) {
        self.buffer.set_layout(DenseLayout::new(new_shape));
    }

    /// Shrinks the capacity of the array with a lower bound.
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.buffer.guard_mut().shrink_to(min_capacity);
    }

    /// Shrinks the capacity of the array as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.buffer.guard_mut().shrink_to_fit();
    }

    /// Shortens the array along the outer dimension, keeping the first `size` elements.
    pub fn truncate(&mut self, size: usize) {
        if size < self.size(D::RANK - 1) {
            let new_layout = self.layout().resize_dim(D::RANK - 1, size);
            let mut guard = self.buffer.guard_mut();

            guard.set_layout(new_layout);
            guard.truncate(new_layout.len());
        }
    }

    /// Tries to reserve capacity for at least the additional number of elements in the array.
    /// # Errors
    /// If the capacity overflows, or the allocator reports a failure, then an error is returned.
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.buffer.guard_mut().try_reserve(additional)
    }

    /// Tries to reserve the minimum capacity for the additional number of elements in the array.
    /// # Errors
    /// If the capacity overflows, or the allocator reports a failure, then an error is returned.
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.buffer.guard_mut().try_reserve_exact(additional)
    }

    /// Creates a new, empty array with the specified capacity and allocator.
    #[cfg(feature = "nightly")]
    pub fn with_capacity_in(capacity: usize, alloc: A) -> Self {
        unsafe { Self::from_parts(Vec::with_capacity_in(capacity, alloc), Layout::default()) }
    }

    pub(crate) unsafe fn from_parts(vec: vec_t!(T, A), layout: DenseLayout<D>) -> Self {
        debug_assert!(vec.len() == layout.len(), "length mismatch");

        Self { buffer: GridBuffer::from_parts(vec, layout) }
    }
}

#[cfg(not(feature = "nightly"))]
impl<T, D: Dim> GridArray<T, D> {
    /// Creates an array from the given element.
    pub fn from_elem(shape: D::Shape, elem: &T) -> Self
    where
        T: Clone,
    {
        let len = shape[..].iter().fold(1usize, |acc, &x| acc.saturating_mul(x));
        let mut vec = Vec::<T>::with_capacity(len);

        unsafe {
            for i in 0..len {
                vec.as_mut_ptr().add(i).write(elem.clone());
                vec.set_len(i + 1);
            }

            Self::from_parts(vec, DenseLayout::new(shape))
        }
    }

    /// Creates an array with the results from the given function.
    pub fn from_fn(shape: D::Shape, mut f: impl FnMut(D::Shape) -> T) -> Self {
        let len = shape[..].iter().fold(1usize, |acc, &x| acc.saturating_mul(x));
        let mut vec = Vec::with_capacity(len);

        unsafe {
            from_fn::<T, D, Global, D::Lower>(&mut vec, shape, D::Shape::default(), &mut f);

            Self::from_parts(vec, DenseLayout::new(shape))
        }
    }

    /// Creates an array from raw components of another array.
    /// # Safety
    /// The pointer must be a valid allocation given the shape and capacity.
    pub unsafe fn from_raw_parts(ptr: *mut T, shape: D::Shape, capacity: usize) -> Self {
        let layout = DenseLayout::new(shape);
        let vec = Vec::from_raw_parts(ptr, layout.len(), capacity);

        Self::from_parts(vec, layout)
    }

    /// Decomposes an array into its raw components.
    pub fn into_raw_parts(self) -> (*mut T, D::Shape, usize) {
        let (vec, layout) = self.buffer.into_parts();
        let mut vec = mem::ManuallyDrop::new(vec);

        (vec.as_mut_ptr(), layout.shape(), vec.capacity())
    }

    /// Creates a new, empty array.
    pub fn new() -> Self {
        unsafe { Self::from_parts(Vec::new(), Layout::default()) }
    }

    /// Creates a new, empty array with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        unsafe { Self::from_parts(Vec::with_capacity(capacity), Layout::default()) }
    }
}

#[cfg(feature = "nightly")]
impl<T, D: Dim> GridArray<T, D> {
    /// Creates an array from the given element.
    pub fn from_elem(shape: D::Shape, elem: &T) -> Self
    where
        T: Clone,
    {
        Self::from_elem_in(shape, elem, Global)
    }

    /// Creates an array with the results from the given function.
    pub fn from_fn(shape: D::Shape, f: impl FnMut(D::Shape) -> T) -> Self {
        Self::from_fn_in(shape, f, Global)
    }

    /// Creates an array from raw components of another array.
    /// # Safety
    /// The pointer must be a valid allocation given the shape and capacity.
    pub unsafe fn from_raw_parts(ptr: *mut T, shape: D::Shape, capacity: usize) -> Self {
        Self::from_raw_parts_in(ptr, shape, capacity, Global)
    }

    /// Decomposes an array into its raw components.
    pub fn into_raw_parts(self) -> (*mut T, D::Shape, usize) {
        let (ptr, shape, capacity, _) = self.into_raw_parts_with_alloc();

        (ptr, shape, capacity)
    }

    /// Creates a new, empty array.
    pub fn new() -> Self {
        Self::new_in(Global)
    }

    /// Creates a new, empty array with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_in(capacity, Global)
    }
}

impl<T, D: Dim> Default for GridArray<T, D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: 'a + Copy, A: 'a + Allocator> Extend<&'a T> for GridArray<T, Const<1>, A> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        let mut guard = self.buffer.guard_mut();

        guard.extend(iter);
        guard.set_layout(DenseLayout::new([guard.len()]));
    }
}

impl<T, A: Allocator> Extend<T> for GridArray<T, Const<1>, A> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let mut guard = self.buffer.guard_mut();

        guard.extend(iter);
        guard.set_layout(DenseLayout::new([guard.len()]));
    }
}

impl<T: Clone> From<&[T]> for GridArray<T, Const<1>> {
    fn from(slice: &[T]) -> Self {
        Self::from(slice.to_vec())
    }
}

macro_rules! impl_from_array {
    ($n:tt, ($($size:tt),+), $array:tt) => {
        impl<T, $(const $size: usize),+> From<$array> for GridArray<T, Const<$n,>> {
            #[cfg(not(feature = "nightly"))]
            fn from(array: $array) -> Self {
                if [$($size),+].contains(&0) {
                    Self::new()
                } else {
                    let mut vec = std::mem::ManuallyDrop::new(Vec::from(array));
                    let (ptr, capacity) = (vec.as_mut_ptr(), vec.capacity());

                    unsafe {
                        let capacity = capacity * (mem::size_of_val(&*ptr) / mem::size_of::<T>());

                        Self::from_raw_parts(ptr.cast(), [$($size),+], capacity)
                    }
                }
            }

            #[cfg(feature = "nightly")]
            fn from(array: $array) -> Self {
                if [$($size),+].contains(&0) {
                    Self::new()
                } else {
                    let (ptr, _, capacity, alloc) = Vec::from(array).into_raw_parts_with_alloc();

                    unsafe {
                        let capacity = capacity * (mem::size_of_val(&*ptr) / mem::size_of::<T>());

                        Self::from_raw_parts_in(ptr.cast(), [$($size),+], capacity, alloc)
                    }
                }
            }
        }
    };
}

impl_from_array!(1, (X), [T; X]);
impl_from_array!(2, (X, Y), [[T; X]; Y]);
impl_from_array!(3, (X, Y, Z), [[[T; X]; Y]; Z]);
impl_from_array!(4, (X, Y, Z, W), [[[[T; X]; Y]; Z]; W]);
impl_from_array!(5, (X, Y, Z, W, U), [[[[[T; X]; Y]; Z]; W]; U]);
impl_from_array!(6, (X, Y, Z, W, U, V), [[[[[[T; X]; Y]; Z]; W]; U]; V]);

impl<T, D: Dim, A: Allocator> From<GridArray<T, D, A>> for vec_t!(T, A) {
    fn from(grid: GridArray<T, D, A>) -> Self {
        grid.into_vec()
    }
}

impl<T, A: Allocator> From<vec_t!(T, A)> for GridArray<T, Const<1>, A> {
    fn from(vec: vec_t!(T, A)) -> Self {
        let layout = DenseLayout::new([vec.len()]);

        unsafe { Self::from_parts(vec, layout) }
    }
}

impl<T> FromIterator<T> for GridArray<T, Const<1>> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from(Vec::from_iter(iter))
    }
}

impl<T, D: Dim, A: Allocator> IntoIterator for GridArray<T, D, A> {
    type Item = T;
    type IntoIter = <vec_t!(T, A) as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

unsafe fn extend_from_span<T: Clone, F: Format, A: Allocator>(
    vec: &mut vec_t!(T, A),
    other: &SpanArray<T, impl Dim, F>,
) {
    if F::IS_UNIFORM {
        for x in other.flatten().iter() {
            debug_assert!(vec.len() < vec.capacity(), "index exceeds capacity");

            vec.as_mut_ptr().add(vec.len()).write(x.clone());
            vec.set_len(vec.len() + 1);
        }
    } else {
        for x in other.outer_iter() {
            #[cfg(not(feature = "nightly"))]
            extend_from_span::<_, _, A>(vec, &x);
            #[cfg(feature = "nightly")]
            extend_from_span(vec, &x);
        }
    }
}

unsafe fn from_fn<T, D: Dim, A: Allocator, I: Dim>(
    vec: &mut vec_t!(T, A),
    shape: D::Shape,
    mut index: D::Shape,
    f: &mut impl FnMut(D::Shape) -> T,
) {
    for i in 0..shape[I::RANK] {
        index[I::RANK] = i;

        if I::RANK == 0 {
            debug_assert!(vec.len() < vec.capacity(), "index exceeds capacity");

            vec.as_mut_ptr().add(vec.len()).write(f(index));
            vec.set_len(vec.len() + 1);
        } else {
            from_fn::<T, D, A, I::Lower>(vec, shape, index, f);
        }
    }
}

fn map<T: Default, F: Format>(this: &mut SpanArray<T, impl Dim, F>, f: &mut impl FnMut(T) -> T) {
    if F::IS_UNIFORM {
        for x in this.flatten_mut().iter_mut() {
            *x = f(mem::take(x));
        }
    } else {
        for mut x in this.outer_iter_mut() {
            map(&mut x, f);
        }
    }
}
