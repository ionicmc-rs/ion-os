//! I/O Errors
//! 
//! IO errors are very complex, as `I/O` isn't one thing used in a small amount of places - I/O is everything.
//! 
//! So, that means we need to support a large range of errors - how can we do that?
//! # The [`Error`] type
//! The error type is adapted from `std::io` to fit our kernel, it is truly a beautiful API, and [this blog](https://matklad.github.io/2020/10/15/study-of-std-io-error.html) shows off the true brilliance of `std::io::Error`.
//! 
//! These [`Error`]s can either be predefined errors ([`ErrorKind`]), a simple message (string literals), OS Error Codes ([`libc::get_errno`](crate::c_lib::libc::get_errno)), or custom Errors that implement [`core::error::Error`]
//! # The Importance of [`Error`]
//! Without [`Error`], we would not have a stable API for I/O error handling. Unfortunately, in the modern era of OSDev, many widely used OSes,
//! (like Windows and MacOS) do not have stable APIs for basic things.
//! 
//! if we want Ion OS to become big - stabilization is key.
//! # Custom VS. Os errors.
//! In reality, you should not compare the two - in this case, we are the OS; we create the errors and get to decide what to use.
//! 
//! OS Errors are used for low-level error-management that can be accessed by processes, while custom errors are for internal OS errors.
//! # Example
//! ```
//! fn read_file() -> io::Result<&str> {
//!     // ...
//! # let contents = "";
//! # let failed = false;
//!     if failed {
//!         return Err(Error::new(ErrorKind::NotFound, "could not find the file."))
//!     } else {
//!         return Ok(contents)
//!     }
//! }
//! ```

use core::{error, fmt::Display};

use alloc::boxed::Box;

use crate::c_lib::libc;

/// The Result type for I/O errors
/// 
/// note that this may allocate.
pub type Result<T = ()> = core::result::Result<T, Error>;

/// An I/O Error.
/// 
/// see the [`module level documentation`](self)
#[derive(Debug)]
pub struct Error {
    data: ErrorData
}

/// An I/O error, evaluated at compile type
pub macro const_err($kind:ident, $msg:literal) {
    $crate::io::Error::from_static_msg($crate::io::ErrorKind::$kind, $msg)
}

