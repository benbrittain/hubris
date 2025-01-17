use super::expm1f;
use super::k_expo2f;

#[cfg_attr(all(test, assert_no_panic), no_panic::no_panic)]
#[no_mangle]
pub extern "C" fn sinhf(x: f32) -> f32 {
    let mut h = 0.5f32;
    let mut ix = x.to_bits();
    if (ix >> 31) != 0 {
        h = -h;
    }
    /* |x| */
    ix &= 0x7fffffff;
    let absx = f32::from_bits(ix);
    let w = ix;

    /* |x| < log(FLT_MAX) */
    if w < 0x42b17217 {
        let t = expm1f(absx);
        if w < 0x3f800000 {
            if w < (0x3f800000 - (12 << 23)) {
                return x;
            }
            return h * (2. * t - t * t / (t + 1.));
        }
        return h * (t + t / (t + 1.));
    }

    /* |x| > logf(FLT_MAX) or nan */
    2. * h * k_expo2f(absx)
}
