import test from 'ava'

import { MuseDevice } from '../index.js'

test('MuseDevice creation', (t) => {
  const device = new MuseDevice({})
  t.truthy(device)
  t.is(device.isConnected, false)
  t.is(device.isStreaming, false)
})

test('MuseDevice with options', (t) => {
  const device = new MuseDevice({
    bleUuid: 'test-uuid-123',
    rssiIntervalMs: 1000,
    xdfRecordPath: '/tmp/test.xdf'
  })
  t.truthy(device)
  t.is(device.isConnected, false)
})

test('MuseDevice getters throw when not connected', (t) => {
  const device = new MuseDevice({})

  t.throws(() => device.bleName, { message: 'Device not connected' })
  t.throws(() => device.bleUuid, { message: 'Device not connected' })
})

test('MuseDevice connect attempt (will fail without real device)', async (t) => {
  const device = new MuseDevice({})

  await device.connect()
  t.is(device.isConnected, true)
  await new Promise(resolve => setTimeout(resolve, 5000))
  t.truthy(device.bleName)
  t.truthy(device.bleUuid)
  await device.disconnect()
  t.is(device.isConnected, false)
}, 20000)
