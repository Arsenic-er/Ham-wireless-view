//! Validated, offline PNG/PDF report encoding and atomic file persistence.

use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, RawImage, XObjectTransform};

pub const REPORT_WIDTH: u32 = 1_600;
pub const REPORT_HEIGHT: u32 = 1_100;
pub const MAX_REPORT_DATA_URL_CHARS: usize = 12_000_000;
pub const MAX_REPORT_PNG_BYTES: usize = 8_000_000;
const PNG_DATA_URL_PREFIX: &str = "data:image/png;base64,";
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportFormat {
    Png,
    Pdf,
}

impl ReportFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Pdf => "pdf",
        }
    }
}

pub fn validate_suggested_file_name(name: &str, format: ReportFormat) -> Result<(), ExportError> {
    if name.is_empty() || name.len() > 120 {
        return Err(ExportError::InvalidInput(
            "suggested export file name must contain 1 to 120 bytes".into(),
        ));
    }
    if !name
        .bytes()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'.'))
    {
        return Err(ExportError::InvalidInput(
            "suggested export file name contains unsafe characters".into(),
        ));
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case(format.extension()) {
        return Err(ExportError::InvalidInput(
            "suggested export file extension does not match the requested format".into(),
        ));
    }
    Ok(())
}

pub fn path_with_format_extension(path: &Path, format: ReportFormat) -> PathBuf {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(format.extension()))
    {
        return path.to_path_buf();
    }
    let mut result = path.to_path_buf();
    result.set_extension(format.extension());
    result
}

pub fn encode_report(data_url: &str, format: ReportFormat) -> Result<Vec<u8>, ExportError> {
    let png = decode_report_png_data_url(data_url)?;
    match format {
        ReportFormat::Png => Ok(png),
        ReportFormat::Pdf => build_pdf_from_png(&png),
    }
}

pub fn decode_report_png_data_url(data_url: &str) -> Result<Vec<u8>, ExportError> {
    if data_url.len() > MAX_REPORT_DATA_URL_CHARS {
        return Err(ExportError::InvalidInput(
            "export report data URL exceeds the fixed safety limit".into(),
        ));
    }
    let payload = data_url.strip_prefix(PNG_DATA_URL_PREFIX).ok_or_else(|| {
        ExportError::InvalidInput("export report must be a base64 PNG data URL".into())
    })?;
    let png = BASE64_STANDARD
        .decode(payload)
        .map_err(|error| ExportError::InvalidInput(format!("invalid report base64: {error}")))?;
    if png.len() > MAX_REPORT_PNG_BYTES {
        return Err(ExportError::InvalidInput(
            "decoded export report exceeds the fixed safety limit".into(),
        ));
    }
    validate_png_structure(&png)?;
    let mut warnings = Vec::new();
    let decoded = RawImage::decode_from_bytes(&png, &mut warnings)
        .map_err(|error| ExportError::InvalidInput(format!("invalid report PNG: {error}")))?;
    if decoded.width != REPORT_WIDTH as usize || decoded.height != REPORT_HEIGHT as usize {
        return Err(ExportError::InvalidInput(format!(
            "export report must decode to {REPORT_WIDTH}x{REPORT_HEIGHT} pixels"
        )));
    }
    Ok(png)
}

