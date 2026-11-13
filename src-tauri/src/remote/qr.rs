//! The code to scan, as a module matrix the interface draws.

use nocturne_contract::QrCode;
use qrcode::EcLevel;

/// Level M. Every link here is short ASCII, well inside what a code holds.
pub fn matrix(text: &str) -> QrCode {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M)
        .expect("a pairing link fits in a code");
    let size = code.width();
    let modules = code
        .to_colors()
        .iter()
        .map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' })
        .collect();
    QrCode {
        size: size as u32,
        modules,
    }
}
