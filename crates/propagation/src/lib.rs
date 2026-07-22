//! Versioned propagation primitives and the narrow Rust/C++ boundary for NTIA ITM.

use std::error::Error;
use std::f64::consts::PI;
use std::fmt;

const SPEED_OF_LIGHT_M_PER_S: f64 = 299_792_458.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum Polarization {
    Horizontal = 0,
    Vertical = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundParameters {
    pub relative_permittivity: f64,
    pub conductivity_s_per_m: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelDefaults {
    pub version: &'static str,
    pub climate: i32,
    pub surface_refractivity_n_units: f64,
    pub variability_mode: i32,
    pub time_percent: f64,
    pub location_percent: f64,
    pub situation_percent: f64,
    pub land: GroundParameters,
    pub water: GroundParameters,
}

impl ModelDefaults {
    /// Provisional Phase 0 values used for reproducible engineering benchmarks.
    /// They are not yet the production land/water decision.
    pub const PHASE0_V1: Self = Self {
        version: "phase0-v1",
        climate: 5,
        surface_refractivity_n_units: 301.0,
        variability_mode: 12,
        time_percent: 50.0,
        location_percent: 50.0,
        situation_percent: 50.0,
        land: GroundParameters {
            relative_permittivity: 15.0,
            conductivity_s_per_m: 0.008,
        },
        water: GroundParameters {
            relative_permittivity: 15.0,
            conductivity_s_per_m: 0.008,
        },
    };

    /// Production candidate based on Table 3 of NTIA TR-82-100. Because the
    /// product intentionally has one water class, the fresh-water constants
    /// are used as the conservative uniform default for ocean, lake, and river
    /// samples. Heterogeneous paths use sample-fraction linear interpolation.
    pub const LAND_WATER_V1: Self = Self {
        version: "land-water-v1",
        climate: 5,
        surface_refractivity_n_units: 301.0,
        variability_mode: 12,
        time_percent: 50.0,
        location_percent: 50.0,
        situation_percent: 50.0,
        land: GroundParameters {
            relative_permittivity: 15.0,
            conductivity_s_per_m: 0.005,
        },
        water: GroundParameters {
            relative_permittivity: 81.0,
            conductivity_s_per_m: 0.010,
        },
    };

    pub fn ground_for_water_fraction(
        self,
        water_fraction: f64,
    ) -> Result<GroundParameters, PropagationError> {
        if !water_fraction.is_finite() || !(0.0..=1.0).contains(&water_fraction) {
            return Err(PropagationError::InvalidInput(
                "water fraction must be finite and in 0..=1".into(),
            ));
        }
        Ok(GroundParameters {
            relative_permittivity: self.land.relative_permittivity
                + (self.water.relative_permittivity - self.land.relative_permittivity)
                    * water_fraction,
            conductivity_s_per_m: self.land.conductivity_s_per_m
                + (self.water.conductivity_s_per_m - self.land.conductivity_s_per_m)
                    * water_fraction,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictionInputs {
    pub tx_height_m: f64,
    pub rx_height_m: f64,
    pub frequency_mhz: f64,
    pub polarization: Polarization,
    pub climate: i32,
    pub surface_refractivity_n_units: f64,
    pub ground: GroundParameters,
    pub variability_mode: i32,
    pub time_percent: f64,
    pub location_percent: f64,
    pub situation_percent: f64,
}

impl PredictionInputs {
    pub fn phase0(frequency_mhz: f64, polarization: Polarization) -> Self {
        let defaults = ModelDefaults::PHASE0_V1;
        Self {
            tx_height_m: 20.0,
            rx_height_m: 1.5,
            frequency_mhz,
            polarization,
            climate: defaults.climate,
            surface_refractivity_n_units: defaults.surface_refractivity_n_units,
            ground: defaults.land,
            variability_mode: defaults.variability_mode,
            time_percent: defaults.time_percent,
            location_percent: defaults.location_percent,
            situation_percent: defaults.situation_percent,
        }
    }

    pub fn land_water_v1(frequency_mhz: f64, polarization: Polarization) -> Self {
        let defaults = ModelDefaults::LAND_WATER_V1;
        Self {
            tx_height_m: 20.0,
            rx_height_m: 1.5,
            frequency_mhz,
            polarization,
            climate: defaults.climate,
            surface_refractivity_n_units: defaults.surface_refractivity_n_units,
            ground: defaults.land,
            variability_mode: defaults.variability_mode,
            time_percent: defaults.time_percent,
            location_percent: defaults.location_percent,
            situation_percent: defaults.situation_percent,
        }
    }

    fn validate(self) -> Result<(), PropagationError> {
        validate_range("tx height", self.tx_height_m, 0.5, 3000.0)?;
        validate_range("rx height", self.rx_height_m, 0.5, 3000.0)?;
        validate_range("frequency", self.frequency_mhz, 20.0, 20_000.0)?;
        validate_range(
            "surface refractivity",
            self.surface_refractivity_n_units,
            250.0,
            400.0,
        )?;
        if !(1..=7).contains(&self.climate) {
            return Err(PropagationError::InvalidInput(
                "climate must be in 1..=7".into(),
            ));
        }
        if !self.ground.relative_permittivity.is_finite()
            || self.ground.relative_permittivity <= 1.0
        {
            return Err(PropagationError::InvalidInput(
                "relative permittivity must be finite and greater than 1".into(),
            ));
        }
        if !self.ground.conductivity_s_per_m.is_finite() || self.ground.conductivity_s_per_m <= 0.0
        {
            return Err(PropagationError::InvalidInput(
                "conductivity must be finite and positive".into(),
            ));
        }
        for (name, value) in [
            ("time", self.time_percent),
            ("location", self.location_percent),
            ("situation", self.situation_percent),
        ] {
            if !value.is_finite() || value <= 0.0 || value >= 100.0 {
                return Err(PropagationError::InvalidInput(format!(
                    "{name} percentage must be between 0 and 100"
                )));
            }
        }
        Ok(())
    }
}

fn validate_range(
    name: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), PropagationError> {
    if !value.is_finite() || value < minimum || value > maximum {
        return Err(PropagationError::InvalidInput(format!(
            "{name} must be finite and in {minimum}..={maximum}"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainProfile {
    pfl: Vec<f64>,
}

impl TerrainProfile {
    pub fn new(sample_spacing_m: f64, elevations_m: Vec<f64>) -> Result<Self, PropagationError> {
        if !sample_spacing_m.is_finite() || sample_spacing_m <= 0.0 {
            return Err(PropagationError::InvalidProfile(
                "sample spacing must be finite and positive".into(),
            ));
        }
        if elevations_m.len() < 2 {
            return Err(PropagationError::InvalidProfile(
                "a profile needs at least two elevations".into(),
            ));
        }
        if elevations_m.iter().any(|value| !value.is_finite()) {
            return Err(PropagationError::InvalidProfile(
                "all elevations must be finite".into(),
            ));
        }

        let mut pfl = Vec::with_capacity(elevations_m.len() + 2);
        pfl.push((elevations_m.len() - 1) as f64);
        pfl.push(sample_spacing_m);
        pfl.extend(elevations_m);
        Ok(Self { pfl })
    }

    pub fn from_pfl(pfl: Vec<f64>) -> Result<Self, PropagationError> {
        validate_pfl(&pfl)?;
        Ok(Self { pfl })
    }

    pub fn as_pfl(&self) -> &[f64] {
        &self.pfl
    }

    pub fn distance_m(&self) -> f64 {
        self.pfl[0] * self.pfl[1]
    }

    pub fn elevation_count(&self) -> usize {
        self.pfl.len() - 2
    }
}

fn validate_pfl(pfl: &[f64]) -> Result<(), PropagationError> {
    if pfl.len() < 4 || !pfl[0].is_finite() || pfl[0] < 1.0 {
        return Err(PropagationError::InvalidProfile(
            "PFL header or length is invalid".into(),
        ));
    }
    let interval_count = pfl[0].round() as usize;
    if (pfl[0] - interval_count as f64).abs() > 1e-9 || interval_count + 3 != pfl.len() {
        return Err(PropagationError::InvalidProfile(
            "PFL interval count does not match elevation count".into(),
        ));
    }
    if !pfl[1].is_finite() || pfl[1] <= 0.0 {
        return Err(PropagationError::InvalidProfile(
            "PFL sample spacing must be finite and positive".into(),
        ));
    }
    if pfl[2..].iter().any(|value| !value.is_finite()) {
        return Err(PropagationError::InvalidProfile(
            "PFL contains a non-finite elevation".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropagationMode {
    LineOfSight,
    Diffraction,
    Troposcatter,
    Unknown(i32),
}

impl From<i32> for PropagationMode {
    fn from(value: i32) -> Self {
        match value {
            1 => Self::LineOfSight,
            2 => Self::Diffraction,
            3 => Self::Troposcatter,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictionOutput {
    pub basic_transmission_loss_db: f64,
    pub warnings: u64,
    pub mode: PropagationMode,
    pub free_space_loss_db: f64,
    pub reference_attenuation_db: f64,
    pub distance_km: f64,
    pub terrain_irregularity_m: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropagationError {
    InvalidInput(String),
    InvalidProfile(String),
    Itm(i32),
}

impl fmt::Display for PropagationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid model input: {message}"),
            Self::InvalidProfile(message) => {
                write!(formatter, "invalid terrain profile: {message}")
            }
            Self::Itm(code) => write!(formatter, "NTIA ITM returned error code {code}"),
        }
    }
}

impl Error for PropagationError {}

#[repr(C)]
#[derive(Default)]
struct NativeResult {
    loss_db: f64,
    warnings: u64,
    mode: i32,
    free_space_loss_db: f64,
    reference_attenuation_db: f64,
    distance_km: f64,
    terrain_irregularity_m: f64,
}

unsafe extern "C" {
    fn hh_itm_p2p_tls(
        tx_height_m: f64,
        rx_height_m: f64,
        profile: *const f64,
        profile_len: usize,
        climate: i32,
        surface_refractivity_n_units: f64,
        frequency_mhz: f64,
        polarization: i32,
        relative_permittivity: f64,
        conductivity_s_per_m: f64,
        variability_mode: i32,
        time_percent: f64,
        location_percent: f64,
        situation_percent: f64,
        result: *mut NativeResult,
    ) -> i32;
}

pub fn predict_p2p(
    profile: &TerrainProfile,
    inputs: PredictionInputs,
) -> Result<PredictionOutput, PropagationError> {
    predict_p2p_pfl(profile.as_pfl(), inputs)
}

/// Predicts a point-to-point path from a validated ITM PFL slice. This form is
/// intended for coverage workers that reuse one allocation for many profiles.
pub fn predict_p2p_pfl(
    pfl: &[f64],
    inputs: PredictionInputs,
) -> Result<PredictionOutput, PropagationError> {
    validate_pfl(pfl)?;
    inputs.validate()?;
    let mut native = NativeResult::default();

    // SAFETY: both pointers remain valid for the call, the lengths match the
    // validated PFL header, and the C wrapper does not retain either pointer.
    let error = unsafe {
        hh_itm_p2p_tls(
            inputs.tx_height_m,
            inputs.rx_height_m,
            pfl.as_ptr(),
            pfl.len(),
            inputs.climate,
            inputs.surface_refractivity_n_units,
            inputs.frequency_mhz,
            inputs.polarization as i32,
            inputs.ground.relative_permittivity,
            inputs.ground.conductivity_s_per_m,
            inputs.variability_mode,
            inputs.time_percent,
            inputs.location_percent,
            inputs.situation_percent,
            &mut native,
        )
    };

    // NTIA uses 0 for success without warnings and 1 for success with warning
    // flags. Only validation/model errors (1000+) and wrapper errors (<0) fail.
    if error != 0 && error != 1 {
        return Err(PropagationError::Itm(error));
    }
    Ok(PredictionOutput {
        basic_transmission_loss_db: native.loss_db,
        warnings: native.warnings,
        mode: native.mode.into(),
        free_space_loss_db: native.free_space_loss_db,
        reference_attenuation_db: native.reference_attenuation_db,
        distance_km: native.distance_km,
        terrain_irregularity_m: native.terrain_irregularity_m,
    })
}

pub fn free_space_loss_db(distance_km: f64, frequency_mhz: f64) -> Result<f64, PropagationError> {
    if !distance_km.is_finite() || distance_km <= 0.0 {
        return Err(PropagationError::InvalidInput(
            "distance must be finite and positive".into(),
        ));
    }
    validate_range("frequency", frequency_mhz, 20.0, 20_000.0)?;
    let distance_m = distance_km * 1000.0;
    let frequency_hz = frequency_mhz * 1_000_000.0;
    Ok(20.0 * (4.0 * PI * distance_m * frequency_hz / SPEED_OF_LIGHT_M_PER_S).log10())
}

pub fn watts_to_dbm(watts: f64) -> Result<f64, PropagationError> {
    if !watts.is_finite() || watts <= 0.0 {
        return Err(PropagationError::InvalidInput(
            "power in watts must be finite and positive".into(),
        ));
    }
    Ok(10.0 * (watts * 1000.0).log10())
}

pub fn dbm_to_watts(dbm: f64) -> Result<f64, PropagationError> {
    if !dbm.is_finite() {
        return Err(PropagationError::InvalidInput(
            "power in dBm must be finite".into(),
        ));
    }
    Ok(10.0_f64.powf(dbm / 10.0) / 1000.0)
}

pub fn dbd_to_dbi(dbd: f64) -> Result<f64, PropagationError> {
    if !dbd.is_finite() {
        return Err(PropagationError::InvalidInput(
            "antenna gain must be finite".into(),
        ));
    }
    Ok(dbd + 2.15)
}

pub fn received_power_dbm(
    tx_power_dbm: f64,
    tx_gain_dbi: f64,
    rx_gain_dbi: f64,
    basic_transmission_loss_db: f64,
) -> Result<f64, PropagationError> {
    let values = [
        tx_power_dbm,
        tx_gain_dbi,
        rx_gain_dbi,
        basic_transmission_loss_db,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return Err(PropagationError::InvalidInput(
            "link-budget values must be finite".into(),
        ));
    }
    Ok(tx_power_dbm + tx_gain_dbi + rx_gain_dbi - basic_transmission_loss_db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_round_trip_is_precise() {
        for watts in [0.1, 1.0, 5.0, 25.0, 1000.0] {
            let dbm = watts_to_dbm(watts).unwrap();
            let round_trip = dbm_to_watts(dbm).unwrap();
            assert!((round_trip - watts).abs() < 1e-12);
        }
    }

    #[test]
    fn dbd_conversion_matches_product_rule() {
        assert!((dbd_to_dbi(0.0).unwrap() - 2.15).abs() < 1e-12);
    }

    #[test]
    fn profile_distance_uses_interval_count() {
        let profile = TerrainProfile::new(90.0, vec![100.0, 110.0, 120.0]).unwrap();
        assert_eq!(profile.elevation_count(), 3);
        assert!((profile.distance_m() - 180.0).abs() < 1e-12);
    }

    #[test]
    fn free_space_loss_at_one_kilometre_is_reasonable() {
        let loss = free_space_loss_db(1.0, 145.0).unwrap();
        assert!((loss - 75.68).abs() < 0.02);
    }

    #[test]
    fn itm_accepts_the_closest_one_kilometre_grid_receiver() {
        let spacing_m = 1000.0 / 12.0;
        let profile = TerrainProfile::new(spacing_m, vec![100.0; 13]).unwrap();
        let output = predict_p2p(
            &profile,
            PredictionInputs::phase0(145.0, Polarization::Vertical),
        )
        .unwrap();
        assert!((output.distance_km - 1.0).abs() < 1e-12);
        assert_eq!(output.warnings, 0);
        assert_eq!(output.mode, PropagationMode::LineOfSight);
        assert!(output.basic_transmission_loss_db.is_finite());
    }

    #[test]
    fn land_water_mixing_preserves_endpoints_and_interpolates() {
        let defaults = ModelDefaults::LAND_WATER_V1;
        assert_eq!(
            defaults.ground_for_water_fraction(0.0).unwrap(),
            defaults.land
        );
        assert_eq!(
            defaults.ground_for_water_fraction(1.0).unwrap(),
            defaults.water
        );
        let half = defaults.ground_for_water_fraction(0.5).unwrap();
        assert!((half.relative_permittivity - 48.0).abs() < 1e-12);
        assert!((half.conductivity_s_per_m - 0.0075).abs() < 1e-12);
        assert!(defaults.ground_for_water_fraction(-0.01).is_err());
        assert!(defaults.ground_for_water_fraction(1.01).is_err());
    }
}
