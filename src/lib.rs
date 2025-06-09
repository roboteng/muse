use btleplug::platform::Peripheral as PlatformPeripheral;
use napi::{Env, JsBoolean, JsNumber, JsString, Result};
use napi_derive::napi;
use std::sync::{Arc, RwLock};
use tokio::sync::{mpsc, Mutex};

mod ble;
use ble::{BleConnector, DataType};

#[napi]
pub struct MuseDevice {
  connector: Arc<Mutex<Option<BleConnector<PlatformPeripheral>>>>,
  target_uuid: Option<String>,
  #[allow(dead_code)]
  rssi_interval_ms: Option<u32>,
  #[allow(dead_code)]
  xdf_record_path: Option<String>,
  connected: Arc<RwLock<bool>>,
  device_name: Arc<RwLock<Option<String>>>,
  device_uuid: Arc<RwLock<Option<String>>>,
  streaming: Arc<RwLock<bool>>,
  streaming_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

#[napi]
impl MuseDevice {
  #[napi(constructor)]
  pub fn new(options: DeviceAdapterOptions) -> Self {
    let target_uuid = options.ble_uuid.and_then(|js_str| {
      js_str.into_utf8().ok().and_then(|utf8| utf8.as_str().ok().map(|s| s.to_string()))
    });
    let rssi_interval_ms = options.rssi_interval_ms.and_then(|js_num| js_num.get_uint32().ok());
    let xdf_record_path = options.xdf_record_path.and_then(|js_str| {
      js_str.into_utf8().ok().and_then(|utf8| utf8.as_str().ok().map(|s| s.to_string()))
    });

    Self {
      connector: Arc::new(Mutex::new(None)),
      target_uuid,
      rssi_interval_ms,
      xdf_record_path,
      connected: Arc::new(RwLock::new(false)),
      device_name: Arc::new(RwLock::new(None)),
      device_uuid: Arc::new(RwLock::new(None)),
      streaming: Arc::new(RwLock::new(false)),
      streaming_task: Arc::new(Mutex::new(None)),
    }
  }

  #[napi]
  pub async fn connect(&self) -> napi::Result<()> {
    let mut connector_guard = self.connector.lock().await;

    if connector_guard.is_none() {
      let connector = BleConnector::new()
        .await
        .map_err(|e| napi::Error::from_reason(format!("Failed to create BLE connector: {}", e)))?;
      *connector_guard = Some(connector);
    }

    if let Some(connector) = connector_guard.as_mut() {
      let (device_name, device_uuid) =
        connector
          .connect(self.target_uuid.clone())
          .await
          .map_err(|e| {
            napi::Error::from_reason(format!("Failed to connect to Muse device: {}", e))
          })?;

      *self.connected.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = true;
      *self.device_name.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = Some(device_name);
      *self.device_uuid.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = Some(device_uuid);
    }

    Ok(())
  }

  #[napi]
  pub async fn start_streaming(&self) -> napi::Result<()> {
    let mut connector_guard = self.connector.lock().await;
    
    if let Some(connector) = connector_guard.as_mut() {
      // Create channel for data streaming
      let (data_tx, data_rx) = mpsc::unbounded_channel::<DataType>();
      
      // Start BLE streaming with the sender
      connector.start_streaming(data_tx).await
        .map_err(|e| napi::Error::from_reason(format!("Failed to start streaming: {}", e)))?;
      
      // Spawn a regular task that handles the blocking LSL operations
      let streaming_handle = tokio::task::spawn(async move {
        // Run LSL operations in a blocking task since they're not Send
        tokio::task::spawn_blocking(move || {
          // Create a new Tokio runtime for LSL operations
          let rt = tokio::runtime::Runtime::new().unwrap();
          rt.block_on(async {
            Self::streaming_task(data_rx).await
          });
        }).await.unwrap_or_else(|e| {
          eprintln!("Streaming task error: {:?}", e);
        });
      });
      
      // Store the task handle
      *self.streaming_task.lock().await = Some(streaming_handle);
      *self.streaming.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = true;
    } else {
      return Err(napi::Error::from_reason("Device not connected"));
    }
    
    Ok(())
  }

  #[napi]
  pub async fn stop_streaming(&self) -> napi::Result<()> {
    let mut connector_guard = self.connector.lock().await;
    
    if let Some(connector) = connector_guard.as_mut() {
      connector.stop_streaming().await
        .map_err(|e| napi::Error::from_reason(format!("Failed to stop streaming: {}", e)))?;
    }
    
    // Stop the background streaming task
    if let Some(handle) = self.streaming_task.lock().await.take() {
      handle.abort();
    }
    
    *self.streaming.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = false;
    
    Ok(())
  }

