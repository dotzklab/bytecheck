//! # bytecheck
//!
//! bytecheck is a type validation framework for Rust.
//!
//! For some types, creating an invalid value immediately results in undefined
//! behavior. This can cause some issues when trying to validate potentially
//! invalid bytes, as just casting the bytes to your type can technically cause
//! errors. This makes it difficult to write validation routines, because until
//! you're certain that the bytes represent valid values you cannot cast them.
//!
//! bytecheck provides a framework for performing these byte-level validations
//! and implements checks for basic types along with a derive macro to implement
//! validation for custom structs and enums.
//!
//! ## Design
//!
//! [`CheckBytes`] is at the heart of bytecheck, and does the heavy lifting of
//! verifying that some bytes represent a valid type. Implementing it can be
//! done manually or automatically with the [derive macro](macro@CheckBytes).
//!
//! ## Examples
//!
//! ```
//! use bytecheck::{CheckBytes, FailureContext};
//!
//! #[derive(CheckBytes, Debug)]
//! struct Test {
//!     a: u32,
//!     b: bool,
//!     c: char,
//! }
//! #[repr(C, align(16))]
//! struct Aligned<const N: usize>([u8; N]);
//!
//! macro_rules! bytes {
//!     ($($byte:literal,)*) => {
//!         (&Aligned([$($byte,)*]).0 as &[u8]).as_ptr()
//!     };
//!     ($($byte:literal),*) => {
//!         bytes!($($byte,)*)
//!     };
//! }
//!
//! // This type is laid out as (u32, char, bool)
//! // In this example, the architecture is assumed to be little-endian
//! # #[cfg(target_endian = "little")]
//! unsafe {
//!     // These are valid bytes for (0, 'x', true)
//!     Test::check_bytes(
//!         bytes![
//!             0u8, 0u8, 0u8, 0u8, 0x78u8, 0u8, 0u8, 0u8,
//!             1u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8
//!         ].cast(),
//!         &mut FailureContext,
//!     ).unwrap();
//!
//!     // Changing the bytes for the u32 is OK, any bytes are a valid u32
//!     Test::check_bytes(
//!         bytes![
//!             42u8, 16u8, 20u8, 3u8, 0x78u8, 0u8, 0u8, 0u8,
//!             1u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8
//!         ].cast(),
//!         &mut FailureContext,
//!     ).unwrap();
//!
//!     // Characters outside the valid ranges are invalid
//!     Test::check_bytes(
//!         bytes![
//!             0u8, 0u8, 0u8, 0u8, 0x00u8, 0xd8u8, 0u8, 0u8,
//!             1u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8
//!         ].cast(),
//!         &mut FailureContext,
//!     ).unwrap_err();
//!     Test::check_bytes(
//!         bytes![
//!             0u8, 0u8, 0u8, 0u8, 0x00u8, 0x00u8, 0x11u8, 0u8,
//!             1u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8
//!         ].cast(),
//!         &mut FailureContext,
//!     ).unwrap_err();
//!
//!     // 0 is a valid boolean value (false) but 2 is not
//!     Test::check_bytes(
//!         bytes![
//!             0u8, 0u8, 0u8, 0u8, 0x78u8, 0u8, 0u8, 0u8,
//!             0u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8
//!         ].cast(),
//!         &mut FailureContext,
//!     ).unwrap();
//!     Test::check_bytes(
//!         bytes![
//!             0u8, 0u8, 0u8, 0u8, 0x78u8, 0u8, 0u8, 0u8,
//!             2u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8
//!         ].cast(),
//!         &mut FailureContext,
//!     ).unwrap_err();
//! }
//! ```
//!
//! ## Features
//!
//! - `alloc`: (Enabled by default) Enables alloc library support.
//! - `std`: (Enabled by default) Enables standard library support.
//!
//! ## Crate support
//!
//! Some common crates need to be supported by bytecheck before an official integration has been
//! made. Support is provided by bytecheck for these crates, but in the future crates should depend
//! on bytecheck and provide their own implementations. The crates that already have support
//! provided by bytecheck should work toward integrating the implementations into themselves.
//!
//! Crates supported by bytecheck:
//!
//! - [`uuid`](https://docs.rs/uuid)

