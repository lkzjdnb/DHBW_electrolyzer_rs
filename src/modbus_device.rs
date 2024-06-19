use serde::{Deserialize, Serialize};
use serde_json;
use std::fs::File;
use std::net::SocketAddr;
use std::{collections::HashMap, io::Error};
use tokio_modbus::{
    client::sync::{self, Context, Reader},
    Address, Quantity,
};

use crate::register::{self, Register};

// maximum number of register that can be read at once (limited by the protocol)
const MODBUS_MAX_READ_LEN: u16 = 125;

pub struct ModbusDevice {
    pub ctx: Context,
    pub input_registers: HashMap<String, Register>,
}

pub trait ModbusConnexion {
    fn read_raw_input_registers(&mut self, addr: Address, nb: Quantity) -> Result<Vec<u16>, Error>;
    fn read_input_registers_by_name(
        &mut self,
        names: Vec<String>,
    ) -> Result<HashMap<String, RegisterValue>, std::io::Error>;
    fn read_input_registers(
        &mut self,
        regs: Vec<Register>,
    ) -> Result<HashMap<String, RegisterValue>, Error>;

    fn dump_input_registers(&mut self) -> Result<HashMap<String, RegisterValue>, Error>;
}

#[derive(Debug)]
pub enum RegisterValue {
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    S32(i32),
    Enum16(u16),
    Sized([u8; 66]),
    Float32(f32),
    Boolean(bool),
}

#[derive(Serialize, Deserialize)]
enum DataType {
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    Int32,
    Enum16,
    #[serde(rename = "Sized+Uint16[31]")]
    Sized,
    #[serde(rename = "IEEE-754 float32")]
    Float32,
    #[serde(rename = "boolean")]
    Boolean,
}

impl Into<register::DataType> for DataType {
    fn into(self) -> register::DataType {
        match self {
            Self::UInt16 => register::DataType::UInt16,
            Self::UInt32 => register::DataType::UInt32,
            Self::UInt64 => register::DataType::UInt64,
            Self::UInt128 => register::DataType::UInt128,
            Self::Int32 => register::DataType::Int32,
            Self::Enum16 => register::DataType::Enum16,
            Self::Sized => register::DataType::Sized,
            Self::Float32 => register::DataType::Float32,
            Self::Boolean => register::DataType::Boolean,
        }
    }
}

impl From<(Vec<u16>, register::DataType)> for RegisterValue {
    fn from((raw, kind): (Vec<u16>, register::DataType)) -> Self {
        let raw_b: Vec<u8> = raw.iter().map(|v| v.to_le_bytes()).flatten().collect();
        match kind {
            register::DataType::UInt16 => RegisterValue::U16(raw[0]),
            register::DataType::UInt32 => {
                RegisterValue::U32(u32::from_le_bytes(raw_b.try_into().unwrap()))
            }
            register::DataType::UInt64 => {
                RegisterValue::U64(u64::from_le_bytes(raw_b.try_into().unwrap()))
            }
            register::DataType::UInt128 => {
                RegisterValue::U128(u128::from_le_bytes(raw_b.try_into().unwrap()))
            }
            register::DataType::Int32 => {
                RegisterValue::S32(i32::from_le_bytes(raw_b.try_into().unwrap()))
            }
            register::DataType::Enum16 => RegisterValue::Enum16(raw[0]),
            register::DataType::Sized => RegisterValue::Sized(raw_b.try_into().unwrap()),
            register::DataType::Float32 => {
                RegisterValue::Float32(f32::from_le_bytes(raw_b.try_into().unwrap()))
            }
            register::DataType::Boolean => RegisterValue::Boolean(!raw[0] == 0),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RawRegister {
    id: u16,
    name: String,
    #[serde(rename = "type")]
    type_: DataType,
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
                len: f.len / 16,
                data_type: f.type_.into(),
            },
        );
    }
    return Ok(m);
}

pub fn connect(addr: SocketAddr) -> Result<Context, std::io::Error> {
    sync::tcp::connect(addr)
}

impl ModbusConnexion for ModbusDevice {
    // read input registers by address
    fn read_raw_input_registers(&mut self, addr: Address, nb: Quantity) -> Result<Vec<u16>, Error> {
        println!("read register {addr} x{nb}");
        self.ctx.read_input_registers(addr, nb)
    }

    // read input registers by name
    fn read_input_registers_by_name(
        &mut self,
        names: Vec<String>,
    ) -> Result<HashMap<String, RegisterValue>, std::io::Error> {
        let registers_to_read: Vec<Register> = names
            .iter()
            .filter_map(|n| match self.input_registers.get(n) {
                Some(reg) => Some(reg.to_owned()),
                None => {
                    eprintln!("Register {n} does not exist, skipping it");
                    None
                }
            })
            .collect();
        self.read_input_registers(registers_to_read)
    }

    fn read_input_registers(
        &mut self,
        mut regs: Vec<Register>,
    ) -> Result<HashMap<String, RegisterValue>, std::io::Error> {
        // read registers in order of address
        regs.sort_by_key(|s| s.addr);

        // index of the start and end register for the current range
        let mut reg_range_start = 0;
        let mut reg_range_end = 0;

        let mut result: HashMap<String, RegisterValue> = HashMap::new();

        for (i, r) in regs.iter().enumerate() {
            // if the range is greater than the max request size we read this batch
            if r.addr - regs[reg_range_start].addr > MODBUS_MAX_READ_LEN
                || r.addr != regs[reg_range_end].addr + regs[reg_range_end].len
            {
                let s_reg = &regs[reg_range_start];
                let e_reg = &regs[reg_range_end];

                // Read the values
                println!(
                    "reading range {0}:{1}",
                    s_reg.addr,
                    e_reg.addr + e_reg.len - s_reg.addr
                );
                let read_regs: Vec<u16> =
                    self.read_raw_input_registers(s_reg.addr, e_reg.addr + e_reg.len - s_reg.addr)?;

                // convert them to the types and make the association with the registers
                let read_regs_map: HashMap<String, RegisterValue> = regs
                    [reg_range_start..reg_range_end]
                    .iter()
                    .map(|v| {
                        let start_off = v.addr - s_reg.addr;
                        let value: Vec<u16> =
                            read_regs[start_off.into()..(start_off + v.len).into()].to_vec();
                        let conv_value: RegisterValue = (value, v.data_type).into();
                        (v.name.to_owned(), conv_value)
                    })
                    .collect();

                // merge it with the result
                result.extend(read_regs_map);

                // reset range
                reg_range_start = i;
            }
            reg_range_end = i;
        }

        return Ok(result);
    }

    fn dump_input_registers(&mut self) -> Result<HashMap<String, RegisterValue>, Error> {
        let registers = self.input_registers.to_owned();
        let keys: Vec<String> = registers.into_keys().collect();
        self.read_input_registers_by_name(keys)
    }
}
