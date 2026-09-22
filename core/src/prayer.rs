use chrono::{NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

/// Where on earth. Latitude north-positive, longitude east-positive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self { latitude, longitude }
    }

    /// Whether these are coordinates at all, as opposed to the zeroes a
    /// settings file starts with.
    pub fn is_set(&self) -> bool {
        (-90.0..=90.0).contains(&self.latitude)
            && (-180.0..=180.0).contains(&self.longitude)
            && (self.latitude != 0.0 || self.longitude != 0.0)
    }
}

/// Which convention to follow. These differ only in the angles they place
/// Fajr and Isha at, except Umm al-Qura, which sets Isha by the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// Muslim World League — 18° and 17°.
    #[default]
    MuslimWorldLeague,
    /// Islamic Society of North America — 15° and 15°.
    Isna,
    /// Egyptian General Authority of Survey — 19.5° and 17.5°.
    Egyptian,
    /// Umm al-Qura, Makkah — 18.5°, and Isha 90 minutes after Maghrib.
    UmmAlQura,
    /// University of Islamic Sciences, Karachi — 18° and 18°.
    Karachi,
    /// Institute of Geophysics, Tehran — 17.7° and 14°.
    Tehran,
    /// Shia Ithna Ashari — 16° and 14°.
    Jafari,
}

impl Method {
    pub fn all() -> [Method; 7] {
        [
            Method::MuslimWorldLeague,
            Method::Isna,
            Method::Egyptian,
            Method::UmmAlQura,
            Method::Karachi,
            Method::Tehran,
            Method::Jafari,
        ]
    }

    pub fn key(self) -> &'static str {
        match self {
            Method::MuslimWorldLeague => "muslim-world-league",
            Method::Isna => "isna",
            Method::Egyptian => "egyptian",
            Method::UmmAlQura => "umm-al-qura",
            Method::Karachi => "karachi",
            Method::Tehran => "tehran",
            Method::Jafari => "jafari",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Method::MuslimWorldLeague => "Muslim World League",
            Method::Isna => "ISNA (North America)",
            Method::Egyptian => "Egyptian General Authority",
            Method::UmmAlQura => "Umm al-Qura (Makkah)",
            Method::Karachi => "Karachi",
            Method::Tehran => "Tehran",
            Method::Jafari => "Jafari (Shia)",
        }
    }

    pub fn parse(text: &str) -> Self {
        Method::all().into_iter().find(|m| m.key() == text).unwrap_or_default()
    }

    /// Degrees below the horizon for Fajr.
    fn fajr_angle(self) -> f64 {
        match self {
            Method::MuslimWorldLeague => 18.0,
            Method::Isna => 15.0,
            Method::Egyptian => 19.5,
            Method::UmmAlQura => 18.5,
            Method::Karachi => 18.0,
            Method::Tehran => 17.7,
            Method::Jafari => 16.0,
        }
    }

    /// How Isha is defined: an angle, or a wait after Maghrib.
    fn isha(self) -> Isha {
        match self {
            Method::MuslimWorldLeague => Isha::Angle(17.0),
            Method::Isna => Isha::Angle(15.0),
            Method::Egyptian => Isha::Angle(17.5),
            Method::UmmAlQura => Isha::AfterMaghrib(90),
            Method::Karachi => Isha::Angle(18.0),
            Method::Tehran => Isha::Angle(14.0),
            Method::Jafari => Isha::Angle(14.0),
        }
    }

    /// Maghrib is sunset for most, and a small angle for two.
    fn maghrib_angle(self) -> f64 {
        match self {
            Method::Tehran => 4.5,
            Method::Jafari => 4.0,
            _ => SUNSET_ANGLE,
        }
    }
}

enum Isha {
    Angle(f64),
    AfterMaghrib(i64),
}

/// Which shadow length marks Asr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Asr {
    /// Shafi'i, Maliki, Hanbali — shadow equal to the object.
    #[default]
    Standard,
    /// Hanafi — shadow twice the object.
    Hanafi,
}

impl Asr {
    fn shadow_factor(self) -> f64 {
        match self {
            Asr::Standard => 1.0,
            Asr::Hanafi => 2.0,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Asr::Standard => "standard",
            Asr::Hanafi => "hanafi",
        }
    }

