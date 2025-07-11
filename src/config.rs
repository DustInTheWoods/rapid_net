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
    pub unix_path: Option<String>,
}

impl RapidServerConfig {
    // New constructor with unix_path parameter
    pub fn new_with_unix_path(address: String, no_delay: bool, socket_type: Option<SocketType>, unix_path: Option<String>) -> Self {
        Self { 
            no_delay, 
            address, 
            socket_type: socket_type.unwrap_or_default(),
            unix_path
        }
    }

    // Backward compatible constructor
    pub fn new(address: String, no_delay: bool, socket_type: Option<SocketType>) -> Self {
        Self::new_with_unix_path(address, no_delay, socket_type, None)
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

    pub fn unix_path(&self) -> Option<&str> {
        self.unix_path.as_deref()
    }
}


#[derive(Clone, Debug, Default)]
pub struct RapidClientConfig {
    pub no_delay: bool,
    pub address: String,
    pub socket_type: SocketType,
    pub unix_path: Option<String>,
}

impl RapidClientConfig {
    // New constructor with unix_path parameter
    pub fn new_with_unix_path(address: String, no_delay: bool, socket_type: Option<SocketType>, unix_path: Option<String>) -> Self {
        Self { 
            no_delay, 
            address, 
            socket_type: socket_type.unwrap_or_default(),
            unix_path
        }
    }

    // Backward compatible constructor
    pub fn new(address: String, no_delay: bool, socket_type: Option<SocketType>) -> Self {
        Self::new_with_unix_path(address, no_delay, socket_type, None)
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

    pub fn unix_path(&self) -> Option<&str> {
        self.unix_path.as_deref()
    }
}
