use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs::File;
use std::net::SocketAddr;
use tokio_modbus::{
    client::sync::{self, Context, Reader},
    Address, Exception, Quantity,
};

use crate::register::{self, Register};

// maximum number of register that can be read at once (limited by the protocol)
const MODBUS_MAX_READ_LEN: u16 = 125;

pub struct ModbusDevice {
    pub ctx: Context,
    pub input_registers: HashMap<String, Register>,
    pub holding_registers: HashMap<String, Register>,
}

pub enum ModBusRegisters {
    INPUT,
    HOLDING,
}

#[derive(Debug)]
pub enum ModbusError {
    Exception(Exception),
    IOerror(std::io::Error),
    ModbusError(tokio_modbus::Error),
}
impl From<Exception> for ModbusError {
    fn from(value: Exception) -> Self {
        ModbusError::Exception(value)
    }
}
impl From<std::io::Error> for ModbusError {
    fn from(value: std::io::Error) -> Self {
        ModbusError::IOerror(value)
    }
}
impl From<tokio_modbus::Error> for ModbusError {
    fn from(value: tokio_modbus::Error) -> Self {
        ModbusError::ModbusError(value)
    }
}

pub trait ModbusConnexion {
    fn read_raw_input_registers(
        &mut self,
        addr: Address,
        nb: Quantity,
    ) -> Result<Vec<u16>, ModbusError>;
    fn read_input_registers_by_name(
        &mut self,
        names: Vec<String>,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError>;
    fn read_input_registers(
        &mut self,
        regs: Vec<Register>,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError>;

    fn read_register(
        &mut self,
        regs: Vec<Register>,
        source: ModBusRegisters,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError>;

    fn dump_input_registers(&mut self) -> Result<HashMap<String, RegisterValue>, ModbusError>;

    fn read_raw_holding_registers(
        &mut self,
        addr: Address,
        nb: Quantity,
    ) -> Result<Vec<u16>, ModbusError>;
    fn read_holding_registers_by_name(
        &mut self,
        names: Vec<String>,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError>;
    fn read_holding_registers(
        &mut self,
        regs: Vec<Register>,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError>;

    fn dump_holding_registers(&mut self) -> Result<HashMap<String, RegisterValue>, ModbusError>;
}

#[derive(Debug, Clone, Copy)]
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
    #[serde(alias = "Uint16")]
    UInt16,
    #[serde(alias = "Uint32")]
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

impl TryFrom<(Vec<u16>, register::DataType)> for RegisterValue {
    fn try_from((raw, kind): (Vec<u16>, register::DataType)) -> Result<Self, Self::Error> {
        let raw_b: Vec<u8> = raw
            .iter()
            .map(|v| v.to_be_bytes())
            .flatten()
            .rev()
            .collect();
        match kind {
            register::DataType::UInt16 => Ok(RegisterValue::U16(raw[0])),
            register::DataType::UInt32 => match raw_b.try_into() {
                Ok(res) => Ok(RegisterValue::U32(u32::from_le_bytes(res))),
                Err(err) => Err(err),
            },
            register::DataType::UInt64 => match raw_b.try_into() {
                Ok(res) => Ok(RegisterValue::U64(u64::from_le_bytes(res))),
                Err(err) => Err(err),
            },
            register::DataType::UInt128 => match raw_b.try_into() {
                Ok(res) => Ok(RegisterValue::U128(u128::from_le_bytes(res))),
                Err(err) => Err(err),
            },
            register::DataType::Int32 => match raw_b.try_into() {
                Ok(res) => Ok(RegisterValue::S32(i32::from_le_bytes(res))),
                Err(err) => Err(err),
            },
            register::DataType::Enum16 => Ok(RegisterValue::Enum16(raw[0])),
            register::DataType::Sized => match raw_b.try_into() {
                Ok(res) => Ok(RegisterValue::Sized(res)),
                Err(err) => Err(err),
            },
            register::DataType::Float32 => match raw_b.try_into() {
                Ok(res) => Ok(RegisterValue::Float32(f32::from_le_bytes(res))),
                Err(err) => Err(err),
            },
            register::DataType::Boolean => Ok(RegisterValue::Boolean(!raw[0] == 0)),
        }
    }

    type Error = Vec<u8>;
}

fn return_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct RawRegister {
    id: u16,
    name: String,
    #[serde(rename = "type")]
    type_: DataType,
    len: u16,
    #[serde(default = "return_true")]
    read: bool,
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
                read: f.read,
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
    fn read_raw_input_registers(
        &mut self,
        addr: Address,
        nb: Quantity,
    ) -> Result<Vec<u16>, ModbusError> {
        debug!("read register {addr} x{nb}");
        let res = self.ctx.read_input_registers(addr, nb);
        match res {
            Ok(res) => match res {
                Ok(res) => return Ok(res),
                Err(err) => Err(err.into()),
            },
            Err(err) => return Err(err.into()),
        }
    }

    fn read_input_registers_by_name(
        &mut self,
        names: Vec<String>,
    ) -> Result<HashMap<std::string::String, RegisterValue>, ModbusError> {
        let registers_to_read: Vec<Register> = names
            .iter()
            .filter_map(|n| match self.input_registers.get(n) {
                Some(reg) => Some(reg.to_owned()),
                None => {
                    warn!("Register {n} does not exist, skipping it");
                    None
                }
            })
            .collect();
        self.read_input_registers(registers_to_read)
    }

    fn read_register(
        &mut self,
        mut regs: Vec<Register>,
        source: ModBusRegisters,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError> {
        // read registers in order of address
        regs.sort_by_key(|s| s.addr);

        // index of the start and end register for the current range
        let mut reg_range_start = 0;
        let mut reg_range_end = 0;

        let mut result: HashMap<String, RegisterValue> = HashMap::new();

        for (mut i, r) in regs.iter().skip(1).enumerate() {
            i = i + 1;
            // if the range is greater than the max request size we read this batch
            if r.addr - regs[reg_range_start].addr > MODBUS_MAX_READ_LEN
                || r.addr != regs[reg_range_end].addr + regs[reg_range_end].len
                || i == regs.len() - 1
            {
                let s_reg = &regs[reg_range_start];
                let e_reg = &regs[reg_range_end];

                // Read the values
                debug!(
                    "reading range {0}:{1}",
                    s_reg.addr,
                    e_reg.addr + e_reg.len - s_reg.addr
                );
                let read_regs: Vec<u16> = match source {
                    ModBusRegisters::INPUT => self.read_raw_input_registers(
                        s_reg.addr,
                        e_reg.addr + e_reg.len - s_reg.addr,
                    )?,
                    ModBusRegisters::HOLDING => self.read_raw_holding_registers(
                        s_reg.addr,
                        e_reg.addr + e_reg.len - s_reg.addr,
                    )?,
                };

                // convert them to the types and make the association with the registers
                let read_regs_map: HashMap<String, RegisterValue> = regs
                    [reg_range_start..reg_range_end + 1]
                    .iter()
                    .filter_map(|v| {
                        let start_off = v.addr - s_reg.addr;
                        let value: Vec<u16> =
                            read_regs[start_off.into()..(start_off + v.len).into()].to_vec();
                        match (value, v.data_type).try_into() {
                            Ok(res) => Some((v.name.to_owned(), res)),
                            Err(err) => {
                                warn!(
                                    "There was an error converting field {0} dropping it ({err:?})",
                                    v.name
                                );
                                None
                            }
                        }
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

    fn read_input_registers(
        &mut self,
        regs: Vec<Register>,
    ) -> Result<HashMap<std::string::String, RegisterValue>, ModbusError> {
        self.read_register(regs, ModBusRegisters::INPUT)
    }

    fn dump_input_registers(
        &mut self,
    ) -> Result<HashMap<std::string::String, RegisterValue>, ModbusError> {
        let registers = self.input_registers.to_owned();
        let keys: Vec<String> = registers.into_keys().collect();
        self.read_input_registers_by_name(keys)
    }

    fn read_raw_holding_registers(
        &mut self,
        addr: Address,
        nb: Quantity,
    ) -> Result<Vec<u16>, ModbusError> {
        debug!("read register {addr} x{nb}");
        let res = self.ctx.read_holding_registers(addr, nb);
        match res {
            Ok(res) => match res {
                Ok(res) => return Ok(res),
                Err(err) => Err(err.into()),
            },
            Err(err) => return Err(err.into()),
        }
    }

    fn read_holding_registers_by_name(
        &mut self,
        names: Vec<String>,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError> {
        let registers_to_read: Vec<Register> = names
            .iter()
            .filter_map(|n| match self.holding_registers.get(n) {
                Some(reg) => Some(reg.to_owned()),
                None => {
                    warn!("Register {n} does not exist, skipping it");
                    None
                }
            })
            .collect();
        self.read_holding_registers(registers_to_read)
    }
    fn read_holding_registers(
        &mut self,
        regs: Vec<Register>,
    ) -> Result<HashMap<String, RegisterValue>, ModbusError> {
        self.read_register(regs, ModBusRegisters::HOLDING)
    }

    fn dump_holding_registers(&mut self) -> Result<HashMap<String, RegisterValue>, ModbusError> {
        let registers = self.holding_registers.to_owned();

        let keys: Vec<String> = registers
            .iter()
            .filter_map(|v| match v.1.read {
                true => Some(v.0.to_owned()),
                false => None,
            })
            .collect();
        self.read_holding_registers_by_name(keys)
    }
}
