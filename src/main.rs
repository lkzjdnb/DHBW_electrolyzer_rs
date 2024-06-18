mod electrolyzer;
use std::fs::File;

use electrolyzer::Electrolyzer;

mod modbus_device;
mod register;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let f = File::open("input_registers.json")?;
    let e = Electrolyzer {
        input_registers: electrolyzer::get_defs_from_json(f)?,
    };

    println!("{0:?}", e.input_registers);

    return Ok(());
    // use tokio_modbus::prelude::*;

    // let socket_addr = "127.0.0.1:4502".parse().unwrap();
    // let mut ctx = sync::tcp::connect(socket_addr).unwrap();
    // let buff = ctx.read_input_registers(0, 125).unwrap();
    // println!("Response is '{buff:?}'");
}