impl Error {
    /// Creates a new Error with the given [`ErrorKind`].
    /// 
    /// # Warning to callers
    /// This function allocates to the heap, so it cannot be used for allocation errors.
    /// # Example
    /// ```
    /// let error = Error::new(ErrorKind::NotFound, "Could not find the required resource.");
    /// ```
    #[inline]
    pub fn new<E: Into<Box<dyn error::Error + Send + Sync + 'static>>>(kind: ErrorKind, error: E) -> Self {
        Error {
            data: ErrorData::Custom(Custom  { kind, error: error.into() })
        }
    }

    /// Creates a new error with the [`Other`](ErrorKind::Other) [`ErrorKind`]
    /// 
    /// see [`new`](Self::new) for more info and examples.
    #[inline]
    pub fn other<E: Into<Box<dyn error::Error + Send + Sync + 'static>>>(error: E) -> Self {
        Self::new(ErrorKind::Other, error)
    }

    /// Creates an error from a string literal.
    /// 
    /// used in [`const_err`]
    /// # Example
    /// example using [`const_err`]
    /// ```
    /// const NOT_FOUND: Error = const_err!(NotFound, "The resource was not found");
    /// ```
    #[inline]
    pub const fn from_static_msg(kind: ErrorKind, msg: &'static str) -> Self {
        Self { data: ErrorData::SimpleMessage(kind, msg) }
    }

    /// Gets the last OS error as an [`Error`]
    /// 
    /// if you're trying to retrieve an error from a function called, call it immediately after the function to minimize the risk of
    /// interrupt handlers setting error codes.
    /// # Example
    /// ```
    /// let file_read = read_file();
    /// let last_os_err = Error::last_os_err(); // file error if read fails
    /// ```
    #[inline]
    #[must_use]
    #[doc(alias = "GetLastError")]
    #[doc(alias = "errno")]
    pub fn last_os_err() -> Self {
        Self { data: ErrorData::Os(*libc::get_errno()) }
    }

    /// Returns the OS code if available.
    /// 
    /// Only returns [`Some`] if this [`Error`] was created using [`last_os_err`](Error::last_os_err).
    ///
    /// # Examples
    ///
    /// ```
    /// // ...
    /// use ion_kernel::io::error::Error;
    ///
    /// let error = Error::last_os_err();
    /// assert_eq!(error.os_code(), libc::get_errno());
    /// ```
    #[inline]
    pub fn os_code(&self) -> Option<i32> {
        match self.data {
            ErrorData::Os(v) => Some(v),
            _ => None
        }
    }

    /// Returns the kind of the error, if available.
    ///
    /// # Examples
    ///
    /// ```
    /// use ion_kernel::io::error::Error;
    ///
    /// let error = Error::NOT_FOUND;
    /// assert_eq!(error.kind(), ErrorKind::NotFound);
    /// ```
    pub fn kind(&self) -> Option<ErrorKind> {
        match self.data {
            ErrorData::Custom(Custom { kind, .. }) => Some(kind),
            ErrorData::Os(code) => decode_os_code(code),
            ErrorData::Simple(kind) => Some(kind),
            ErrorData::SimpleMessage(..) => None
        }
    }

    /// Returns a reference to the inner error.
    /// 
    /// This calls [`Box::as_ref`](core::convert::AsRef::as_ref).
    ///
    /// # Examples
    ///
    /// ```
    /// use ion_kernel::io::error::Error;
    ///
    /// let error = Error::NOT_FOUND;
    /// let ref_to_err = error.get_ref();
    /// ```
    pub fn get_ref(&self) -> Option<&(dyn error::Error + Send + Sync + 'static)> {
        match self.data {
            ErrorData::Custom(Custom { ref error, .. }) => Some(error.as_ref()),
            _ => None,
        }
    }

    /// Returns a mutable reference to the inner error.
    /// 
    /// This calls [`Box::as_mut`](core::convert::AsMut::as_mut).
    /// # Examples
    ///
    /// ```
    /// use ion_kernel::io::error::Error;
    ///
    /// let error = Error::NOT_FOUND;
    /// let ref_to_err = error.get_mut();
    /// ```
    pub fn get_mut(&mut self) -> Option<&mut (dyn error::Error + Send + Sync + 'static)> {
        match self.data {
            ErrorData::Custom(Custom { ref mut error, .. }) => Some(error.as_mut()),
            _ => None,
        }
    }

    /// Consumes the `Error`, returning its inner error (if any).
    ///
    /// If this [`Error`] was constructed via [`new`] or [`other`],
    /// then this function will return [`Some`],
    /// otherwise it will return [`None`].
    ///
    /// [`new`]: Error::new
    /// [`other`]: Error::other
    /// # Example
    /// ```
    /// let error = Error::other("It failed!");
    /// 
    /// let original = error.into_inner();
    /// assert_eq!(original.downcast_ref().unwrap(), "It failed!");
    /// ```
    #[must_use = "`self` will be dropped if the result is not used"]
    #[inline]
    pub fn into_inner(self) -> Option<Box<dyn error::Error + Send + Sync>> {
        match self.data {
            ErrorData::Os(..) => None,
            ErrorData::Simple(..) => None,
            ErrorData::SimpleMessage(..) => None,
            ErrorData::Custom(c) => Some(c.error),
        }
    }

    /// attempts to downcast the [`Error`] into `E`, consuming the [`Error`].
    /// 
    /// This is useful if you know the type of the error
    /// # Example
    /// ```
    /// let err = Error::other("Error!");
    /// assert_eq!(err.downcast::<&str>(), "Error!");
    /// ```
    pub fn downcast<E: error::Error + Send + Sync + 'static>(self) -> Option<E> {
        match self.data {
            ErrorData::Custom(c) if c.error.is::<E>() => {
                unsafe {
                    // explicit use of unwrap_unchecked to show we are sure the value is correct.
                    let downcast = c.error.downcast::<E>().unwrap_unchecked();
                    Some(*downcast)
                }
            }
            _ => None
        }
    }

    // CONSTS

    /// An Error for Seekers that do not have ends, which are called with `SeekFrom::End`.
    pub const UNSEEKABLE_INFINITE: Self = const_err!(NotSeekable, "cannot `SeekFrom::End` because the reader/writer is endless.");

    /// An error for [`Read`](super::Read)ers that do not have ends, but are required to be fully read.
    pub const UNREADABLE_INFINITE: Self = const_err!(WouldBlock, "cannot read the reader because it would block infinitely.");

    /// An General errors for resources that are missing.
    pub const NOT_FOUND: Self = const_err!(NotFound, "the resource was not found.");

    /// Error for invalid utf8 in readers.
    pub const INVALID_UTF8: Self = const_err!(InvalidData, "the reader did not contain valid utf8");

    /// Error for invalid 0 timeouts.
    pub const ZERO_TIMEOUT: Self = const_err!(InvalidInput, "cannot set a 0 timeout");

    /// Error for timing out.
    pub const TIME_OUT: Self = const_err!(TimedOut, "could not finish operation in time");

    // EOF

    /// Error for early EOF.
    pub const EARLY_EOF: Self = const_err!(UnexpectedEof, "early EOF");

    /// Error for requiring a non-empty reader to write, but read returns [`Ok(0)`](Ok)
    pub const WRITE_TO_EMPTY: Self = const_err!(WriteZero, "needed a non-empty reader, but `read` returned EOF");
}

