use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RapidServerConfig {
    pub no_delay: bool,
    pub address: String,
}

impl RapidServerConfig {
    pub fn new(address: String, no_delay: bool) -> Self {
        Self { no_delay, address }
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn no_delay(&self) -> bool {
        self.no_delay
    }
}


#[derive(Clone, Debug, Default)]
pub struct RapidClientConfig {
    pub no_delay: bool,
    pub address: String,
}

impl RapidClientConfig {
    pub fn new(address: String, no_delay: bool) -> Self {
        Self { no_delay, address }
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn no_delay(&self) -> bool {
        self.no_delay
    }
}
