// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use hamheatmap_propagation::{
    GroundParameters, Polarization, PredictionInputs, TerrainProfile, predict_p2p,
};

fn numeric_rows(path: &Path, skip_header: bool) -> Vec<Vec<f64>> {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .skip(usize::from(skip_header))
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split(',')
                .map(|value| value.trim().parse::<f64>().unwrap())
                .collect()
        })
        .collect()
}

#[test]
fn ntia_v1_4_point_to_point_reference_cases_match() {
    let itm_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/ntia-itm");
    let cases = numeric_rows(&itm_root.join("p2p.csv"), true);
    let profiles = numeric_rows(&itm_root.join("pfls.csv"), false);
    assert_eq!(cases.len(), profiles.len());
    assert!(!cases.is_empty());

    for (index, (case, pfl)) in cases.into_iter().zip(profiles).enumerate() {
        assert_eq!(case.len(), 13, "unexpected p2p.csv column count");
        let profile = TerrainProfile::from_pfl(pfl).unwrap();
        let inputs = PredictionInputs {
            tx_height_m: case[0],
            rx_height_m: case[1],
            frequency_mhz: case[5],
            polarization: if case[6] == 0.0 {
                Polarization::Horizontal
            } else {
                Polarization::Vertical
            },
            climate: case[7] as i32,
            surface_refractivity_n_units: case[4],
            ground: GroundParameters {
                relative_permittivity: case[2],
                conductivity_s_per_m: case[3],
            },
            variability_mode: case[11] as i32,
            time_percent: case[8],
            location_percent: case[9],
            situation_percent: case[10],
        };
        let expected_loss_db = case[12];
        let actual = predict_p2p(&profile, inputs).unwrap();
        assert!(
            (actual.basic_transmission_loss_db - expected_loss_db).abs() <= 0.011,
            "case {index}: expected {expected_loss_db:.2} dB, got {:.6} dB",
            actual.basic_transmission_loss_db
        );
    }
}
