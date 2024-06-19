mod modbus_device;
use std::fs::File;

use modbus_device::ModbusConnexion;
use modbus_device::ModbusDevice;

mod register;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;
    let mut now = Instant::now();

    let electrolyzer_input_registers_json = File::open("input_registers.json")?;
    let mut electrolyzer = ModbusDevice {
        ctx: modbus_device::connect("192.168.0.2:502".parse()?)?,
        input_registers: modbus_device::get_defs_from_json(electrolyzer_input_registers_json)?,
    };

    let time_to_load = now.elapsed();
    println!("Time to load registers definition : {0:?}", time_to_load);

    println!("{0:?}", electrolyzer.input_registers);

    loop {
        now = Instant::now();
        let register_vals = electrolyzer.dump_input_registers();
        let time_to_read = now.elapsed();

        println!("Time ro read all input registers : {0:?}", time_to_read);

        println!("{0:?}", register_vals);
    }

    // return Ok(());
}
