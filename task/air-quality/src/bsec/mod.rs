use bme::{BmeSensor, BmeSettingsHandle};
use core::borrow::Borrow;
use core::convert::{From, TryFrom, TryInto};
use core::fmt::Debug;
use core::hash::Hash;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use error::{BsecError, ConversionError, Error};
#[cfg(not(feature = "docs-rs"))]
use sys::{
    bsec_bme_settings_t, bsec_do_steps, bsec_get_configuration, bsec_get_state,
    bsec_get_version, bsec_init, bsec_input_t, bsec_library_return_t,
    bsec_output_t, bsec_physical_sensor_t, bsec_reset_output,
    bsec_sensor_configuration_t, bsec_sensor_control, bsec_set_configuration,
    bsec_set_state, bsec_update_subscription, bsec_version_t,
    bsec_virtual_sensor_t, BSEC_MAX_PHYSICAL_SENSOR,
    BSEC_MAX_PROPERTY_BLOB_SIZE, BSEC_MAX_STATE_BLOB_SIZE,
    BSEC_MAX_WORKBUFFER_SIZE,
};

pub mod bme;
pub mod error;

use sys::*;

static BSEC_IN_USE: AtomicBool = AtomicBool::new(false);

/// Handle to encapsulates the *Bosch BSEC* library and related state.
pub struct Bsec<S: BmeSensor> {
    bme: S,
    subscribed: u32,
    ulp_plus_queue: u32,
    next_measurement: i64,
}

fn get_nanos() -> i64 {
    (userlib::sys_get_timer().now * 1000) as i64
}

