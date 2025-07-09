use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RapidServerConfig {
    pub no_delay: bool,
    pub address: String,
    pub thread_count: Option<usize>,
}

impl RapidServerConfig {
    pub fn new(address: String, no_delay: bool) -> Self {
        Self { no_delay, address, thread_count: None }
    }

    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = Some(thread_count);
        self
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn no_delay(&self) -> bool {
        self.no_delay
    }

    pub fn thread_count(&self) -> Option<usize> {
        self.thread_count
    }
}


#[derive(Clone, Debug, Default)]
pub struct RapidClientConfig {
    pub no_delay: bool,
    pub address: String,
    pub thread_count: Option<usize>,
}

impl RapidClientConfig {
    pub fn new(address: String, no_delay: bool) -> Self {
        Self { no_delay, address, thread_count: None }
    }

    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = Some(thread_count);
        self
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn no_delay(&self) -> bool {
        self.no_delay
    }

    pub fn thread_count(&self) -> Option<usize> {
        self.thread_count
    }
}
