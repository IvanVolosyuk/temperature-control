use std::cmp::{min, max};
use std::f64;

trait Control {
    fn get_mode(&mut self, temp: f64, target: f64, future_target: f64, current_time_ms: f64) -> (bool, u32);
    fn set_output(&mut self, on: bool, delay: u32, current_time_ms: f64);
}

struct SimpleControl {
    is_on: bool,
}

impl SimpleControl {
    fn new() -> Self {
        Self { is_on: false }
    }
}

impl Control for SimpleControl {
    fn get_mode(&mut self, temp: f64, target: f64, _future_target: f64, _current_time_ms: f64) -> (bool, u32) {
        let dt = temp - target;
        if dt > 0.1 {
            (false, 0)
        } else if dt < -0.1 {
            (true, 0)
        } else {
            (self.is_on, 0)
        }
    }

    fn set_output(&mut self, on: bool, _delay: u32, _current_time_ms: f64) {
        self.is_on = on;
    }
}

struct PWMControl {
    is_on: bool,
    is_on_time: f64,
    smooth_t: f64,
    initial_offset: f64,
    new_mode: bool,
    new_mode_time: f64,
    last_sensor_temp: f64,
}

impl PWMControl {
    fn new(initial_offset: f64) -> Self {
        Self {
            is_on: false,
            is_on_time: 0.0,
            smooth_t: -1.0,
            initial_offset,
            new_mode: true,
            new_mode_time: 0.0,
            last_sensor_temp: 0.0,
        }
    }

    fn get_avg_offset(&self) -> f64 {
        self.initial_offset
    }

    fn update_avg_offset(&mut self, above_target: f64, is_on: bool) {
        let mut offset = self.get_avg_offset();
        if (is_on && above_target > 0.0) || (!is_on && above_target < 0.0) {
            offset += above_target;
        }
        let avg_interval = 40.0;
        offset = ((avg_interval - 1.0) * self.initial_offset + offset) / avg_interval;
        self.initial_offset = offset.clamp(-0.7, 0.3);
    }
}

impl Control for PWMControl {
    fn get_mode(&mut self, temp: f64, target: f64, future_target: f64, current_time_ms: f64) -> (bool, u32) {
        self.last_sensor_temp = temp;

        if current_time_ms > self.new_mode_time {
            if self.is_on != self.new_mode {
                self.is_on_time = self.new_mode_time;
            }
            self.is_on = self.new_mode;
        }

        let t_s = 0.5;
        if self.smooth_t == -1.0 {
            self.smooth_t = temp;
        } else {
            self.smooth_t = self.smooth_t * t_s + temp * (1.0 - t_s);
        }

        let above_target = self.smooth_t - future_target;
        let offset = self.get_avg_offset();
        let dt = above_target + offset;
        print!("dt {:.2} offset={:.2} ", above_target, offset);

        if future_target == target {
            self.update_avg_offset(above_target, self.is_on);
        }

        if dt <= -0.9 && self.is_on {
            print!("[1:inf] ");
            return (true, 0);
        }
        if dt >= -0.1 && !self.is_on {
            print!("[0:inf] ");
            return (false, 0);
        }

        let minutes = (current_time_ms - self.is_on_time) / 60_000.0;
        let pulse_width = if self.is_on {
            dt * -10.0
        } else {
            (1.0 + dt) * 10.0
        };

        if minutes < pulse_width {
            print!("[{}:{:.1} vs {:.1}] ", self.is_on as u8, minutes, pulse_width);
            (!self.is_on, ((pulse_width - minutes) * 60000.0) as u32)
        } else {
            print!("[{}!]", (!self.is_on) as u8);
            (!self.is_on, 0)
        }
    }

    fn set_output(&mut self, on: bool, delay: u32, current_time_ms: f64) {
        if delay < 60_000 {
            self.new_mode = on;
            self.new_mode_time = current_time_ms + delay as f64;
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::pwm::Control;
    use crate::pwm::PWMControl;

    struct Room {
        heater_t: f64,
        room_t: f64,
        sensor_room_t: f64,
        window_t: f64,
    }

    impl Room {
        fn new() -> Self {
            Self {
                heater_t: 17.0,
                room_t: 17.0,
                sensor_room_t: 17.0,
                window_t: 16.0,
            }
        }

        fn update(&mut self, mode: bool, ms: f64) {
            if mode {
                self.heater_t += 0.7 / 60000.0 * ms;
            }
            self.balance();
        }

        fn get_sensor_t_raw(&self) -> f64 {
            self.sensor_room_t
        }

        fn get_sensor_t(&self) -> f64 {
            (self.get_sensor_t_raw() * 10.0).round() / 10.0
        }

        fn balance(&mut self) {
            let mut window = self.window_t;
            Self::exchange(&mut self.heater_t, 0.5, &mut self.room_t, 1.0, 0.03);
            Self::exchange(&mut window, 1.0, &mut self.sensor_room_t, 1.0, 0.03);
            Self::exchange(&mut self.room_t, 1.0, &mut self.sensor_room_t, 1.0, 0.04);
        }

        fn exchange(t1: &mut f64, weight1: f64, t2: &mut f64, weight2: f64, speed: f64) {
            let energy1 = *t1 * weight1;
            let energy2 = *t2 * weight2;
            let exchanged = (*t1 - *t2) * speed;
            *t1 = (energy1 - exchanged) / weight1;
            *t2 = (energy2 + exchanged) / weight2;
        }
    }

    struct TargetGen {
        t: f64,
        step: f64,
        phase: usize,
        index: usize,
    }

    impl TargetGen {
        fn new() -> Self {
            Self {
                t: 19.0,
                step: 1.7 / 120.0,
                phase: 0,
                index: 0,
            }
        }
    }

    impl Iterator for TargetGen {
        type Item = f64;

        fn next(&mut self) -> Option<Self::Item> {
            if self.phase > 4 {
                return None;
            }

            let result = self.t;

            match self.phase {
                1 => self.t -= self.step,
                3 => self.t += self.step,
                _ => {}
            }

            self.index += 1;
            if self.index >= 120 {
                self.index = 0;
                self.phase += 1;
            }

            Some(result)
        }
    }

    #[test]
    fn integration_test() {
        let mut room = Room::new();
        let mut pwm = PWMControl::new(-0.57);
        let mut time = 10_000_000.0;
        let mut mode = true;
        let mut req_mode = true;
        let mut req_time = 0.0;

        let mut total_samples = 0.0;
        let mut total_error = 0.0;

        for curr_target in TargetGen::new() {
            print!("target {:.1} ", curr_target);
            let old_time = time;
            time += 60000.0;

            if req_time >= old_time && req_time < time {
                let before = req_time - old_time;
                let after = time - req_time;
                room.update(mode, before);
                mode = req_mode;
                room.update(mode, after);
            } else {
                room.update(mode, time - old_time);
            }

            let (new_mode, delay_ms) = pwm.get_mode(room.get_sensor_t(), curr_target, curr_target, time);
            pwm.set_output(new_mode, delay_ms, time);
            req_mode = new_mode;
            req_time = time + delay_ms as f64;

            total_samples += 1.0;
            total_error += (room.get_sensor_t_raw() - curr_target).abs();

            println!(
                "t={:.2} [{}] -> [{},{:.1}m]",
                room.get_sensor_t(),
                mode as u8,
                req_mode as u8,
                delay_ms as f64 / 60000.0
            );
        }

        let avg_err = total_error / total_samples;
        println!("Avg error {:.4}", avg_err);
        assert!(avg_err > 0.23 && avg_err < 0.26);
    }
}