impl<S: BmeSensor> Bsec<S> {
    /// Initialize the *Bosch BSEC* library and return a handle to interact with
    /// it.
    ///
    /// * `bme`: [`BmeSensor`] implementation to communicate with the BME sensor.
    pub fn init(bme: S) -> Result<Self, Error<S::Error>> {
        if BSEC_IN_USE
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            unsafe {
                bsec_init().into_result()?;
            }
            Ok(Self {
                bme,
                subscribed: 0,
                ulp_plus_queue: 0,
                next_measurement: get_nanos(),
            })
        } else {
            Err(Error::BsecAlreadyInUse)
        }
    }

    /// Change subscription to virtual sensor outputs.
    ///
    /// * `requests`: Configuration of virtual sensors and their sample
    ///   rates to subscribe to.
    ///
    /// Returns a vector describing physical sensor and sampling rates required
    /// as input to the BSEC algorithm.
    pub fn update_subscription(
        &mut self,
        bsec_requested_outputs: &[SubscriptionRequest],
        required_sensor_settings: &mut [RequiredInput],
    ) -> Result<usize, Error<S::Error>> {
        let mut n_required_sensor_settings = BSEC_MAX_PHYSICAL_SENSOR as u8;
        unsafe {
            bsec_update_subscription(
                bsec_requested_outputs.as_ptr()
                    as *const bsec_sensor_configuration_t,
                bsec_requested_outputs.len() as u8,
                required_sensor_settings.as_mut_ptr(),
                &mut n_required_sensor_settings,
            )
            .into_result()?
        }
        for changed in bsec_requested_outputs.iter() {
            match changed.sample_rate {
                SampleRate::Disabled => {
                    self.subscribed &= !(changed.sensor as u32);
                    self.ulp_plus_queue &= !(changed.sensor as u32);
                }
                SampleRate::UlpMeasurementOnDemand => {
                    self.ulp_plus_queue |= changed.sensor as u32;
                }
                _ => {
                    self.subscribed |= changed.sensor as u32;
                }
            }
        }
        Ok(n_required_sensor_settings as usize)
    }

    /// Returns the timestamp when the next measurement has to be triggered.
    pub fn next_measurement(&self) -> i64 {
        self.next_measurement
    }

    /// Trigger the next measurement.
    ///
    /// Returns the duration until the measurement becomes available. Call
    /// [`Self::process_last_measurement`] after the duration has passed.
    pub fn start_next_measurement(
        &mut self,
    ) -> nb::Result<Duration, Error<S::Error>> {
        let mut bme_settings = bsec_bme_settings_t {
            next_call: 0,
            process_data: 0,
            heater_temperature: 0,
            heating_duration: 0,
            run_gas: 0,
            pressure_oversampling: 0,
            temperature_oversampling: 0,
            humidity_oversampling: 0,
            trigger_measurement: 0,
        };
        unsafe {
            bsec_sensor_control(get_nanos(), &mut bme_settings)
                .into_result()
                .map_err(Error::BsecError)?;
        }
        self.next_measurement = bme_settings.next_call;
        if bme_settings.trigger_measurement != 1 {
            return Err(nb::Error::WouldBlock);
        }
        self.bme
            .start_measurement(&BmeSettingsHandle::new(&bme_settings))
            .map_err(Error::BmeSensorError)
            .map_err(nb::Error::Other)
    }

    /// Process the last triggered measurement.
    ///
    /// Call this method after the duration returned from a call to
    /// [`Self::start_next_measurement`] has passed.
    ///
    /// Returns a vector of virtual sensor outputs calculated by the
    /// *Bosch BSEC* library.
    pub fn process_last_measurement(
        &mut self,
        outputs: &mut [Output],
    ) -> nb::Result<usize, Error<S::Error>> {
        let time_stamp = get_nanos();
        let mut inputs: [Input; 8] = [Input {
            sensor_id: 0,
            signal_dimensions: 1,
            signal: 0.0,
            time_stamp,
        }; 8];
        let inputs_num = self
            .bme
            .get_measurement(&mut inputs)
            .map_err(|e| e.map(Error::BmeSensorError))?;

        assert!(
            outputs.len()
                == (self.subscribed | self.ulp_plus_queue).count_ones()
                    as usize
        );

        let mut num_outputs: u8 = outputs
            .len()
            .try_into()
            .or(Err(Error::ArgumentListTooLong))?;
        self.ulp_plus_queue = 0;
        unsafe {
            bsec_do_steps(
                inputs.as_ptr(),
                inputs_num as u8,
                outputs.as_mut_ptr() as *mut bsec_output_t,
                &mut num_outputs,
            )
            .into_result()
            .map_err(Error::BsecError)?;
        }

        Ok(num_outputs as usize)
    }

    /// Get the current raw *Bosch BSEC* state, e.g. to persist it before
    /// shutdown.
    pub fn get_state(
        &self,
        state: &mut [u8; BSEC_MAX_PROPERTY_BLOB_SIZE as usize],
    ) -> Result<usize, Error<S::Error>> {
        let mut work_buffer = [0u8; BSEC_MAX_WORKBUFFER_SIZE as usize];
        let mut state_length = BSEC_MAX_STATE_BLOB_SIZE;
        unsafe {
            bsec_get_state(
                0,
                state.as_mut_ptr(),
                state.len() as u32,
                work_buffer.as_mut_ptr(),
                work_buffer.len() as u32,
                &mut state_length,
            )
            .into_result()?;
        }
        Ok(state_length as usize)
    }

    /// Set the raw *Bosch BSEC* state, e.g. to restore persisted state after
    /// shutdown.
    pub fn set_state(&mut self, state: &[u8]) -> Result<(), Error<S::Error>> {
        let mut work_buffer = [0u8; BSEC_MAX_WORKBUFFER_SIZE as usize];
        unsafe {
            bsec_set_state(
                state.as_ptr(),
                state.len() as u32,
                work_buffer.as_mut_ptr(),
                work_buffer.len() as u32,
            )
            .into_result()?;
        }
        Ok(())
    }

    /// Get the current (raw) *Bosch BSEC* configuration.
    pub fn get_configuration(
        &self,
        serialized_settings: &mut [u8; BSEC_MAX_PROPERTY_BLOB_SIZE as usize],
    ) -> Result<usize, Error<S::Error>> {
        let mut serialized_settings_length = 0u32;
        let mut work_buffer = [0u8; BSEC_MAX_WORKBUFFER_SIZE as usize];
        unsafe {
            bsec_get_configuration(
                0,
                serialized_settings.as_mut_ptr(),
                serialized_settings.len() as u32,
                work_buffer.as_mut_ptr(),
                work_buffer.len() as u32,
                &mut serialized_settings_length,
            )
            .into_result()?;
        }
        Ok(serialized_settings_length as usize)
    }

    /// Set the (raw) *Bosch BSEC* configuration.
    ///
    /// Your copy of the *Bosch BSEC* library should contain several different
    /// configuration files. See the Bosch BSEC documentation for more
    /// information.
    pub fn set_configuration(
        &mut self,
        serialized_settings: &[u8],
    ) -> Result<(), Error<S::Error>> {
        let mut work_buffer = [0u8; BSEC_MAX_WORKBUFFER_SIZE as usize];
        unsafe {
            bsec_set_configuration(
                serialized_settings.as_ptr(),
                serialized_settings.len() as u32,
                work_buffer.as_mut_ptr(),
                work_buffer.len() as u32,
            )
            .into_result()?
        }
        Ok(())
    }

    /// See documentation of `bsec_reset_output` in the *Bosch BSEC*
    /// documentation.
    pub fn reset_output(
        &mut self,
        sensor: OutputKind,
    ) -> Result<(), Error<S::Error>> {
        unsafe {
            bsec_reset_output(bsec_virtual_sensor_t::from(sensor) as u8)
                .into_result()?;
        }
        Ok(())
    }
}

