#include "itm_wrapper.h"

#include "itm.h"

#include <cmath>
#include <limits>

namespace {
constexpr std::int32_t kNullPointer = -1001;
constexpr std::int32_t kInvalidProfile = -1002;
}

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
    HhItmResult* result) {
    if (profile == nullptr || result == nullptr) {
        return kNullPointer;
    }

    *result = HhItmResult{
        std::numeric_limits<double>::quiet_NaN(),
        0,
        0,
        std::numeric_limits<double>::quiet_NaN(),
        std::numeric_limits<double>::quiet_NaN(),
        std::numeric_limits<double>::quiet_NaN(),
        std::numeric_limits<double>::quiet_NaN(),
    };

    if (profile_len < 4 || !std::isfinite(profile[0]) || profile[0] < 1.0) {
        return kInvalidProfile;
    }
    const auto interval_count = static_cast<std::size_t>(std::llround(profile[0]));
    if (std::fabs(profile[0] - static_cast<double>(interval_count)) > 1e-9 ||
        interval_count + 3 != profile_len || !std::isfinite(profile[1]) ||
        profile[1] <= 0.0) {
        return kInvalidProfile;
    }

    long native_warnings = 0;
    double loss_db = std::numeric_limits<double>::quiet_NaN();
    IntermediateValues intermediate{};

    // The pinned NTIA v1.4 implementation only reads PFL values, although its
    // public signature predates const-correctness. Keep the cast inside this
    // narrow, version-tested boundary instead of exposing it to Rust.
    auto* mutable_profile = const_cast<double*>(profile);
    const int error = ITM_P2P_TLS_Ex(
        tx_height_m,
        rx_height_m,
        mutable_profile,
        climate,
        surface_refractivity_n_units,
        frequency_mhz,
        polarization,
        relative_permittivity,
        conductivity_s_per_m,
        variability_mode,
        time_percent,
        location_percent,
        situation_percent,
        &loss_db,
        &native_warnings,
        &intermediate);

    result->loss_db = loss_db;
    result->warnings = static_cast<std::uint64_t>(
        static_cast<unsigned long>(native_warnings));
    result->mode = static_cast<std::int32_t>(intermediate.mode);
    result->free_space_loss_db = intermediate.A_fs__db;
    result->reference_attenuation_db = intermediate.A_ref__db;
    result->distance_km = intermediate.d__km;
    result->terrain_irregularity_m = intermediate.delta_h__meter;
    return static_cast<std::int32_t>(error);
}