#![deny(
    future_incompatible,
    missing_docs,
    nonstandard_style,
    unsafe_op_in_unsafe_fn,
    unused,
    warnings,
    clippy::all,
    clippy::missing_safety_doc,
    clippy::undocumented_unsafe_blocks,
    rustdoc::broken_intra_doc_links,
    rustdoc::missing_crate_level_docs
)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
mod boxed_error;
#[cfg(feature = "alloc")]
mod thin_box;

// Support for various common crates. These are primarily to get users off the
// ground and build some momentum.

// These are NOT PLANNED to remain in bytecheck for the final release. Much like
// serde, these implementations should be moved into their respective crates
// over time. Before adding support for another crate, please consider getting
// bytecheck support in the crate instead.

#[cfg(feature = "uuid")]
pub mod uuid;

#[cfg(not(feature = "simdutf8"))]
use core::str::{from_utf8, Utf8Error};
#[cfg(target_has_atomic = "8")]
use core::sync::atomic::{AtomicBool, AtomicI8, AtomicU8};
#[cfg(target_has_atomic = "16")]
use core::sync::atomic::{AtomicI16, AtomicU16};
#[cfg(target_has_atomic = "32")]
use core::sync::atomic::{AtomicI32, AtomicU32};
#[cfg(target_has_atomic = "64")]
use core::sync::atomic::{AtomicI64, AtomicU64};
use core::{
    fmt::{self, Display},
    marker::{PhantomData, PhantomPinned},
    mem::ManuallyDrop,
    num::{
        NonZeroI128, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8,
        NonZeroU128, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8,
    },
    ops, ptr,
};
use ptr_meta::PtrExt;
#[cfg(all(feature = "simdutf8"))]
use simdutf8::basic::from_utf8;

pub use bytecheck_derive::CheckBytes;

macro_rules! define_error_trait {
    ($($supertraits:tt)*) => {
        /// An error that can be debugged and displayed.
        ///
        /// Without the `std` feature enabled, this has supertraits of
        /// [`core::fmt::Debug`] and [`core::fmt::Display`]. With the `std`
        /// feature enabled, this also has a supertrait of [`std::error::Error`]
        /// instead.
        ///
        /// This trait is always `Send + Sync + 'static`.
        #[cfg_attr(feature = "alloc", ptr_meta::pointee)]
        pub trait Error: $($supertraits)* {
            /// Returns this error as its supertraits.
            fn downcast(&self) -> &(dyn $($supertraits)*);
        }

        impl<T: $($supertraits)*> Error for T {
            fn downcast(&self) -> &(dyn $($supertraits)*) {
                self
            }
        }
    };
}

#[cfg(feature = "std")]
define_error_trait!(std::error::Error + Send + Sync + 'static);

#[cfg(not(feature = "std"))]
define_error_trait!(
    core::fmt::Debug + core::fmt::Display + Send + Sync + 'static
);

/// An error type which can be uniformly constructed from a bytecheck [`Error`].
///
/// All types which can fail to check require a context with a `Contextual`
/// error type.
pub trait Contextual: Sized + Error {
    /// Returns a new `Self` using the given [`Error`].
    ///
    /// Depending on the specific implementation, this may box the error or
    /// discard it and only remember that some error occurred.
    fn new<T: Error>(source: T) -> Self;

    /// Returns a new `Self` using the [`Error`] returned by calling
    /// `make_source`.
    ///
    /// Depending on the specific implementation, this may box the error or
    /// discard it and only remember that some error occurred.
    fn new_with<T: Error, F: FnOnce() -> T>(make_source: F) -> Self {
        Self::new(make_source())
    }

    /// Adds additional context to this error, returning a new error.
    fn context<T: fmt::Debug + Display + Send + Sync + 'static>(
        self,
        context: T,
    ) -> Self;
}

/// A validation context.
pub trait Context {
    /// The error type that can be produced when validating with this context.
    type Error: Contextual;
}

/// A validation context that simply records success or failure, throwing away
/// any detailed error messages.
#[derive(Debug)]
pub struct Failure;

impl Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to check bytes")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Failure {}

impl Contextual for Failure {
    fn new<T: Display>(_: T) -> Self {
        Self
    }

    fn new_with<T: Display, F: FnOnce() -> T>(_: F) -> Self {
        Self
    }