impl<S: BmeSensor> Drop for Bsec<S> {
    fn drop(&mut self) {
        BSEC_IN_USE.store(false, Ordering::Release);
    }
}

/// Return the *Bosch BSEC* version.
///
/// The returned tuple consists of *major*, *minor*, *major bugfix*, and
/// *minor bugfix* version.
pub fn get_version() -> Result<(u8, u8, u8, u8), BsecError> {
    let mut version = bsec_version_t {
        major: 0,
        minor: 0,
        major_bugfix: 0,
        minor_bugfix: 0,
    };
    unsafe {
        bsec_get_version(&mut version).into_result()?;
    }
    Ok((
        version.major,
        version.minor,
        version.major_bugfix,
        version.minor_bugfix,
    ))
}

/// Encapsulates data read from a BME physical sensor.
type Input = bsec_input_t;

/// Single virtual sensor output of the BSEC algorithm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    /// Timestamp (nanoseconds) of the measurement.
    ///
    /// This timestamp is based on the [`Clock`] instance used by [`Bsec`].
    pub timestamp_ns: i64,

    /// Signal value of the virtual sensor.
    pub signal: f64,

    /// Type of virtual sensor.
    pub sensor: OutputKind,

    /// Accuracy of the virtual sensor.
    pub accuracy: Accuracy,
}

impl TryFrom<&bsec_output_t> for Output {
    type Error = ConversionError;
    fn try_from(output: &bsec_output_t) -> Result<Self, ConversionError> {
        Ok(Self {
            timestamp_ns: output.time_stamp,
            signal: output.signal.into(),
            sensor: output.sensor_id.try_into()?,
            accuracy: output.accuracy.try_into()?,
        })
    }
}

/// Sensor accuracy level.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Accuracy {
    Unreliable = 0,
    LowAccuracy = 1,
    MediumAccuracy = 2,
    HighAccuracy = 3,
}

impl TryFrom<u8> for Accuracy {
    type Error = ConversionError;
    fn try_from(accuracy: u8) -> Result<Self, ConversionError> {
        use Accuracy::*;
        match accuracy {
            0 => Ok(Unreliable),
            1 => Ok(LowAccuracy),
            2 => Ok(MediumAccuracy),
            3 => Ok(HighAccuracy),
            _ => Err(ConversionError::InvalidAccuracy(accuracy)),
        }
    }
}

/// Describes a virtual sensor output to request from the *Bosch BSEC* library.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionRequest {
    /// Desired sample rate of the virtual sensor output.
    pub sample_rate: SampleRate,
    /// Desired virtual output to sample.
    pub sensor: OutputKind,
}

impl From<&SubscriptionRequest> for bsec_sensor_configuration_t {
    fn from(sensor_configuration: &SubscriptionRequest) -> Self {
        Self {
            sample_rate: sensor_configuration.sample_rate.into(),
            sensor_id: bsec_virtual_sensor_t::from(sensor_configuration.sensor)
                as u8,
        }
    }
}

/// Describes a physical BME sensor that needs to be sampled.
type RequiredInput = bsec_sensor_configuration_t;

/// Valid sampling rates for the BSEC algorithm.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SampleRate {
    /// Disabled, not being sampled.
    Disabled,
    /// Ultra-low power, see *Bosch BSEC* documentation.
    Ulp,
    /// Continuous mode for testing, see *Bosch BSEC* documentation.
    Continuous,
    /// Low power, see *Bosch BSEC* documentation.
    Lp,
    /// Perform a single measurement on demand between sampling intervals.
    ///
    /// See *Bosch BSEC* documentation.
    UlpMeasurementOnDemand,
}

