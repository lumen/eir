use std::str::FromStr;
use std::fmt::{Display, Formatter};
use std::convert::TryInto;
use std::cmp::Ordering;
use std::ops::{Neg, Mul, Not, Div, Rem, Add, Sub};

pub use num_traits::{ToPrimitive, FromPrimitive};
use num_bigint::{BigInt, ParseBigIntError};

#[derive(Debug, Clone)]
pub enum Integer {
    Small(i64),
    Big(BigInt),
}

impl Integer {

    pub fn to_float(&self) -> f64 {
        match self {
            Integer::Small(int) => *int as f64,
            Integer::Big(int) => crate::bigint_to_double(int),
        }
    }

    pub fn to_bigint(self) -> BigInt {
        match self {
            Integer::Small(int) => int.into(),
            Integer::Big(int) => int,
        }
    }

    pub fn shrink(self) -> Self {
        match self {
            Integer::Small(int) => Integer::Small(int),
            Integer::Big(int) => {
                if let Some(small) = int.to_i64() {
                    Integer::Small(small)
                } else {
                    Integer::Big(int)
                }
            },
        }
    }

    pub fn from_string_radix(string: &str, radix: u32) -> Option<Integer> {
        if let Ok(i) = i64::from_str_radix(string, radix) {
            return Some(Integer::Small(i));
        }
        let bi = BigInt::parse_bytes(string.as_bytes(), radix)?;
        Some(Integer::Big(bi))
    }

}

impl Display for Integer {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Integer::Small(int) => int.fmt(f),
            Integer::Big(int) => int.fmt(f),
        }
    }
}

impl FromStr for Integer {
    type Err = ParseBigIntError;
    fn from_str(s: &str) -> Result<Self, ParseBigIntError> {
        match s.parse::<i64>() {
            Ok(int) => Ok(Integer::Small(int)),
            Err(_) => {
                match s.parse::<BigInt>() {
                    Ok(int) => Ok(Integer::Big(int)),
                    Err(err) => Err(err),
                }
            },
        }
    }
}

impl PartialEq for Integer {
    fn eq(&self, rhs: &Integer) -> bool {
        match (self, rhs) {
            (Integer::Small(lhs), Integer::Small(rhs)) => lhs.eq(rhs),
            (Integer::Small(lhs), Integer::Big(rhs)) => lhs.eq(rhs),
            (Integer::Big(lhs), Integer::Small(rhs)) => lhs.eq(rhs),
            (Integer::Big(lhs), Integer::Big(rhs)) => lhs.eq(rhs),
        }
    }
}
impl Eq for Integer {}

impl PartialOrd for Integer {
    fn partial_cmp(&self, rhs: &Integer) -> Option<Ordering> {
        match (self, rhs) {
            (Integer::Small(lhs), Integer::Small(rhs)) => lhs.partial_cmp(rhs),
            (Integer::Small(lhs), Integer::Big(rhs)) => lhs.partial_cmp(rhs),
            (Integer::Big(lhs), Integer::Small(rhs)) => lhs.partial_cmp(rhs),
            (Integer::Big(lhs), Integer::Big(rhs)) => lhs.partial_cmp(rhs),
        }
    }
}
impl Ord for Integer {
    fn cmp(&self, rhs: &Integer) -> Ordering {
        self.partial_cmp(rhs).unwrap()
    }
}

impl PartialEq<f64> for Integer {
    fn eq(&self, rhs: &f64) -> bool {
        match self {
            Integer::Small(lhs) => (*lhs as f64).eq(rhs),
            Integer::Big(lhs) => crate::bigint_to_double(lhs).eq(rhs),
        }
    }
}
impl PartialEq<Integer> for f64 {
    fn eq(&self, rhs: &Integer) -> bool {
        rhs.eq(self)
    }
}
impl PartialOrd<f64> for Integer {
    fn partial_cmp(&self, rhs: &f64) -> Option<Ordering> {
        match self {
            Integer::Small(lhs) => (*lhs as f64).partial_cmp(rhs),
            Integer::Big(lhs) => crate::bigint_to_double(lhs).partial_cmp(rhs),
        }
    }
}
impl PartialOrd<Integer> for f64 {
    fn partial_cmp(&self, rhs: &Integer) -> Option<Ordering> {
        rhs.partial_cmp(self).map(|v| v.reverse())
    }
}

impl PartialEq<char> for Integer {
    fn eq(&self, rhs: &char) -> bool {
        match self {
            Integer::Small(lhs) => lhs.eq(&(*rhs as i64)),
            Integer::Big(lhs) => lhs.eq(&(*rhs as i64)),
        }
    }
}
impl PartialEq<Integer> for char {
    fn eq(&self, rhs: &Integer) -> bool {
        rhs.eq(self)
    }
}
impl PartialOrd<char> for Integer {
    fn partial_cmp(&self, rhs: &char) -> Option<Ordering> {
        match self {
            Integer::Small(lhs) => lhs.partial_cmp(&(*rhs as i64)),
            Integer::Big(lhs) => lhs.partial_cmp(&(*rhs as i64)),
        }
    }
}
impl PartialOrd<Integer> for char {
    fn partial_cmp(&self, rhs: &Integer) -> Option<Ordering> {
        rhs.partial_cmp(self).map(|v| v.reverse())
    }
}