    fn context<T: Display>(self, _: T) -> Self {
        self
    }
}

/// A basic validation context with an error type of [`Failure`].
pub struct FailureContext;

impl Context for FailureContext {
    type Error = Failure;
}

#[cfg(feature = "alloc")]
pub use boxed_error::BoxedError;

#[cfg(feature = "alloc")]
/// A basic validation context with an error type of [`BoxedError`].
pub struct ErrorContext;

#[cfg(feature = "alloc")]
impl Context for ErrorContext {
    type Error = BoxedError;
}

/// A type that can check whether a pointer points to a valid value.
///
/// `CheckBytes` can be derived with [`CheckBytes`](macro@CheckBytes) or
/// implemented manually for custom behavior.
///
/// # Safety
///
/// `check_bytes` must only return `Ok` if `value` points to a valid instance of
/// `Self`. Because `value` must always be properly aligned for `Self` and point
/// to enough bytes to represent the type, this implies that `value` may be
/// dereferenced safely.
pub unsafe trait CheckBytes<C: Context + ?Sized> {
    /// Checks whether the given pointer points to a valid value within the
    /// given context.
    ///
    /// # Safety
    ///
    /// The passed pointer must be aligned and point to enough initialized bytes
    /// to represent the type.
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error>;
}

macro_rules! impl_primitive {
    ($type:ty) => {
        // SAFETY: All bit patterns are valid for these primitive types.
        unsafe impl<C: Context + ?Sized> CheckBytes<C> for $type {
            #[inline]
            unsafe fn check_bytes(
                _: *const Self,
                _: &mut C,
            ) -> Result<(), C::Error> {
                Ok(())
            }
        }
    };
}

macro_rules! impl_primitives {
    ($($type:ty),* $(,)?) => {
        $(
            impl_primitive!($type);
        )*
    }
}

impl_primitives! {
    (),
    i8, i16, i32, i64, i128,
    u8, u16, u32, u64, u128,
    f32, f64,
}
#[cfg(target_has_atomic = "8")]
impl_primitives!(AtomicI8, AtomicU8);
#[cfg(target_has_atomic = "16")]
impl_primitives!(AtomicI16, AtomicU16);
#[cfg(target_has_atomic = "32")]
impl_primitives!(AtomicI32, AtomicU32);
#[cfg(target_has_atomic = "64")]
impl_primitives!(AtomicI64, AtomicU64);

// SAFETY: `PhantomData` is a zero-sized type and so all bit patterns are valid.
unsafe impl<T: ?Sized, C: Context + ?Sized> CheckBytes<C> for PhantomData<T> {
    #[inline]
    unsafe fn check_bytes(_: *const Self, _: &mut C) -> Result<(), C::Error> {
        Ok(())
    }
}

// SAFETY: `PhantomPinned` is a zero-sized type and so all bit patterns are
// valid.
unsafe impl<C: Context + ?Sized> CheckBytes<C> for PhantomPinned {
    #[inline]
    unsafe fn check_bytes(_: *const Self, _: &mut C) -> Result<(), C::Error> {
        Ok(())
    }
}

#[derive(Debug)]
struct ManuallyDropContext;

impl Display for ManuallyDropContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "while `ManuallyDrop`")
    }
}

// SAFETY: `ManuallyDrop<T>` is a `#[repr(transparent)]` wrapper around a `T`,
// and so `value` points to a valid `ManuallyDrop<T>` if it also points to a
// valid `T`.
unsafe impl<C, T> CheckBytes<C> for ManuallyDrop<T>
where
    C: Context + ?Sized,
    T: CheckBytes<C> + ?Sized,
{
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        c: &mut C,
    ) -> Result<(), C::Error> {
        let inner_ptr =
            // SAFETY: Because `ManuallyDrop<T>` is `#[repr(transparent)]`, a
            // pointer to a `ManuallyDrop<T>` is guaranteed to be the same as a
            // pointer to `T`. We can't call `.cast()` here because `T` may be
            // an unsized type.
            unsafe { core::mem::transmute::<*const Self, *const T>(value) };
        // SAFETY: The caller has guaranteed that `value` is aligned for
        // `ManuallyDrop<T>` and points to enough bytes to represent
        // `ManuallyDrop<T>`. Since `ManuallyDrop<T>` is `#[repr(transparent)]`,
        // `inner_ptr` is also aligned for `T` and points to enough bytes to
        // represent it.
        unsafe {
            T::check_bytes(inner_ptr, c)
                .map_err(|e| e.context(ManuallyDropContext))
        }
    }
}