impl From<SampleRate> for f32 {
    fn from(sample_rate: SampleRate) -> Self {
        f64::from(sample_rate) as f32
    }
}

impl From<SampleRate> for f64 {
    fn from(sample_rate: SampleRate) -> Self {
        use sys::*;
        use SampleRate::*;
        match sample_rate {
            Disabled => BSEC_SAMPLE_RATE_DISABLED,
            Ulp => BSEC_SAMPLE_RATE_ULP,
            Continuous => BSEC_SAMPLE_RATE_CONT,
            Lp => BSEC_SAMPLE_RATE_LP,
            UlpMeasurementOnDemand => {
                BSEC_SAMPLE_RATE_ULP_MEASUREMENT_ON_DEMAND
            }
        }
    }
}

/// Identifies a physical BME sensor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputKind {
    /// Pressure sensor.
    Pressure,
    /// Humidity sensor.
    Humidity,
    /// Temperature sensor.
    Temperature,
    /// Gas resistance sensor.
    GasResistor,
    /// Compensation for nearby heat sources.
    HeatSource,
    /// Pseudo-sensor to disable the baseline tracker.
    DisableBaselineTracker,
    /// Other sensor only known by magic number.
    Other(u32),
}

impl From<u8> for InputKind {
    fn from(physical_sensor: u8) -> Self {
        Self::from(physical_sensor as u32)
    }
}

impl From<u32> for InputKind {
    fn from(physical_sensor: u32) -> Self {
        #![allow(non_upper_case_globals)]
        use sys::*;
        use InputKind::*;
        match physical_sensor {
            bsec_physical_sensor_t_BSEC_INPUT_PRESSURE => Pressure,
            bsec_physical_sensor_t_BSEC_INPUT_HUMIDITY => Humidity,
            bsec_physical_sensor_t_BSEC_INPUT_TEMPERATURE => Temperature,
            bsec_physical_sensor_t_BSEC_INPUT_GASRESISTOR => GasResistor,
            bsec_physical_sensor_t_BSEC_INPUT_HEATSOURCE => HeatSource,
            bsec_physical_sensor_t_BSEC_INPUT_DISABLE_BASELINE_TRACKER => {
                DisableBaselineTracker
            }
            physical_sensor => Other(physical_sensor),
        }
    }
}

impl From<InputKind> for bsec_physical_sensor_t {
    fn from(physical_sensor: InputKind) -> Self {
        use sys::*;
        use InputKind::*;
        match physical_sensor {
            Pressure => bsec_physical_sensor_t_BSEC_INPUT_PRESSURE,
            Humidity => bsec_physical_sensor_t_BSEC_INPUT_HUMIDITY,
            Temperature => bsec_physical_sensor_t_BSEC_INPUT_TEMPERATURE,
            GasResistor => bsec_physical_sensor_t_BSEC_INPUT_GASRESISTOR,
            HeatSource => bsec_physical_sensor_t_BSEC_INPUT_HEATSOURCE,
            DisableBaselineTracker => {
                bsec_physical_sensor_t_BSEC_INPUT_DISABLE_BASELINE_TRACKER
            }
            Other(sensor) => sensor,
        }
    }
}

impl From<InputKind> for u8 {
    fn from(physical_sensor: InputKind) -> Self {
        bsec_physical_sensor_t::from(physical_sensor) as Self
    }
}

/// *Bosch BSEC* virtual sensor output.
///
/// See *Bosch BSEC* documentation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutputKind {
    Iaq = 0x0001,
    StaticIaq = 0x0002,
    Co2Equivalent = 0x0004,
    BreathVocEquivalent = 0x0008,
    RawTemperature = 0x0010,
    RawPressure = 0x0020,
    RawHumidity = 0x0040,
    RawGas = 0x0080,
    StabilizationStatus = 0x0100,
    RunInStatus = 0x0200,
    SensorHeatCompensatedTemperature = 0x0400,
    SensorHeatCompensatedHumidity = 0x0800,
    //    DebugCompensatedGas = 0x1000,
    GasPercentage = 0x2000,
}

