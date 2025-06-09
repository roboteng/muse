use btleplug::api::{Central, Characteristic, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral as PlatformPeripheral};
use futures::stream::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::time::timeout;
use uuid::{Uuid, uuid};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const MUSE_SERVICE_UUID: Uuid = uuid!("0000fe8d-0000-1000-8000-00805f9b34fb");

// Control Characteristic UUID
const CONTROL_UUID: Uuid = uuid!("273e0001-4c4d-454d-96be-f03bac821358");

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
  Eeg([f32; 5]), // 5 EEG channels: TP9, AF7, AF8, TP10, AUX
  Ppg([f32; 3]), // 3 PPG channels: AMBIENT, INFRARED, RED
}

// Data structures for chunking like TypeScript implementation
const EEG_CHUNK_SIZE: usize = 12;
const PPG_CHUNK_SIZE: usize = 6;
const EEG_CHANNEL_COUNT: usize = 5;
const PPG_CHANNEL_COUNT: usize = 3;

#[derive(Clone)]
struct ChannelChunks {
  eeg_chunks: [[u8; EEG_CHUNK_SIZE]; EEG_CHANNEL_COUNT], // [channel_count][chunk_size]
  ppg_chunks: [[f32; PPG_CHUNK_SIZE]; PPG_CHANNEL_COUNT], // [channel_count][chunk_size]
}

impl ChannelChunks {
  fn new() -> Self {
    Self {
      eeg_chunks: [[0u8; EEG_CHUNK_SIZE]; EEG_CHANNEL_COUNT],
      ppg_chunks: [[0.0f32; PPG_CHUNK_SIZE]; PPG_CHANNEL_COUNT],
    }
  }

  fn reset_eeg(&mut self) {
    for chunk in &mut self.eeg_chunks {
      chunk.fill(0);
    }
  }

  fn reset_ppg(&mut self) {
    for chunk in &mut self.ppg_chunks {
      chunk.fill(0.0);
    }
  }
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

    // Send device control commands like TypeScript implementation
    for command in &["h", "p50", "s", "d"] {
      self.send_control_command(command.as_bytes()).await?;
    }

