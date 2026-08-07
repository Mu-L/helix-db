//! Exact mathematical normalization for indexed numeric property values.
//!
//! Integer inputs never pass through floating point. Finite values retain an
//! odd significand and a binary exponent, making cross-variant equality exact.

use core::cmp::Ordering;

use super::property_value::PropertyValue;

/// Exact finite magnitude represented as `odd_significand * 2^exponent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CanonicalFinite {
    pub(crate) exponent: i16,
    pub(crate) odd_significand: u64,
    pub(crate) floor_log2: i16,
    pub(crate) normalized_significand: u64,
}

impl CanonicalFinite {
    fn magnitude_cmp(self, other: Self) -> Ordering {
        self.floor_log2.cmp(&other.floor_log2).then_with(|| {
            self.normalized_significand
                .cmp(&other.normalized_significand)
        })
    }
}

/// Exact non-NaN numeric value shared by equality, predicates, and range keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CanonicalNumber {
    NegativeInfinity,
    NegativeFinite(CanonicalFinite),
    Zero,
    PositiveFinite(CanonicalFinite),
    PositiveInfinity,
}

impl CanonicalNumber {
    pub(crate) fn from_i64(value: i64) -> Self {
        if value == 0 {
            return Self::Zero;
        }
        Self::finite(value.is_negative(), value.unsigned_abs(), 0)
    }

    pub(crate) fn from_f64(value: f64) -> Option<Self> {
        let bits = value.to_bits();
        let negative = bits >> 63 != 0;
        let exponent_bits = ((bits >> 52) & 0x7FF) as i16;
        let fraction = bits & ((1_u64 << 52) - 1);
        if exponent_bits == 0x7FF {
            return (fraction == 0).then_some(if negative {
                Self::NegativeInfinity
            } else {
                Self::PositiveInfinity
            });
        }
        if exponent_bits == 0 && fraction == 0 {
            return Some(Self::Zero);
        }
        let (significand, exponent) = if exponent_bits == 0 {
            (fraction, -1074)
        } else {
            ((1_u64 << 52) | fraction, exponent_bits - 1023 - 52)
        };
        Some(Self::finite(negative, significand, exponent))
    }

    pub(crate) fn from_f32(value: f32) -> Option<Self> {
        let bits = value.to_bits();
        let negative = bits >> 31 != 0;
        let exponent_bits = ((bits >> 23) & 0xFF) as i16;
        let fraction = u64::from(bits & ((1_u32 << 23) - 1));
        if exponent_bits == 0xFF {
            return (fraction == 0).then_some(if negative {
                Self::NegativeInfinity
            } else {
                Self::PositiveInfinity
            });
        }
        if exponent_bits == 0 && fraction == 0 {
            return Some(Self::Zero);
        }
        let (significand, exponent) = if exponent_bits == 0 {
            (fraction, -149)
        } else {
            ((1_u64 << 23) | fraction, exponent_bits - 127 - 23)
        };
        Some(Self::finite(negative, significand, exponent))
    }

    /// Returns `None` for non-numeric values and for NaN.
    pub(crate) fn from_property(value: &PropertyValue) -> Option<Self> {
        match value {
            PropertyValue::I64(value) => Some(Self::from_i64(*value)),
            PropertyValue::F64(value) => Self::from_f64(*value),
            PropertyValue::F32(value) => Self::from_f32(*value as f32),
            PropertyValue::Null
            | PropertyValue::Bool(_)
            | PropertyValue::DateTime(_)
            | PropertyValue::String(_)
            | PropertyValue::Bytes(_)
            | PropertyValue::I64Array(_)
            | PropertyValue::F64Array(_)
            | PropertyValue::F32Array(_)
            | PropertyValue::StringArray(_)
            | PropertyValue::Array(_)
            | PropertyValue::Object(_) => None,
        }
    }

    fn finite(negative: bool, significand: u64, exponent: i16) -> Self {
        debug_assert_ne!(significand, 0);
        let trailing = significand.trailing_zeros() as i16;
        let odd_significand = significand >> trailing;
        let exponent = exponent + trailing;
        let floor_log2 = exponent + (u64::BITS - 1 - odd_significand.leading_zeros()) as i16;
        let finite = CanonicalFinite {
            exponent,
            odd_significand,
            floor_log2,
            normalized_significand: odd_significand << odd_significand.leading_zeros(),
        };
        if negative {
            Self::NegativeFinite(finite)
        } else {
            Self::PositiveFinite(finite)
        }
    }
}

impl Ord for CanonicalNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        use CanonicalNumber::{
            NegativeFinite, NegativeInfinity, PositiveFinite, PositiveInfinity, Zero,
        };
        match (*self, *other) {
            (NegativeInfinity, NegativeInfinity)
            | (Zero, Zero)
            | (PositiveInfinity, PositiveInfinity) => Ordering::Equal,
            (NegativeInfinity, _) | (_, PositiveInfinity) => Ordering::Less,
            (_, NegativeInfinity) | (PositiveInfinity, _) => Ordering::Greater,
            (NegativeFinite(left), NegativeFinite(right)) => right.magnitude_cmp(left),
            (PositiveFinite(left), PositiveFinite(right)) => left.magnitude_cmp(right),
            (NegativeFinite(_), _) | (Zero, PositiveFinite(_)) => Ordering::Less,
            (_, NegativeFinite(_)) | (PositiveFinite(_), Zero) => Ordering::Greater,
        }
    }
}

impl PartialOrd for CanonicalNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_cross_numeric_boundary_does_not_round_through_f64() {
        let exact = CanonicalNumber::from_i64(9_007_199_254_740_992);
        let next = CanonicalNumber::from_i64(9_007_199_254_740_993);
        let float = CanonicalNumber::from_f64(9_007_199_254_740_992.0).unwrap();

        assert_eq!(exact, float);
        assert_ne!(next, float);
        assert!(next > float);
    }

    #[test]
    fn finite_and_infinite_order_covers_every_numeric_class() {
        let ordered = [
            CanonicalNumber::from_f64(f64::NEG_INFINITY).unwrap(),
            CanonicalNumber::from_i64(i64::MIN),
            CanonicalNumber::from_f64(-f64::MIN_POSITIVE).unwrap(),
            CanonicalNumber::from_f64(-f64::from_bits(1)).unwrap(),
            CanonicalNumber::from_f64(-0.0).unwrap(),
            CanonicalNumber::from_f64(f64::from_bits(1)).unwrap(),
            CanonicalNumber::from_f64(f64::MIN_POSITIVE).unwrap(),
            CanonicalNumber::from_i64(i64::MAX),
            CanonicalNumber::from_f64(f64::INFINITY).unwrap(),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            CanonicalNumber::from_f32(-0.0),
            CanonicalNumber::from_f64(0.0)
        );
        assert!(CanonicalNumber::from_f64(f64::NAN).is_none());
        assert!(CanonicalNumber::from_f32(f32::NAN).is_none());
    }
}
