use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use memmap2::Mmap;
use rayon::prelude::*;

/// Fast delta generator for bulb package manager
#[derive(Parser)]
#[command(name = "bulb-delta", version, about = "Generate binary delta patches between package versions")]
struct Cli {
    /// Old package file (base version)
    old: PathBuf,

    /// New package file (target version)
    new: PathBuf,

    /// Output delta file (default: <new>.delta)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Generate stats only (don't write delta)
    #[arg(long)]
    stats_only: bool,

    /// Use memory-mapped I/O for large files
    #[arg(long)]
    mmap: bool,
}

/// Delta file format:
/// [old_blake3: 32 bytes][new_blake3: 32 bytes][bsdiff_data: remaining]
const BLAKE3_SIZE: usize = 32;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Validate inputs
    if !cli.old.exists() {
        anyhow::bail!("Old package not found: {}", cli.old.display());
    }
    if !cli.new.exists() {
        anyhow::bail!("New package not found: {}", cli.new.display());
    }

    // Determine output path
    let output = cli.output.unwrap_or_else(|| {
        let mut out = cli.new.clone();
        out.set_extension("delta");
        out
    });

    // Generate delta
    let result = if cli.mmap {
        generate_delta_mmap(&cli.old, &cli.new, &output, cli.stats_only)?
    } else {
        generate_delta(&cli.old, &cli.new, &output, cli.stats_only)?
    };

    // Print stats
    println!("Delta generated successfully!");
    println!("  Old: {} ({:.2} MB)", cli.old.display(), result.old_size as f64 / 1_048_576.0);
    println!("  New: {} ({:.2} MB)", cli.new.display(), result.new_size as f64 / 1_048_576.0);
    println!("  Delta: {} ({:.2} MB)", output.display(), result.delta_size as f64 / 1_048_576.0);
    println!("  Ratio: {:.1}%", result.ratio * 100.0);
    println!("  Speed: {:.2} MB/s", result.throughput);

    Ok(())
}

struct DeltaResult {
    old_size: u64,
    new_size: u64,
    delta_size: u64,
    ratio: f64,
    throughput: f64,
}

