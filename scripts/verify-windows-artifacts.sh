#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target_root="$project_root/app/src-tauri/target"
release_root="$target_root/x86_64-pc-windows-msvc/release"
app_exe="$release_root/HamHeatmap.exe"
nsis_exe="$release_root/bundle/nsis/HamHeatmap_0.1.0_x64-setup.exe"
readobj="$project_root/.tools/llvm-20.1.8/bin/llvm-readobj"
seven_zip="$project_root/.tools/archive/usr/lib/p7zip/7z"
nsis_utils="$target_root/.tauri/NSIS/Plugins/x86-unicode/additional/nsis_tauri_utils.dll"

fail() {
    echo "Windows artifact verification failed: $*" >&2
    exit 1
}

for command in find grep sed sort stat sha256sum realpath mktemp rm cmp wc tr; do
    command -v "$command" >/dev/null 2>&1 || fail "required host command is missing: $command"
done
[[ -x "$readobj" ]] || fail "llvm-readobj is missing or not executable: $readobj"
[[ -x "$seven_zip" ]] || fail "project-local 7z is missing or not executable: $seven_zip"
for artifact in "$app_exe" "$nsis_exe" "$nsis_utils"; do
    [[ -s "$artifact" ]] || fail "artifact is missing or empty: $artifact"
done

resolved_project="$(realpath -m "$project_root")"
resolved_target="$(realpath -m "$target_root")"
[[ "$resolved_target" == "$resolved_project/app/src-tauri/target" ]] ||
    fail "unexpected target root: $resolved_target"

assert_field() {
    local text="$1" expected="$2" label="$3"
    printf '%s\n' "$text" | grep -F "$expected" >/dev/null ||
        fail "$label is missing expected PE field: $expected"
}

assert_unsigned() {
    assert_field "$1" "CertificateTableRVA: 0x0" "$2"
    assert_field "$1" "CertificateTableSize: 0x0" "$2"
}

artifact_line() {
    local label="$1" path="$2" sha
    sha="$(sha256sum "$path")"
    printf '%-24s size=%s sha256=%s path=%s\n' \
        "$label" "$(stat -c '%s' "$path")" "${sha%% *}" "$path"
}

app_headers="$("$readobj" --file-headers "$app_exe")"
for field in \
    "Format: COFF-x86-64" \
    "Arch: x86_64" \
    "Machine: IMAGE_FILE_MACHINE_AMD64" \
    "Subsystem: IMAGE_SUBSYSTEM_WINDOWS_GUI" \
    "IMAGE_DLL_CHARACTERISTICS_DYNAMIC_BASE" \
    "IMAGE_DLL_CHARACTERISTICS_HIGH_ENTROPY_VA" \
    "IMAGE_DLL_CHARACTERISTICS_NX_COMPAT"; do
    assert_field "$app_headers" "$field" "application EXE"
done
assert_unsigned "$app_headers" "application EXE"

nsis_headers="$("$readobj" --file-headers "$nsis_exe")"
# NSIS 3 uses a 32-bit bootstrap stub even when its application payload is x64.
for field in \
    "Format: COFF-i386" \
    "Machine: IMAGE_FILE_MACHINE_I386" \
    "Subsystem: IMAGE_SUBSYSTEM_WINDOWS_GUI" \
    "IMAGE_DLL_CHARACTERISTICS_DYNAMIC_BASE" \
    "IMAGE_DLL_CHARACTERISTICS_NX_COMPAT"; do
    assert_field "$nsis_headers" "$field" "NSIS installer"
done
assert_unsigned "$nsis_headers" "NSIS installer"

imports="$("$readobj" --coff-imports "$app_exe")"
imported_dlls="$(printf '%s\n' "$imports" | sed -n 's/^  Name: //p')"
[[ -n "$imported_dlls" ]] || fail "application import table contains no DLL names"
if printf '%s\n' "$imported_dlls" |
    grep -Eqi '(^|[^a-z])(vcruntime|msvcp|ucrtbase|api-ms-win-crt)'; then
    printf '%s\n' "$imported_dlls" >&2
    fail "dynamic MSVC/UCRT runtime import detected"
fi

mapfile -d '' webview_candidates < <(
    find "$target_root/.tauri/x64" -mindepth 2 -maxdepth 2 -type f \
        -name 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe' -print0
)
[[ "${#webview_candidates[@]}" -eq 1 ]] ||
    fail "expected exactly one cached x64 WebView2 installer, found ${#webview_candidates[@]}"
webview_cache="${webview_candidates[0]}"
[[ -s "$webview_cache" ]] || fail "cached WebView2 installer is empty"
webview_headers="$("$readobj" --file-headers "$webview_cache")"
webview_cert_size="$(printf '%s\n' "$webview_headers" |
    sed -n 's/.*CertificateTableSize: \(0x[0-9A-Fa-f][0-9A-Fa-f]*\).*/\1/p')"
[[ "$webview_cert_size" =~ ^0x[0-9A-Fa-f]+$ ]] ||
    fail "cannot read WebView2 certificate table size"
((webview_cert_size > 0)) || fail "WebView2 has no Authenticode certificate table blob"