    pub fn parse(text: &str) -> Self {
        if text == "hanafi" {
            Asr::Hanafi
        } else {
            Asr::Standard
        }
    }
}

/// The sun's altitude at sunrise and sunset: half a degree for the disc, and a
/// third for atmospheric refraction.
const SUNSET_ANGLE: f64 = 0.833;

/// One day's times. Any of them can be absent: far enough north or south, the
/// sun does not reach the angle Fajr or Isha is defined by, and inventing a
/// time would be worse than saying so.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Times {
    /// The sun a chosen angle below the horizon before dawn. Which angle is
    /// the only thing the calculation methods really disagree about.
    pub fajr: Option<NaiveTime>,
    /// The sun at -0.833°, allowing for refraction and the disc's radius.
    pub sunrise: Option<NaiveTime>,
    /// Solar noon, corrected for longitude and the equation of time.
    pub dhuhr: Option<NaiveTime>,
    /// When an object's shadow is its own length past its noon shadow, or
    /// twice that in the Hanafi reckoning.
    pub asr: Option<NaiveTime>,
    /// Sunset, and for two methods a small angle past it.
    pub maghrib: Option<NaiveTime>,
    /// The sun a chosen angle below the horizon after dusk, or a fixed wait
    /// after Maghrib.
    pub isha: Option<NaiveTime>,
}

/// The five prayers plus sunrise, in the order a day goes.
pub const NAMES: [&str; 6] = ["Fajr", "Sunrise", "Dhuhr", "Asr", "Maghrib", "Isha"];

impl Times {
    /// Paired with [`NAMES`], for a client that wants to loop.
    pub fn in_order(&self) -> [Option<NaiveTime>; 6] {
        [self.fajr, self.sunrise, self.dhuhr, self.asr, self.maghrib, self.isha]
    }

    /// The next prayer at or after `now`, as an index into [`NAMES`].
    ///
    /// Sunrise is skipped: it marks the end of Fajr rather than a prayer, and
    /// counting down to it would say the wrong thing.
    pub fn next_after(&self, now: NaiveTime) -> Option<usize> {
        self.in_order()
            .into_iter()
            .enumerate()
            .filter(|(index, _)| *index != 1)
            .find_map(|(index, time)| time.filter(|t| *t >= now).map(|_| index))
    }
}

/// How the whole calculation is configured.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Params {
    pub method: Method,
    pub asr: Asr,
}

/// The day's times at `location`, for a clock `utc_offset` hours ahead of UTC.
///
/// The offset is supplied rather than looked up for the same reason the rest
/// of the core takes naive times: a timezone database is the shell's problem,
/// and this crate stays free of one.
pub fn times(date: NaiveDate, location: Location, params: Params, utc_offset: f64) -> Times {
    let declination = sun_declination(date);
    let noon = solar_noon(date, location.longitude, utc_offset);
    let latitude = location.latitude;

    // Fajr and Isha sit either side of noon by the same kind of hour angle,
    // negative before and positive after.
    let fajr =
        hour_angle(-params.method.fajr_angle(), latitude, declination).map(|hours| noon - hours);
    let sunrise = hour_angle(-SUNSET_ANGLE, latitude, declination).map(|hours| noon - hours);
    let maghrib =
        hour_angle(-params.method.maghrib_angle(), latitude, declination).map(|hours| noon + hours);

    let isha = match params.method.isha() {
        Isha::Angle(angle) => hour_angle(-angle, latitude, declination).map(|hours| noon + hours),
        // Umm al-Qura waits a fixed time after sunset rather than naming an
        // angle, which is why it keeps working at latitudes where the angle
        // methods run out.
        Isha::AfterMaghrib(minutes) => maghrib.map(|m| m + minutes as f64 / 60.0),
    };

    // Asr is the altitude at which a shadow has grown by the object's own
    // length (or twice it), on top of whatever shadow it casts at noon.
    let shadow = params.asr.shadow_factor();
    let altitude = (1.0 / (shadow + (latitude - declination).abs().to_radians().tan())).atan();
    let asr = hour_angle(altitude.to_degrees(), latitude, declination).map(|hours| noon + hours);

    Times {
        fajr: to_time(fajr),
        sunrise: to_time(sunrise),
        dhuhr: to_time(Some(noon)),
        asr: to_time(asr),
        maghrib: to_time(maghrib),
        isha: to_time(isha),
    }
}

