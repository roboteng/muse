use napi_derive::napi;

pub struct InnerMuseDevice {}

#[napi]
pub struct MuseDevice {
  inner: InnerMuseDevice,
}

#[napi]
impl MuseDevice {
  #[napi(constructor)]
  pub fn new() -> Self {
    todo!()
  }

  #[napi]
  pub async fn connect(&self) -> napi::Result<()> {
    todo!()
  }

  #[napi]
  pub async fn start_streaming(&self, options: StreamOptions) -> napi::Result<()> {
    todo!()
  }

  #[napi]
  pub async fn stop_streaming(&self) -> napi::Result<()> {
    todo!()
  }
}

#[napi(object)]
pub struct StreamOptions {}
