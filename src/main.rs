use log::{debug, info};

use std::time::Instant;

mod modbus_device;
use std::env;

use std::fs::File;

use chrono;
use influxdb::Type;

use modbus_device::ModbusConnexion;
use modbus_device::ModbusDevice;
use modbus_device::RegisterValue;

mod register;

use influxdb::{Client, InfluxDbWriteable, Timestamp};

impl Into<Type> for RegisterValue {
    fn into(self) -> Type {
        match self {
            RegisterValue::U16(val) => val.into(),
            RegisterValue::U32(val) => val.into(),
            RegisterValue::U64(val) => val.into(),
            RegisterValue::U128(val) => val.to_string().into(),
            RegisterValue::S32(val) => val.into(),
            RegisterValue::Enum16(val) => val.into(),
            RegisterValue::Sized(val) => std::str::from_utf8(&val).unwrap().into(),
            RegisterValue::Float32(val) => match val.is_nan() {
                true => (0).into(),
                _ => val.into(),
            },
            RegisterValue::Boolean(val) => val.into(),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut now = Instant::now();

    let args: Vec<String> = env::args().collect();

    let token: String = env::var("INFLUXDB_TOKEN")?;

    let electrolyzer_input_registers_json = File::open("input_registers.json")?;
    let mut electrolyzer = ModbusDevice {
        ctx: modbus_device::connect(args[1].parse()?)?,
        input_registers: modbus_device::get_defs_from_json(electrolyzer_input_registers_json)?,
    };

    let time_to_load = now.elapsed();
    info!("Time to load registers definition : {0:?}", time_to_load);

    debug!("{0:?}", electrolyzer.input_registers);

    let client = Client::new(
        "https://dhbw-influx.leserveurdansmongrenier.uk",
        "electrolyzer",
    )
    .with_token(token);

    loop {
        now = Instant::now();
        let register_vals = electrolyzer.dump_input_registers()?;
        let time_to_read = now.elapsed();

        info!("Time ro read all input registers : {0:?}", time_to_read);

        now = Instant::now();
        let mut write_query =
            Timestamp::from(chrono::offset::Local::now()).into_query("electrolyzer");

        for (name, reg) in &register_vals {
            debug!("sending {name} {reg:?}");
            write_query = write_query.add_field(name, reg);
        }

        let res = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(client.query(write_query))?;

        let time_to_query = now.elapsed();

        info!("Time to send query : {0:?}", time_to_query);

        info!("query result : {res}");

        debug!("{0:?}", register_vals);
    }

    // return Ok(());
}
