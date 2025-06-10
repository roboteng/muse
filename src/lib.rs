use btleplug::platform::Peripheral as PlatformPeripheral;
use napi::{Env, JsBoolean, JsNumber, JsString, Result};
use napi_derive::napi;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, mpsc};

mod ble;
mod lsl_manager;
mod device_state;

use ble::{BleConnector, DataType};
use lsl_manager::LslStreamManager;
use device_state::DeviceStateManager;

// Global shared runtime for LSL operations to reduce thread creation
static SHARED_LSL_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

#[napi]
pub struct MuseDevice {
  connector: Arc<Mutex<Option<BleConnector<PlatformPeripheral>>>>,
  target_uuid: Option<String>,
  #[allow(dead_code)]
  rssi_interval_ms: Option<u32>,
  #[allow(dead_code)]
  xdf_record_path: Option<String>,
  state: Arc<Mutex<DeviceStateManager>>,
}

#[napi]
impl MuseDevice {
  #[napi(constructor)]
  pub fn new(options: DeviceAdapterOptions) -> Self {
    let target_uuid = options.ble_uuid.and_then(|js_str| {
      js_str
        .into_utf8()
        .ok()
        .and_then(|utf8| utf8.as_str().ok().map(|s| s.to_string()))
    });
    let rssi_interval_ms = options
      .rssi_interval_ms
      .and_then(|js_num| js_num.get_uint32().ok());
    let xdf_record_path = options.xdf_record_path.and_then(|js_str| {
      js_str
        .into_utf8()
        .ok()
        .and_then(|utf8| utf8.as_str().ok().map(|s| s.to_string()))
    });

    Self {
      connector: Arc::new(Mutex::new(None)),
      target_uuid,
      rssi_interval_ms,
      xdf_record_path,
      state: Arc::new(Mutex::new(DeviceStateManager::new())),
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

      // Update device state
      self.state.lock().await.set_connected(device_name, device_uuid);
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
      connector
        .start_streaming(data_tx)
        .await
        .map_err(|e| napi::Error::from_reason(format!("Failed to start streaming: {}", e)))?;

      // Use spawn_blocking with a shared runtime for LSL operations
      let _streaming_handle = tokio::task::spawn_blocking(move || {
        // Get or create the shared single-threaded runtime
        let rt = SHARED_LSL_RUNTIME.get_or_init(|| {
          tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create shared LSL runtime")
        });

        rt.block_on(async { LslStreamManager::process_data_stream(data_rx).await });
      });

      // Update streaming state
      self.state.lock().await.set_streaming_started()
        .map_err(|e| napi::Error::from_reason(e))?;
    } else {
      return Err(napi::Error::from_reason("Device not connected"));
    }

    Ok(())
  }

  #[napi]
  pub async fn stop_streaming(&self) -> napi::Result<()> {
    let mut connector_guard = self.connector.lock().await;

    if let Some(connector) = connector_guard.as_mut() {
      connector
        .stop_streaming()
        .await
        .map_err(|e| napi::Error::from_reason(format!("Failed to stop streaming: {}", e)))?;
    }

    // Note: Background streaming task will stop when data_rx channel closes
    // due to BLE connector cleanup above
    
    // Brief pause to allow LSL cleanup to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Update streaming state
    self.state.lock().await.set_streaming_stopped();

    Ok(())
  }

  #[napi]
  pub async fn restart_streaming(&self) -> napi::Result<()> {
    // Stop and restart without full disconnect to avoid thread churn
    self.stop_streaming().await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await; // Brief pause for cleanup
    self.start_streaming().await?;
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

    // Update device state
    self.state.lock().await.set_disconnected();

    Ok(())
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_name(&self, env: Env) -> Result<JsString> {
    let state = self.state.try_lock()
      .map_err(|_| napi::Error::from_reason("Failed to acquire state lock"))?;
    match state.get_device_name() {
      Some(name) => env.create_string(name),
      None => Err(napi::Error::from_reason("Device not connected")),
    }
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_uuid(&self, env: Env) -> Result<JsString> {
    let state = self.state.try_lock()
      .map_err(|_| napi::Error::from_reason("Failed to acquire state lock"))?;
    match state.get_device_uuid() {
      Some(uuid) => env.create_string(uuid),
      None => Err(napi::Error::from_reason("Device not connected")),
    }
  }

  #[napi(getter)]
  pub fn is_streaming(&self, env: Env) -> Result<JsBoolean> {
    let state = self.state.try_lock()
      .map_err(|_| napi::Error::from_reason("Failed to acquire state lock"))?;
    env.get_boolean(state.is_streaming())
  }

  #[napi(getter)]
  pub fn is_connected(&self, env: Env) -> Result<JsBoolean> {
    let state = self.state.try_lock()
      .map_err(|_| napi::Error::from_reason("Failed to acquire state lock"))?;
    env.get_boolean(state.is_connected())
  }

}

#[napi(object)]
pub struct DeviceAdapterOptions {
  pub ble_uuid: Option<JsString>,
  pub rssi_interval_ms: Option<JsNumber>,
  /// If present, this will record the XDF to this path
  pub xdf_record_path: Option<JsString>,
}
