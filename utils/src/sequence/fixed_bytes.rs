use crate::{Array, Span};
use bytes::{Buf, BufMut};
use commonware_codec::{Error as CodecError, FixedArray, FixedSize, Read, ReadExt, Write};
use commonware_formatting::Hex;
use core::{
    cmp::{Ord, PartialOrd},
    fmt::{Debug, Display},
    hash::Hash,
    ops::Deref,
};
use thiserror::Error;
use zeroize::Zeroize;

/// Errors returned by `Bytes` functions.
#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("invalid length")]
    InvalidLength,
}

/// An `Array` implementation for fixed-length byte arrays.
#[derive(Clone, Eq, PartialEq, Hash, Debug, FixedArray)]
#[fixed_array(infallible, bytes([u8; N]))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[repr(transparent)]
pub struct FixedBytes<const N: usize>([u8; N]);

/// Orders lexicographically, like `[u8; N]`, comparing eight bytes at a time as big-endian
/// words: inlined, this is about twice as fast in sorts and binary searches as the `memcmp`
/// call the array's ordering makes.
impl<const N: usize> Ord for FixedBytes<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let (a, b) = (&self.0, &other.0);
        let mut i = 0;
        while i + 8 <= N {
            let x = u64::from_be_bytes(a[i..i + 8].try_into().expect("eight bytes"));
            let y = u64::from_be_bytes(b[i..i + 8].try_into().expect("eight bytes"));
            if x != y {
                return x.cmp(&y);
            }
            i += 8;
        }
        a[i..].cmp(&b[i..])
    }
}

impl<const N: usize> PartialOrd for FixedBytes<N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> FixedBytes<N> {
    /// Creates a new `FixedBytes` instance from an array of length `N`.
    pub const fn new(value: [u8; N]) -> Self {
        Self(value)
    }
}

impl<const N: usize> Write for FixedBytes<N> {
    fn write(&self, buf: &mut impl BufMut) {
        self.0.write(buf);
    }
}

impl<const N: usize> Read for FixedBytes<N> {
    type Cfg = ();

    fn read_cfg(buf: &mut impl Buf, _: &()) -> Result<Self, CodecError> {
        Ok(Self(<[u8; N]>::read(buf)?))
    }
}

impl<const N: usize> FixedSize for FixedBytes<N> {
    const SIZE: usize = N;
}

impl<const N: usize> Span for FixedBytes<N> {}

impl<const N: usize> Array for FixedBytes<N> {}

impl<const N: usize> AsRef<[u8]> for FixedBytes<N> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<const N: usize> Deref for FixedBytes<N> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl<const N: usize> Display for FixedBytes<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", Hex(&self.0))
    }
}

impl<const N: usize> Zeroize for FixedBytes<N> {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed_bytes;
    use bytes::{Buf, BytesMut};
    use commonware_codec::{DecodeExt, Encode};

    #[test]
    fn test_order_matches_byte_arrays() {
        fn check<const N: usize>() {
            let mut state = 0x9e37_79b9_7f4a_7c15u64;
            let mut next = || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            for _ in 0..2000 {
                let mut a = [0u8; N];
                let mut b = [0u8; N];
                for i in 0..N {
                    a[i] = (next() % 3) as u8;
                    b[i] = (next() % 3) as u8;
                }
                assert_eq!(FixedBytes::new(a).cmp(&FixedBytes::new(b)), a.cmp(&b));
                assert_eq!(FixedBytes::new(a).cmp(&FixedBytes::new(a)), a.cmp(&a));
            }
        }
        check::<0>();
        check::<3>();
        check::<8>();
        check::<20>();
        check::<32>();
        check::<33>();
    }

    #[test]
    fn test_codec() {
        let original = FixedBytes::new([1, 2, 3, 4]);
        let encoded = original.encode();
        assert_eq!(encoded.len(), original.len());
        let decoded = FixedBytes::decode(encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_bytes_creation_and_conversion() {
        let value = [1, 2, 3, 4];
        let bytes = FixedBytes::new(value);
        assert_eq!(bytes.as_ref(), &value);

        let bytes_into = value.into();
        assert_eq!(bytes, bytes_into);

        let slice = [1, 2, 3, 4];
        let bytes_from_slice = FixedBytes::decode(slice.as_ref()).unwrap();
        assert_eq!(bytes_from_slice, bytes);

        let vec = vec![1, 2, 3, 4];
        let bytes_from_vec = FixedBytes::decode(vec.as_ref()).unwrap();
        assert_eq!(bytes_from_vec, bytes);

        // Test with incorrect length
        let slice_too_short = [1, 2, 3];
        assert!(matches!(
            FixedBytes::<4>::decode(slice_too_short.as_ref()),
            Err(CodecError::EndOfBuffer)
        ));

        let vec_too_long = vec![1, 2, 3, 4, 5];
        assert!(matches!(
            FixedBytes::<4>::decode(vec_too_long.as_ref()),
            Err(CodecError::ExtraData(1))
        ));
    }

    #[test]
    fn test_read() {
        let mut buf = BytesMut::from(&[1, 2, 3, 4][..]);
        let bytes = FixedBytes::<4>::read(&mut buf).unwrap();
        assert_eq!(bytes.as_ref(), &[1, 2, 3, 4]);
        assert_eq!(buf.remaining(), 0);

        let mut buf = BytesMut::from(&[1, 2, 3][..]);
        let result = FixedBytes::<4>::read(&mut buf);
        assert!(matches!(result, Err(CodecError::EndOfBuffer)));

        let mut buf = BytesMut::from(&[1, 2, 3, 4, 5][..]);
        let bytes = FixedBytes::<4>::read(&mut buf).unwrap();
        assert_eq!(bytes.as_ref(), &[1, 2, 3, 4]);
        assert_eq!(buf.remaining(), 1);
        assert_eq!(buf[0], 5);
    }

    #[test]
    fn test_display() {
        let bytes = fixed_bytes!("0x01020304");
        assert_eq!(format!("{bytes}"), "01020304");
    }

    #[test]
    fn test_ord_and_eq() {
        let a = FixedBytes::new([1, 2, 3, 4]);
        let b = FixedBytes::new([1, 2, 3, 5]);
        assert!(a < b);
        assert_ne!(a, b);

        let c = FixedBytes::new([1, 2, 3, 4]);
        assert_eq!(a, c);
    }

    #[cfg(feature = "arbitrary")]
    mod conformance {
        use super::*;
        use commonware_codec::conformance::CodecConformance;

        commonware_conformance::conformance_tests! {
            CodecConformance<FixedBytes<16>>,
        }
    }
}
