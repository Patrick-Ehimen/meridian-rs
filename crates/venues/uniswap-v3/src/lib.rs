use alloy::primitives::{U256, U512, uint};

// Uniswap v3 sqrtPriceX96 primitives.
/// Number of binary fractional bits in a Q64.96 fixed-point value.
pub const SQRT_PRICE_SHIFT: u32 = 96;

/// The Q64.96 scaling factor: 2^96. Multiplying a real number by this
/// value and truncating yields the on-chain `sqrtPriceX96` representation.
pub const Q96: u128 = 1u128 << SQRT_PRICE_SHIFT;
/// Minimum `sqrtPriceX96` a Uniswap V3 pool permits, corresponding to
/// tick = -887272. Below this, the pool refuses swaps.
/// Source: Uniswap v3-core, `contracts/libraries/TickMath.sol::MIN_SQRT_RATIO`.
pub const MIN_SQRT_RATIO: U256 = uint!(4295128739_U256);

/// Maximum `sqrtPriceX96` a Uniswap V3 pool permits, corresponding to
/// tick = 887272. Above this, the pool refuses swaps.
/// Source: Uniswap v3-core, `contracts/libraries/TickMath.sol::MAX_SQRT_RATIO`.
///
/// This value has 49 decimal digits and exceeds `u128::MAX` by ~10 decimal
/// digits. Any Rust code that stores a real-world `sqrtPriceX96` in a
/// `u128` is a latent overflow bug on high-price pools. See Q1 sub-part 2.
pub const MAX_SQRT_RATIO: U256 =
    uint!(1461446703485210103287273052203988822378723970342_U256);

/// Number of binary fractional bits after squaring a `sqrtPriceX96`.
/// Squaring doubles Q64.96's fractional bits from 96 to 192, giving
/// Q128.192 fixed point for the raw squared price.
pub const PRICE_SHIFT: u32 = 2 * SQRT_PRICE_SHIFT;

/// The Q128.192 scaling factor: 2^192. To recover the real price ratio
/// from `sqrtPriceX96 * sqrtPriceX96`, divide by this value (or shift
/// right by `PRICE_SHIFT`).
///
/// 2^192 needs bit 192 set, which fits in `U256`'s 256 bits.
pub const Q192: U256 =
    uint!(6277101735386680763835789423207666416102355444464034512896_U256);


#[cfg(test)]
mod tests {
    use super::*;

    // Q96 must equal 2^96 exactly. If this ever fails, the constant is wrong.
    #[test]
    fn q96_matches_shift() {
        let computed = 2u128.pow(SQRT_PRICE_SHIFT);
        println!("SQRT_PRICE_SHIFT = {}", SQRT_PRICE_SHIFT);
        println!("Q96              = {}", Q96);
        println!("2^SHIFT          = {}", computed);
        assert_eq!(Q96, computed);
    }

    /// 96 binary fractional bits carries ~28.9 decimal digits of precision.
    /// See docs/notes/questions.md, Q1 sub-part 1.
    #[test]
    fn fractional_precision_is_at_least_28_decimal_digits() {
        // 10^28 < 2^96 < 10^29, so the resolution 1/2^96 is finer than
        // 10^-28 and coarser than 10^-29.
        let lower = 10u128.pow(28);
        let upper_proxy = u128::MAX / 10;
        println!("lower bound (10^28) = {}", lower);
        println!("Q96                 = {}", Q96);
        println!("upper proxy (MAX/10)= {}", upper_proxy);
        println!("Q96 > 10^28 ? {}", Q96 > lower);
        println!("Q96 < MAX/10 ? {}", Q96 < upper_proxy);
        assert!(lower < Q96);
        assert!(Q96 < upper_proxy);
        // Cannot assert Q96 < 10^29 directly since 10^29 overflows u128,
        // but log10(2^96) = 28.898..., so 10^28 < Q96 < 10^29 holds.
    }

    // Locks Q96 to Uniswap's documented sqrtPriceX96-for-price-1.0 value.
    // Catches any off-by-one in SQRT_PRICE_SHIFT.
    #[test]
    fn q96_matches_uniswap_reference_value() {
        // 2^96 = 79_228_162_514_264_337_593_543_950_336
        // This is also the encoded sqrtPriceX96 for a real price of 1.0
        // (matched decimals), per Uniswap V3 whitepaper section 6.
        let reference = 79_228_162_514_264_337_593_543_950_336u128;
        println!("Q96       = {}", Q96);
        println!("reference = {}", reference);
        println!("delta     = {}", Q96.abs_diff(reference));
        assert_eq!(Q96, reference);
    }

    // Turns "~28.9 decimal digits" into a concrete assertion on the printed
    // form. Fails if SQRT_PRICE_SHIFT changes in either direction.
    #[test]
    fn q96_prints_as_29_decimal_digits() {
        let s = Q96.to_string();
        println!("Q96 as string = {}", s);
        println!("length        = {}", s.len());
        assert_eq!(s.len(), 29);
    }

    // Documents the mechanics: shift left by 96, shift right by 96, recover
    // the original. Catches direction typos and value-loss bugs for inputs
    // that fit inside the u128 headroom (128 - 96 = 32 top bits available).
    #[test]
    fn shift_round_trips_small_integers() {
        for v in [0u128, 1, 2, 1_000, u32::MAX as u128] {
            let shifted = v << SQRT_PRICE_SHIFT;
            let recovered = shifted >> SQRT_PRICE_SHIFT;
            println!(
                "v = {:>10}   shifted = {:>40}   recovered = {:>10}",
                v, shifted, recovered
            );
            assert_eq!(recovered, v);
        }
    }