  #[napi]
  pub async fn disconnect(&self) -> napi::Result<()> {
    let mut connector_guard = self.connector.lock().await;

    if let Some(connector) = connector_guard.as_mut() {
      connector
        .disconnect()
        .await
        .map_err(|e| napi::Error::from_reason(format!("Failed to disconnect: {}", e)))?;
    }

    *self.connected.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = false;
    *self.device_name.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = None;
    *self.device_uuid.write().map_err(|_| napi::Error::from_reason("Failed to acquire write lock"))? = None;

    Ok(())
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_name(&self, env: Env) -> Result<JsString> {
    let name_guard = self.device_name.read().map_err(|_| napi::Error::from_reason("Failed to acquire read lock"))?;
    match name_guard.as_ref() {
      Some(name) => env.create_string(name),
      None => Err(napi::Error::from_reason("Device not connected")),
    }
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_uuid(&self, env: Env) -> Result<JsString> {
    let uuid_guard = self.device_uuid.read().map_err(|_| napi::Error::from_reason("Failed to acquire read lock"))?;
    match uuid_guard.as_ref() {
      Some(uuid) => env.create_string(uuid),
      None => Err(napi::Error::from_reason("Device not connected")),
    }
  }

  #[napi(getter)]
  pub fn is_streaming(&self, env: Env) -> Result<JsBoolean> {
    let streaming = *self.streaming.read().map_err(|_| napi::Error::from_reason("Failed to acquire read lock"))?;
    env.get_boolean(streaming)
  }

  #[napi(getter)]
  pub fn is_connected(&self, env: Env) -> Result<JsBoolean> {
    let connected = *self.connected.read().map_err(|_| napi::Error::from_reason("Failed to acquire read lock"))?;
    env.get_boolean(connected)
  }

  async fn streaming_task(mut data_rx: mpsc::UnboundedReceiver<DataType>) {
    use lsl::{StreamOutlet, StreamInfo, ChannelFormat, Pushable};
    
    // Create EEG StreamInfo with metadata
    let mut eeg_info = match StreamInfo::new(
      "Muse S Gen 2 EEG",
      "EEG", 
      5, // 5 EEG channels
      256.0, // EEG sample rate
      ChannelFormat::Float32,
      "muse-eeg"
    ) {
      Ok(info) => info,
      Err(_) => return, // Exit task if we can't create stream info
    };
    
    // Add EEG metadata
    eeg_info.desc().append_child_value("manufacturer", "Interaxon");
    
    // Add EEG channel information
    let mut eeg_channels = eeg_info.desc().append_child("channels");
    let eeg_channel_names = ["EEG_TP9", "EEG_AF7", "EEG_AF8", "EEG_TP10", "EEG_AUX"];
    
    for channel_name in &eeg_channel_names {
      eeg_channels.append_child("channel")
        .append_child_value("label", channel_name)
        .append_child_value("unit", "microvolt")
        .append_child_value("type", "EEG");
    }
    
    // Add acquisition system metadata
    eeg_info.desc().append_child("acquisition")
      .append_child_value("manufacturer", "Interaxon")
      .append_child_value("model", "Muse S Gen 2");
    
    let eeg_outlet = match StreamOutlet::new(&eeg_info, 12, 360) {
      Ok(outlet) => outlet,
      Err(_) => return,
    };

    // Create PPG StreamInfo with metadata
    let mut ppg_info = match StreamInfo::new(
      "Muse S Gen 2 PPG",
      "PPG",
      3, // 3 PPG channels
      64.0, // PPG sample rate
      ChannelFormat::Float32,
      "muse-s-ppg"
    ) {
      Ok(info) => info,
      Err(_) => return,
    };
    
    // Add PPG metadata
    ppg_info.desc().append_child_value("manufacturer", "Interaxon");
    
    // Add PPG channel information
    let mut ppg_channels = ppg_info.desc().append_child("channels");
    let ppg_channel_names = ["PPG_AMBIENT", "PPG_INFRARED", "PPG_RED"];
    
    for channel_name in &ppg_channel_names {
      ppg_channels.append_child("channel")
        .append_child_value("label", channel_name)
        .append_child_value("unit", "N/A")
        .append_child_value("type", "PPG");
    }
    
    // Add acquisition system metadata
    ppg_info.desc().append_child("acquisition")
      .append_child_value("manufacturer", "Interaxon")
      .append_child_value("model", "Muse S Gen 2");
    
    let ppg_outlet = match StreamOutlet::new(&ppg_info, 6, 360) {
      Ok(outlet) => outlet,
      Err(_) => return,
    };

    // Process incoming data
    while let Some(data_type) = data_rx.recv().await {
      match data_type {
        DataType::Eeg(samples) => {
          let _ = eeg_outlet.push_sample(&samples.to_vec());
        }
        DataType::Ppg(samples) => {
          let _ = ppg_outlet.push_sample(&samples.to_vec());
        }
      }
    }
  }
}

#[napi(object)]
pub struct DeviceAdapterOptions {
  pub ble_uuid: Option<JsString>,
  pub rssi_interval_ms: Option<JsNumber>,
  /// If present, this will record the XDF to this path
  pub xdf_record_path: Option<JsString>,
}
