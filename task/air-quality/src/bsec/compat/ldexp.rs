#[cfg_attr(all(test, assert_no_panic), no_panic::no_panic)]
#[no_mangle]
pub extern "C" fn ldexp(x: f64, n: i32) -> f64 {
    super::scalbn(x, n)
}
