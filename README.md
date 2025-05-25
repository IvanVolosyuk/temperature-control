# Temperature Control System

This project implements a server and related utilities to manage a fleet of custom temperature sensor and relay devices. The primary goal is to maintain configured temperature levels in different rooms. Communication with the devices is handled via a proprietary UDP-based protocol.

**Note:** The firmware and specific hardware details for the sensor/relay devices are not included in this repository. As such, the practical use of this project code is primarily for personal use by the author.

## Features

*   **Multi-Room Temperature Management:** Monitors and controls heating relays for multiple rooms.
*   **Web Interface:** A **React/Tailwind based** web server running on `http://localhost:8080` provides:
    *   Real-time temperature, target temperature, and relay status display.
    *   Historical temperature charts.
    *   Manual control to turn relays on/off.
    *   Ability to temporarily disable heating for a room.
*   **Console Logging:** The server application outputs detailed real-time logs of received messages and actions taken.
*   **Flexible Control Strategies:** Implements both simple threshold-based control and a PWM-like control strategy for more nuanced temperature regulation.
*   **Sensor Correction:** Applies configurable correction factors to raw temperature readings.
*   **Netdata Integration:** Outputs temperature, target temperature, and humidity data to files compatible with the Netdata monitoring agent (`/var/lib/temperature/`).
*   **UDP Protocol with Fragmentation:** Handles potentially large UDP messages through a custom fragmentation and reassembly layer.
*   **Device Logging Utility:** A separate `logger` application can be used to listen for and display diagnostic logs sent by the devices.
*   **Command-Line Utilities:** Includes tools for basic relay testing (`udp-test`) and controlling device logger settings (`enable`).

## Project Structure

The project is organized into a workspace with several crates:

*   `protocol/`:
    *   Defines the UDP communication protocol using Protocol Buffers (`.proto` files).
    *   Includes the `FragmentCombiner` logic for handling message fragmentation and reassembly.
    *   Contains code for serializing/deserializing messages and basic relay control commands.
*   `apps/server/`:
    *   The main temperature control server application.
    *   Handles incoming sensor data and relay reports.
    *   Implements control logic to manage heating relays.
    *   Serves the web interface (Axum-based).
*   `apps/logger/`:
    *   A command-line utility that listens for log messages broadcast by the devices and prints them to the console.
*   `apps/udp-test/`:
    *   A simple command-line tool for sending basic on/off commands to a relay device for testing purposes.
*   `apps/enable/`:
    *   A command-line utility to send control messages to the logger service on the devices (e.g., to enable/disable serial logging, store logs, or restart the device).
*   `temperature-react-ui/`:
    *   Contains the source code for the React/Tailwind frontend application.
    *   The build output from this directory (the `dist` folder) is served by the `temperature-server`.

## Getting Started

### Prerequisites

