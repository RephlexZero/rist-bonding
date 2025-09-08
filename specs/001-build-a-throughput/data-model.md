# Data Model (Phase 1)

## Entities

### BondingTransmitter
- id: string
- target_bitrate_kbps: u32
- session_state: enum { Idle, Connecting, Streaming, Degraded, Recovering }
- active_links: [LinkId]
- metrics: AggregatedMetrics

### NetworkLink
- id: string (e.g., modem/SIM/cell descriptor)
- status: enum { Up, Down, Degraded }
- throughput_kbps: u32 (EWMA)
- rtt_ms: u32 (EWMA)
- jitter_ms: u32 (EWMA)
- loss_rate: f32 (0..1)
- data_cap_mb: Option<u32>
- data_used_mb: u32
- cost_class: enum { Low, Medium, High }

### BondingSession
- id: string
- start_time: timestamp
- target_bitrate_kbps: u32
- current_bitrate_kbps: u32
- health: enum { Healthy, AtRisk, Unhealthy }
- events: [Event]

### ReceiverForwarder
- id: string
- destinations: [Destination]
- output_bitrate_kbps: u32
- status: enum { Idle, Streaming, Error }

### Destination
- id: string
- name: string
- status: enum { Active, Error }
- last_error: Option<string>

### AggregatedMetrics
- bitrate_kbps: u32
- link_count: u8
- loss_rate: f32
- latency_ms: u32

### Event
- time: timestamp
- severity: enum { Info, Warn, Error }
- message: string

## Relationships
- BondingTransmitter has many NetworkLinks
- BondingSession references BondingTransmitter and ReceiverForwarder
- ReceiverForwarder has many Destinations

## Validation Rules
- target_bitrate_kbps > 0
- data_used_mb <= data_cap_mb when cap is set
- At least one NetworkLink must be Up to enter Streaming

## State Transitions
- Idle → Connecting → Streaming
- Streaming → Degraded when aggregate < target for sustained window
- Degraded → Streaming upon recovery
- Any → Recovering on receiver outage
