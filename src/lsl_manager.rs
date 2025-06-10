use lsl::{ChannelFormat, Pushable, StreamInfo, StreamOutlet};
use tokio::sync::mpsc;
use std::sync::mpsc as std_mpsc;
use crate::ble::DataType;

pub struct LslStreamManager {
    eeg_outlet: StreamOutlet,
    ppg_outlet: StreamOutlet,
}

impl LslStreamManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let eeg_outlet = Self::create_eeg_outlet()?;
        let ppg_outlet = Self::create_ppg_outlet()?;
        
        Ok(Self {
            eeg_outlet,
            ppg_outlet,
        })
    }

    fn create_eeg_outlet() -> Result<StreamOutlet, Box<dyn std::error::Error>> {
        // Create EEG StreamInfo with metadata
        let mut eeg_info = StreamInfo::new(
            "Muse S Gen 2 EEG",
            "EEG",
            5,     // 5 EEG channels
            256.0, // EEG sample rate
            ChannelFormat::Float32,
            "muse-eeg",
        )?;

        // Add EEG metadata
        eeg_info
            .desc()
            .append_child_value("manufacturer", "Interaxon");

        // Add EEG channel information
        let mut eeg_channels = eeg_info.desc().append_child("channels");
        let eeg_channel_names = ["EEG_TP9", "EEG_AF7", "EEG_AF8", "EEG_TP10", "EEG_AUX"];

        for channel_name in &eeg_channel_names {
            eeg_channels
                .append_child("channel")
                .append_child_value("label", channel_name)
                .append_child_value("unit", "microvolt")
                .append_child_value("type", "EEG");
        }

        // Add acquisition system metadata
        eeg_info
            .desc()
            .append_child("acquisition")
            .append_child_value("manufacturer", "Interaxon")
            .append_child_value("model", "Muse S Gen 2");

        Ok(StreamOutlet::new(&eeg_info, 12, 360)?)
    }

    fn create_ppg_outlet() -> Result<StreamOutlet, Box<dyn std::error::Error>> {
        // Create PPG StreamInfo with metadata
        let mut ppg_info = StreamInfo::new(
            "Muse S Gen 2 PPG",
            "PPG",
            3,    // 3 PPG channels
            64.0, // PPG sample rate
            ChannelFormat::Float32,
            "muse-s-ppg",
        )?;

        // Add PPG metadata
        ppg_info
            .desc()
            .append_child_value("manufacturer", "Interaxon");

        // Add PPG channel information
        let mut ppg_channels = ppg_info.desc().append_child("channels");
        let ppg_channel_names = ["PPG_AMBIENT", "PPG_INFRARED", "PPG_RED"];

        for channel_name in &ppg_channel_names {
            ppg_channels
                .append_child("channel")
                .append_child_value("label", channel_name)
                .append_child_value("unit", "N/A")
                .append_child_value("type", "PPG");
        }

        // Add acquisition system metadata
        ppg_info
            .desc()
            .append_child("acquisition")
            .append_child_value("manufacturer", "Interaxon")
            .append_child_value("model", "Muse S Gen 2");

        Ok(StreamOutlet::new(&ppg_info, 6, 360)?)
    }

    pub fn push_sample(&self, data_type: DataType) -> Result<(), Box<dyn std::error::Error>> {
        match data_type {
            DataType::Eeg(samples) => {
                self.eeg_outlet.push_sample(&samples.to_vec())?;
            }
            DataType::Ppg(samples) => {
                self.ppg_outlet.push_sample(&samples.to_vec())?;
            }
        }
        Ok(())
    }

    pub async fn process_data_stream(mut data_rx: mpsc::UnboundedReceiver<DataType>) {
        // Create the LSL manager
        let lsl_manager = match Self::new() {
            Ok(manager) => manager,
            Err(e) => {
                eprintln!("Failed to create LSL manager: {}", e);
                return;
            }
        };

        // Process incoming data
        while let Some(data_type) = data_rx.recv().await {
            if let Err(e) = lsl_manager.push_sample(data_type) {
                eprintln!("Failed to push LSL sample: {}", e);
            }
        }

        // Explicit cleanup happens automatically when lsl_manager is dropped
    }

    pub fn process_data_stream_blocking(mut data_rx: mpsc::UnboundedReceiver<DataType>) {
        // Create the LSL manager
        let lsl_manager = match Self::new() {
            Ok(manager) => manager,
            Err(e) => {
                eprintln!("Failed to create LSL manager: {}", e);
                return;
            }
        };

        // Use blocking_recv instead of async recv to avoid tokio runtime
        let rt = tokio::runtime::Handle::try_current();
        if rt.is_ok() {
            // We're in a tokio context, use async approach
            let rt = rt.unwrap();
            rt.block_on(async {
                while let Some(data_type) = data_rx.recv().await {
                    if let Err(e) = lsl_manager.push_sample(data_type) {
                        eprintln!("Failed to push LSL sample: {}", e);
                    }
                }
            });
        } else {
            // Convert to std channel for true blocking operation
            let (std_tx, std_rx) = std_mpsc::channel::<DataType>();
            
            // Spawn a small tokio runtime just to drain the async channel
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .expect("Failed to create drain runtime");
                
                rt.block_on(async {
                    while let Some(data_type) = data_rx.recv().await {
                        if std_tx.send(data_type).is_err() {
                            break; // Main thread dropped the receiver
                        }
                    }
                });
            });

            // Process data using blocking std channel
            while let Ok(data_type) = std_rx.recv() {
                if let Err(e) = lsl_manager.push_sample(data_type) {
                    eprintln!("Failed to push LSL sample: {}", e);
                }
            }
        }

        // Explicit cleanup happens automatically when lsl_manager is dropped
    }
}