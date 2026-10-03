//! Parse original Phigros charts without creating a window or audio device.
use anyhow::{Context, Result};
use prpr::{core::ChartExtra, parse::parse_phigros};

fn main() -> Result<()> {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).with_context(|| path.clone())?;
        let chart = parse_phigros(&text, ChartExtra::default()).with_context(|| path.clone())?;
        println!("OK: {path} ({} lines, {} blocks)", chart.lines.len(), chart.block_areas.len());
    }
    Ok(())
}
