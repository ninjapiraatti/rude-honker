# Rude Honker

A wireless omniwheel robot and its single-joystick controller, written entirely in `no_std` Rust for the ESP32-C6. The platform can drive forward/back, *strafe* sideways, and *spin in place*; the two boards find each other over ESP-NOW with no network or pairing.

---

## Overview

The setup is two boards talking to each other:

- **The controller** — an ESP32-C6 with a single analog joystick and a button.
- **The platform** — a second ESP32-C6 driving four omniwheels through two TB6612FNG motor drivers.

They talk over ESP-NOW, Espressif's connectionless radio protocol — no access point, no DHCP, no pairing. Power on both boards and they find each other.

Everything runs `no_std` on top of [`esp-hal`](https://github.com/esp-rs/esp-hal) 1.0 with the [Embassy](https://embassy.dev/) async executor. No RTOS, no heap-heavy runtime — just async tasks on a microcontroller.

## Repository layout

This is a Cargo workspace:

| Crate | What it is |
|-------|------------|
| `common/` | The wire protocol (`MessageType`, `MoveCommand`, `DriveMode`), shared by both firmwares so they can't disagree about the message format. |
| `controller/` | The joystick + button + radio. Reads the stick, toggles drive mode, broadcasts commands. |
| `omniwheels/` | The four-motor platform. Receives commands, mixes them into wheel speeds, drives the motors. |
| `robot-arm/` | A work-in-progress arm that reuses the same transport and message types. |

## Hardware

- 2 × ESP32-C6 dev boards (one controller, one platform)
- 2 × TB6612FNG dual motor drivers
- 4 × gear motors with omniwheels
- 1 × analog joystick module (2 axes + push button)
- Power: 4 × rechargeable AA (NiMH) for the motors, 1 × 18650 lithium cell for the platform's logic — see [Power](#power)

## Pin mapping

### Platform (`omniwheels`)

Motors are grouped as two chips (A, B), two motors each (`a1/a2`, `b1/b2`). Each motor needs two direction pins and one PWM pin; `STBY` is shared.

| Signal | Motor a1 | Motor a2 | Motor b1 | Motor b2 |
|--------|----------|----------|----------|----------|
| Dir 1  | GPIO23 | GPIO21 | GPIO18 | GPIO9 |
| Dir 2  | GPIO22 | GPIO20 | GPIO3  | GPIO13 |
| PWM    | GPIO15 | GPIO19 | GPIO2  | GPIO12 |

Shared driver standby: **GPIO11** (`STBY`, held low until init completes).

### Controller

| Signal | Pin |
|--------|-----|
| Joystick VRx (ADC) | GPIO3 |
| Joystick VRy (ADC) | GPIO4 |
| Joystick button    | GPIO5 (internal pull-up, active low) |
| Status LED         | GPIO8 |

## Power

Motors are electrically noisy — they pull big gulps of current on start/stall, and every H-bridge switch slams the supply rail. Sharing that rail with the MCU browns it out and resets it mid-drive. So the platform runs **two separate supplies with a common ground**:

- **Four AA (NiMH) cells for the motors** — ~4.8 V under load, cheap, rechargeable, and able to dump burst current without sagging too hard.
- **One 18650 for the ESP32-C6** — the logic supply stays isolated from the motor rail, so the C6 always sees a clean voltage. ~3.7 V nominal regulates cleanly to 3.3 V.

The one wire you must not forget is the **shared ground** between the two supplies; without it the MCU's logic levels float relative to the driver and it behaves unpredictably. The `STBY` pin is held low until the firmware finishes initialising, so the motors can't twitch on power-up.

## Building & flashing

Toolchain and target are pinned in `rust-toolchain.toml` (stable, `riscv32imac-unknown-none-elf`, `rust-src`). Flashing uses [`espflash`](https://crates.io/crates/espflash):

```sh
cargo install espflash
```

Each firmware builds and flashes from its own crate directory (the per-crate `.cargo/config.toml` sets the target, `build-std`, and the `espflash` runner). Plug in the target board over USB, then:

```sh
cd omniwheels && cargo run     # build + flash + serial monitor
cd controller && cargo run
```

> Build from inside a crate directory, not the workspace root — the target and `build-std` settings live in each crate's `.cargo/config.toml`, and Cargo only picks them up from the directory you build in.

## Controls

1. Power on both boards. The controller's LED blinks while it searches; it goes solid once the platform answers.
2. **Joystick** — forward/back always drives forward/back. Left/right depends on the mode.
3. **Joystick button** — toggles the drive mode:
   - **Strafe** — left/right slides the platform sideways without turning.
   - **Rotate** — left/right spins the platform in place.

## Protocol

Communication is ESP-NOW broadcast. A tiny handshake gives a sense of "connected": the controller broadcasts `Ping` and blinks; the platform replies `Pong`; the controller then starts sending movement.

```rust
#[repr(u8)]
pub enum MessageType { Ping = 0x01, Pong = 0x02, Move = 0x03 }
```

Movement is a hand-rolled 6-byte packet — `[Move, x_high, x_low, y_high, y_low, mode]`:

```rust
/// x, y are -100..100; mode is Strafe or Rotate
pub struct MoveCommand {
    pub x: i16,        // -100 (left) .. 100 (right)
    pub y: i16,        // -100 (back) .. 100 (forward)
    pub mode: DriveMode,
}
```

Both `to_bytes` and `from_bytes` live in `common`, so encoding and decoding can't drift apart. The parser also tolerates the older 5-byte packets (before the mode byte) by defaulting the mode.

## How it works — the interesting bits

### Four motors from one MCPWM

The C6's **MCPWM** peripheral doesn't work like most PWM APIs. Instead of "give me PWM on this pin," it's three layers you wire together: a **timer** (sets the frequency), an **operator** (sets the duty cycle), and the output **pins**. An operator does nothing until you bind it to a timer with `set_timer`.

The C6 has only one MCPWM, with three operators — which at first looks one short of four motors. The unlock: **each operator drives two pins** that share its timer but have independent duty cycles. So one operator runs two motors at the same frequency but different speeds, and four motors need only two operators (with a third to spare). The timer runs at 20 kHz (above audible, so no whine) with a period of 100 counts, giving a clean 1% duty resolution.

### Omniwheel mixing

Two drive modes, differing only in how `x` mixes into the wheels:

```rust
let (fl, fr, bl, br) = match mode {
    // Strafe: front-left/back-right pair against front-right/back-left
    DriveMode::Strafe => (y + x, y - x, y - x, y + x),
    // Rotate: the whole left side against the whole right side
    DriveMode::Rotate => (y - x, y + x, y - x, y + x),
};
```

### Calibrate the ADC

The joystick read up to +95 one way but only −43 the other. The culprit was the ESP32's raw ADC: it's nonlinear and has a zero-voltage offset, so the low end never reaches zero. The idiomatic esp-hal fix is `enable_pin_with_cal` with `AdcCalCurve`, which uses the chip's factory efuse data to return linearised **millivolts** instead of raw counts — symmetric readings, and the scaling code gets simpler.

### Debounce the button

Toggling drive modes *sometimes* left rotation spinning the wrong way. Not the mixing math — contact bounce. A single press registered as two toggles when the bounce straddled the 50 ms sample, landing in the opposite mode (and since the mixes differ only in the back-wheel signs, "opposite mode" looked exactly like "reversed rotation"). A time-based debounce lockout — ignore new edges for 250 ms after a toggle — fixed it.

### Broadcast is fire-and-forget

ESP-NOW broadcast has no ack, so a packet can vanish. For continuous stick data that's fine — another update is 50 ms away. But a mode switch is a one-shot event; if that packet drops, the platform keeps strafing while you wonder why it won't turn. The fix is to resend the mode change a few times instead of once — redundancy instead of guaranteed delivery.

## Roadmap

- **Robot arm** (`robot-arm/`) — reuse the same ESP-NOW transport and `common` message types to drive an arm alongside the wheels.
- Closed-loop control, telemetry back to the controller, battery monitoring.
