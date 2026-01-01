// Copied from libc

/// Implement `Clone` and `Copy` for a tuple struct, as well as `Debug`, `Eq`, `Hash`,
/// and `PartialEq` if the `extra_traits` feature is enabled.
///
/// This is the same as [`s`] but works for tuple structs.
pub macro s_paren {
    ($(
        $(#[$attr:meta])*
        pub struct $i:ident ( $($field:tt)* );
    )*) => ($(
        __item! {
            #[::core::prelude::v1::derive(
                ::core::clone::Clone,
                ::core::marker::Copy,
                ::core::fmt::Debug,
            )]
            $(#[$attr])*
            pub struct $i ( $($field)* );
        }
    )*)
}

/// Implement `Clone`, `Copy`, and `Debug` since those can be derived, but exclude `PartialEq`,
/// `Eq`, and `Hash`.
pub macro s {
    ($(
        $(#[$attr:meta])*
        $pub:vis $t:ident $i:ident { $($field:tt)* }
    )*) => ($(
        s_no_extra_traits!(it: $(#[$attr])* $pub $t $i { $($field)* });
    )*),

    (it: $(#[$attr:meta])* $pub:vis union $i:ident { $($field:tt)* }) => (
        __item! {
            #[repr(C)]
            #[::core::prelude::v1::derive(::core::clone::Clone, ::core::marker::Copy)]
            $(#[$attr])*
            $pub union $i { $($field)* }
        }

        impl ::core::fmt::Debug for $i {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_struct(::core::stringify!($i)).finish_non_exhaustive()
            }
        }
    ),

    (it: $(#[$attr:meta])* $pub:vis struct $i:ident { $($field:tt)* }) => (
        __item! {
            #[repr(C)]
            #[::core::prelude::v1::derive(
                ::core::clone::Clone,
                ::core::marker::Copy,
                ::core::fmt::Debug,
            )]
            $(#[$attr])*
            $pub struct $i { $($field)* }
        }
    )
}

/// Create an uninhabited type that can't be constructed.
///
/// Really what we want here is something that also can't be named without indirection (in
/// ADTs or function signatures), but this doesn't exist.
pub macro extern_ty {
    ($(
        $(#[$attr:meta])*
        pub enum $i:ident {}
    )*) => ($(
        $(#[$attr])*
        // FIXME(1.0): the type is uninhabited so these traits are unreachable and could be
        // removed.
        #[::core::prelude::v1::derive(
            ::core::clone::Clone,
            ::core::marker::Copy,
            ::core::fmt::Debug,
        )]
        pub enum $i { }
    )*)
}

/// Represent a C enum as Rust constants and a type.
///
/// C enums can't soundly be mapped to Rust enums since C enums are allowed to have duplicates or
/// unlisted values, but this is UB in Rust. This enum doesn't implement any traits, its main
/// purpose is to calculate the correct enum values.
///
/// Use the magic name `#anon` if the C enum doesn't create a type.
///
pub macro c_enum {
    // Matcher for multiple enums
    ($(
        $(#[repr($repr:ty)])?
        pub enum $($ty_name:ident)? $(#$anon:ident)? {
            $($vis:vis $variant:ident $(= $value:expr)?,)+
        }
    )+) => {
        $(c_enum!(@single;
            $(#[repr($repr)])?
            pub enum $($ty_name)? $(#$anon)? {
                $($vis $variant $(= $value)?,)+
            }
        );)+
    },

    // Matcher for a single enum
    (@single;
        $(#[repr($repr:ty)])?
        pub enum $ty_name:ident {
            $($vis:vis $variant:ident $(= $value:expr)?,)+
        }
    ) => {
        pub type $ty_name = c_enum!(@ty $($repr)?);
        c_enum! {
            @variant;
            ty: $ty_name;
            default: 0;
            variants: [$($vis $variant $(= $value)?,)+]
        }
    },

    // Matcher for a single anonymous enum
    (@single;
        $(#[repr($repr:ty)])?
        pub enum #anon {
            $($vis:vis $variant:ident $(= $value:expr)?,)+
        }
    ) => {
        c_enum! {
            @variant;
            ty: c_enum!(@ty $($repr)?);
            default: 0;
            variants: [$($vis $variant $(= $value)?,)+]
        }
    },

    // Matcher for variants: eats a single variant then recurses with the rest
    (@variant; ty: $_ty_name:ty; default: $_idx:expr; variants: []) => { /* end of the chain */ },
    (
        @variant;
        ty: $ty_name:ty;
        default: $default_val:expr;
        variants: [
            $vis:vis $variant:ident $(= $value:expr)?,
            $($tail:tt)*
        ]
    ) => {
        $vis const $variant: $ty_name = {
            #[allow(unused_variables)]
            let r = $default_val;
            $(let r = $value;)?
            r
        };

        // The next value is always one more than the previous value, unless
        // set explicitly.
        c_enum! {
            @variant;
            ty: $ty_name;
            default: $variant + 1;
            variants: [$($tail)*]
        }
    },

    // Use a specific type if provided, otherwise default to `CEnumRepr`
    (@ty $repr:ty) => { $repr },
    (@ty) => { $crate::c_lib::libc::CEnumRepr },
}

/// Define a `unsafe` function.
pub macro f {
    ($(
        $(#[$attr:meta])*
        // Less than ideal hack to match either `fn` or `const fn`.
        pub $(fn $i:ident)? $(const fn $const_i:ident)?
        ($($arg:ident: $argty:ty),* $(,)*) -> $ret:ty
            $body:block
    )+) => {$(
        #[inline]
        $(#[$attr])*
        pub $(unsafe extern "C" fn $i)? $(const unsafe extern "C" fn $const_i)?
        ($($arg: $argty),*) -> $ret
            $body
    )+},
}

/// Define a safe function.
pub macro safe_f {
    ($(
        $(#[$attr:meta])*
        // Less than ideal hack to match either `fn` or `const fn`.
        pub $(fn $i:ident)? $(const fn $const_i:ident)?
        ($($arg:ident: $argty:ty),* $(,)*) -> $ret:ty
            $body:block
    )+) => {$(
        #[inline]
        $(#[$attr])*
        pub $(extern "C" fn $i)? $(const extern "C" fn $const_i)?
        ($($arg: $argty),*) -> $ret
            $body
    )+},
}

pub macro __item {
    ($i:item) => {
        $i
    },
}