verify_parent="$target_root/verify"
mkdir -p "$verify_parent"
resolved_verify_parent="$(realpath -m "$verify_parent")"
[[ "$resolved_verify_parent" == "$resolved_target/verify" ]] ||
    fail "unexpected verification parent: $resolved_verify_parent"
verify_dir="$(mktemp -d "$verify_parent/windows-artifacts.XXXXXX")"
cleanup() {
    local resolved
    resolved="$(realpath -m "$verify_dir" 2>/dev/null || true)"
    case "$resolved" in
        "$resolved_verify_parent"/windows-artifacts.*)
            [[ -d "$resolved" ]] && rm -rf -- "$resolved"
            ;;
        *) echo "refusing to clean unexpected verification path: $resolved" >&2 ;;
    esac
}
trap cleanup EXIT INT TERM

echo "NSIS archive listing:"
"$seven_zip" l "$nsis_exe"
"$seven_zip" x -y "-o$verify_dir" "$nsis_exe" >/dev/null

actual_files="$(find "$verify_dir" -type f -printf '%P\n' | LC_ALL=C sort)"
expected_files=$'$PLUGINSDIR/StartMenu.dll\n$PLUGINSDIR/System.dll\n$PLUGINSDIR/modern-wizard.bmp\n$PLUGINSDIR/nsDialogs.dll\n$PLUGINSDIR/nsis_tauri_utils.dll\n$TEMP/MicrosoftEdgeWebView2RuntimeInstaller.exe\nHamHeatmap.exe\nTHIRD_PARTY_LICENSES.md'
if [[ "$actual_files" != "$expected_files" ]]; then
    printf 'Expected NSIS files:\n%s\nActual NSIS files:\n%s\n' \
        "$expected_files" "$actual_files" >&2
    fail "NSIS contains an unexpected or missing file"
fi

embedded_app="$verify_dir/HamHeatmap.exe"
embedded_license="$verify_dir/THIRD_PARTY_LICENSES.md"
embedded_webview="$verify_dir/\$TEMP/MicrosoftEdgeWebView2RuntimeInstaller.exe"
embedded_nsis_utils="$verify_dir/\$PLUGINSDIR/nsis_tauri_utils.dll"
for artifact in "$embedded_app" "$embedded_license" "$embedded_webview" "$embedded_nsis_utils"; do
    [[ -s "$artifact" ]] || fail "embedded artifact is missing or empty: $artifact"
done

sha_of() {
    local value
    value="$(sha256sum "$1")"
    printf '%s' "${value%% *}"
}
[[ "$(sha_of "$embedded_webview")" == "$(sha_of "$webview_cache")" ]] ||
    fail "embedded WebView2 differs from its Tauri cache source"
[[ "$(sha_of "$embedded_nsis_utils")" == "$(sha_of "$nsis_utils")" ]] ||
    fail "embedded nsis_tauri_utils differs from its Tauri cache source"
[[ "$(stat -c '%s' "$embedded_app")" == "$(stat -c '%s' "$app_exe")" ]] ||
    fail "embedded and standalone application sizes differ"

difference_report="$(cmp -l "$app_exe" "$embedded_app" || true)"
difference_count="$(printf '%s\n' "$difference_report" | sed '/^$/d' | wc -l | tr -d '[:space:]')"
[[ "$difference_count" == "3" ]] || {
    printf 'Standalone/embedded cmp report:\n%s\n' "$difference_report" >&2
    fail "expected three Tauri bundle-marker differences, found $difference_count"
}
first_difference="$(printf '%s\n' "$difference_report" |
    sed -n '1{s/^[[:space:]]*\([0-9][0-9]*\).*/\1/p;}')"
last_difference="$(printf '%s\n' "$difference_report" |
    sed -n '$s/^[[:space:]]*\([0-9][0-9]*\).*/\1/p')"
[[ -n "$first_difference" && -n "$last_difference" ]] ||
    fail "cannot parse Tauri bundle-marker offsets"
((last_difference - first_difference <= 8)) ||
    fail "application differences are not confined to one bundle-marker region"

echo
echo "Application imported DLLs:"
printf '%s\n' "$imported_dlls"
printf 'DLL count: %s\n' "$(printf '%s\n' "$imported_dlls" | wc -l | tr -d '[:space:]')"
echo
echo "Verified artifacts:"
artifact_line "standalone application" "$app_exe"
artifact_line "NSIS installer" "$nsis_exe"
artifact_line "cached WebView2" "$webview_cache"
artifact_line "cached nsis utils" "$nsis_utils"
artifact_line "embedded application" "$embedded_app"
artifact_line "embedded WebView2" "$embedded_webview"
artifact_line "embedded nsis utils" "$embedded_nsis_utils"
printf 'bundle marker bytes      count=%s offsets=%s..%s\n' \
    "$difference_count" "$first_difference" "$last_difference"
echo "Application and NSIS certificate tables are empty: both artifacts are intentionally unsigned."
echo "WebView2 CertificateTableSize=$webview_cert_size: Authenticode blob present; chain trust not verified on Linux."
echo "Windows artifact verification passed."