fn decode_os_code(code: i32) -> Option<ErrorKind> {
    match code {
        0 => None,
        // 1 is simply "Failed", so we route it to other.
        2 => Some(ErrorKind::InvalidData),
        3 => Some(ErrorKind::Interrupted),
        // 4 is Process Failure which makes no sense for ErrorKind
        5 => Some(ErrorKind::MemoryError),
        6 => Some(ErrorKind::InvalidInput),
        7 => Some(ErrorKind::Unsupported),
        _ => Some(ErrorKind::Other)
    }
}

impl From<ErrorKind> for Error {
    fn from(value: ErrorKind) -> Self {
        Error {
            data: ErrorData::Simple(value)
        }
    }
}

impl From<alloc::alloc::AllocError> for Error {
    fn from(_: alloc::alloc::AllocError) -> Self {
        ErrorKind::MemoryError.into()
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.data {
            ErrorData::Custom(Custom { kind, error }) => write!(f, "{}: {error}", kind.as_str()),
            ErrorData::Os(v) => write!(f, "{} (os error {v})", decode_os_code(*v).unwrap_or_default().as_str()),
            ErrorData::Simple(v) => write!(f, "{}", v.as_str()),
            ErrorData::SimpleMessage(kind, msg) => write!(f, "{}: {msg}", kind.as_str()),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self.data {
            ErrorData::Custom(Custom { ref error, .. }) => Some(error.as_ref()),
            _ => None
        }
    }
}

#[derive(Debug)]
struct Custom {
    kind: ErrorKind,
    error: Box<dyn error::Error + Sync + Send>
}

#[derive(Debug)]
enum ErrorData {
    Os(i32),
    Simple(ErrorKind),
    SimpleMessage(ErrorKind, &'static str),
    Custom(Custom)
}

/// A Kind of Error
/// 
/// see the [`module level docs`](self) for more info
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorKind {
    /// Resource not found
    NotFound,

    /// Something is unsupported/deprecated.
    Unsupported,

    /// Permission was denied to preform this operation
    PermissionDenied,

    /// The requested resource already exists
    AlreadyExists,

    /// The operation would block, but is requested not to.
    WouldBlock,

    /// Received invalid input.
    InvalidInput,

    /// Data is invalid
    /// 
    /// more general form of [`InvalidInput`](ErrorKind::InvalidInput)
    InvalidData,

    /// Timed Out.
    TimedOut,

    /// An error returned when an operation could not be completed because a
    /// call to [`write`] returned [`Ok(0)`].
    ///
    /// This typically means that an operation could only succeed if it wrote a
    /// particular number of bytes but only a smaller number of bytes could be
    /// written.
    ///
    /// [`write`]: crate::io::Write::write
    /// [`Ok(0)`]: Ok
    WriteZero,

    /// Could not store data because the storage is full.
    StorageFull,

    /// The reader/writer is not seekable
    NotSeekable,

    /// The Quota was exceeded.
    QuotaExceeded,

    /// The Resource is Busy.
    ResourceBusy,

    /// Avoided a Deadlock
    Deadlock,

    /// Too many Arguments.
    TooManyArguments,

    /// The operation was interrupted.
    Interrupted,

    /// A memory Error.
    MemoryError,

    // Errors to be posted in `ERRNO` are to be put above this comment

    /// EOF, but it was unexpected.
    UnexpectedEof,

    /// The operation was partially successful and needs to be checked
    /// later on due to not blocking.
    InProgress,
        

    // OS Errors not to be posted in ERRNO are to be put above this comment

    /// Represents errors that do not fall into any other category.
    #[default]
    Other
}

impl ErrorKind {
    /// String representation of the [`ErrorKind`]
    pub fn as_str(&self) -> &'static str {
        use ErrorKind::*;
        match self {
            AlreadyExists => "already exists",
            Deadlock => "avoided deadlock",
            InProgress => "in progress",
            Interrupted => "interrupted",
            InvalidData => "invalid data",
            InvalidInput => "invalid input",
            MemoryError => "memory error",
            NotFound => "resource not found",
            NotSeekable => "seek on unseekable reader",
            PermissionDenied => "permission denied",
            QuotaExceeded => "quota exceeded",
            ResourceBusy => "resource busy",
            StorageFull => "storage full",
            TimedOut => "timed out",
            TooManyArguments => "too many arguments",
            UnexpectedEof => "unexpected end of file",
            Unsupported => "unsupported operation",
            WouldBlock => "non blocking operation must block",
            WriteZero => "write to zeroed reader",
            Other => "unknown error"
        }
    }
}

// Convert Impls

// alloc
impl From<core::alloc::LayoutError> for Error {
    /// Converts to this type from the input type.
    /// 
    /// (Note that the LayoutError cannot be retrieved from the error due to us not being able to allocate)
    fn from(_: core::alloc::LayoutError) -> Self {
        const_err!(InvalidInput, "invalid size and/or align for `Layout::from_size_align`")
    }
}

impl From<alloc::ffi::NulError> for Error {
    fn from(value: alloc::ffi::NulError) -> Self {
        Self::new(ErrorKind::InvalidData, value)
    }
}