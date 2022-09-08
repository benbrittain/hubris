// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use minimq::embedded_time::fraction::Fraction;

pub struct ClockLayer {}

impl minimq::embedded_time::Clock for ClockLayer {
    type T = u32;

    const SCALING_FACTOR: Fraction = Fraction::new(1, 64_000);

    fn try_now(
        &self,
    ) -> Result<
        minimq::embedded_time::Instant<Self>,
        minimq::embedded_time::clock::Error,
    > {
        let t = userlib::sys_get_timer().now as u32;
        Ok(minimq::embedded_time::Instant::<ClockLayer>::new(t))
    }
}
