//! Error type for the codec.
//!
//! `pack-io` uses a single, `#[non_exhaustive]` error enum that covers every
//! failure mode of both encode and decode. The encode side is infallible for
//! sized in-memory values today; it remains fallible at the type level so the
//! streaming API can wrap the underlying `Write` failure mode in `0.3`.
//!
//! The variants are kept small and concrete: each one names a single failure
//! mode and carries the smallest amount of context needed to act on it. None
//! of them include the malformed bytes themselves — error messages from the
//! codec never echo untrusted input back into a log line.

use core::fmt;

/// Every error returned by the codec.
///
/// `#[non_exhaustive]` so additional variants can be added in a MINOR release
/// without breaking downstream `match` arms. Callers MUST include a wildcard
/// arm.
///
/// # Examples
///
/// ```
/// use pack_io::{decode, SerialError};
///
/// // A length prefix that runs off the end of the buffer is rejected, not
/// // accepted-and-corrected.
/// let bad: &[u8] = &[0xff, 0xff, 0xff, 0xff, 0x0f]; // varint = u32::MAX
/// match decode::<String>(bad) {
///     Ok(_) => unreachable!("hostile length should not decode"),
///     Err(SerialError::InvalidLength { .. })
///     | Err(SerialError::UnexpectedEof { .. }) => {} // expected
///     Err(other) => panic!("unexpected error variant: {other}"),
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    /// The decoder needed more bytes than the input contained.
    ///
    /// `needed` is the number of additional bytes the codec required to make
    /// progress; `remaining` is what was actually left in the buffer.
    UnexpectedEof {
        /// Number of bytes the codec required to make progress.
        needed: usize,
        /// Number of bytes still available in the input when the read failed.
        remaining: usize,
    },

    /// A length prefix declared a value larger than the buffer can hold.
    ///
    /// This is the primary defence against a hostile length-prefix attack:
    /// the decoder refuses to allocate or read past the available input.
    InvalidLength {
        /// The length declared by the prefix, in bytes.
        declared: u64,
        /// Bytes remaining in the input when the prefix was read.
        remaining: usize,
    },

    /// A LEB128 varint exceeded the maximum legal byte count for its target
    /// width (10 bytes for `u64`, 5 for `u32`, 19 for `u128`, etc.).
    VarintOverflow,

    /// A decoded varint did not fit in the requested integer width
    /// (e.g. `u64` decoded successfully but the target was `u32`).
    IntegerOutOfRange,

    /// A boolean byte was neither `0x00` nor `0x01`.
    InvalidBool {
        /// The offending byte. Kept so the caller can log a sanitised summary
        /// (`{:02x}`) without echoing the surrounding payload.
        byte: u8,
    },

    /// A length-prefixed byte run was not valid UTF-8 when decoding a
    /// `String`.
    InvalidUtf8,

    /// A tag byte for `Option` (`0x00` / `0x01`) or `Result` (`0x00` / `0x01`)
    /// was outside the legal range.
    InvalidTag {
        /// Name of the type that owns this tag (`"Option"`, `"Result"`).
        kind: &'static str,
        /// The offending tag byte.
        tag: u8,
    },

    /// A decoded enum variant index did not correspond to any known variant
    /// of the target type. Produced by the `#[derive(Deserialize)]` /
    /// `#[derive(DeserializeView)]` enum deserialisers in `pack-io-derive`.
    ///
    /// `kind` is the enum's type name; `index` is the offending varint
    /// value (read as `u64` so any overflow case is representable).
    UnknownVariant {
        /// Name of the enum that was being decoded.
        kind: &'static str,
        /// The offending varint variant index.
        index: u64,
    },

    /// The input buffer contained trailing bytes after a strict decode
    /// completed. Returned only by [`crate::decode`], which requires the
    /// payload to be fully consumed.
    TrailingBytes {
        /// Number of bytes left over after the value was decoded.
        remaining: usize,
    },

    /// An underlying `std::io::Write` / `std::io::Read` operation failed
    /// while a streaming codec was in flight. Returned only by the
    /// `std`-gated I/O integration (`IoEncoder`, `IoDecoder`,
    /// `encode_into`, `decode_from`).
    ///
    /// The error kind and a stringified message are captured so the variant
    /// remains `Clone + Eq`. The original `std::io::Error` is not preserved
    /// — log the captured `message` field for diagnostics.
    #[cfg(feature = "std")]
    Io {
        /// Classification of the underlying I/O failure.
        kind: std::io::ErrorKind,
        /// Human-readable rendering of the original `std::io::Error`.
        message: alloc::string::String,
    },
}

