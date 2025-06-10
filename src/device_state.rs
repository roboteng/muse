#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub name: String,
    pub uuid: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connected(DeviceInfo),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamingState {
    Stopped,
    Streaming,
}

pub struct DeviceStateManager {
    connection_state: ConnectionState,
    streaming_state: StreamingState,
}

impl DeviceStateManager {
    pub fn new() -> Self {
        Self {
            connection_state: ConnectionState::Disconnected,
            streaming_state: StreamingState::Stopped,
        }
    }

    // Connection state management
    pub fn set_connected(&mut self, name: String, uuid: String) {
        self.connection_state = ConnectionState::Connected(DeviceInfo { name, uuid });
    }

    pub fn set_disconnected(&mut self) {
        // When disconnecting, also stop streaming
        self.set_streaming_stopped();
        self.connection_state = ConnectionState::Disconnected;
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection_state, ConnectionState::Connected(_))
    }

    pub fn get_device_info(&self) -> Option<&DeviceInfo> {
        match &self.connection_state {
            ConnectionState::Connected(info) => Some(info),
            ConnectionState::Disconnected => None,
        }
    }

    pub fn get_device_name(&self) -> Option<&str> {
        self.get_device_info().map(|info| info.name.as_str())
    }

    pub fn get_device_uuid(&self) -> Option<&str> {
        self.get_device_info().map(|info| info.uuid.as_str())
    }

    // Streaming state management
    pub fn set_streaming_started(&mut self) -> Result<(), &'static str> {
        if !self.is_connected() {
            return Err("Cannot start streaming when not connected");
        }

        self.streaming_state = StreamingState::Streaming;
        Ok(())
    }

    pub fn set_streaming_stopped(&mut self) {
        self.streaming_state = StreamingState::Stopped;
    }

    pub fn is_streaming(&self) -> bool {
        matches!(self.streaming_state, StreamingState::Streaming)
    }

    // Validation methods
    pub fn can_start_streaming(&self) -> bool {
        self.is_connected() && !self.is_streaming()
    }

    pub fn can_stop_streaming(&self) -> bool {
        self.is_streaming()
    }

    // State reporting for debugging
    pub fn get_state_summary(&self) -> String {
        match self.get_device_info() {
            Some(info) => format!(
                "Connected to {} ({}), Streaming: {}", 
                info.name, info.uuid, self.is_streaming()
            ),
            None => format!("Disconnected, Streaming: {}", self.is_streaming()),
        }
    }
}

impl Default for DeviceStateManager {
    fn default() -> Self {
        Self::new()
    }
}