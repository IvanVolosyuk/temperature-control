Port c++ implementation of the server, keep stdout output as close as possible. The rust set_relay() is pretty basic and server specific logging should be moved to server.

Here is the code I have so far, which works, but missing business logic:

pub mod pwm;

use anyhow::Result;
use std::collections::HashMap;
use temperature_protocol::fragment_combiner::FragmentCombiner;
use temperature_protocol::fragment_combiner::MessageHandler;
use temperature_protocol::protos::generated::dev::DeviceMessage;

struct Server {
    _hosts: HashMap<std::net::SocketAddr, i64>,
}

impl Server {
    fn new() -> Server {
        Server {
            _hosts: HashMap::new(),
        }
    }
}

impl MessageHandler<DeviceMessage> for Server {
    fn on_message(
        &mut self,
        _src: std::net::SocketAddr,
        _msg: DeviceMessage,
    ) -> anyhow::Result<()> {
        // TODO: need to port the handler
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut server = Server::new();
    FragmentCombiner::new(&mut server).main_loop("0.0.0.0:4000")
}

Use following interfaces:
Available in pwm.rs in the current create:

trait Control {
    fn get_mode(&mut self, temp: f64, target: f64, future_target: f64, current_time: DateTime<Local>) -> (bool, u32);
    fn set_output(&mut self, on: bool, delay: u32, current_time: DateTime<Local>);
}

struct SimpleControl { /*...*/ }

impl SimpleControl {
    fn new() -> Self {/*...*/ }
}

struct PWMControl { /*...*/ }

impl PWMControl {
    fn new(initial_offset: f64) -> Self {/*...*/ }
    /*...*/
}

Protobuf message shared by the implementations dev.proto :

syntax = "proto2";

enum MsgType {
  M_DEBUG = 0;
  M_WARN = 1;
  M_ERROR = 2;
};

message LogMsg {
  optional MsgType type = 1;
  optional uint64 ts = 2;
  optional string text = 3;
}

message LoggerProto {
  repeated LogMsg record = 1;
  optional uint64 last_ts = 2;
  optional uint64 current_ts = 3;
}

message LoggerControl {
  optional bool log_to_serial = 1;
  optional bool store_log = 2;
  optional bool send_once = 3;
  optional bool device_restart = 4;
  optional bool send_log = 5;

  // Enable experimental code
  optional bool experiment = 6;
};

enum RelayState {
  OFF = 0;
  ON = 1;
};

message RelayControl {
  optional RelayState state = 1;
  // Add something to make non-zero packet size.
  optional bool dummy = 2;
  // Delay state change
  optional uint32 delay = 3;
};

enum SensorError {
  S_TIMEOUT_LOW_PULSE = 150;
  S_TIMEOUT_HIGH_PULSE = 151;
  S_TIME_PULSE = 152;
  S_CHECKSUM = 153;
  S_BUTTON_EVENT = 160;
};

enum ButtonState {
  B_OFF = 0;
  B_FORCE_ON = 1;
};

message DeviceInfo {
  optional uint32 id = 1;
  optional bool started = 2;
  optional uint32 offline_sec = 3;
};

message RelayReport {
  optional DeviceInfo info = 1;
  optional bool relay_status = 2;
};

message SensorReport {
  optional DeviceInfo info = 10;
  optional int32 temperature_deci = 2;
  optional uint32 humidity_deci = 3;
  optional SensorError sensor_error = 4;
  optional ButtonState button = 5;
  reserved 1, 6 to 9;
};

message DeviceMessage {
  optional SensorReport sensor = 1;
  optional RelayReport relay = 2;
  optional bool format_diag = 3;
  optional bool heat_on = 4;
};

You can use:
use temperature_protocol::relay::set_relay;
Only function available there:
pub fn set_relay(addr: &str, on: bool, delay: u32) -> Result<()> {
    let udp = UdpSocket::bind("0.0.0.0:0")?;
    let mut msg: RelayControl = RelayControl::new();
    msg.set_dummy(true);
    msg.set_state(if on { RelayState::ON } else { RelayState::OFF });
    msg.set_delay(delay);
    let out_bytes: Vec<u8> = msg.write_to_bytes()?;
    udp.send_to(&out_bytes, addr.to_owned() + ":4210")?;
    Ok(())
}

Here is c++ business logic implementation to port:

#include <arpa/inet.h>
#include <fcntl.h>
#include <memory>
#include <netdb.h>
#include <netinet/in.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>
#include <absl/strings/str_format.h>

#include "lib.hpp"
#include "pwm.hpp"
#include "udp_listener.hpp"

#define PORT 4000
#define MAXLINE 1024

const char *relays[3] = {
    // bedroom (original relay)
    "esp8266-relay0.local",  // 192.168.0.210
    // irina (v2)
    "esp8266-relay1.local",  // 192.168.0.211
    // kids room (v3)
    "esp8266-relay2.local",  // 192.168.0.212
};

float correction[3] = {
    -0.0,
    -0.9,
    -0.6,
};

std::unique_ptr<Control> control[3] = {
    std::make_unique<PWMControl>(-0.36),
    std::make_unique<SimpleControl>(),
    std::make_unique<PWMControl>(-0.36),
};

class ReportCombiner : public FragmentCombiner<DeviceMessage, MAXLINE> {
private:
  enum HeaderStatus {
    FAILURE,
    OK,
    HAS_STATUS_UPDATE,
  };

public:
  ReportCombiner(int sockfd) : relay_(sockfd) {}
  HeaderStatus printHeader(const char *client_address, const DeviceInfo &info);
  void newSensorReport(const char *client_address, const SensorReport &r);
  void newRelayReport(const char *client_address, const RelayReport &r);
  void formatDiag(const char *client_address, int port);
  void forceChildrenHeatOn(const char *client_address, int port);
  void newMessage(const char *client_address, int port, const DeviceMessage &dm) override;

