use log::{debug, error, info, warn};

use core::panic;
use std::time::Instant;

use metrics::{gauge, KeyName};

use std::fs::File;

use chrono;
use influxdb::Type;

use modbus_device::ModbusConnexion;
use modbus_device::ModbusDevice;
use modbus_device::ModbusError;
use modbus_device::RegisterValue;

use influxdb::{Client, InfluxDbWriteable, Timestamp};

use clap::Parser;

use backoff::{Error, ExponentialBackoff};

use metrics_exporter_prometheus::PrometheusBuilder;

use metrics_util::MetricKindMask;
use std::time::Duration;

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
        long,
        default_value = "input_registers.json",
        help = "Path to the json file containing the registers definition"
    )]
    input_register_path: String,

    #[arg(
        long,
        default_value = "holding_registers.json",
        help = "Path to the json file containing the registers definition"
    )]
    holding_register_path: String,

    #[arg(long, help = "Activate the InfluxDB connexion", requires_all(["token", "influxdb_url", "db_bucket"]), )]
    influx_db: bool,

    #[arg(
        short,
        long,
        required = false,
        env = "INFLUXDB_TOKEN",
        help = "The influxDB API token",
        long_help = "InfluxDB API token, can also be defined with INFLUXDB_TOKEN environment variable"
    )]
    token: Option<String>,

    #[arg(
        short,
        long,
        required = false,
        help = "InfluxDB URL",
        long_help = "URL of the InfluxDB server"
    )]
    influxdb_url: Option<String>,
    #[arg(
        short,
        long,
        required = false,
        help = "Prometheus PushGateway URL",
        long_help = "URL of the Prometheus server"
    )]
    prometheus_url: Option<String>,

    #[arg(
        long,
        required = false,
        required_if_eq("influx_db", "true"),
        default_value = "electrolyzer",
        help = "Bucket in which to store the data"
    )]
    db_bucket: Option<String>,

    #[arg(long, action, help = "Activate the Prometheus PushGateway connexion", requires_all(["prometheus_url"]))]
    prometheus: bool,
}

struct LocalRegisterValue(RegisterValue);

impl Into<Type> for LocalRegisterValue {
    fn into(self) -> Type {
        match self.0 {
            RegisterValue::U16(val) => val.into(),
            RegisterValue::U32(val) => val.into(),
            RegisterValue::U64(val) => val.into(),
            RegisterValue::U128(val) => val.to_string().into(),
            RegisterValue::S32(val) => val.into(),
            RegisterValue::Enum16(val) => val.into(),
            RegisterValue::Sized(val) => format!("{0:x?}", &val).into(),
            RegisterValue::Float32(val) => match val.is_nan() {
                true => (-1.0).into(),
                _ => val.into(),
            },
            RegisterValue::Boolean(val) => val.into(),
        }
    }
}
impl Into<f64> for LocalRegisterValue {
    fn into(self) -> f64 {
        match self.0 {
            RegisterValue::U16(val) => val.into(),
            RegisterValue::U32(val) => val.into(),
            RegisterValue::U64(val) => val as f64,
            RegisterValue::U128(val) => val as f64,
            RegisterValue::S32(val) => val.into(),
            RegisterValue::Enum16(val) => val.into(),
            RegisterValue::Sized(_) => 0 as f64,
            RegisterValue::Float32(val) => match val.is_nan() {
                true => (-1.0).into(),
                _ => val.into(),
            },
            RegisterValue::Boolean(val) => val.into(),
        }
    }
}