#[derive(Debug)]
struct BoolCheckError {
    byte: u8,
}

impl Display for BoolCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "expected bool byte to be 0 or 1, actual was {}",
            self.byte
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BoolCheckError {}

// SAFETY: A bool is a one byte value that must either be 0 or 1. `check_bytes`
// only returns `Ok` if `value` is 0 or 1.
unsafe impl<C: Context + ?Sized> CheckBytes<C> for bool {
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        _: &mut C,
    ) -> Result<(), C::Error> {
        // SAFETY: `value` is a pointer to a `bool`, which has a size and
        // alignment of one. `u8` also has a size and alignment of one, and all
        // bit patterns are valid for `u8`. So we can cast `value` to a `u8`
        // pointer and read from it.
        let byte = unsafe { *value.cast::<u8>() };
        match byte {
            0 | 1 => Ok(()),
            _ => Err(C::Error::new_with(|| BoolCheckError { byte })),
        }
    }
}

#[cfg(target_has_atomic = "8")]
// SAFETY: `AtomicBool` has the same ABI as `bool`, so if `value` points to a
// valid `bool` then it also points to a valid `AtomicBool`.
unsafe impl<C: Context + ?Sized> CheckBytes<C> for AtomicBool {
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error> {
        // SAFETY: `AtomicBool` has the same ABI as `bool`, so a pointer that is
        // aligned for `AtomicBool` and points to enough bytes for `AtomicBool`
        // is also aligned for `bool` and points to enough bytes for `bool`.
        unsafe { bool::check_bytes(value.cast(), context) }
    }
}

// SAFETY: If `char::try_from` succeeds with the pointed-to-value, then it must
// be a valid value for `char`.
unsafe impl<C: Context + ?Sized> CheckBytes<C> for char {
    #[inline]
    unsafe fn check_bytes(ptr: *const Self, _: &mut C) -> Result<(), C::Error> {
        // SAFETY: `char` and `u32` are both four bytes, but we're not
        // guaranteed that they have the same alignment. Using `read_unaligned`
        // ensures that we can read a `u32` regardless and try to convert it to
        // a `char`.
        let value = unsafe { ptr.cast::<u32>().read_unaligned() };
        char::try_from(value).map_err(C::Error::new)?;
        Ok(())
    }
}

#[derive(Debug)]
struct TupleIndexContext {
    index: usize,
}

impl Display for TupleIndexContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "while checking index {} of tuple", self.index)
    }
}

macro_rules! impl_tuple {
    ($($type:ident $index:tt),*) => {
        // SAFETY: A tuple is valid if all of its elements are valid, and
        // `check_bytes` only returns `Ok` when all of the elements validated
        // successfully.
        unsafe impl<$($type,)* C> CheckBytes<C> for ($($type,)*)
        where
            $($type: CheckBytes<C>,)*
            C: Context + ?Sized,
        {
            #[inline]
            #[allow(clippy::unneeded_wildcard_pattern)]
            unsafe fn check_bytes(
                value: *const Self,
                context: &mut C,
            ) -> Result<(), C::Error> {
                $(
                    // SAFETY: The caller has guaranteed that `value` points to
                    // enough bytes for this tuple and is properly aligned, so
                    // we can create pointers to each element and check them.
                    unsafe {
                        <$type>::check_bytes(
                            ptr::addr_of!((*value).$index),
                            context,
                        ).map_err(|e| e.context(TupleIndexContext {
                            index: $index,
                        }))?;
                    }
                )*
                Ok(())
            }
        }
    }
}

