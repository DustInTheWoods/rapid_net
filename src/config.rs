use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum SocketType {
    Tcp,
    Unix,
}

impl Default for SocketType {
    fn default() -> Self {
        SocketType::Tcp
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RapidServerConfig {
    pub no_delay: bool,
    pub address: String,
    pub socket_type: SocketType,
}

impl RapidServerConfig {
    pub fn new(address: String, no_delay: bool, socket_type: Option<SocketType>) -> Self {
        Self { 
            no_delay, 
            address, 
            socket_type: socket_type.unwrap_or_default() 
        }
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn no_delay(&self) -> bool {
        self.no_delay
    }

    pub fn socket_type(&self) -> &SocketType {
        &self.socket_type
    }
}


#[derive(Clone, Debug, Default)]
pub struct RapidClientConfig {
    pub no_delay: bool,
    pub address: String,
    pub socket_type: SocketType,
}

impl RapidClientConfig {
    pub fn new(address: String, no_delay: bool, socket_type: Option<SocketType>) -> Self {
        Self { 
            no_delay, 
            address, 
            socket_type: socket_type.unwrap_or_default() 
        }
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn no_delay(&self) -> bool {
        self.no_delay
    }

    pub fn socket_type(&self) -> &SocketType {
        &self.socket_type
    }
}