fn generate_delta(old_path: &Path, new_path: &Path, output_path: &Path, stats_only: bool) -> Result<DeltaResult> {
    let start = std::time::Instant::now();

    // Read files
    let old_bytes = std::fs::read(old_path)
        .with_context(|| format!("Failed to read {}", old_path.display()))?;
    let new_bytes = std::fs::read(new_path)
        .with_context(|| format!("Failed to read {}", new_path.display()))?;

    let old_size = old_bytes.len() as u64;
    let new_size = new_bytes.len() as u64;

    // Generate delta
    let mut delta_data = Vec::new();
    bsdiff::diff(&old_bytes, &new_bytes, &mut delta_data)
        .context("bsdiff failed")?;

    let delta_size = delta_data.len() as u64;

    // Compute hashes
    let old_hash = blake3::hash(&old_bytes);
    let new_hash = blake3::hash(&new_bytes);

    if !stats_only {
        // Write delta file: [old_hash][new_hash][delta_data]
        let mut output = Vec::with_capacity(BLAKE3_SIZE * 2 + delta_data.len());
        output.extend_from_slice(old_hash.as_bytes());
        output.extend_from_slice(new_hash.as_bytes());
        output.extend_from_slice(&delta_data);

        std::fs::write(output_path, &output)
            .with_context(|| format!("Failed to write {}", output_path.display()))?;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let throughput = (old_size + new_size) as f64 / elapsed / 1_048_576.0;
    let ratio = delta_size as f64 / old_size as f64;

    Ok(DeltaResult {
        old_size,
        new_size,
        delta_size,
        ratio,
        throughput,
    })
}

fn generate_delta_mmap(old_path: &Path, new_path: &Path, output_path: &Path, stats_only: bool) -> Result<DeltaResult> {
    let start = std::time::Instant::now();

    // Memory-map files for large packages
    let old_file = std::fs::File::open(old_path)
        .with_context(|| format!("Failed to open {}", old_path.display()))?;
    let new_file = std::fs::File::open(new_path)
        .with_context(|| format!("Failed to open {}", new_path.display()))?;

    let old_mmap = unsafe { Mmap::map(&old_file) }
        .context("Failed to mmap old file")?;
    let new_mmap = unsafe { Mmap::map(&new_file) }
        .context("Failed to mmap new file")?;

    let old_size = old_mmap.len() as u64;
    let new_size = new_mmap.len() as u64;

    // Generate delta
    let mut delta_data = Vec::new();
    bsdiff::diff(&old_mmap, &new_mmap, &mut delta_data)
        .context("bsdiff failed")?;

    let delta_size = delta_data.len() as u64;

    // Compute hashes
    let old_hash = blake3::hash(&old_mmap);
    let new_hash = blake3::hash(&new_mmap);

    if !stats_only {
        // Write delta file: [old_hash][new_hash][delta_data]
        let mut output = Vec::with_capacity(BLAKE3_SIZE * 2 + delta_data.len());
        output.extend_from_slice(old_hash.as_bytes());
        output.extend_from_slice(new_hash.as_bytes());
        output.extend_from_slice(&delta_data);

        std::fs::write(output_path, &output)
            .with_context(|| format!("Failed to write {}", output_path.display()))?;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let throughput = (old_size + new_size) as f64 / elapsed / 1_048_576.0;
    let ratio = delta_size as f64 / old_size as f64;

    Ok(DeltaResult {
        old_size,
        new_size,
        delta_size,
        ratio,
        throughput,
    })
}

/// Batch generate deltas for multiple package pairs
#[allow(dead_code)]
fn batch_generate(pairs: &[(PathBuf, PathBuf)], output_dir: &Path, mmap: bool) -> Result<Vec<DeltaResult>> {
    std::fs::create_dir_all(output_dir)?;

    let results: Result<Vec<DeltaResult>> = pairs
        .par_iter()
        .map(|(old, new)| {
            let mut output = output_dir.join(new.file_name().unwrap());
            output.set_extension("delta");

            if mmap {
                generate_delta_mmap(old, new, &output, false)
            } else {
                generate_delta(old, new, &output, false)
            }
        })
        .collect();

    results
}

/// Verify a delta file matches expected hashes
#[allow(dead_code)]
fn verify_delta(delta_path: &Path, expected_old: &str, expected_new: &str) -> Result<bool> {
    let data = std::fs::read(delta_path)
        .with_context(|| format!("Failed to read {}", delta_path.display()))?;

    if data.len() < BLAKE3_SIZE * 2 {
        anyhow::bail!("Delta file too small");
    }

    let old_hash = blake3::Hash::from_bytes(data[..BLAKE3_SIZE].try_into().unwrap());
    let new_hash = blake3::Hash::from_bytes(data[BLAKE3_SIZE..BLAKE3_SIZE * 2].try_into().unwrap());

    Ok(old_hash.to_hex().as_str() == expected_old && new_hash.to_hex().as_str() == expected_new)
}

/// Apply a delta to reconstruct the new package
#[allow(dead_code)]
fn apply_delta(old_path: &Path, delta_path: &Path, output_path: &Path) -> Result<()> {
    let start = std::time::Instant::now();

    // Read old package
    let old_bytes = std::fs::read(old_path)
        .with_context(|| format!("Failed to read {}", old_path.display()))?;

    // Read delta file
    let delta_data = std::fs::read(delta_path)
        .with_context(|| format!("Failed to read {}", delta_path.display()))?;

    if delta_data.len() < BLAKE3_SIZE * 2 {
        anyhow::bail!("Delta file too small");
    }

    // Verify old hash
    let expected_old = blake3::Hash::from_bytes(delta_data[..BLAKE3_SIZE].try_into().unwrap());
    let actual_old = blake3::hash(&old_bytes);
    if expected_old != actual_old {
        anyhow::bail!(
            "Old package hash mismatch: expected {}, got {}",
            expected_old.to_hex(),
            actual_old.to_hex()
        );
    }

    // Extract delta payload
    let bsdiff_data = &delta_data[BLAKE3_SIZE * 2..];

    // Apply delta
    let mut new_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(bsdiff_data);
    bsdiff::patch(&old_bytes, &mut cursor, &mut new_bytes)
        .context("bspatch failed")?;

    // Verify new hash
    let expected_new = blake3::Hash::from_bytes(
        delta_data[BLAKE3_SIZE..BLAKE3_SIZE * 2].try_into().unwrap()
    );
    let actual_new = blake3::hash(&new_bytes);
    if expected_new != actual_new {
        anyhow::bail!(
            "New package hash mismatch: expected {}, got {}",
            expected_new.to_hex(),
            actual_new.to_hex()
        );
    }

    // Write output
    std::fs::write(output_path, &new_bytes)
        .with_context(|| format!("Failed to write {}", output_path.display()))?;

    let elapsed = start.elapsed().as_secs_f64();
    println!("Delta applied in {:.2}s", elapsed);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_files(dir: &Path, size: usize, change_fraction: f64) -> (PathBuf, PathBuf) {
        let old_path = dir.join("old.bin");
        let new_path = dir.join("new.bin");

        let old_data: Vec<u8> = (0..size).map(|i| ((i * 7 + 13) % 256) as u8).collect();
        let mut new_data = old_data.clone();
        let change_count = (size as f64 * change_fraction) as usize;
        for i in 0..change_count {
            new_data[i] = 42;
        }

        std::fs::write(&old_path, &old_data).unwrap();
        std::fs::write(&new_path, &new_data).unwrap();

        (old_path, new_path)
    }

    #[test]
    fn test_delta_create_and_apply() {
        let tmp = TempDir::new().unwrap();
        let (old_path, new_path) = create_test_files(tmp.path(), 10_000_000, 0.01);
        let delta_path = tmp.path().join("test.delta");
        let result_path = tmp.path().join("result.bin");

        let result = generate_delta(&old_path, &new_path, &delta_path, false).unwrap();
        assert!(result.ratio > 0.0);

        apply_delta(&old_path, &delta_path, &result_path).unwrap();

        let expected = std::fs::read(&new_path).unwrap();
        let actual = std::fs::read(&result_path).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_delta_mmap() {
        let tmp = TempDir::new().unwrap();
        let (old_path, new_path) = create_test_files(tmp.path(), 10_000_000, 0.01);
        let delta_path = tmp.path().join("test.delta");

        let result = generate_delta_mmap(&old_path, &new_path, &delta_path, false).unwrap();
        assert!(result.ratio > 0.0);
    }

    #[test]
    fn test_batch_generate() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("deltas");

        let pairs: Vec<(PathBuf, PathBuf)> = (0..3)
            .map(|i| {
                let old_path = tmp.path().join(format!("old_{i}.bin"));
                let new_path = tmp.path().join(format!("new_{i}.bin"));

                let old_data: Vec<u8> = (0..10_000_000).map(|j| ((j + i) % 256) as u8).collect();
                let mut new_data = old_data.clone();
                let change_count = 100_000;
                for j in 0..change_count {
                    new_data[j] = 42;
                }

                std::fs::write(&old_path, &old_data).unwrap();
                std::fs::write(&new_path, &new_data).unwrap();

                (old_path, new_path)
            })
            .collect();

        let results = batch_generate(&pairs, &output_dir, false).unwrap();
        assert_eq!(results.len(), 3);
    }
}