fn validate_png_structure(png: &[u8]) -> Result<(), ExportError> {
    if png.len() < 24 || &png[..8] != PNG_SIGNATURE || &png[12..16] != b"IHDR" {
        return Err(ExportError::InvalidInput(
            "export report has an invalid PNG signature or IHDR".into(),
        ));
    }
    let width = u32::from_be_bytes(png[16..20].try_into().expect("four PNG width bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("four PNG height bytes"));
    if width != REPORT_WIDTH || height != REPORT_HEIGHT {
        return Err(ExportError::InvalidInput(format!(
            "export report PNG must be {REPORT_WIDTH}x{REPORT_HEIGHT} pixels"
        )));
    }
    Ok(())
}

fn build_pdf_from_png(png: &[u8]) -> Result<Vec<u8>, ExportError> {
    let mut warnings = Vec::new();
    let image = RawImage::decode_from_bytes(png, &mut warnings)
        .map_err(|error| ExportError::InvalidInput(format!("invalid report PNG: {error}")))?;
    let mut document = PdfDocument::new("HamHeatmap propagation report");
    let image_id = document.add_image(&image);
    let page = PdfPage::new(
        Mm(297.0),
        Mm(210.0),
        vec![Op::UseXobject {
            id: image_id,
            transform: XObjectTransform {
                translate_x: None,
                translate_y: Some(Pt(8.0)),
                rotate: None,
                scale_x: None,
                scale_y: None,
                dpi: Some(137.0),
            },
        }],
    );
    let bytes = document
        .with_pages(vec![page])
        .save(&PdfSaveOptions::default(), &mut warnings);
    if !bytes.starts_with(b"%PDF-") {
        return Err(ExportError::Encoding(
            "PDF encoder returned an invalid header".into(),
        ));
    }
    Ok(bytes)
}

pub fn write_report_atomic(path: &Path, bytes: &[u8]) -> Result<u64, ExportError> {
    let parent = path.parent().ok_or_else(|| {
        ExportError::InvalidInput("export destination has no parent directory".into())
    })?;
    if !parent.is_dir() {
        return Err(ExportError::InvalidInput(
            "export destination directory does not exist".into(),
        ));
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| ExportError::InvalidInput("export destination has no file name".into()))?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.hamheatmap-{}-{unique}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| -> Result<(), ExportError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        commit_temporary_file(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(bytes.len() as u64)
}

#[cfg(not(windows))]
fn commit_temporary_file(temporary: &Path, destination: &Path) -> Result<(), std::io::Error> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn commit_temporary_file(temporary: &Path, destination: &Path) -> Result<(), std::io::Error> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let from = temporary
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let to = destination
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub enum ExportError {
    InvalidInput(String),
    Encoding(String),
    Io(std::io::Error),
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "{message}"),
            Self::Encoding(message) => write!(formatter, "{message}"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidInput(_) | Self::Encoding(_) => None,
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG header");
            let pixels = vec![0xf5; width as usize * height as usize * 4];
            writer.write_image_data(&pixels).expect("PNG pixels");
        }
        bytes
    }

    fn report_data_url() -> (String, Vec<u8>) {
        let png = report_png(REPORT_WIDTH, REPORT_HEIGHT);
        let url = format!("{PNG_DATA_URL_PREFIX}{}", BASE64_STANDARD.encode(&png));
        (url, png)
    }

    #[test]
    fn validated_png_round_trips_with_fixed_dimensions() {
        let (url, png) = report_data_url();
        assert_eq!(decode_report_png_data_url(&url).unwrap(), png);
        assert_eq!(encode_report(&url, ReportFormat::Png).unwrap(), png);
    }

    #[test]
    fn pdf_is_parseable_single_page_a4_landscape() {
        let (url, _) = report_data_url();
        let pdf = encode_report(&url, ReportFormat::Pdf).unwrap();
        let parsed = lopdf::Document::load_mem(&pdf).expect("parse generated PDF");
        assert_eq!(parsed.get_pages().len(), 1);
        assert!(pdf.starts_with(b"%PDF-"));
        let last_content = pdf
            .iter()
            .rposition(|value| !value.is_ascii_whitespace())
            .expect("non-empty PDF");
        assert!(pdf[..=last_content].ends_with(b"%%EOF"));
    }

    #[test]
    fn invalid_mime_base64_dimensions_and_oversized_payload_are_rejected() {
        assert!(decode_report_png_data_url("data:text/plain;base64,SGk=").is_err());
        assert!(decode_report_png_data_url("data:image/png;base64,***").is_err());
        let wrong = report_png(1, 1);
        let wrong_url = format!("{PNG_DATA_URL_PREFIX}{}", BASE64_STANDARD.encode(wrong));
        assert!(decode_report_png_data_url(&wrong_url).is_err());
        let oversized = format!(
            "{PNG_DATA_URL_PREFIX}{}",
            "A".repeat(MAX_REPORT_DATA_URL_CHARS)
        );
        assert!(decode_report_png_data_url(&oversized).is_err());
    }

    #[test]
    fn file_name_and_extension_contract_is_strict() {
        assert!(validate_suggested_file_name("HamHeatmap_145p25.png", ReportFormat::Png).is_ok());
        assert!(validate_suggested_file_name("../report.png", ReportFormat::Png).is_err());
        assert!(validate_suggested_file_name("report.pdf", ReportFormat::Png).is_err());
        assert_eq!(
            path_with_format_extension(Path::new("report.txt"), ReportFormat::Pdf),
            PathBuf::from("report.pdf")
        );
    }

    #[test]
    fn atomic_write_replaces_target_and_leaves_no_partial_file() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("report.png");
        fs::write(&target, b"old").unwrap();
        assert_eq!(write_report_atomic(&target, b"new report").unwrap(), 10);
        assert_eq!(fs::read(&target).unwrap(), b"new report");
        let entries = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![target.file_name().unwrap()]);
    }

    #[test]
    fn failed_atomic_commit_preserves_target_and_cleans_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("existing-directory.png");
        fs::create_dir(&target).unwrap();

        assert!(write_report_atomic(&target, b"report").is_err());
        assert!(target.is_dir());
        let entries = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![target.file_name().unwrap()]);
    }
}
