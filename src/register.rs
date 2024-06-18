use std::fmt::Debug;

pub struct Register {
    pub name: String,
    pub addr: u16,
    pub len: u16,
    pub data_type: String,
}

impl Debug for Register {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registre")
            .field("name", &self.name)
            .field("addr", &self.addr)
            .field("len", &self.len)
            .field("data_type", &self.data_type)
            .finish()
    }
}