    // The whole point of sub-part 2: proves in code that the real maximum
    // sqrtPriceX96 does not fit in a u128, so any adapter storing it in
    // one is a latent overflow bug.
    #[test]
    fn max_sqrt_ratio_exceeds_u128_max() {
        let u128_max = U256::from(u128::MAX);
        println!("MAX_SQRT_RATIO = {}", MAX_SQRT_RATIO);
        println!("u128::MAX      = {}", u128_max);
        println!("MAX > u128::MAX ? {}", MAX_SQRT_RATIO > u128_max);
        assert!(MAX_SQRT_RATIO > u128_max);
    }

    // Contrast to the test above: the minimum sqrtPriceX96 comfortably fits
    // in a u128 (it is only ~4.3 * 10^9). Only the maximum overflows.
    // Prevents the mistake of "well the min is fine, so u128 must be OK".
    #[test]
    fn min_sqrt_ratio_fits_in_u128() {
        let u128_max = U256::from(u128::MAX);
        println!("MIN_SQRT_RATIO = {}", MIN_SQRT_RATIO);
        println!("u128::MAX      = {}", u128_max);
        assert!(MIN_SQRT_RATIO < u128_max);
    }

    // The bounds must be ordered and non-empty. Guards against copy-paste
    // swap of the two constants.
    #[test]
    fn bounds_are_ordered() {
        println!("MIN_SQRT_RATIO = {}", MIN_SQRT_RATIO);
        println!("MAX_SQRT_RATIO = {}", MAX_SQRT_RATIO);
        assert!(MIN_SQRT_RATIO < MAX_SQRT_RATIO);
    }

    // The maximum sits at ~2^160, which needs 49 decimal digits to print.
    // Catches accidental edits to the MAX literal.
    #[test]
    fn max_sqrt_ratio_prints_as_49_decimal_digits() {
        let s = MAX_SQRT_RATIO.to_string();
        println!("MAX_SQRT_RATIO as string = {}", s);
        println!("length                   = {}", s.len());
        assert_eq!(s.len(), 49);
    }

    // Q192 must equal 2^192 exactly. If PRICE_SHIFT or the literal drifts,
    // this fails. Same shape as `q96_matches_shift`, one level up.
    #[test]
    fn q192_matches_double_shift() {
        let computed = U256::from(1u8) << PRICE_SHIFT;
        println!("PRICE_SHIFT = {}", PRICE_SHIFT);
        println!("Q192        = {}", Q192);
        println!("1 << SHIFT  = {}", computed);
        assert_eq!(Q192, computed);
    }

    // Anchors the squaring relationship: (Q96)^2 == Q192. Documents in code
    // that squaring sqrtPriceX96 lands in the Q128.192 fixed-point space.
    #[test]
    fn q192_equals_q96_squared() {
        let q96_as_u256 = U256::from(Q96);
        let squared = q96_as_u256 * q96_as_u256;
        println!("Q96^2 = {}", squared);
        println!("Q192  = {}", Q192);
        assert_eq!(squared, Q192);
    }

    // The sub-part 3 headline finding: squaring MAX_SQRT_RATIO does not
    // fit in U256. Any adapter that squares sqrtPriceX96 in U256 without
    // widening to U512 first has a latent overflow bug on high-price pools.
    #[test]
    fn squaring_max_sqrt_ratio_overflows_u256() {
        let overflows = MAX_SQRT_RATIO.checked_mul(MAX_SQRT_RATIO).is_none();
        println!("MAX_SQRT_RATIO            = {}", MAX_SQRT_RATIO);
        println!("MAX * MAX overflows U256? {}", overflows);
        assert!(overflows);
    }

    // The same product does fit in U512. Confirms the widening actually
    // buys the headroom we need.
    #[test]
    fn squaring_max_sqrt_ratio_fits_in_u512() {
        let widened = U512::from(MAX_SQRT_RATIO);
        let squared = widened * widened;
        println!("MAX_SQRT_RATIO as U512 = {}", widened);
        println!("MAX^2 in U512          = {}", squared);
        // Non-zero and, since neither operand was zero, must exceed either input.
        assert!(squared > widened);
    }

    // Post-squaring fractional precision is ~57.8 decimal digits.
    // log10(2^192) = 57.796..., so 10^57 < Q192 < 10^58.
    #[test]
    fn q192_sits_between_10_to_the_57_and_10_to_the_58() {
        // 10^57 (57 zeros)
        let lower = uint!(1000000000000000000000000000000000000000000000000000000000_U256);
        // 10^58 (58 zeros)
        let upper = uint!(10000000000000000000000000000000000000000000000000000000000_U256);
        println!("lower (10^57) = {}", lower);
        println!("Q192          = {}", Q192);
        println!("upper (10^58) = {}", upper);
        println!("Q192 > 10^57 ? {}", Q192 > lower);
        println!("Q192 < 10^58 ? {}", Q192 < upper);
        assert!(Q192 > lower);
        assert!(Q192 < upper);
    }

}
