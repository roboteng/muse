use btleplug::api::{Central, Characteristic, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral as PlatformPeripheral};
use futures::stream::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::timeout;
use uuid::{Uuid, uuid};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const MUSE_SERVICE_UUID: Uuid = uuid!("0000fe8d-0000-1000-8000-00805f9b34fb");
// EEG Characteristic UUIDs
const EEG_TP9_UUID: Uuid = uuid!("273e0003-4c4d-454d-96be-f03bac821358");
const EEG_AF7_UUID: Uuid = uuid!("273e0004-4c4d-454d-96be-f03bac821358");
const EEG_AF8_UUID: Uuid = uuid!("273e0005-4c4d-454d-96be-f03bac821358");
const EEG_TP10_UUID: Uuid = uuid!("273e0006-4c4d-454d-96be-f03bac821358");
const EEG_AUX_UUID: Uuid = uuid!("273e0007-4c4d-454d-96be-f03bac821358");

// PPG Characteristic UUIDs  
const PPG_AMBIENT_UUID: Uuid = uuid!("273e000f-4c4d-454d-96be-f03bac821358");
const PPG_INFRARED_UUID: Uuid = uuid!("273e0010-4c4d-454d-96be-f03bac821358");
const PPG_RED_UUID: Uuid = uuid!("273e0011-4c4d-454d-96be-f03bac821358");

#[derive(Debug, Clone)]
pub enum DataType {
  Eeg([f32; 5]),  // 5 EEG channels: TP9, AF7, AF8, TP10, AUX
  Ppg([f32; 3]),  // 3 PPG channels: AMBIENT, INFRARED, RED
}

pub struct BleConnector<P: Peripheral> {
  adapter: Adapter,
  device: Option<P>,
  characteristics: Mutex<HashMap<Uuid, Characteristic>>,
  streaming: Arc<RwLock<bool>>,
  data_tx: Option<mpsc::UnboundedSender<DataType>>,
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
      streaming: Arc::new(RwLock::new(false)),
      data_tx: None,
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
    // Stop streaming first
    self.stop_streaming().await?;
    
    if let Some(device) = &self.device {
      device.disconnect().await?;
    }
    self.device = None;
    Ok(())
  }

  pub fn is_connected(&self) -> bool {
    self.device.is_some()
  }


  pub async fn start_streaming(&mut self, data_tx: mpsc::UnboundedSender<DataType>) -> Result<()> {
    if !self.is_connected() {
      return Err("Device not connected".into());
    }

    self.data_tx = Some(data_tx);
    
    // Discover and setup characteristics for notifications
    self.setup_notifications().await?;
    
    *self.streaming.write().await = true;
    Ok(())
  }

  pub async fn stop_streaming(&mut self) -> Result<()> {
    *self.streaming.write().await = false;
    
    // Stop notifications on all characteristics
    if let Some(device) = &self.device {
      let eeg_uuids = [EEG_TP9_UUID, EEG_AF7_UUID, EEG_AF8_UUID, EEG_TP10_UUID, EEG_AUX_UUID];
      let ppg_uuids = [PPG_AMBIENT_UUID, PPG_INFRARED_UUID, PPG_RED_UUID];
      
      for uuid in eeg_uuids.iter().chain(ppg_uuids.iter()) {
        if let Some(char) = self.get_characteristic(uuid).await {
          let _ = device.unsubscribe(&char).await; // Ignore errors
        }
      }
    }
    
    // Clear data sender
    self.data_tx = None;
    Ok(())
  }


  async fn setup_notifications(&mut self) -> Result<()> {
    let device = self.device.as_ref().ok_or("Device not connected")?;
    
    // Discover characteristics
    let eeg_uuids = [EEG_TP9_UUID, EEG_AF7_UUID, EEG_AF8_UUID, EEG_TP10_UUID, EEG_AUX_UUID];
    let ppg_uuids = [PPG_AMBIENT_UUID, PPG_INFRARED_UUID, PPG_RED_UUID];
    
    let mut chars = self.characteristics.lock().await;
    
    for service in device.services() {
      for char in service.characteristics {
        let char_uuid = char.uuid;
        if eeg_uuids.contains(&char_uuid) || ppg_uuids.contains(&char_uuid) {
          chars.insert(char_uuid, char.clone());
          
          // Subscribe to characteristic notifications
          device.subscribe(&char).await?;
        }
      }
    }
    
    // Start a task to read notifications and send them through the channel
    if let Some(data_tx) = &self.data_tx {
      let tx = data_tx.clone();
      let device_clone = device.clone();
      let streaming = self.streaming.clone();
      
      tokio::spawn(async move {
        let mut notifications = device_clone.notifications().await.unwrap();
        
        while let Some(notification) = notifications.next().await {
          let streaming_guard = streaming.read().await;
          if *streaming_guard {
            let char_uuid = notification.uuid;
            let data = notification.value;
            
            if let Ok(parsed_data) = parse_muse_data(&data) {
              let data_type = if eeg_uuids.contains(&char_uuid) {
                if let Ok(eeg_array) = parsed_data.get(0..5).unwrap_or(&[]).try_into() {
                  DataType::Eeg(eeg_array)
                } else {
                  continue; // Skip if not enough EEG data
                }
              } else if ppg_uuids.contains(&char_uuid) {
                if let Ok(ppg_array) = parsed_data.get(0..3).unwrap_or(&[]).try_into() {
                  DataType::Ppg(ppg_array)
                } else {
                  continue; // Skip if not enough PPG data
                }
              } else {
                continue;
              };
              
              let _ = tx.send(data_type);
            }
          }
        }
      });
    }
    
    Ok(())
  }

  async fn get_characteristic(&self, uuid: &Uuid) -> Option<Characteristic> {
    self.characteristics.lock().await.get(uuid).cloned()
  }
}

fn parse_muse_data(data: &[u8]) -> Result<Vec<f32>> {
  // Parse Muse data format (this is a simplified version)
  // The actual format may need adjustment based on Muse protocol
  let mut samples = Vec::new();
  
  for chunk in data.chunks(4) {
    if chunk.len() == 4 {
      let value = i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f32;
      samples.push(value);
    }
  }
  
  Ok(samples)
}
