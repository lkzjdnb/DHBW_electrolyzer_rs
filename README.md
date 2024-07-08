# Electrolyzer modbus link

Utilities to poll modbus data and send them to a remote server (InfluxDB or Prometheus)

## Usage
```
Usage: dhbw_electrolyzer_rs [OPTIONS]

Options:
  -r, --remote <REMOTE>
          The device ip address as a parseable string ex : 127.0.0.1:502
          
          [default: 127.0.0.1:502]

      --input-register-path <INPUT_REGISTER_PATH>
          Path to the json file containing the registers definition
          
          [default: input_registers.json]

      --holding-register-path <HOLDING_REGISTER_PATH>
          Path to the json file containing the registers definition
          
          [default: holding_registers.json]

      --influx-db
          Activate the InfluxDB connexion

  -t, --token <TOKEN>
          InfluxDB API token, can also be defined with INFLUXDB_TOKEN environment variable
          
          [env: INFLUXDB_TOKEN=]

  -i, --influxdb-url <INFLUXDB_URL>
          URL of the InfluxDB server

  -p, --prometheus-url <PROMETHEUS_URL>
          URL of the Prometheus server

      --db-bucket <DB_BUCKET>
          Bucket in which to store the data
          
          [default: electrolyzer]

      --prometheus
          Activate the Prometheus PushGateway connexion

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### Example
#### InfluxDB
```
export INFLUXDB_TOKEN="tKjdyrIvwdg5sOvNOWRKN4EVtLdxJd484E42t3hWCy1bbP4eGbJuTrTygJAMOpLA17hxvQL0fWIMYXLJ2EyLTb9=="
RUST_LOG=info dhbw_electrolyzer --influx-db --influxdb-url "https://influxdb-domain.dom" --db-bucket "bucket-name" -r "192.168.1.12:502"
```

#### Prometheus
```
RUST_LOG=info dhbw_electrolyzer -r "192.168.1.12:502" --prometheus --prometheus-url "http://prometheus-url:9091"
```

#### Both
```
RUST_LOG=info dhbw_electrolyzer --influx-db --influxdb-url "https://influxdb-domain.dom" --db-bucket "bucket-name" -r "192.168.1.12:502" --prometheus --prometheus-url "http://prometheus-url:9091"
```

### Log level
The log level can be controlled using `RUST_LOG` environment variable see https://docs.rs/env_logger/latest/env_logger

## Build
The project can be built as any cargo project : 

Clone the project : 
`git clone https://github.com/lkzjdnb/DHBW_electrolyzer_rs.git`

Build it : 
`cargo build --release`

This will generate a binary in `./target/release`
