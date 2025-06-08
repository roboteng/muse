
### Useful Constants

```typescript
class MuseDeviceStreamer {
    private readonly bleLocalName = 'MuseS'

    private readonly eegCharNames = MuseDeviceStreamer.eegCharacteristicNames
    private readonly eegChunkSize = MuseDeviceStreamer.eegChunkSize
    private readonly eegNumChannels = this.eegCharNames.length

    private readonly ppgCharNames = MuseDeviceStreamer.ppgCharacteristicNames
    private readonly ppgChunkSize = MuseDeviceStreamer.ppgChunkSize
    private readonly ppgNumChannels = this.ppgCharNames.length

    private readonly eegCharUuids = this.eegCharNames.map(
        (name) => CHAR_UUIDS[name]
    )

    private readonly ppgCharUuids = this.ppgCharNames.map(
        (name) => CHAR_UUIDS[name]
    )

    private static readonly eegChunkSize = 12
    private static readonly eegSampleRate = 256
    private static readonly ppgChunkSize = 6
    private static readonly ppgSampleRate = 64

    private static readonly eegCharacteristicNames = [
        'EEG_TP9',
        'EEG_AF7',
        'EEG_AF8',
        'EEG_TP10',
        'EEG_AUX',
    ]

    private static readonly ppgCharacteristicNames = [
        'PPG_AMBIENT',
        'PPG_INFRARED',
        'PPG_RED',
    ]

    private static readonly eegOutletOptions = {
        name: 'Muse S Gen 2 EEG',
        type: 'EEG',
        channelNames: this.eegCharacteristicNames,
        sampleRate: this.eegSampleRate,
        channelFormat: 'float32' as ChannelFormat,
        sourceId: 'muse-eeg',
        manufacturer: 'Interaxon Inc.',
        unit: 'microvolt',
        chunkSize: this.eegChunkSize,
        maxBuffered: 360,
    }

    private static readonly ppgOutletOptions = {
        name: 'Muse S Gen 2 PPG',
        type: 'PPG',
        channelNames: this.ppgCharacteristicNames,
        sampleRate: this.ppgSampleRate,
        channelFormat: 'float32' as ChannelFormat,
        sourceId: 'muse-s-ppg',
        manufacturer: 'Interaxon Inc.',
        unit: 'N/A',
        chunkSize: this.ppgChunkSize,
        maxBuffered: 360,
    }
}
```