impl fmt::Display for SerialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { needed, remaining } => write!(
                f,
                "unexpected end of input: needed {needed} more byte(s), {remaining} remaining"
            ),
            Self::InvalidLength {
                declared,
                remaining,
            } => write!(
                f,
                "length prefix exceeds remaining buffer: declared {declared}, remaining {remaining}"
            ),
            Self::VarintOverflow => {
                f.write_str("varint exceeds the maximum byte count for its target width")
            }
            Self::IntegerOutOfRange => {
                f.write_str("decoded integer does not fit in the requested width")
            }
            Self::InvalidBool { byte } => write!(f, "invalid boolean byte: 0x{byte:02x}"),
            Self::InvalidUtf8 => f.write_str("length-prefixed bytes were not valid UTF-8"),
            Self::InvalidTag { kind, tag } => write!(f, "invalid {kind} tag: 0x{tag:02x}"),
            Self::UnknownVariant { kind, index } => {
                write!(f, "unknown {kind} variant index: {index}")
            }
            Self::TrailingBytes { remaining } => {
                write!(
                    f,
                    "trailing input after strict decode: {remaining} byte(s) unread"
                )
            }
            #[cfg(feature = "std")]
            Self::Io { kind, message } => write!(f, "I/O error ({kind:?}): {message}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SerialError {}

/// Convenience alias for `Result<T, SerialError>`.
///
/// Used throughout the codec so the trait surface stays terse. Crates that
/// implement `Serialize` / `Deserialize` for their own types are encouraged to
/// use it as well; nothing in the public API requires it.
///
/// # Examples
///
/// ```
/// use pack_io::Result;
///
/// fn parse_header(_bytes: &[u8]) -> Result<u32> {
///     Ok(0)
/// }
/// ```
pub type Result<T> = core::result::Result<T, SerialError>;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;

    #[test]
    fn display_unexpected_eof_reports_counts() {
        let err = SerialError::UnexpectedEof {
            needed: 4,
            remaining: 1,
        };
        let msg = err.to_string();
        assert!(msg.contains("needed 4"));
        assert!(msg.contains("1 remaining"));
    }

    #[test]
    fn display_invalid_length_reports_declared_and_remaining() {
        let err = SerialError::InvalidLength {
            declared: 1 << 20,
            remaining: 16,
        };
        let msg = err.to_string();
        assert!(msg.contains("1048576"));
        assert!(msg.contains("16"));
    }

    #[test]
    fn display_invalid_bool_is_hex_with_zero_pad() {
        let err = SerialError::InvalidBool { byte: 0x2a };
        assert_eq!(err.to_string(), "invalid boolean byte: 0x2a");
    }

    #[test]
    fn display_invalid_tag_carries_kind_and_byte() {
        let err = SerialError::InvalidTag {
            kind: "Option",
            tag: 0x7f,
        };
        assert!(err.to_string().contains("Option"));
        assert!(err.to_string().contains("0x7f"));
    }

    #[test]
    fn equality_distinguishes_variants() {
        let a = SerialError::VarintOverflow;
        let b = SerialError::VarintOverflow;
        let c = SerialError::IntegerOutOfRange;
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn clone_preserves_variant() {
        let err = SerialError::TrailingBytes { remaining: 8 };
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    #[test]
    fn debug_format_does_not_panic() {
        // We never put untrusted bytes into Debug, so it's safe to print.
        let _ = format!("{:?}", SerialError::InvalidUtf8);
    }
}