    *self.streaming.write().await = true;
    Ok(())
  }

  pub async fn stop_streaming(&mut self) -> Result<()> {
    // Send halt command like TypeScript implementation
    self.send_control_command("h".as_bytes()).await?;

    *self.streaming.write().await = false;

    // Stop notifications on all characteristics
    if let Some(device) = &self.device {
      let eeg_uuids = [
        EEG_TP9_UUID,
        EEG_AF7_UUID,
        EEG_AF8_UUID,
        EEG_TP10_UUID,
        EEG_AUX_UUID,
      ];
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

  async fn send_control_command(&self, cmd: &[u8]) -> Result<()> {
    let device = self.device.as_ref().ok_or("Device not connected")?;
    let control_char = self
      .get_characteristic(&CONTROL_UUID)
      .await
      .ok_or("Control characteristic not found")?;

    // Create command buffer like TypeScript implementation: X{cmd}\n
    let mut buffer = Vec::with_capacity(cmd.len() + 2);
    buffer.push(b'X');
    buffer.extend_from_slice(cmd);
    buffer.push(b'\n');

    // Set first byte to length - 1 (like TypeScript encoded[0] = encoded.length - 1)
    buffer[0] = (buffer.len() - 1) as u8;

    device
      .write(
        &control_char,
        &buffer,
        btleplug::api::WriteType::WithoutResponse,
      )
      .await?;
    Ok(())
  }

  async fn setup_notifications(&mut self) -> Result<()> {
    let device = self.device.as_ref().ok_or("Device not connected")?;

    // Discover characteristics
    let eeg_uuids = [
      EEG_TP9_UUID,
      EEG_AF7_UUID,
      EEG_AF8_UUID,
      EEG_TP10_UUID,
      EEG_AUX_UUID,
    ];
    let ppg_uuids = [PPG_AMBIENT_UUID, PPG_INFRARED_UUID, PPG_RED_UUID];

    let mut chars = self.characteristics.lock().await;

    for service in device.services() {
      for char in service.characteristics {
        let char_uuid = char.uuid;
        if eeg_uuids.contains(&char_uuid) || ppg_uuids.contains(&char_uuid) {
          chars.insert(char_uuid, char.clone());

          // Subscribe to characteristic notifications
          device.subscribe(&char).await?;
        } else if char_uuid == CONTROL_UUID {
          // Store control characteristic for sending commands
          chars.insert(char_uuid, char.clone());
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
        let mut chunks = ChannelChunks::new();

        while let Some(notification) = notifications.next().await {
          let streaming_guard = streaming.read().await;
          if *streaming_guard {
            let char_uuid = notification.uuid;
            let data = notification.value;

            if eeg_uuids.contains(&char_uuid) {
              // Handle EEG data - parse as raw bytes for chunking
              if let Ok(channel_values) = parse_eeg_data(&data) {
                let channel_idx = eeg_uuids
                  .iter()
                  .position(|&uuid| uuid == char_uuid)
                  .unwrap();

                // Store chunk data for this channel
                if channel_values.len() >= EEG_CHUNK_SIZE {
                  chunks.eeg_chunks[channel_idx].copy_from_slice(&channel_values[..EEG_CHUNK_SIZE]);
                }

                // Check if this is the last channel (AUX = index 4)
                if channel_idx == 4 {
                  // Push all samples for this chunk
                  for sample_idx in 0..EEG_CHUNK_SIZE {
                    let sample: [f32; 5] = [
                      chunks.eeg_chunks[0][sample_idx] as f32,
                      chunks.eeg_chunks[1][sample_idx] as f32,
                      chunks.eeg_chunks[2][sample_idx] as f32,
                      chunks.eeg_chunks[3][sample_idx] as f32,
                      chunks.eeg_chunks[4][sample_idx] as f32,
                    ];
                    let _ = tx.send(DataType::Eeg(sample));
                  }
                  chunks.reset_eeg();
                }
              }
            } else if ppg_uuids.contains(&char_uuid) {
              // Handle PPG data - decode 24-bit values
              if let Ok(decoded_values) = parse_ppg_data(&data) {
                let channel_idx = ppg_uuids
                  .iter()
                  .position(|&uuid| uuid == char_uuid)
                  .unwrap();

                // Store chunk data for this channel
                if decoded_values.len() >= PPG_CHUNK_SIZE {
                  chunks.ppg_chunks[channel_idx].copy_from_slice(&decoded_values[..PPG_CHUNK_SIZE]);
                }

                // Check if this is the last channel (RED = index 2)
                if channel_idx == 2 {
                  // Push all samples for this chunk
                  for sample_idx in 0..PPG_CHUNK_SIZE {
                    let sample: [f32; 3] = [
                      chunks.ppg_chunks[0][sample_idx],
                      chunks.ppg_chunks[1][sample_idx],
                      chunks.ppg_chunks[2][sample_idx],
                    ];
                    let _ = tx.send(DataType::Ppg(sample));
                  }
                  chunks.reset_ppg();
                }
              }
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

fn parse_eeg_data(data: &[u8]) -> Result<Vec<u8>> {
  // EEG data: slice from index 2 like TypeScript (Array.from(data).slice(2))
  if data.len() < 2 {
    return Err("EEG data too short".into());
  }
  Ok(data[2..].to_vec())
}

fn parse_ppg_data(data: &[u8]) -> Result<Vec<f32>> {
  // PPG data: slice from index 2, then decode as 24-bit unsigned integers
  if data.len() < 2 {
    return Err("PPG data too short".into());
  }
  let channel_values = &data[2..];
  decode_unsigned_24_bit_data(channel_values)
}

fn decode_unsigned_24_bit_data(samples: &[u8]) -> Result<Vec<f32>> {
  let mut decoded_samples = Vec::new();
  let num_bytes_per_sample = 3;

  for chunk in samples.chunks(num_bytes_per_sample) {
    if chunk.len() == num_bytes_per_sample {
      let most_significant_byte = (chunk[0] as u32) << 16;
      let middle_byte = (chunk[1] as u32) << 8;
      let least_significant_byte = chunk[2] as u32;

      let val = most_significant_byte | middle_byte | least_significant_byte;
      decoded_samples.push(val as f32);
    }
  }

  Ok(decoded_samples)
}