impl_tuple!(T0 0);
impl_tuple!(T0 0, T1 1);
impl_tuple!(T0 0, T1 1, T2 2);
impl_tuple!(T0 0, T1 1, T2 2, T3 3);
impl_tuple!(T0 0, T1 1, T2 2, T3 3, T4 4);
impl_tuple!(T0 0, T1 1, T2 2, T3 3, T4 4, T5 5);
impl_tuple!(T0 0, T1 1, T2 2, T3 3, T4 4, T5 5, T6 6);
impl_tuple!(T0 0, T1 1, T2 2, T3 3, T4 4, T5 5, T6 6, T7 7);
impl_tuple!(T0 0, T1 1, T2 2, T3 3, T4 4, T5 5, T6 6, T7 7, T8 8);
impl_tuple!(T0 0, T1 1, T2 2, T3 3, T4 4, T5 5, T6 6, T7 7, T8 8, T9 9);
impl_tuple!(T0 0, T1 1, T2 2, T3 3, T4 4, T5 5, T6 6, T7 7, T8 8, T9 9, T10 10);
impl_tuple!(
    T0 0, T1 1, T2 2, T3 3, T4 4, T5 5, T6 6, T7 7, T8 8, T9 9, T10 10, T11 11
);
impl_tuple!(
    T0 0, T1 1, T2 2, T3 3, T4 4, T5 5, T6 6, T7 7, T8 8, T9 9, T10 10, T11 11,
    T12 12
);

#[derive(Debug)]
struct ArrayCheckContext {
    index: usize,
}

impl Display for ArrayCheckContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "while checking index '{}' of array", self.index)
    }
}

// SAFETY: `check_bytes` only returns `Ok` if each element of the array is
// valid. If each element of the array is valid then the whole array is also
// valid.
unsafe impl<T, C, const N: usize> CheckBytes<C> for [T; N]
where
    T: CheckBytes<C>,
    C: Context + ?Sized,
{
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error> {
        let base = value.cast::<T>();
        for index in 0..N {
            // SAFETY: The caller has guaranteed that `value` points to enough
            // bytes for this array and is properly aligned, so we can create
            // pointers to each element and check them.
            unsafe {
                T::check_bytes(base.add(index), context)
                    .map_err(|e| e.context(ArrayCheckContext { index }))?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct SliceCheckContext {
    index: usize,
}

impl Display for SliceCheckContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "while checking index '{}' of slice", self.index)
    }
}

// SAFETY: `check_bytes` only returns `Ok` if each element of the slice is
// valid. If each element of the slice is valid then the whole slice is also
// valid.
unsafe impl<T: CheckBytes<C>, C: Context + ?Sized> CheckBytes<C> for [T] {
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error> {
        let (data_address, len) = PtrExt::to_raw_parts(value);
        let base = data_address.cast::<T>();
        for index in 0..len {
            // SAFETY: The caller has guaranteed that `value` points to enough
            // bytes for this slice and is properly aligned, so we can create
            // pointers to each element and check them.
            unsafe {
                T::check_bytes(base.add(index), context)
                    .map_err(|e| e.context(SliceCheckContext { index }))?;
            }
        }
        Ok(())
    }
}

// SAFETY: `check_bytes` only returns `Ok` if the bytes pointed to by `str` are
// valid UTF-8. If they are valid UTF-8 then the overall `str` is also valid.
unsafe impl<C: Context + ?Sized> CheckBytes<C> for str {
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        _: &mut C,
    ) -> Result<(), C::Error> {
        let slice_ptr = value as *const [u8];
        // SAFETY: The caller has guaranteed that `value` is properly-aligned
        // and points to enough bytes for its `str`. Because a `u8` slice has
        // the same layout as a `str`, we can dereference it for UTF-8
        // validation.
        let slice = unsafe { &*slice_ptr };
        from_utf8(slice).map(|_| ()).map_err(C::Error::new)
    }
}

#[cfg(feature = "std")]
// SAFETY: `check_bytes` only returns `Ok` when the bytes constitute a valid
// `CStr` per `CStr::from_bytes_with_nul`.
unsafe impl<C: Context + ?Sized> CheckBytes<C> for std::ffi::CStr {
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        _: &mut C,
    ) -> Result<(), C::Error> {
        let slice_ptr = value as *const [u8];
        // SAFETY: The caller has guaranteed that `value` is properly-aligned
        // and points to enough bytes for its `CStr`. Because a `u8` slice has
        // the same layout as a `CStr`, we can dereference it for validation.
        let slice = unsafe { &*slice_ptr };
        std::ffi::CStr::from_bytes_with_nul(slice).map_err(C::Error::new)?;
        Ok(())
    }
}