impl PartialEq<i64> for Integer {
    fn eq(&self, rhs: &i64) -> bool {
        match self {
            Integer::Small(lhs) => lhs.eq(rhs),
            Integer::Big(lhs) => lhs.eq(rhs),
        }
    }
}
impl PartialEq<Integer> for i64 {
    fn eq(&self, rhs: &Integer) -> bool {
        rhs.eq(self)
    }
}
impl PartialOrd<i64> for Integer {
    fn partial_cmp(&self, rhs: &i64) -> Option<Ordering> {
        match self {
            Integer::Small(lhs) => lhs.partial_cmp(rhs),
            Integer::Big(lhs) => lhs.partial_cmp(rhs),
        }
    }
}
impl PartialOrd<Integer> for i64 {
    fn partial_cmp(&self, rhs: &Integer) -> Option<Ordering> {
        rhs.partial_cmp(self).map(|v| v.reverse())
    }
}

impl Mul<i64> for Integer {
    type Output = Integer;
    fn mul(self, rhs: i64) -> Integer {
        match self {
            Integer::Small(lhs) => {
                let mut int: BigInt = lhs.into();
                int = int * rhs;
                Integer::Big(int).shrink()
            },
            Integer::Big(lhs) => Integer::Big(lhs * rhs),
        }
    }
}
impl Mul<&Integer> for Integer {
    type Output = Integer;
    fn mul(self, rhs: &Integer) -> Integer {
        let mut lhs = self.to_bigint();
        match rhs {
            Integer::Small(rhs) => lhs = lhs * rhs,
            Integer::Big(rhs) => lhs *= rhs,
        }
        Integer::Big(lhs).shrink()
    }
}
impl Div<&Integer> for Integer {
    type Output = Integer;
    fn div(self, rhs: &Integer) -> Integer {
        let mut lhs = self.to_bigint();
        match rhs {
            Integer::Small(rhs) => lhs = lhs / rhs,
            Integer::Big(rhs) => lhs /= rhs,
        }
        Integer::Big(lhs).shrink()
    }
}
impl Add<&Integer> for Integer {
    type Output = Integer;
    fn add(self, rhs: &Integer) -> Integer {
        let mut lhs = self.to_bigint();
        match rhs {
            Integer::Small(rhs) => lhs = lhs + rhs,
            Integer::Big(rhs) => lhs += rhs,
        }
        Integer::Big(lhs).shrink()
    }
}
impl Sub<&Integer> for Integer {
    type Output = Integer;
    fn sub(self, rhs: &Integer) -> Integer {
        let mut lhs = self.to_bigint();
        match rhs {
            Integer::Small(rhs) => lhs = lhs - rhs,
            Integer::Big(rhs) => lhs -= rhs,
        }
        Integer::Big(lhs).shrink()
    }
}
impl Rem<&Integer> for Integer {
    type Output = Integer;
    fn rem(self, rhs: &Integer) -> Integer {
        let mut lhs = self.to_bigint();
        match rhs {
            Integer::Small(rhs) => lhs = lhs % rhs,
            Integer::Big(rhs) => lhs %= rhs,
        }
        Integer::Big(lhs).shrink()
    }
}

impl Neg for Integer {
    type Output = Integer;
    fn neg(self) -> Self {
        match self {
            Integer::Small(int) => Integer::Small(-int),
            Integer::Big(int) => Integer::Big(-int),
        }
    }
}
impl Not for Integer {
    type Output = Integer;
    fn not(self) -> Self {
        match self {
            Integer::Small(int) => Integer::Small(!int),
            Integer::Big(int) => Integer::Big(!int),
        }
    }
}

impl ToPrimitive for Integer {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Integer::Small(int) => int.to_i64(),
            Integer::Big(int) => int.to_i64(),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        match self {
            Integer::Small(int) => int.to_u64(),
            Integer::Big(int) => int.to_u64(),
        }
    }
}

impl FromPrimitive for Integer {
    fn from_i64(n: i64) -> Option<Integer> {
        Some(Integer::Small(n))
    }
    fn from_u64(n: u64) -> Option<Integer> {
        if let Ok(int) = n.try_into() {
            Some(Integer::Small(int))
        } else {
            Some(Integer::Big(n.into()))
        }
    }
}

impl From<i64> for Integer {
    fn from(i: i64) -> Integer {
        Integer::from_i64(i).unwrap()
    }
}
impl From<i32> for Integer {
    fn from(i: i32) -> Integer {
        Integer::from_i32(i).unwrap()
    }
}
impl From<usize> for Integer {
    fn from(i: usize) -> Integer {
        Integer::from_usize(i).unwrap()
    }
}
