use napi::{JsBoolean, JsNumber, JsString, Result};
use napi_derive::napi;

#[napi]
pub struct MuseDevice {}

#[napi]
impl MuseDevice {
  #[napi(constructor)]
  pub fn new(_options: DeviceAdapterOptions) -> Self {
    todo!()
  }

  #[napi]
  pub async fn connect(&self) -> napi::Result<()> {
    todo!()
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
    todo!()
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_name(&self) -> Result<JsString> {
    todo!()
  }

  /// @throws if its not connected
  #[napi(getter)]
  pub fn ble_uuid(&self) -> Result<JsString> {
    todo!()
  }

  #[napi(getter)]
  pub fn is_streaming(&self) -> JsBoolean {
    todo!()
  }

  #[napi(getter)]
  pub fn is_connected(&self) -> JsBoolean {
    todo!()
  }
}

#[napi(object)]
pub struct DeviceAdapterOptions {
  pub ble_uuid: Option<JsString>,
  pub rssi_interval_ms: Option<JsNumber>,
  /// If present, this will record the XDF to this path
  pub xdf_record_path: Option<JsString>,
}
