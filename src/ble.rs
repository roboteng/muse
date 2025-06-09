use btleplug::api::{Central, Characteristic, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral as PlatformPeripheral};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use uuid::{Uuid, uuid};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const MUSE_SERVICE_UUID: Uuid = uuid!("0000fe8d-0000-1000-8000-00805f9b34fb");
const CONTROL_CHAR_UUID: &str = "273e0001-4c4d-454d-96be-f03bac821358";

pub struct BleConnector<P: Peripheral> {
  adapter: Adapter,
  device: Option<P>,
  characteristics: Mutex<HashMap<Uuid, Characteristic>>,
}

impl BleConnector<PlatformPeripheral> {
  pub async fn new() -> Result<Self> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let adapter = adapters.into_iter().next().ok_or("No BLE adapter found")?;

    Ok(Self {
      adapter,
      device: None,
      characteristics: Mutex::new(HashMap::new()),
    })
  }

  pub async fn connect(&mut self, target_uuid: Option<String>) -> Result<(String, String)> {
    let service_uuid = MUSE_SERVICE_UUID;
    let filter = ScanFilter {
      services: vec![service_uuid],
    };

    self.adapter.start_scan(filter).await?;

    let device = timeout(Duration::from_secs(10), async {
      loop {
        let peripherals = self
          .adapter
          .peripherals()
          .await
          .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        for peripheral in peripherals {
          let properties = peripheral
            .properties()
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
          if let Some(props) = properties {
            if let Some(name) = &props.local_name {
              if name.contains("Muse") || name == "MuseS" {
                if let Some(target) = &target_uuid {
                  if peripheral.id().to_string() != *target {
                    continue;
                  }
                }
                return Ok::<PlatformPeripheral, Box<dyn std::error::Error + Send + Sync>>(
                  peripheral,
                );
              }
            }
          }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
      }
    })
    .await??;

    self.adapter.stop_scan().await?;

    device.connect().await?;
    device.discover_services().await?;

    let properties = device
      .properties()
      .await?
      .ok_or("Failed to get device properties")?;
    let device_name = properties
      .local_name
      .unwrap_or_else(|| "Unknown Muse".to_string());
    let device_uuid = device.id().to_string();

    self.device = Some(device);

    Ok((device_name, device_uuid))
  }

  pub async fn disconnect(&mut self) -> Result<()> {
    if let Some(device) = &self.device {
      device.disconnect().await?;
    }
    self.device = None;
    Ok(())
  }

  pub fn is_connected(&self) -> bool {
    self.device.is_some()
  }

  async fn get_characteristic(&self, uuid: &Uuid) -> Option<Characteristic> {
    self.characteristics.lock().await.get(uuid).cloned()
  }
}