fn manage_modbus_error(
    err: ModbusError,
    electrolyzer: &mut ModbusDevice,
) -> Result<(), ModbusError> {
    match err {
        ModbusError::ModbusError(tokio_modbus::Error::Transport(err)) => match err.kind() {
            std::io::ErrorKind::BrokenPipe => {
                error!("Broken pipe while reading register reconnecting to device ({err})");
                backoff::retry(ExponentialBackoff::default(), || {
                    match electrolyzer.connect() {
                        Ok(res) => {
                            info!("Reconnexion successful !");
                            Ok(res)
                        }
                        Err(err) => {
                            warn!("Connexion error on reconnect, re-trying ({err})");
                            Err(backoff::Error::transient(err))
                        }
                    }
                })
                .unwrap();
                return Err(err.into());
            }
            _ => {
                error!("IOError reading registers, trying again ({err})");
                return Err(err.into());
            }
        },
        err => {
            error!("Error reading registers, trying again ({err:?})");
            return Err(err.into());
        }
    };
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut now = Instant::now();

    let args = Args::parse();

    let electrolyzer_input_registers_json = match File::open(&args.input_register_path) {
        Ok(file) => file,
        Err(err) => panic!(
            "Could not open the file containing the input registers definition : {0} ({err:?})",
            &args.input_register_path
        ),
    };
    let electrolyzer_holding_registers_json = match File::open(&args.holding_register_path) {
        Ok(file) => file,
        Err(err) => panic!(
            "Could not open the file containing the holding registers definition : {0} ({err:?})",
            &args.holding_register_path
        ),
    };

    let electrolyzer_address = match args.remote.parse() {
        Ok(addr) => addr,
        Err(err) => panic!("Invalid remote address entered {0} ({err})", args.remote),
    };

    let mut electrolyzer = ModbusDevice {
        addr: electrolyzer_address,
        ctx: match modbus_device::connect(electrolyzer_address) {
            Ok(ctx) => ctx,
            Err(err) => panic!("Error connecting to device {electrolyzer_address} ({err})"),
        },
        input_registers: match modbus_device::get_defs_from_json(electrolyzer_input_registers_json)
        {
            Ok(registers) => registers,
            Err(err) => panic!("Could not load input registers definition from file ({err})"),
        },
        holding_registers: match modbus_device::get_defs_from_json(
            electrolyzer_holding_registers_json,
        ) {
            Ok(registers) => registers,
            Err(err) => panic!("Could not load holding registers definition from file ({err})"),
        },
    };

    let time_to_load = now.elapsed();
    info!("Time to load registers definition : {0:?}", time_to_load);

    debug!("{0:?}", electrolyzer.input_registers);

    let mut influx_client: Option<influxdb::Client> = None;
    if args.influx_db {
        influx_client = Some(
            Client::new(args.influxdb_url.unwrap(), args.db_bucket.unwrap())
                .with_token(args.token.unwrap()),
        );
    }

    if args.prometheus {
        PrometheusBuilder::new()
            .with_push_gateway(
                args.prometheus_url.unwrap(),
                Duration::from_secs(15),
                None,
                None,
            )?
            .idle_timeout(MetricKindMask::GAUGE, Some(Duration::from_secs(10)))
            .install()?;
    }

    loop {
        now = Instant::now();
        let register_read_result = electrolyzer.dump_input_registers();
        let register_vals = match register_read_result {
            Ok(vals) => vals.clone(),
            Err(err) => match manage_modbus_error(err, &mut electrolyzer) {
                Ok(_) => panic!("Mismatched error result"),
                Err(_) => continue,
            },
        };

        let time_to_read = now.elapsed();

        info!("Time to read all input registers : {0:?}", time_to_read);

        if args.influx_db {
            now = Instant::now();
            let mut write_query =
                Timestamp::from(chrono::offset::Local::now()).into_query("electrolyzer");

            for (name, reg) in register_vals.clone() {
                debug!("sending {name} {reg:?}");
                write_query = write_query.add_field(name, LocalRegisterValue(reg));
            }

            match backoff::retry(ExponentialBackoff::default(), || {
                match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on((influx_client.as_ref()).unwrap().query(&write_query))
                {
                    Ok(res) => match res.is_empty() {
                        true => Ok(res),
                        false => {
                            error!("Could not send data to influxDB ({res})");
                            Ok(res)
                        }
                    },
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

            info!("Time to send InfluxDB query : {0:?}", time_to_query);
        }

        if args.prometheus {
            for (name, reg) in register_vals.clone() {
                debug!("sending {name} {reg:?}");
                gauge!(KeyName::from_const_str((name.clone()).leak()))
                    .set::<f64>(LocalRegisterValue(reg).into());
            }
        }

        // debug!("{0:?}", register_vals);
    }

    // return Ok(());
}
