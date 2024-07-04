use std::fmt::Debug;

#[derive(Debug, Copy, Clone)]
pub enum DataType {
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    Int32,
    Enum16,
    Sized,
    Float32,
    Boolean,
}

#[derive(Clone)]
pub struct Register {
    pub name: String,
    pub addr: u16,
    pub len: u16, // in 16bits
    pub data_type: DataType,
    pub read: bool,
}

impl Debug for Register {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registre")
            .field("name", &self.name)
            .field("addr", &self.addr)
            .field("len", &self.len)
            .field("data_type", &self.data_type)
            .field("read", &self.read)
            .finish()
    }
}
