use chrono::{DateTime, Local, Duration, TimeZone};
use std::f64;

pub trait Control: Send + Clone {
    fn get_mode(
        &mut self,
        current_temp: f64,
        target_temp: f64,
        future_target_temp: f64,
        current_time: DateTime<Local>,
    ) -> (bool, u32);
    fn set_output(&mut self, mode_on: bool, delay_ms: u32, current_time: DateTime<Local>);
}

#[derive(Clone)]
pub struct SimpleControl {
    is_on: bool,
    last_on_time: Option<DateTime<Local>>,
    last_off_time: Option<DateTime<Local>>,
}

impl SimpleControl {
    pub fn new() -> Self {
        Self { is_on: false, last_on_time: None, last_off_time: None }
    }
}

impl Control for SimpleControl {
    fn get_mode(&mut self, temp: f64, target: f64, _future_target: f64, _current_time: DateTime<Local>) -> (bool, u32) {
        let dt = temp - target;
        if dt > 0.1 {
            (false, 0)
        } else if dt < -0.1 {
            (true, 0)
        } else {
            (self.is_on, 0)
        }
    }

    fn set_output(&mut self, on: bool, _delay: u32, _current_time: DateTime<Local>) {
        self.is_on = on;
        if on {
            self.last_on_time = Some(Local::now());
        } else {
            self.last_off_time = Some(Local::now());
        }
    }
}

#[derive(Clone)]
pub struct PWMControl {
    is_on: bool,
    is_on_time: DateTime<Local>,
    smooth_t: f64,
    initial_offset: f64,
    new_mode: bool,
    new_mode_time: DateTime<Local>,
    last_sensor_temp: f64,
    correction: f64,
    last_on_time: Option<DateTime<Local>>,
    last_off_time: Option<DateTime<Local>>,
}

impl PWMControl {
    pub fn new(initial_offset: f64) -> Self {
        let epoch_time = Local.timestamp_opt(0, 0).unwrap();
        Self {
            is_on: false,
            is_on_time: epoch_time,
            smooth_t: -1.0,
            initial_offset,
            new_mode: true,
            new_mode_time: epoch_time,
            last_sensor_temp: 0.0,
            correction: 0.0,
            last_on_time: None,
            last_off_time: None,
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
    fn get_mode(&mut self, temp: f64, target: f64, future_target: f64, current_time: DateTime<Local>) -> (bool, u32) {
        self.last_sensor_temp = temp;

        if current_time >= self.new_mode_time {
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

        let duration_on_state = current_time.signed_duration_since(self.is_on_time);
        let minutes = duration_on_state.num_milliseconds() as f64 / 60_000.0;

        let pulse_width = if self.is_on {
            dt * -10.0
        } else {
            (1.0 + dt) * 10.0
        };

        if minutes < pulse_width {
            print!("[{}:{:.1} vs {:.1}] ", self.is_on as u8, minutes, pulse_width);
            (!self.is_on, ((pulse_width - minutes) * 60000.0).max(0.0) as u32)
        } else {
            print!("[{}!]", (!self.is_on) as u8);
            (!self.is_on, 0)
        }
    }

    fn set_output(&mut self, on: bool, delay: u32, current_time: DateTime<Local>) {
        if delay < 60_000 {
            self.new_mode = on;
            self.new_mode_time = current_time + Duration::milliseconds(delay as i64);
        }
        if on {
            self.last_on_time = Some(current_time);
        } else {
            self.last_off_time = Some(current_time);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, TimeZone};


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

        let initial_timestamp_ms = 10_000_000i64;
        let mut current_time: DateTime<Local> = Local.timestamp_millis_opt(initial_timestamp_ms).unwrap();

        // Actual current mode of the heater
        let mut mode = true;
        // Mode requested by PWM for next state
        let mut req_mode = true;
        // Initialize req_time to a time before current_time to ensure first pwm.set_output call is effective
        let mut req_time: DateTime<Local> = Local.timestamp_millis_opt(0).unwrap();


        let mut total_samples = 0.0;
        let mut total_error = 0.0;

        for curr_target in TargetGen::new() {
            print!("target {:.1} ", curr_target);
            let old_time = current_time;
            current_time = current_time + Duration::milliseconds(60_000);

            // Check if the requested mode change falls within the current time step
            if req_time >= old_time && req_time < current_time {
                let before_ms = req_time.signed_duration_since(old_time).num_milliseconds() as f64;
                let after_ms = current_time.signed_duration_since(req_time).num_milliseconds() as f64;

                // Update room with old mode until req_time
                room.update(mode, before_ms);
                mode = req_mode;
                // Update room with new mode for the rest of the step
                room.update(mode, after_ms);
            } else {
                // No mode change in this step, or req_time is outside this step
                room.update(mode, current_time.signed_duration_since(old_time).num_milliseconds() as f64);
            }

            let (new_mode, delay_ms_u32) = pwm.get_mode(room.get_sensor_t(), curr_target, curr_target, current_time);
            pwm.set_output(new_mode, delay_ms_u32, current_time);

            req_mode = new_mode;
            req_time = current_time + Duration::milliseconds(delay_ms_u32 as i64);

            total_samples += 1.0;
            total_error += (room.get_sensor_t_raw() - curr_target).abs();

            println!(
                "t={:.2} [{}] -> [{},{:.1}m]",
                room.get_sensor_t(),
                mode as u8,
                req_mode as u8,
                delay_ms_u32 as f64 / 60000.0
            );
        }

        let avg_err = total_error / total_samples;
        assert!(avg_err > 0.22 && avg_err < 0.25, "Average error: {:.4}", avg_err);
    }
}
