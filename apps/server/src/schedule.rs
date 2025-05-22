
// --- Temperature Constants for Schedules ---
const BEDROOM_TEMP_NIGHT: f64 = 19.3;
const BEDROOM_TEMP_DAY: f64 = 21.0;
const BEDROOM_TEMP_DAY_OFF: f64 = 12.0; // Power saving

// --- Interval Definitions ---
// Points are (hour_of_day, temperature_target)
// Hour_of_day is a float from 0.0 (midnight) to 24.0 (midnight next day).

const BEDROOM_INTERVALS: &[(f64, f64)] = &[
    (0.0,  BEDROOM_TEMP_DAY),     // Start of day temperature
    (3.0,  BEDROOM_TEMP_NIGHT),
    (5.0,  BEDROOM_TEMP_NIGHT),   // Maintain night temp
    (8.0,  BEDROOM_TEMP_DAY),     // Transition to day temp
    (9.5,  BEDROOM_TEMP_DAY),     // Maintain day temp
    (16.0, BEDROOM_TEMP_DAY_OFF), // Power saving period
    (23.0, BEDROOM_TEMP_DAY),     // Transition back to normal day temp before night
    (24.0, BEDROOM_TEMP_DAY)      // Ensures behavior up to midnight
];

// For Irina, the temperature is constant.
const IRINA_INTERVALS: &[(f64, f64)] = &[
    (0.0, 21.5), // Temp at 00:00 is 21.5. Any time after will take this value.
    // (24.0, 21.5) // Could add this for consistency, interpolate_fn_rust handles single point correctly.
];

const CHILDREN_TEMP_NIGHT: f64 = 18.3;
const CHILDREN_TEMP_DAY_OFF: f64 = 12.0;
const CHILDREN_TEMP_MORNING: f64 = 20.0;
const CHILDREN_TEMP_EVENING: f64 = 20.0;

const CHILDREN_INTERVALS: &[(f64, f64)] = &[
    (0.0,  CHILDREN_TEMP_EVENING),  // Start of day with previous evening's temp
    (2.5,  CHILDREN_TEMP_NIGHT),
    (4.5,  CHILDREN_TEMP_NIGHT),    // Maintain night temp
    (7.0,  CHILDREN_TEMP_MORNING),  // Transition to morning temp
    (9.0,  CHILDREN_TEMP_MORNING),  // Maintain morning temp
    (17.0, CHILDREN_TEMP_DAY_OFF),  // Power saving period
    (22.0, CHILDREN_TEMP_EVENING),  // Transition to evening temp
    (24.0, CHILDREN_TEMP_EVENING)   // Ensures behavior up to midnight
];

pub const INTERPOLATE_INTERVALS: [&[(f64, f64)]; 3] = [
    BEDROOM_INTERVALS,
    IRINA_INTERVALS,
    CHILDREN_INTERVALS,
];

