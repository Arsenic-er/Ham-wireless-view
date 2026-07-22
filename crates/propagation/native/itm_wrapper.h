#pragma once

#include <cstddef>
#include <cstdint>

struct HhItmResult {
    double loss_db;
    std::uint64_t warnings;
    std::int32_t mode;
    double free_space_loss_db;
    double reference_attenuation_db;
    double distance_km;
    double terrain_irregularity_m;
};

extern "C" std::int32_t hh_itm_p2p_tls(
    double tx_height_m,
    double rx_height_m,
    const double* profile,
    std::size_t profile_len,
    std::int32_t climate,
    double surface_refractivity_n_units,
    double frequency_mhz,
    std::int32_t polarization,
    double relative_permittivity,
    double conductivity_s_per_m,
    std::int32_t variability_mode,
    double time_percent,
    double location_percent,
    double situation_percent,
    HhItmResult* result);