*   **Rust Toolchain:** Ensure you have Rust installed (visit [rust-lang.org](https://www.rust-lang.org/tools/install)).
*   **Node.js and Yarn (or npm):** Required for building and developing the React frontend. Visit [nodejs.org](https://nodejs.org/) to install Node.js (which includes npm). Yarn can be installed via npm: `npm install --global yarn`.
*   **Protocol Buffer Compiler (`protoc`):** The `protocol` crate's `build.rs` script uses `protobuf-codegen` which in turn requires the `protoc` compiler. Installation instructions can be found on the [Protocol Buffers documentation site](https://grpc.io/docs/protoc-installation/).

    It's often provided by a package named `protobuf-compiler`. For example:
    *   On systems using `apt` (like Debian/Ubuntu): `sudo apt install protobuf-compiler`
    *   On Gentoo: `emerge dev-libs/protobuf`

### Building the Project

#### Building the Backend Applications (Rust)

To build all backend applications in the workspace:

```bash
cargo build --all
```

To build a specific application, for example, the server:

```bash
cargo build -p temperature-server
```

#### Building the Frontend (React UI)

Before running the main server with the new UI, or after making changes to the UI, you need to build the React application:

```bash
cd temperature-react-ui
yarn install  # Or: npm install - if you prefer npm and have a package-lock.json
yarn build
```
This will create a `dist` directory inside `temperature-react-ui/` containing the static assets for the frontend.

### Testing the Project

To run all unit and integration tests in the workspace:

```bash
cargo test --all
```

To run tests for a specific package:

```bash
cargo test -p temperature-protocol
```

### Running the Applications

#### Temperature Server

To run the main temperature control server:

```bash
cargo run -p temperature-server # or cargo run --bin temperature-server
```

The server will start listening for UDP messages on `0.0.0.0:4000`.
The web interface, built from the `temperature-react-ui` project, will be served from its `dist` directory and available at `http://localhost:8080`.
**Note:** Ensure you have built the frontend application (`cd temperature-react-ui && yarn build`) before running the server if you want to use the web interface.

#### Device Logger

To run the device log monitoring utility:

```bash
cargo run -p temperature-logger
```

By default, it attempts to bind to `192.168.0.1:6001`. This might need adjustment in `apps/logger/src/main.rs` depending on your network configuration and where the devices are sending logs.

#### Frontend Development Server (React UI)

For developing the React UI with live reloading, you can run the Vite development server:

```bash
cd temperature-react-ui
yarn dev
```
This will typically start the frontend on a different port (e.g., `http://localhost:5173` - check your terminal output). API requests to `/api` will be proxied to the Rust backend running on `http://localhost:8080` (as configured in `temperature-react-ui/vite.config.ts`). This allows you to develop the UI independently while still communicating with the live backend.

## Usage

### Web Interface

Navigate to `http://localhost:8080` in your web browser. The interface displays:
*   Current temperature, target temperature, and heater status for each configured room.
*   Availability status for sensors and relays.
*   Graphs of temperature history.
*   Buttons to manually toggle relays or temporarily disable heating for a room.

### Console Output

The `temperature-server` application provides verbose logging to the console, showing:
*   Received sensor reports (temperature, humidity).
*   Calculated target temperatures.
*   Relay command decisions (ON/OFF, delays).
*   Confirmation status of relay commands.
*   Diagnostic messages.

### Command-Line Utilities

*   **`udp-test`**:
    *   Usage: `cargo run -p temperature-udp-test -- [host] (1|0)`
    *   Example: `cargo run -p temperature-udp-test -- esp8266-relay0.local 1` (turns relay on)
    *   If only host is provided, it will toggle the relay on then off.

*   **`enable`**:
    *   Usage: `cargo run -p temperature-enable -- [host] [(+|-)(store|send|serial|once|exp)] [restart]`
    *   Example: `cargo run -p temperature-enable -- esp8266-sensor0.local +serial -store restart`
    *   This utility modifies logging behavior on the target device.

## Configuration

Most of the core configuration is currently hardcoded within `apps/server/src/main.rs`:

*   **Relay Hostnames:** `RELAYS` constant.
*   **Temperature Corrections:** `CORRECTION` constant.
*   **Expected IP Addresses for Staleness Checks:** `BEDROOM_SENSOR_EXPECTED_IP`, etc.
*   **Netdata Path:** `NETDATA_PATH_PREFIX`.
*   **Temperature Schedules:** `INTERPOLATE_INTERVALS` in `apps/server/src/schedule.rs`.

To change these, you would need to modify the source code and recompile.

## Protocol Overview

The system uses a custom UDP-based protocol for communication between the server and the devices.
*   **Message Serialization:** Protocol Buffers are used to define and serialize message structures. The `.proto` definitions can be found in `protocol/src/protos/`.
*   **Fragmentation:** To handle messages larger than a single UDP packet, a fragmentation layer is implemented in `FragmentCombiner`. This layer prepends a small header to each fragment, allowing the receiver to reassemble the original message.
*   **Device-Side Implementation:** The code for the microcontrollers running on the sensor and relay devices is not part of this repository.

