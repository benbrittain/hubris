use bme68x_rust::*;
use drv_i2c_api as i2c_api;
use drv_i2c_api::I2cDevice;
use userlib::*;

pub struct Bme {
    pub bme: Device<NrfI2c>,
}

impl Bme {
    pub fn initialize() -> Result<Self, Error> {
        let i2c = i2c_config::devices::bme68x(crate::I2C.get_task_id())[0];

        let mut bme = match Device::initialize(NrfI2c { i2c }) {
            Err(e) => {
                sys_log!("Error in air-quality {:?}", e);
                panic!();
            }
            Ok(b) => b,
        };

        // configure device
        bme.set_config(
            DeviceConfig::default()
                .filter(Filter::Off)
                .odr(Odr::StandbyNone)
                .oversample_humidity(Sample::X16)
                .oversample_pressure(Sample::Once)
                .oversample_temperature(Sample::X2),
        )?;

        // configure heater
        bme.set_gas_heater_conf(
            OperationMode::Forced,
            GasHeaterConfig::default()
                .enable()
                .heater_temp(300)
                .heater_duration(100),
        )?;

        Ok(Bme { bme })
    }
}

#[derive(Debug)]
pub struct NrfI2c {
    pub i2c: i2c_api::I2cDevice,
}

impl Interface for NrfI2c {
    fn interface_type(&self) -> CommInterface {
        CommInterface::I2C
    }

    fn read(
        &self,
        reg_addr: u8,
        reg_data: &mut [u8],
    ) -> Result<(), bme68x_rust::Error> {
        let r = self.i2c.read_reg_into(reg_addr, reg_data);
        //sys_log!("erro: {:?}", r);
        Ok(())
    }

    fn write(
        &self,
        reg_addr: u8,
        buf: &[u8],
    ) -> Result<(), bme68x_rust::Error> {
        let mut new_buf = [0; 16];
        new_buf[0] = reg_addr;
        new_buf[1..buf.len() + 1].copy_from_slice(buf);
        //sys_log!("WRITE {:x}: {:x?}", reg_addr, buf);
        let r = self.i2c.write(&new_buf[..buf.len() + 1]);
        //sys_log!("res: {:?}", r);

        //sys_log!("WRITE 2");
        //self.i2c.write(buf);
        Ok(())
    }

    fn delay(&self, d: u32) {
        userlib::hl::sleep_for((d / 100).into())
    }
}

include!(concat!(env!("OUT_DIR"), "/i2c_config.rs"));