/// Days since the start of 2000, the epoch the solar formulas below use.
fn days_since_epoch(date: NaiveDate) -> f64 {
    let epoch = NaiveDate::from_ymd_opt(2000, 1, 1).expect("2000-01-01 is a date");
    // Noon rather than midnight: the sun's position is wanted for the middle
    // of the day, and using midnight biases every time by half a day's drift.
    (date - epoch).num_days() as f64 + 0.5
}

/// The sun's declination — how far north or south it stands — in degrees.
fn sun_declination(date: NaiveDate) -> f64 {
    let d = days_since_epoch(date);
    let mean_anomaly = (357.529 + 0.98560028 * d).rem_euclid(360.0);
    let mean_longitude = (280.459 + 0.98564736 * d).rem_euclid(360.0);
    let ecliptic_longitude = (mean_longitude
        + 1.915 * mean_anomaly.to_radians().sin()
        + 0.020 * (2.0 * mean_anomaly).to_radians().sin())
    .rem_euclid(360.0);
    let obliquity = 23.439 - 0.00000036 * d;

    (obliquity.to_radians().sin() * ecliptic_longitude.to_radians().sin()).asin().to_degrees()
}

/// The difference between clock noon and the sun actually being highest, in
/// hours — the equation of time.
fn equation_of_time(date: NaiveDate) -> f64 {
    let d = days_since_epoch(date);
    let mean_anomaly = (357.529 + 0.98560028 * d).rem_euclid(360.0);
    let mean_longitude = (280.459 + 0.98564736 * d).rem_euclid(360.0);
    let ecliptic_longitude = (mean_longitude
        + 1.915 * mean_anomaly.to_radians().sin()
        + 0.020 * (2.0 * mean_anomaly).to_radians().sin())
    .rem_euclid(360.0);
    let obliquity = 23.439 - 0.00000036 * d;

    let right_ascension = (obliquity.to_radians().cos() * ecliptic_longitude.to_radians().sin())
        .atan2(ecliptic_longitude.to_radians().cos())
        .to_degrees()
        .rem_euclid(360.0);

    // In hours, and wrapped: the two can be on opposite sides of 360°.
    let difference = (mean_longitude - right_ascension + 180.0).rem_euclid(360.0) - 180.0;
    difference / 15.0
}

/// Local clock time, in hours, at which the sun is highest.
fn solar_noon(date: NaiveDate, longitude: f64, utc_offset: f64) -> f64 {
    12.0 + utc_offset - longitude / 15.0 - equation_of_time(date)
}

/// Hours between solar noon and the sun standing at `altitude` degrees.
///
/// `None` when the sun never reaches that altitude on that day, which is what
/// happens to Fajr and Isha in a northern summer. Returning nothing is the
/// honest answer; the client says so rather than showing a made-up time.
fn hour_angle(altitude: f64, latitude: f64, declination: f64) -> Option<f64> {
    let numerator =
        altitude.to_radians().sin() - latitude.to_radians().sin() * declination.to_radians().sin();
    let denominator = latitude.to_radians().cos() * declination.to_radians().cos();
    if denominator == 0.0 {
        return None;
    }
    let cosine = numerator / denominator;
    if !(-1.0..=1.0).contains(&cosine) {
        return None;
    }
    Some(cosine.acos().to_degrees() / 15.0)
}

/// Hours since local midnight to a clock time, wrapping across the day.
fn to_time(hours: Option<f64>) -> Option<NaiveTime> {
    let hours = hours?;
    if !hours.is_finite() {
        return None;
    }
    let wrapped = hours.rem_euclid(24.0);
    // Rounded to the minute, which is the precision anyone acts on, and the
    // precision every published table is given in.
    let minutes = (wrapped * 60.0).round() as u32 % (24 * 60);
    NaiveTime::from_hms_opt(minutes / 60, minutes % 60, 0)
}

