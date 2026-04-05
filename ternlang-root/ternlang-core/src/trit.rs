use std::fmt;
use std::ops::{Add, Mul, Neg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trit {
    Reject = -1,   // logical -1 — conflict, negation
    Tend   =  0,   // logical  0 — hold, uncertainty
    Affirm =  1,   // logical +1 — truth, confirmation
}

impl From<i8> for Trit {
    fn from(val: i8) -> Self {
        match val {
            -1 => Trit::Reject,
            0 => Trit::Tend,
            1 => Trit::Affirm,
            _ => panic!("Invalid trit value: {}", val),
        }
    }
}

impl fmt::Display for Trit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trit::Reject => write!(f, "reject"),
            Trit::Tend   => write!(f, "tend"),
            Trit::Affirm => write!(f, "affirm"),
        }
    }
}

impl Neg for Trit {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Trit::Reject => Trit::Affirm,
            Trit::Tend => Trit::Tend,
            Trit::Affirm => Trit::Reject,
        }
    }
}

impl Add for Trit {
    type Output = (Self, Self); // (Sum, Carry)

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Trit::Reject, Trit::Reject) => (Trit::Affirm, Trit::Reject),
            (Trit::Reject, Trit::Tend) => (Trit::Reject, Trit::Tend),
            (Trit::Reject, Trit::Affirm) => (Trit::Tend, Trit::Tend),
            (Trit::Tend, Trit::Reject) => (Trit::Reject, Trit::Tend),
            (Trit::Tend, Trit::Tend) => (Trit::Tend, Trit::Tend),
            (Trit::Tend, Trit::Affirm) => (Trit::Affirm, Trit::Tend),
            (Trit::Affirm, Trit::Reject) => (Trit::Tend, Trit::Tend),
            (Trit::Affirm, Trit::Tend) => (Trit::Affirm, Trit::Tend),
            (Trit::Affirm, Trit::Affirm) => (Trit::Reject, Trit::Affirm),
        }
    }
}

impl Mul for Trit {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Trit::Tend, _) | (_, Trit::Tend) => Trit::Tend,
            (Trit::Affirm, Trit::Affirm) | (Trit::Reject, Trit::Reject) => Trit::Affirm,
            (Trit::Affirm, Trit::Reject) | (Trit::Reject, Trit::Affirm) => Trit::Reject,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negation() {
        assert_eq!(-Trit::Reject, Trit::Affirm);
        assert_eq!(-Trit::Tend, Trit::Tend);
        assert_eq!(-Trit::Affirm, Trit::Reject);
    }

    #[test]
    fn test_addition() {
        assert_eq!(Trit::Reject + Trit::Reject, (Trit::Affirm, Trit::Reject));
        assert_eq!(Trit::Reject + Trit::Tend, (Trit::Reject, Trit::Tend));
        assert_eq!(Trit::Reject + Trit::Affirm, (Trit::Tend, Trit::Tend));
        assert_eq!(Trit::Tend + Trit::Reject, (Trit::Reject, Trit::Tend));
        assert_eq!(Trit::Tend + Trit::Tend, (Trit::Tend, Trit::Tend));
        assert_eq!(Trit::Tend + Trit::Affirm, (Trit::Affirm, Trit::Tend));
        assert_eq!(Trit::Affirm + Trit::Reject, (Trit::Tend, Trit::Tend));
        assert_eq!(Trit::Affirm + Trit::Tend, (Trit::Affirm, Trit::Tend));
        assert_eq!(Trit::Affirm + Trit::Affirm, (Trit::Reject, Trit::Affirm));
    }

    #[test]
    fn test_multiplication() {
        assert_eq!(Trit::Reject * Trit::Reject, Trit::Affirm);
        assert_eq!(Trit::Reject * Trit::Tend, Trit::Tend);
        assert_eq!(Trit::Reject * Trit::Affirm, Trit::Reject);
        assert_eq!(Trit::Tend * Trit::Reject, Trit::Tend);
        assert_eq!(Trit::Tend * Trit::Tend, Trit::Tend);
        assert_eq!(Trit::Tend * Trit::Affirm, Trit::Tend);
        assert_eq!(Trit::Affirm * Trit::Reject, Trit::Reject);
        assert_eq!(Trit::Affirm * Trit::Tend, Trit::Tend);
        assert_eq!(Trit::Affirm * Trit::Affirm, Trit::Affirm);
    }
}
