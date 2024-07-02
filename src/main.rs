use log::{debug, error, info, warn};

use core::panic;
use std::time::Instant;

mod modbus_device;

use std::fs::File;

use chrono;
use influxdb::Type;

use modbus_device::ModbusConnexion;
use modbus_device::ModbusDevice;
use modbus_device::RegisterValue;

mod register;

use influxdb::{Client, InfluxDbWriteable, Timestamp};

use clap::Parser;

use backoff::{Error, ExponentialBackoff};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "127.0.0.1:502",
        help = "The device address",
        long_help = "The device ip address as a parseable string ex : 127.0.0.1:502"
    )]
    remote: String,

    #[arg(
        short,
        long,
        env = "INFLUXDB_TOKEN",
        help = "The influxDB API token",
        long_help = "InfluxDB API token, can also be defined with INFLUXDB_TOKEN environment variable"
    )]
    token: String,

    #[arg(
        long,
        default_value = "input_registers.json",
        help = "Path to the json file containing the registers definition"
    )]
    register_path: String,

    #[arg(
        short,
        long,
        default_value = "https://dhbw-influx.leserveurdansmongrenier.uk",
        help = "URL to the database used",
        long_help = "URL to the database (InfluxDB)"
    )]
    db_url: String,

    #[arg(
        long,
        default_value = "electrolyzer",
        help = "Bucket in which to store the data"
    )]
    db_bucket: String,
}

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

    let args = Args::parse();

    let token: String = args.token;

    let electrolyzer_input_registers_json = match File::open(&args.register_path) {
        Ok(file) => file,
        Err(err) => panic!(
            "Could not open the file containing the registers definition : {0} ({err:?})",
            &args.register_path
        ),
    };

    let electrolyzer_address = match args.remote.parse() {
        Ok(addr) => addr,
        Err(err) => panic!("Invalid remote address entered {0} ({err})", args.remote),
    };

    let mut electrolyzer = ModbusDevice {
        ctx: match modbus_device::connect(electrolyzer_address) {
            Ok(ctx) => ctx,
            Err(err) => panic!("Error connecting to device {electrolyzer_address} ({err})"),
        },
        input_registers: match modbus_device::get_defs_from_json(electrolyzer_input_registers_json)
        {
            Ok(registers) => registers,
            Err(err) => panic!("Could not load registers definition from file ({err})"),
        },
    };

    let time_to_load = now.elapsed();
    info!("Time to load registers definition : {0:?}", time_to_load);

    debug!("{0:?}", electrolyzer.input_registers);

    let client = Client::new(args.db_url, args.db_bucket).with_token(token);

    loop {
        now = Instant::now();
        let register_vals = match electrolyzer.dump_input_registers() {
            Ok(vals) => vals,
            Err(err) => {
                error!("Error reading registers, trying again ({err})");
                continue;
            }
        };
        let time_to_read = now.elapsed();

        info!("Time ro read all input registers : {0:?}", time_to_read);

        now = Instant::now();
        let mut write_query =
            Timestamp::from(chrono::offset::Local::now()).into_query("electrolyzer");

        for (name, reg) in &register_vals {
            debug!("sending {name} {reg:?}");
            write_query = write_query.add_field(name, reg);
        }

        match backoff::retry(ExponentialBackoff::default(), || {
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(client.query(&write_query))
            {
                Ok(res) => Ok(res),
                Err(err) => {
                    warn!("Could not send data to server, trying again ({err})");
                    Err(err).map_err(Error::transient)
                }
            }
        }) {
            Ok(res) => res,
            Err(err) => {
                error!("There was an error sending data, retrying {err}");
                continue;
            }
        };

        let time_to_query = now.elapsed();

        info!("Time to send query : {0:?}", time_to_query);

        debug!("{0:?}", register_vals);
    }

    // return Ok(());
}