  Relay &relay() { return relay_; }

private:
  Relay relay_;
  std::map<std::string, time_t> last_message;
  std::map<int, double> last_temp;
  std::map<std::string, bool> last_on;
  time_t heat_on_ = 0;
};

ReportCombiner::HeaderStatus
ReportCombiner::printHeader(const char *client_address,
                            const DeviceInfo &info) {
  if (!info.has_id()) {
    printf("Message without id from %s\n", client_address);
    return FAILURE;
  }
  HeaderStatus status = OK;

  time_t current_time = time(nullptr);
  char *c_time_string = ctime(&current_time);
  c_time_string[strlen(c_time_string) - 1] = 0;

  uint32_t id = info.id();
  printf("%s [%d]: ", c_time_string, id);

  if (info.started()) {
    printf("(STARTED) ");
    status = HAS_STATUS_UPDATE;
  }
  if (info.has_offline_sec()) {
    printf("(OFFLINE %.2fm) ", info.offline_sec() * (1. / 60));
    status = HAS_STATUS_UPDATE;
  }

  return status;
}

void ReportCombiner::newRelayReport(const char *client_address,
                                    const RelayReport &s) {
  last_message[client_address] = time(nullptr);

  auto hs = printHeader(client_address, s.info());
  if (hs == FAILURE)
    return;
  last_on[client_address] = s.relay_status();
  relay().relay_confirmation(client_address, s.relay_status());
  printf("Relay: %s%c", s.relay_status() ? "ON" : "OFF",
         hs == HAS_STATUS_UPDATE ? '\n' : '\r');
  fflush(stdout);
}

void ReportCombiner::newSensorReport(const char *client_address,
                                     const SensorReport &s) {
  if (printHeader(client_address, s.info()) == FAILURE)
    return;

  uint32_t id = s.info().id();
  float temp = s.temperature_deci() * 0.1f;
  float humidity = s.humidity_deci() * 0.1f;
  if (s.has_sensor_error()) {
    printf("(%s) ", SensorError_Name(s.sensor_error()).c_str());
  } else {
    printf("t=%0.1f h=%0.1f ", temp, humidity);
  }

  if (!s.has_temperature_deci()) {
    printf("\n");
    return;
  }

  time_t current_time = time(nullptr);
  last_message[client_address] = time(nullptr);

  float target = (id >= 0 && id < 3) ? interpolate[id](current_time) : temp;
  if (id == 2 && current_time < heat_on_ + 3600) {
    printf("(FORCE_TARGET) ");
    target = 21.5;
  }

  if (id >= 0 || id < 3) {
    temp += correction[id];

    printf("%.1f (target %.1f) ", temp, target);
    last_temp[id] = temp;
    bool mode;
    uint32_t delay;
    std::tie(mode, delay) =
        control[id]->getMode(temp, target, interpolate[id](current_time + 10 * 60));
    control[id]->setOutput(mode, delay);
    relay_.set_relay(relays[id], mode, delay);
  } else {
    printf("%.1f ", temp);
  }

  // Reporting for custom netdata collector
  char tmpfile[1024], temperature_file[1024], humidity_file[1024];
  snprintf(tmpfile, 1024, "/var/lib/temperature/new%d", id);
  snprintf(temperature_file, 1024, "/var/lib/temperature/current%d", id);
  snprintf(humidity_file, 1024, "/var/lib/temperature/humidity%d", id);
  int fd = open(tmpfile, O_WRONLY | O_CREAT | O_TRUNC, 0666);
  if (fd != -1) {
    dprintf(fd,
            "SET temperature = %.0f\n"
            "SET target = %.0f\n",
            temp * 10, target * 10);
    close(fd);
  }
  rename(tmpfile, temperature_file);
  fd = open(tmpfile, O_WRONLY | O_CREAT | O_TRUNC, 0666);
  if (fd != -1) {
    dprintf(fd, "SET humidity = %.0f\n", humidity * 10);
    close(fd);
  }
  rename(tmpfile, humidity_file);
  printf("\n");
}

void ReportCombiner::formatDiag(const char *client_address, int client_port) {
  printf("Diag request from %s at %d\n", client_address, client_port);
  char buf[1024];
  snprintf(buf, 1024, "Temp0: %0.1f%s, Temp2: %0.1f%s",
                                    last_temp[0], last_on["192.168.0.210"] ? " [ON]" : "",
                                    last_temp[2], last_on["192.168.0.212"] ? " [ON]" : "");
  std::string out = buf;

  time_t now = time(nullptr);
  // Start warning when no reply from devices after 3 minutes
  if (now - last_message["192.168.0.200"] > 180) { out += "\nFAIL: Bedroom sensor"; }
  else if (now - last_message["192.168.0.210"] > 180) { out += "\nFAIL: Bedroom relay"; }
  if (now - last_message["192.168.0.202"] > 180) { out += "\nFAIL: Kids sensor"; }
  else if (now - last_message["192.168.0.212"] > 180) { out += "\nFAIL: Kids relay"; }

  if (!relay_.send_message(client_address, client_port, out)) {
    printf(" [NDIAG] ");
  }
}

void ReportCombiner::newMessage(const char *client_address, int port,
                                const DeviceMessage &dm) {
  // std::cout << dm.DebugString();
  // std::cout << " from " << client_address << std::endl;
  // std::cout.flush();

  if (dm.has_sensor()) {
    newSensorReport(client_address, dm.sensor());
    return;
  }

  if (dm.has_relay()) {
    newRelayReport(client_address, dm.relay());
    return;
  }

  if (dm.format_diag()) {
    formatDiag(client_address, port);
    return;
  }

  printf("Unknown message type from %s\n", client_address);
}

class TemperatureListener : public UdpListener {
public:
  TemperatureListener() : UdpListener(PORT) {
    combiner_ = std::make_unique<ReportCombiner>(sockfd());
  }

protected:
  void onPacket(const char *client_address, int port, const uint8_t *buffer,
                size_t size) override;

private:
  std::unique_ptr<ReportCombiner> combiner_;
};

void TemperatureListener::onPacket(const char *client_address, int client_port,
                                   const uint8_t *buffer, size_t size) {
  // Legacy sensor
  if (size == 1) {
    DeviceMessage dm;
    dm.mutable_relay();
    ;
    // printf("Confirmation from %d: %s\n", buffer[0], client_address);
    combiner_->newMessage(client_address, client_port, dm);
    return;
  }
  if (size == 3) {
    // printf("Keep alive packet on=%d reconnects=%d recvs=%d from=%s\n",
    //     buffer[0], buffer[1], buffer[2], client_address);
    return;
  }

  if (buffer[0] != FRAG_MAGIC) {
    printf("Bad packet sz=%ld from %s\n", size, client_address);
    return;
  }

  combiner_->addFragment(client_address, client_port, buffer, size);
}

int main() {
  TemperatureListener l;
  l.start();
  return 0;
}


Relay::Relay() {
  // Creating socket file descriptor
  if ( (sockfd_ = socket(AF_INET, SOCK_DGRAM, 0)) < 0 ) {
    perror("relay socket creation failed");
    exit(EXIT_FAILURE);
  }
}

// The messaging should be moved to caller.
void Relay::set_relay(const char* addr, std::optional<bool> on, uint32_t delay) {
  if (delay == 0) {
    printf("%s", on.has_value() ? (*on ? "ON" : "OFF") : "");
  }

  // Filling server information
  RelayControl control;
  control.set_dummy(false);
  if (on.has_value()) control.set_state(*on ? ON : OFF);
  if (delay != 0) control.set_delay(delay);

  std::string serialized;
  control.SerializeToString(&serialized);
  auto resolved_addr = send_message(addr, 4210, serialized);
  if (!resolved_addr) {
    printf(" [NRELAY] ");
    return;
  }

  if (relay_state_[*resolved_addr].unconfirmed) {
    printf(" [UNCONFIRMED] ");
  } else if (delay != 0) {
    printf(" %s", relay_state_[*resolved_addr].on ? "*ON" : "*OFF");
  }
  if (delay != 0) {
    printf(" (%.1fm->%s)", delay / 60'000.,
        on.has_value() ? (*on ? "ON" : "OFF") : "");
  }
  relay_state_[*resolved_addr].unconfirmed = true;
}

// This logic was added by Relay interface by mistake. It should be handled in server instead.
// set_relay()
void Relay::relay_confirmation(const char* addr, bool on) {
  relay_state_[addr] = {.unconfirmed = false, .on = on};
}

// This logic was added by Relay interface by mistake. It should be handled in server instead.
std::optional<std::string> Relay::send_message(
    const char* addr, int port, const std::string& message) const {
  struct sockaddr_in relayaddr;
  memset(&relayaddr, 0, sizeof(relayaddr));

  struct hostent *h = gethostbyname(addr);
  if (!h) {
    return std::nullopt;
  }

  // Filling server information
  relayaddr.sin_family    = AF_INET; // IPv4
  char* resolved_addr = inet_ntoa(*(struct in_addr *)h->h_addr_list[0]);
  relayaddr.sin_addr.s_addr = inet_addr(resolved_addr);
  relayaddr.sin_port = htons(port);

  sendto(sockfd_, message.data(), message.size(),
      MSG_WAITALL, (const struct sockaddr *) &relayaddr,
      sizeof(relayaddr));
  return resolved_addr;
}

Requested temperature curves with gradual cooling at night and day. Some old values commented out:

float linear(float min, float max, float start, float end, float hour) {
  float progress = (hour - start) / (end - start);
  assert(progress >= 0 && progress <= 1);
  return max * progress + min * (1-progress);
}

#define NEXT(h, t) \
  if (hour < h) return linear(prev_t, t, prev_h, h, hour); \
  prev_h = h; \
  prev_t = t;


float interpolate_bedroom(time_t t) {
  const float NIGHT = 19.3;
  const float DAY = 21.;
  const float DAY_OFF = 12;

  struct tm tm;
  localtime_r(&t, &tm);
  float hour = tm.tm_hour + (1./60.) * (tm.tm_min + (1./60.) * tm.tm_sec);
  float prev_h = 0;
  float prev_t = DAY;
  //NEXT(1, MID);
  NEXT(3, NIGHT);

  // saturday, school
  //NEXT(6, 18.3);
  //NEXT(7, 19);
  //NEXT(8, 21);

  // weekday
  NEXT(5, NIGHT);
  //NEXT(7.5, MID);
  NEXT(8, DAY);
  NEXT(9.5, DAY);
  //NEXT(12, 19.0); // test
  NEXT(16, DAY_OFF); // power saving
  //NEXT(18, 19.0); // test
  //NEXT(20, DAY);
  NEXT(23, DAY);
  NEXT(24, DAY);
  return DAY;
}

#define IRINA_NIGHT 17.5
//#define IRINA_MID 17.5
#define IRINA_DAY 19

float interpolate_irina(time_t t) {
  //struct tm tm;
  //localtime_r(&t, &tm);
  //float hour = tm.tm_hour + (1./60.) * (tm.tm_min + (1./60.) * tm.tm_sec);
  //float prev_h = 0;
  //float prev_t = 21;
  //NEXT(24, 21);

  return 21.5;
}


float interpolate_children(time_t t) {
  //const float CHILDREN_NIGHT = 17.5;
  //const float CHILDREN_DAY = 19;
  //const float CHILDREN_MORNING = 20;
  const float CHILDREN_NIGHT = 18.3;
  const float CHILDREN_DAY = 19.5;
  const float CHILDREN_DAY_OFF = 12;
  const float CHILDREN_MORNING = 20;
  const float CHILDREN_EVENING = 20;

  struct tm tm;
  localtime_r(&t, &tm);
  float hour = tm.tm_hour + (1./60.) * (tm.tm_min + (1./60.) * tm.tm_sec);
  float prev_h = 0;
  float prev_t = CHILDREN_EVENING;
  //NEXT(1, CHILDREN_MID);
  NEXT(2.5, CHILDREN_NIGHT);

  // saturday, school
  //NEXT(6, 18.3);
  //NEXT(7, 19);
  //NEXT(8, 20);

  // weekday
  NEXT(4.5, CHILDREN_NIGHT);
  //NEXT(7.5, CHILDREN_MID);
  NEXT(7, CHILDREN_MORNING);
  NEXT(9, CHILDREN_MORNING);
  NEXT(17, CHILDREN_DAY_OFF);
  NEXT(22, CHILDREN_EVENING);
  return CHILDREN_EVENING;
}

float (*interpolate[3])(time_t t) = {
  interpolate_bedroom,
  interpolate_irina,
  interpolate_children,
};