impl From<OutputKind> for bsec_virtual_sensor_t {
    fn from(virtual_sensor: OutputKind) -> Self {
        use sys::*;
        use OutputKind::*;
        match virtual_sensor {
            Iaq => bsec_virtual_sensor_t_BSEC_OUTPUT_IAQ,
            StaticIaq => bsec_virtual_sensor_t_BSEC_OUTPUT_STATIC_IAQ,
            Co2Equivalent => bsec_virtual_sensor_t_BSEC_OUTPUT_CO2_EQUIVALENT,
            BreathVocEquivalent => bsec_virtual_sensor_t_BSEC_OUTPUT_BREATH_VOC_EQUIVALENT,
            RawTemperature => bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_TEMPERATURE,
            RawPressure => bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_PRESSURE,
            RawHumidity => bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_HUMIDITY,
            RawGas => bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_GAS,
            StabilizationStatus => bsec_virtual_sensor_t_BSEC_OUTPUT_STABILIZATION_STATUS,
            RunInStatus => bsec_virtual_sensor_t_BSEC_OUTPUT_RUN_IN_STATUS,
            SensorHeatCompensatedTemperature => {
                bsec_virtual_sensor_t_BSEC_OUTPUT_SENSOR_HEAT_COMPENSATED_TEMPERATURE
            }
            SensorHeatCompensatedHumidity => {
                bsec_virtual_sensor_t_BSEC_OUTPUT_SENSOR_HEAT_COMPENSATED_HUMIDITY
            }
            //            DebugCompensatedGas => bsec_virtual_sensor_t_BSEC_OUTPUT_COMPENSATED_GAS,
            GasPercentage => bsec_virtual_sensor_t_BSEC_OUTPUT_GAS_PERCENTAGE,
        }
    }
}

impl TryFrom<bsec_virtual_sensor_t> for OutputKind {
    type Error = ConversionError;
    fn try_from(
        virtual_sensor: bsec_virtual_sensor_t,
    ) -> Result<Self, ConversionError> {
        #![allow(non_upper_case_globals)]
        use sys::*;
        use OutputKind::*;
        match virtual_sensor {
            bsec_virtual_sensor_t_BSEC_OUTPUT_IAQ => Ok(Iaq),
            bsec_virtual_sensor_t_BSEC_OUTPUT_STATIC_IAQ => Ok(StaticIaq),
            bsec_virtual_sensor_t_BSEC_OUTPUT_CO2_EQUIVALENT => {
                Ok(Co2Equivalent)
            }
            bsec_virtual_sensor_t_BSEC_OUTPUT_BREATH_VOC_EQUIVALENT => {
                Ok(BreathVocEquivalent)
            }
            bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_TEMPERATURE => {
                Ok(RawTemperature)
            }
            bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_PRESSURE => Ok(RawPressure),
            bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_HUMIDITY => Ok(RawHumidity),
            bsec_virtual_sensor_t_BSEC_OUTPUT_RAW_GAS => Ok(RawGas),
            bsec_virtual_sensor_t_BSEC_OUTPUT_STABILIZATION_STATUS => {
                Ok(StabilizationStatus)
            }
            bsec_virtual_sensor_t_BSEC_OUTPUT_RUN_IN_STATUS => Ok(RunInStatus),
            bsec_virtual_sensor_t_BSEC_OUTPUT_SENSOR_HEAT_COMPENSATED_TEMPERATURE => {
                Ok(SensorHeatCompensatedTemperature)
            }
            bsec_virtual_sensor_t_BSEC_OUTPUT_SENSOR_HEAT_COMPENSATED_HUMIDITY => {
                Ok(SensorHeatCompensatedHumidity)
            }
            //           bsec_virtual_sensor_t_BSEC_OUTPUT_COMPENSATED_GAS => Ok(DebugCompensatedGas),
            bsec_virtual_sensor_t_BSEC_OUTPUT_GAS_PERCENTAGE => {
                Ok(GasPercentage)
            }
            _ => Err(ConversionError::InvalidVirtualSensorId(virtual_sensor)),
        }
    }
}

impl TryFrom<u8> for OutputKind {
    type Error = ConversionError;
    fn try_from(virtual_sensor: u8) -> Result<Self, ConversionError> {
        Self::try_from(virtual_sensor as bsec_virtual_sensor_t)
    }
}

trait IntoResult {
    fn into_result(self) -> Result<(), BsecError>;
}

impl IntoResult for bsec_library_return_t {
    fn into_result(self) -> Result<(), BsecError> {
        #![allow(non_upper_case_globals)]
        match self {
            sys::bsec_library_return_t_BSEC_OK => Ok(()),
            error_code => Err(BsecError::from(error_code)),
        }
    }
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(non_upper_case_globals)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bsec.rs"));
}