/// "5:12 AM", matching how the rest of the app writes a time.
pub fn format(time: NaiveTime) -> String {
    let (pm, hour) = time.hour12();
    format!("{hour}:{:02} {}", time.minute(), if pm { "PM" } else { "AM" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn minutes(time: NaiveTime) -> i64 {
        time.hour() as i64 * 60 + time.minute() as i64
    }

    const MAKKAH: Location = Location { latitude: 21.4225, longitude: 39.8262 };
    const LONDON: Location = Location { latitude: 51.5074, longitude: -0.1278 };
    const CAIRO: Location = Location { latitude: 30.0444, longitude: 31.2357 };

    /// The order is the day. Nothing else about the arithmetic matters if the
    /// times come out shuffled.
    #[test]
    fn the_prayers_fall_in_order() {
        for month in 1..=12 {
            let times = times(date(2026, month, 15), MAKKAH, Params::default(), 3.0);
            let all: Vec<i64> = times.in_order().into_iter().flatten().map(minutes).collect();
            assert_eq!(all.len(), 6, "month {month} is missing a time");
            for pair in all.windows(2) {
                assert!(pair[0] < pair[1], "month {month} came out shuffled: {all:?}");
            }
        }
    }

    /// Sunrise and sunset stand the same distance either side of solar noon.
    /// If that fails, the hour angle is wrong and everything built on it is.
    #[test]
    fn sunrise_and_sunset_are_symmetric_about_noon() {
        for month in [1, 4, 7, 10] {
            let t = times(date(2026, month, 15), CAIRO, Params::default(), 2.0);
            let (sunrise, dhuhr, maghrib) = (
                minutes(t.sunrise.unwrap()),
                minutes(t.dhuhr.unwrap()),
                minutes(t.maghrib.unwrap()),
            );
            let before = dhuhr - sunrise;
            let after = maghrib - dhuhr;
            assert!((before - after).abs() <= 1, "month {month}: {before} vs {after}");
        }
    }

    /// On an equinox the sun is up for twelve hours everywhere, so sunrise and
    /// sunset sit six hours either side of local solar noon.
    #[test]
    fn an_equinox_gives_a_twelve_hour_day() {
        // On the meridian, so clock time and solar time agree.
        let greenwich = Location::new(51.4779, 0.0);
        let t = times(date(2026, 3, 20), greenwich, Params::default(), 0.0);
        let day = minutes(t.maghrib.unwrap()) - minutes(t.sunrise.unwrap());
        // A little over twelve hours: refraction and the sun's disc both make
        // the day longer than the geometry alone, by more at this latitude
        // than at the equator, and the equinox is not exactly on the 20th.
        assert!((day - 12 * 60).abs() <= 15, "daylight was {day} minutes");
    }

    /// Dhuhr is solar noon, so a place east of its timezone's meridian sees it
    /// earlier than a place west of it.
    #[test]
    fn longitude_moves_dhuhr_the_right_way() {
        let east = times(date(2026, 6, 1), Location::new(30.0, 45.0), Params::default(), 3.0);
        let west = times(date(2026, 6, 1), Location::new(30.0, 30.0), Params::default(), 3.0);
        assert!(
            minutes(east.dhuhr.unwrap()) < minutes(west.dhuhr.unwrap()),
            "the eastern place should reach noon first"
        );
        // 15° of longitude is an hour of sun.
        let gap = minutes(west.dhuhr.unwrap()) - minutes(east.dhuhr.unwrap());
        assert!((gap - 60).abs() <= 2, "gap was {gap} minutes");
    }

    /// Dhuhr is solar noon, and solar noon is clock noon shifted by how far
    /// the place sits from its timezone's meridian, plus the equation of time.
    ///
    /// The equation of time never exceeds about 17 minutes either way, so that
    /// is the whole range Dhuhr can occupy — a bound that can be checked
    /// without a table to compare against. Published tables often print Dhuhr
    /// a minute or two later as a precaution, since the prayer begins once the
    /// sun has passed the meridian rather than as it crosses; the value here
    /// is the astronomical one.
    #[test]
    fn dhuhr_is_solar_noon_at_this_longitude() {
        for (place, offset) in [(MAKKAH, 3.0), (CAIRO, 2.0), (LONDON, 1.0)] {
            let t = times(date(2026, 6, 15), place, Params::default(), 3.0_f64.min(offset));
            let meridian = 15.0 * 3.0_f64.min(offset);
            let expected = 12.0 * 60.0 + (meridian - place.longitude) * 4.0;
            let drift = minutes(t.dhuhr.unwrap()) as f64 - expected;
            assert!(
                drift.abs() <= 17.0,
                "Dhuhr at {place:?} drifted {drift:.1} minutes from solar noon"
            );
        }
    }

    /// The Hanafi Asr is later, because it waits for a longer shadow.
    #[test]
    fn the_hanafi_asr_comes_after_the_standard_one() {
        let standard = times(date(2026, 6, 15), CAIRO, Params::default(), 2.0);
        let hanafi = times(
            date(2026, 6, 15),
            CAIRO,
            Params { method: Method::default(), asr: Asr::Hanafi },
            2.0,
        );
        assert!(minutes(hanafi.asr.unwrap()) > minutes(standard.asr.unwrap()));
    }

    /// A wider Fajr angle means the sun has further to climb, so Fajr is
    /// earlier. This is the whole difference between the methods.
    #[test]
    fn a_wider_angle_makes_fajr_earlier() {
        let shallow = Params { method: Method::Isna, asr: Asr::Standard }; // 15°
        let deep = Params { method: Method::Egyptian, asr: Asr::Standard }; // 19.5°
        let a = times(date(2026, 6, 15), CAIRO, shallow, 2.0);
        let b = times(date(2026, 6, 15), CAIRO, deep, 2.0);
        assert!(minutes(b.fajr.unwrap()) < minutes(a.fajr.unwrap()));
    }

    /// London in midsummer: the sun never gets 18° below the horizon, so
    /// Fajr and Isha have no angle-based answer. Saying so beats inventing one.
    #[test]
    fn a_northern_summer_has_no_angle_for_fajr() {
        let t = times(date(2026, 6, 21), LONDON, Params::default(), 1.0);
        assert!(t.fajr.is_none(), "expected no Fajr, got {:?}", t.fajr);
        assert!(t.isha.is_none());
        // The ones that do not depend on an angle still work.
        assert!(t.sunrise.is_some());
        assert!(t.dhuhr.is_some());
        assert!(t.maghrib.is_some());
    }

    /// Umm al-Qura counts from sunset, so it keeps answering where the angle
    /// methods cannot.
    #[test]
    fn a_fixed_interval_survives_a_northern_summer() {
        let params = Params { method: Method::UmmAlQura, asr: Asr::Standard };
        let t = times(date(2026, 6, 21), LONDON, params, 1.0);
        assert!(t.isha.is_some(), "Isha is 90 minutes after Maghrib, angle or not");
        assert_eq!(
            minutes(t.isha.unwrap()) - minutes(t.maghrib.unwrap()),
            90,
            "and it is exactly ninety"
        );
    }

    #[test]
    fn the_next_prayer_skips_sunrise() {
        let t = times(date(2026, 6, 15), CAIRO, Params::default(), 2.0);
        let just_after_fajr = t.fajr.unwrap() + chrono::Duration::minutes(1);
        // Sunrise is index 1 and is not a prayer; Dhuhr is index 2.
        assert_eq!(t.next_after(just_after_fajr), Some(2));
        assert_eq!(t.next_after(NaiveTime::from_hms_opt(0, 0, 0).unwrap()), Some(0));
        assert_eq!(t.next_after(NaiveTime::from_hms_opt(23, 59, 0).unwrap()), None);
    }

    #[test]
    fn methods_round_trip_through_their_keys() {
        for method in Method::all() {
            assert_eq!(Method::parse(method.key()), method);
        }
        assert_eq!(Method::parse("nonsense"), Method::default());
        assert_eq!(Asr::parse(Asr::Hanafi.key()), Asr::Hanafi);
    }

    #[test]
    fn an_unset_location_says_so() {
        assert!(!Location::new(0.0, 0.0).is_set());
        assert!(Location::new(21.4225, 39.8262).is_set());
        assert!(!Location::new(95.0, 0.0).is_set(), "not a latitude");
    }
}
