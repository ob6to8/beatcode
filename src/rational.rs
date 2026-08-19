//! Exact rational beats (SPEC §3).
//!
//! Invariants: always reduced by `gcd(|num|, |den|)`, `den > 0` (sign
//! carried by `num`), `0/anything` normalizes to `0/1`. `i64` width with
//! checked arithmetic surfacing clean errors (SPEC §3 "Width").

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rational {
    num: i64,
    den: i64,
}

/// Constructor/arithmetic failures (SPEC §3, §5.10 #12).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RatError {
    ZeroDen,
    Overflow,
}

impl RatError {
    pub fn msg(self) -> &'static str {
        match self {
            RatError::ZeroDen => "rational division by zero",
            RatError::Overflow => "rational overflow",
        }
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    // Non-negative gcd; gcd(0, d) = d, so 0/x normalizes to 0/1.
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn make(n: i128, d: i128) -> Result<Rational, RatError> {
    if d == 0 {
        return Err(RatError::ZeroDen);
    }
    let g = gcd(n.unsigned_abs(), d.unsigned_abs()) as i128;
    let (mut n, mut d) = (n / g, d / g);
    if d < 0 {
        n = -n;
        d = -d;
    }
    match (i64::try_from(n), i64::try_from(d)) {
        (Ok(num), Ok(den)) => Ok(Rational { num, den }),
        _ => Err(RatError::Overflow),
    }
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Rational, RatError> {
        make(i128::from(num), i128::from(den))
    }

    pub fn num(self) -> i64 {
        self.num
    }

    pub fn den(self) -> i64 {
        self.den
    }

    // SPEC §3 names the operations add/mul/divr; they are fallible
    // (checked overflow), so the std operator traits don't fit.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, o: Rational) -> Result<Rational, RatError> {
        let (a, b) = (i128::from(self.num), i128::from(self.den));
        let (c, d) = (i128::from(o.num), i128::from(o.den));
        make(a * d + c * b, b * d)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, o: Rational) -> Result<Rational, RatError> {
        let (a, b) = (i128::from(self.num), i128::from(self.den));
        let (c, d) = (i128::from(o.num), i128::from(o.den));
        make(a * c, b * d)
    }

    /// Rational division (SPEC §3 `divr`).
    pub fn divr(self, o: Rational) -> Result<Rational, RatError> {
        let (a, b) = (i128::from(self.num), i128::from(self.den));
        let (c, d) = (i128::from(o.num), i128::from(o.den));
        make(a * d, b * c)
    }

    /// The only rational→float edge; single correctly-rounded division.
    pub fn to_f(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Floor division toward −∞: `floor_i(-1/4) = -1`.
    pub fn floor_i(self) -> i64 {
        self.num.div_euclid(self.den) // den > 0, so div_euclid floors
    }

    pub fn is_int(self) -> bool {
        self.den == 1
    }

    pub fn to_s(self) -> String {
        format!("{}/{}", self.num, self.den)
    }
}
