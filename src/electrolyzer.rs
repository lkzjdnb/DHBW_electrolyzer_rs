use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs::File;

use crate::modbus_device::ModbusDevice;
use crate::register::Register;

pub struct Electrolyzer {
    pub input_registers: HashMap<String, Register>,
}

impl ModbusDevice for Electrolyzer {}

#[derive(Serialize, Deserialize)]
struct RawRegister {
    id: u16,
    name: String,
    #[serde(rename = "type")]
    type_: String,
    len: u16,
}

#[derive(Serialize, Deserialize)]
struct RegistersFormat {
    metaid: String,
    result: String,
    registers: Vec<RawRegister>,
}

pub fn get_defs_from_json(input: File) -> Result<HashMap<String, Register>, serde_json::Error> {
    let raw: RegistersFormat = serde_json::from_reader(input)?;
    let mut m = HashMap::<String, Register>::new();
    for f in raw.registers {
        m.insert(
            f.name.clone(),
            Register {
                name: f.name,
                addr: f.id,
                len: f.len,
                data_type: f.type_,
            },
        );
    }
    return Ok(m);
}
