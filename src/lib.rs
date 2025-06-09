use btleplug::platform::Peripheral as PlatformPeripheral;
use napi::{Env, JsBoolean, JsNumber, JsString, Result};
use napi_derive::napi;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

mod ble;
use ble::BleConnector;

#[napi]
pub struct MuseDevice {
  connector: Arc<Mutex<Option<BleConnector<PlatformPeripheral>>>>,
  target_uuid: Option<String>,
  rssi_interval_ms: Option<u32>,
  xdf_record_path: Option<String>,
  connected: Arc<RwLock<bool>>,
  device_name: Arc<RwLock<Option<String>>>,
  device_uuid: Arc<RwLock<Option<String>>>,
}

#[napi]
impl MuseDevice {
  #[napi(constructor)]
  pub fn new(options: DeviceAdapterOptions) -> Self {
    let target_uuid = options
      .ble_uuid
      .map(|js_str| js_str.into_utf8().unwrap().as_str().unwrap().to_string());
    let rssi_interval_ms = options
      .rssi_interval_ms
      .map(|js_num| js_num.get_uint32().unwrap());
    let xdf_record_path = options
      .xdf_record_path
      .map(|js_str| js_str.into_utf8().unwrap().as_str().unwrap().to_string());

    Self {
      connector: Arc::new(Mutex::new(None)),
      target_uuid,
      rssi_interval_ms,
      xdf_record_path,
      connected: Arc::new(RwLock::new(false)),
      device_name: Arc::new(RwLock::new(None)),
      device_uuid: Arc::new(RwLock::new(None)),
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

      *self.connected.write().unwrap() = true;
      *self.device_name.write().unwrap() = Some(device_name);
      *self.device_uuid.write().unwrap() = Some(device_uuid);
    }

    Ok(())
  }

  #[napi]
  pub async fn start_streaming(&self) -> napi::Result<()> {
    todo!()
  }

  #[napi]
  pub async fn stop_streaming(&self) -> napi::Result<()> {
    todo!()
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

    *self.connected.write().unwrap() = false;
    *self.device_name.write().unwrap() = None;
    *self.device_uuid.write().unwrap() = None;

    Ok(())
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_name(&self, env: Env) -> Result<JsString> {
    let name_guard = self.device_name.read().unwrap();
    match name_guard.as_ref() {
      Some(name) => env.create_string(name),
      None => Err(napi::Error::from_reason("Device not connected")),
    }
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_uuid(&self, env: Env) -> Result<JsString> {
    let uuid_guard = self.device_uuid.read().unwrap();
    match uuid_guard.as_ref() {
      Some(uuid) => env.create_string(uuid),
      None => Err(napi::Error::from_reason("Device not connected")),
    }
  }

  #[napi(getter)]
  pub fn is_streaming(&self, env: Env) -> JsBoolean {
    env.get_boolean(false).unwrap()
  }

  #[napi(getter)]
  pub fn is_connected(&self, env: Env) -> JsBoolean {
    let connected = *self.connected.read().unwrap();
    env.get_boolean(connected).unwrap()
  }
}

#[napi(object)]
pub struct DeviceAdapterOptions {
  pub ble_uuid: Option<JsString>,
  pub rssi_interval_ms: Option<JsNumber>,
  /// If present, this will record the XDF to this path
  pub xdf_record_path: Option<JsString>,
}