// Generic contexts used by the derive.

/// Context for errors resulting from invalid structs.
#[derive(Debug)]
pub struct StructCheckContext {
    /// The name of the struct with an invalid field.
    pub struct_name: &'static str,
    /// The name of the struct field that was invalid.
    pub field_name: &'static str,
}

impl Display for StructCheckContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "while checking field '{}' of struct '{}'",
            self.field_name, self.struct_name
        )
    }
}

/// Context for errors resulting from invalid tuple structs.
#[derive(Debug)]
pub struct TupleStructCheckContext {
    /// The name of the tuple struct with an invalid field.
    pub tuple_struct_name: &'static str,
    /// The index of the tuple struct field that was invalid.
    pub field_index: usize,
}

impl Display for TupleStructCheckContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "while checking field index {} of tuple struct '{}'",
            self.field_index, self.tuple_struct_name,
        )
    }
}

/// An error resulting from an invalid enum tag.
#[derive(Debug)]
pub struct InvalidEnumDiscriminantError<T> {
    /// The name of the enum with an invalid discriminant.
    pub enum_name: &'static str,
    /// The invalid value of the enum discriminant.
    pub invalid_discriminant: T,
}

impl<T: Display> Display for InvalidEnumDiscriminantError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid discriminant '{}' for enum '{}'",
            self.invalid_discriminant, self.enum_name
        )
    }
}

#[cfg(feature = "std")]
impl<T> std::error::Error for InvalidEnumDiscriminantError<T> where
    T: fmt::Debug + fmt::Display
{
}

/// Context for errors resulting from checking enum variants with named fields.
#[derive(Debug)]
pub struct NamedEnumVariantCheckContext {
    /// The name of the enum with an invalid variant.
    pub enum_name: &'static str,
    /// The name of the variant that was invalid.
    pub variant_name: &'static str,
    /// The name of the field that was invalid.
    pub field_name: &'static str,
}

impl Display for NamedEnumVariantCheckContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "while checking field '{}' of variant '{}' of enum '{}'",
            self.field_name, self.variant_name, self.enum_name,
        )
    }
}

/// Context for errors resulting from checking enum variants with unnamed
/// fields.
#[derive(Debug)]
pub struct UnnamedEnumVariantCheckContext {
    /// The name of the enum with an invalid variant.
    pub enum_name: &'static str,
    /// The name of the variant that was invalid.
    pub variant_name: &'static str,
    /// The name of the field that was invalid.
    pub field_index: usize,
}

impl Display for UnnamedEnumVariantCheckContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "while checking field index {} of variant '{}' of enum '{}'",
            self.field_index, self.variant_name, self.enum_name,
        )
    }
}

// Range types

// SAFETY: A `Range<T>` is valid if its `start` and `end` are both valid, and
// `check_bytes` only returns `Ok` when both `start` and `end` are valid. Note
// that `Range` does not require `start` be less than `end`.
unsafe impl<T, C> CheckBytes<C> for ops::Range<T>
where
    T: CheckBytes<C>,
    C: Context + ?Sized,
{
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error> {
        // SAFETY: The caller has guaranteed that `value` is aligned for a
        // `Range<T>` and points to enough initialized bytes for one, so a
        // pointer projected to the `start` field will be properly aligned for
        // a `T` and point to enough initialized bytes for one too.
        unsafe {
            T::check_bytes(ptr::addr_of!((*value).start), context).map_err(
                |e| {
                    e.context(StructCheckContext {
                        struct_name: "Range",
                        field_name: "start",
                    })
                },
            )?;
        }
        // SAFETY: Same reasoning as above, but for `end`.
        unsafe {
            T::check_bytes(ptr::addr_of!((*value).end), context).map_err(
                |e| {
                    e.context(StructCheckContext {
                        struct_name: "Range",
                        field_name: "end",
                    })
                },
            )?;
        }
        Ok(())
    }
}

