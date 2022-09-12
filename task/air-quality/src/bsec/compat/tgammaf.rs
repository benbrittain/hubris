use super::tgamma;

#[cfg_attr(all(test, assert_no_panic), no_panic::no_panic)]
#[no_mangle]
pub extern "C" fn tgammaf(x: f32) -> f32 {
    tgamma(x as f64) as f32
}
