//! Emit length-delimited normalization cases for cross-toolchain comparison.
//! Compile with Rust 1.88.0 (compatibility) and 1.96.1 (--reference).
mod names;
use std::io::{self, Write};
fn main() -> io::Result<()> {
    let reference = std::env::args().any(|arg| arg == "--reference");
    assert_eq!(
        char::UNICODE_VERSION,
        if reference { (17, 0, 0) } else { (16, 0, 0) }
    );
    let mut output = io::BufWriter::new(io::stdout().lock());
    for c in (0..=0x10ffff).filter_map(char::from_u32) {
        output.write_all(&[u8::from(c.is_whitespace())])?;
        for text in [format!("{c}"), format!("AΣ{c}"), format!("{c}Σ")] {
            let normalized = if reference {
                text.to_lowercase()
            } else {
                names::lowercase(&text)
            };
            let length = u32::try_from(normalized.len()).expect("three-scalar test input");
            output.write_all(&length.to_le_bytes())?;
            output.write_all(normalized.as_bytes())?;
        }
    }
    output.flush()
}
