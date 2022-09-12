//! Interface to BME sensors to read physical measurements.

use super::sys::bsec_bme_settings_t;
use super::Input;
use core::{fmt::Debug, time::Duration};

pub trait BmeSensor {
    /// Error type if an operation with the sensor fails.
    type Error: Debug;

    /// Starts a sensor measurement.
    ///
    /// Use `settings` to configure your BME sensor as requested by the BSEC
    /// algorithm.
    ///
    /// Shoud return the duration after which the measurement will be available
    /// or an error.
    fn start_measurement(
        &mut self,
        settings: &BmeSettingsHandle,
    ) -> Result<Duration, Self::Error>;

    /// Read a finished sensor measurement.
    ///
    /// Returns the sensor measurements as a vector with an item for each
    /// physical sensor read.
    ///
    /// To compensate for heat sources near the sensor, add an additional output
    /// to the vector, using the sensor type [`super::InputKind::HeatSource`]
    /// and the desired correction in degrees Celsius.
    fn get_measurement(
        &mut self,
        out: &mut [Input],
    ) -> nb::Result<usize, Self::Error>;
}

/// Handle to a struct with settings for the BME sensor.
///
/// Retrieve the settings from this handle to configure your BME sensor
/// appropriately in [`BmeSensor::start_measurement`] for the measurements
/// requested by the BSEC algorithm.
pub struct BmeSettingsHandle<'a> {
    bme_settings: &'a bsec_bme_settings_t,
}

impl<'a> BmeSettingsHandle<'a> {
    pub(crate) fn new(bme_settings: &'a bsec_bme_settings_t) -> Self {
        Self { bme_settings }
    }

    /// Returns the desired gas sensor heater target temperature.
    pub fn heater_temperature(&self) -> u16 {
        self.bme_settings.heater_temperature
    }

    /// Returns the desired gas sensor heating duration in milliseconds.
    pub fn heating_duration(&self) -> u16 {
        self.bme_settings.heating_duration
    }

    /// Returns whether to run a gas measurement.
    pub fn run_gas(&self) -> bool {
        self.bme_settings.run_gas == 1
    }

    /// Returns the desired oversampling of barometric pressure measurements.
    pub fn pressure_oversampling(&self) -> u8 {
        self.bme_settings.pressure_oversampling
    }

    /// Returns the desired oversampling of temperature measurements.
    pub fn temperature_oversampling(&self) -> u8 {
        self.bme_settings.temperature_oversampling
    }

    /// Returns the desired oversampling of humidity measurements.
    pub fn humidity_oversampling(&self) -> u8 {
        self.bme_settings.humidity_oversampling
    }
}