// SAFETY: A `RangeFrom<T>` is valid if its `start` is valid, and `check_bytes`
// only returns `Ok` when its `start` is valid.
unsafe impl<T: CheckBytes<C>, C: Context + ?Sized> CheckBytes<C>
    for ops::RangeFrom<T>
{
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error> {
        // SAFETY: The caller has guaranteed that `value` is aligned for a
        // `RangeFrom<T>` and points to enough initialized bytes for one, so a
        // pointer projected to the `start` field will be properly aligned for
        // a `T` and point to enough initialized bytes for one too.
        unsafe {
            T::check_bytes(ptr::addr_of!((*value).start), context).map_err(
                |e| {
                    e.context(StructCheckContext {
                        struct_name: "RangeFrom",
                        field_name: "start",
                    })
                },
            )?;
        }
        Ok(())
    }
}

// SAFETY: `RangeFull` is a ZST and so every pointer to one is valid.
unsafe impl<C: Context + ?Sized> CheckBytes<C> for ops::RangeFull {
    #[inline]
    unsafe fn check_bytes(_: *const Self, _: &mut C) -> Result<(), C::Error> {
        Ok(())
    }
}

// SAFETY: A `RangeTo<T>` is valid if its `end` is valid, and `check_bytes` only
// returns `Ok` when its `end` is valid.
unsafe impl<T, C> CheckBytes<C> for ops::RangeTo<T>
where
    T: CheckBytes<C>,
    C: Context + ?Sized,
{
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error> {
        // SAFETY: The caller has guaranteed that `value` is aligned for a
        // `RangeTo<T>` and points to enough initialized bytes for one, so a
        // pointer projected to the `end` field will be properly aligned for
        // a `T` and point to enough initialized bytes for one too.
        unsafe {
            T::check_bytes(ptr::addr_of!((*value).end), context).map_err(
                |e| {
                    e.context(StructCheckContext {
                        struct_name: "RangeTo",
                        field_name: "end",
                    })
                },
            )?;
        }
        Ok(())
    }
}

// SAFETY: A `RangeToInclusive<T>` is valid if its `end` is valid, and
// `check_bytes` only returns `Ok` when its `end` is valid.
unsafe impl<T, C> CheckBytes<C> for ops::RangeToInclusive<T>
where
    T: CheckBytes<C>,
    C: Context + ?Sized,
{
    #[inline]
    unsafe fn check_bytes(
        value: *const Self,
        context: &mut C,
    ) -> Result<(), C::Error> {
        // SAFETY: The caller has guaranteed that `value` is aligned for a
        // `RangeToInclusive<T>` and points to enough initialized bytes for one,
        // so a pointer projected to the `end` field will be properly aligned
        // for a `T` and point to enough initialized bytes for one too.
        unsafe {
            T::check_bytes(ptr::addr_of!((*value).end), context).map_err(
                |e| {
                    e.context(StructCheckContext {
                        struct_name: "RangeToInclusive",
                        field_name: "end",
                    })
                },
            )?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct NonZeroCheckError;

impl fmt::Display for NonZeroCheckError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "nonzero integer is zero")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NonZeroCheckError {}

macro_rules! impl_nonzero {
    ($nonzero:ident, $underlying:ident) => {
        // SAFETY: `check_bytes` only returns `Ok` when `value` is not zero, the
        // only validity condition for non-zero integer types.
        unsafe impl<C: Context + ?Sized> CheckBytes<C> for $nonzero {
            #[inline]
            unsafe fn check_bytes(
                value: *const Self,
                _: &mut C,
            ) -> Result<(), C::Error> {
                // SAFETY: Non-zero integer types are guaranteed to have the
                // same ABI as their corresponding integer types. Those integers
                // have no validity requirements, so we can cast and dereference
                // value to check if it is equal to zero.
                if unsafe { *value.cast::<$underlying>() } == 0 {
                    Err(C::Error::new_with(|| NonZeroCheckError))
                } else {
                    Ok(())
                }
            }
        }
    };
}

impl_nonzero!(NonZeroI8, i8);
impl_nonzero!(NonZeroI16, i16);
impl_nonzero!(NonZeroI32, i32);
impl_nonzero!(NonZeroI64, i64);
impl_nonzero!(NonZeroI128, i128);
impl_nonzero!(NonZeroU8, u8);
impl_nonzero!(NonZeroU16, u16);
impl_nonzero!(NonZeroU32, u32);
impl_nonzero!(NonZeroU64, u64);
impl_nonzero!(NonZeroU128, u128);
