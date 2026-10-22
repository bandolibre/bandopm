pub mod features;
pub mod scorecard;
pub mod sim;
pub mod tuner;
pub mod wav;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use features::extract_features;
use rayon::prelude::*;
use scorecard::{evaluate_preset, write_scorecard_reports, Scorecard};
use serde::{Deserialize, Serialize};
use sim::{simulate_and_extract, CachedGoldenSample, GoldenManifest};
use std::path::{Path, PathBuf};
use std::time::Instant;
use synth_core::{BandoneonPreset, SynthVersion};
use tuner::tune_preset;
use wav::{load_wav_mono, write_wav_mono_16bit};

#[derive(Parser)]
#[command(
    name = "synth-eval",
    about = "Autonomous evaluation, simulation, parameter tuning, and multi-version comparison harness"
)]
struct Cli {
    /// Path to golden dataset directory
    #[arg(long, default_value = "assets/samples/golden")]
    golden_dir: PathBuf,

    /// Path to preset JSON file
    #[arg(long, default_value = "assets/presets/default_bandoneon.json")]
    preset: PathBuf,

    /// Optional synthesis version override (`v1`, `v2`, `v3_aero`, `v3_beam`, `v3_transient`, `v3`, `v4_cabinet`, `v4_slot`, `v4_coupled`, `v4_celda`, `v4_zapatilla`, `v4`)
    #[arg(long)]
    version: Option<String>,

    /// Path to evaluation output directory
    #[arg(long, default_value = "target/eval")]
    out_dir: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Evaluate the current preset and generate scorecard.json & scorecard.md (default)
    Eval {
        /// Force recomputing golden WAV features cache
        #[arg(long)]
        rebuild_cache: bool,
    },
    /// Tune smooth physical scaling parameters in the preset against the train split
    Tune {
        /// Number of coordinate pattern-search rounds
        #[arg(long, default_value_t = 4)]
        rounds: usize,
    },
    /// Render a specific sample ID to WAV for side-by-side inspection
    Render {
        /// Sample ID from manifest.json (e.g. C3_sustain_mf or A2_stacc_f)
        #[arg(long, default_value = "C3_sustain_mf")]
        sample_id: String,
    },
    /// Print detailed per-hand, per-dynamic, and per-pitch-band physical diagnostics
    Diagnose,
    /// Evaluate and compare all 12 synthesis versions (`v1`..`v4`)
    Compare,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionComparisonEntry {
    pub version_id: u8,
    pub slug: String,
    pub label: String,
    pub scorecard: Scorecard,
}

fn load_or_create_preset(path: &Path, version_override: Option<&str>) -> Result<BandoneonPreset> {
    if let Some(v_str) = version_override {
        let ver = SynthVersion::from_slug(v_str).with_context(|| {
            format!(
                "Unknown synthesis version '{}'. Expected one of: v1, v2, v3_aero, v3_beam, v3_transient, v3, v4_cabinet, v4_slot, v4_coupled, v4_celda, v4_zapatilla, v4",
                v_str
            )
        })?;
        return Ok(BandoneonPreset::for_version(ver));
    }
    if path.exists() {
        let s = std::fs::read_to_string(path)?;
        let preset: BandoneonPreset = serde_json::from_str(&s)?;
        Ok(preset)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let preset = BandoneonPreset::default_bandoneon();
        std::fs::write(path, serde_json::to_string_pretty(&preset)?)?;
        Ok(preset)
    }
}

fn load_or_build_golden_cache(
    golden_dir: &Path,
    out_dir: &Path,
    rebuild: bool,
) -> Result<(u32, Vec<CachedGoldenSample>)> {
    let manifest_path = golden_dir.join("manifest.json");
    let manifest_str = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Missing manifest at {:?}", manifest_path))?;
    let manifest: GoldenManifest = serde_json::from_str(&manifest_str)?;

    let cache_path = out_dir.join("golden_features_cache.json");
    if !rebuild && cache_path.exists() {
        if let Ok(cache_str) = std::fs::read_to_string(&cache_path) {
            if let Ok(cached) = serde_json::from_str::<Vec<CachedGoldenSample>>(&cache_str) {
                if cached.len() == manifest.samples.len() {
                    return Ok((manifest.sample_rate_hz, cached));
                }
            }
        }
    }

    println!(
        "[synth-eval] Extracting acoustic features from {} golden WAV files in {:?}...",
        manifest.samples.len(),
        golden_dir
    );
    let t0 = Instant::now();
    // Read up to 1.8 seconds per WAV (86,400 frames at 48kHz) — covers onset + full steady state
    let max_frames = (manifest.sample_rate_hz as usize) * 18 / 10;

    let cached: Result<Vec<CachedGoldenSample>> = manifest
        .samples
        .par_iter()
        .map(|entry| {
            let wav_path = golden_dir.join(&entry.rel_path);
            let wav = load_wav_mono(&wav_path, Some(max_frames))?;
            let feat = extract_features(
                &wav.samples_mono,
                wav.sample_rate,
                entry.fundamental_hz,
                entry.is_staccato(),
            );
            Ok(CachedGoldenSample {
                meta: entry.clone(),
                features: feat,
            })
        })
        .collect();

    let cached = cached?;
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(&cache_path, serde_json::to_string(&cached)?)?;
    println!(
        "[synth-eval] Cached {} golden feature vectors in {:.2}s -> {:?}",
        cached.len(),
        t0.elapsed().as_secs_f32(),
        cache_path
    );
    Ok((manifest.sample_rate_hz, cached))
}

fn print_summary(card: &scorecard::Scorecard, out_dir: &Path) {
    let s = &card.scores;
    let p = &card.performance;
    let d = &card.diagnostics_signed_deltas;
    println!("=========================================================================");
    println!(
        " BANDOLIBRE SYNTH EVALUATION SCORECARD | Preset: {}",
        card.preset_name
    );
    println!("=========================================================================");
    println!(
        " Composite Score : Train = {:6.2} / 100.0  |  Validation = {:6.2} / 100.0",
        s.composite_train_score, s.composite_val_score
    );
    println!(
        " Performance     : Score = {:6.2} / 100.0  |  Headroom   = {:5.1}x real-time",
        p.performance_score, p.realtime_headroom_factor
    );
    println!(
        " Composite Loss  : Train = {:6.4}          |  Validation = {:6.4} (Reg: {:.4})",
        s.composite_train_loss, s.composite_val_loss, s.regularization_penalty
    );
    println!("-------------------------------------------------------------------------");
    println!(
        " 1. Harmonic Profile (35%) : Train {:5.2} | Val {:5.2} | Centroid mf: {:+.0} Hz",
        s.train_categories.harmonic_profile.score,
        s.val_categories.harmonic_profile.score,
        d.spectral_centroid_err_hz.mf
    );
    println!(
        " 2. Transients       (25%) : Train {:5.2} | Val {:5.2} | Rise Err: {:+.1}ms (sus), {:+.1}ms (stacc)",
        s.train_categories.transients.score,
        s.val_categories.transients.score,
        d.attack_rise_err_ms_sustain,
        d.attack_rise_err_ms_stacc
    );
    println!(
        " 3. Dual-Reed 8'+4'  (20%) : Train {:5.2} | Val {:5.2} | 4'/8' Err (mf): L {:+.1} dB, R {:+.1} dB",
        s.train_categories.dual_reed.score,
        s.val_categories.dual_reed.score,
        d.octave_4ft_to_8ft_err_db_left.mf,
        d.octave_4ft_to_8ft_err_db_right.mf
    );
    println!(
        " 4. Pitch Bend       (10%) : Train {:5.2} | Val {:5.2} | Drift p->f Err: {:+.2} cents",
        s.train_categories.pitch_bend.score,
        s.val_categories.pitch_bend.score,
        d.drift_p_to_f_err_cents
    );
    println!(
        " 5. Body Resonance   (10%) : Train {:5.2} | Val {:5.2} | Air Noise Err: {:+.1} dB",
        s.train_categories.body_resonance.score,
        s.val_categories.body_resonance.score,
        d.air_noise_floor_err_db
    );
    println!("-------------------------------------------------------------------------");
    println!(
        " CPU Performance (128-sample quantum @ {} Hz = {:.1} us real-time window):",
        p.sample_rate_hz, p.realtime_quantum_us
    );
    println!(
        "   -  1 Voice  : {:6.2} us ({:4.2}% core)",
        p.voice_1_us_per_quantum,
        100.0 * p.voice_1_us_per_quantum / p.realtime_quantum_us
    );
    println!(
        "   - 16 Voices : {:6.2} us ({:4.2}% core, budget < {:.0} us) | Perf Score: {:5.2} | Gates: {}",
        p.voice_16_us_per_quantum,
        p.core_utilization_16v_pct,
        p.voice_16_budget_us,
        p.performance_score,
        card.gates.passed_all
    );
    println!("-------------------------------------------------------------------------");
    println!(" Top Actionable Recommendations:");
    for (i, r) in card.top_actionable_recommendations.iter().enumerate() {
        println!("   {}. {}", i + 1, r);
    }
    println!("=========================================================================");
    println!(
        " Reports written to {:?} and {:?}",
        out_dir.join("scorecard.md"),
        out_dir.join("scorecard.json")
    );
}

fn run_compare_all(golden: &[CachedGoldenSample], sr: u32, out_dir: &Path) -> Result<()> {
    println!("========================================================================================================================");
    println!(" MULTI-VERSION BANDONEON SYNTHESIS COMPARISON (n={} Golden WAVs: 219 Train, 109 Held-Out Val)", golden.len());
    println!("========================================================================================================================");
    println!(
        "{:<14} | {:>7} | {:>7} | {:>7} | {:>8} | {:>8} | {:>8} | {:>8} | {:>8} | {:>9} | {:>5}",
        "Version", "Train", "Val", "Perf", "Harm(35)", "Tran(25)", "Dual(20)", "Ptch(10)", "Body(10)", "16v CPU", "Gates"
    );
    println!("------------------------------------------------------------------------------------------------------------------------");

    let mut entries = Vec::new();
    for ver in SynthVersion::ALL {
        let preset = BandoneonPreset::for_version(ver);
        let card = evaluate_preset(&preset, golden, sr, true);
        let s = &card.scores;
        let vc = &s.val_categories;
        println!(
            "{:<14} | {:>7.2} | {:>7.2} | {:>7.2} | {:>8.2} | {:>8.2} | {:>8.2} | {:>8.2} | {:>8.2} | {:>7.1}us | {:>5}",
            ver.slug(),
            s.composite_train_score,
            s.composite_val_score,
            card.performance.performance_score,
            vc.harmonic_profile.score,
            vc.transients.score,
            vc.dual_reed.score,
            vc.pitch_bend.score,
            vc.body_resonance.score,
            card.performance.voice_16_us_per_quantum,
            if card.gates.passed_all { "PASS" } else { "FAIL" }
        );
        entries.push(VersionComparisonEntry {
            version_id: ver.as_u8(),
            slug: ver.slug().to_string(),
            label: ver.label().to_string(),
            scorecard: card,
        });
    }
    println!("========================================================================================================================");

    std::fs::create_dir_all(out_dir)?;
    let cmp_json = serde_json::to_string_pretty(&entries)?;
    std::fs::write(out_dir.join("version_comparison.json"), &cmp_json)?;
    let web_cmp = Path::new("docs/version_comparison.json");
    if web_cmp.parent().map(|p| p.exists()).unwrap_or(false) {
        std::fs::write(web_cmp, &cmp_json)?;
    }

    let mut md = String::from("# Multi-Version Synthesis Comparison Report (`v1` .. `v4`)\n\n");
    md.push_str("| Version | Description | Train Score | Val Score | Perf Score | 1. Harmonic (Val) | 2. Transients (Val) | 3. Dual-Reed (Val) | 4. Pitch (Val) | 5. Body (Val) | 16-Voice CPU | Headroom | Gates |\n");
    md.push_str("| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |\n");
    for e in &entries {
        let s = &e.scorecard.scores;
        let vc = &s.val_categories;
        let p = &e.scorecard.performance;
        md.push_str(&format!(
            "| `{}` | {} | **{:.2}** | **{:.2}** | **{:.2}** | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | `{:.1} us` | `{:.1}x` | {} |\n",
            e.slug,
            e.label,
            s.composite_train_score,
            s.composite_val_score,
            p.performance_score,
            vc.harmonic_profile.score,
            vc.transients.score,
            vc.dual_reed.score,
            vc.pitch_bend.score,
            vc.body_resonance.score,
            p.voice_16_us_per_quantum,
            p.realtime_headroom_factor,
            if e.scorecard.gates.passed_all { "PASS" } else { "FAIL" }
        ));
    }
    std::fs::write(out_dir.join("version_comparison.md"), md)?;
    println!(
        " Comparison reports written to {:?} and {:?}",
        out_dir.join("version_comparison.md"),
        out_dir.join("version_comparison.json")
    );
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let preset = load_or_create_preset(&cli.preset, cli.version.as_deref())?;

    match cli.command.unwrap_or(Commands::Eval {
        rebuild_cache: false,
    }) {
        Commands::Eval { rebuild_cache } => {
            let (sr, golden) =
                load_or_build_golden_cache(&cli.golden_dir, &cli.out_dir, rebuild_cache)?;
            let t0 = Instant::now();
            let card = evaluate_preset(&preset, &golden, sr, true);
            write_scorecard_reports(&card, &cli.out_dir)?;
            print_summary(&card, &cli.out_dir);
            println!(" Evaluation completed in {:.2}s.", t0.elapsed().as_secs_f32());
        }
        Commands::Tune { rounds } => {
            let (sr, golden) = load_or_build_golden_cache(&cli.golden_dir, &cli.out_dir, false)?;
            let t0 = Instant::now();
            let (tuned_preset, card) = tune_preset(&preset, &golden, sr, rounds);
            std::fs::write(&cli.preset, serde_json::to_string_pretty(&tuned_preset)?)?;
            write_scorecard_reports(&card, &cli.out_dir)?;
            print_summary(&card, &cli.out_dir);
            println!(
                " Tuning ({} rounds) completed in {:.2}s. Saved preset to {:?}.",
                rounds,
                t0.elapsed().as_secs_f32(),
                cli.preset
            );
        }
        Commands::Render { sample_id } => {
            let (sr, golden) = load_or_build_golden_cache(&cli.golden_dir, &cli.out_dir, false)?;
            let item = golden
                .iter()
                .find(|g| g.meta.id.eq_ignore_ascii_case(&sample_id))
                .with_context(|| format!("Sample ID '{}' not found in manifest", sample_id))?;

            let (_, audio_opt) =
                simulate_and_extract(&item.meta, &item.features, &preset, sr, true);
            let rendered = audio_opt.unwrap();
            let out_wav = cli
                .out_dir
                .join("renders")
                .join(format!("{}_{}_synth.wav", item.meta.id, preset.version.slug()));
            write_wav_mono_16bit(&out_wav, sr, &rendered)?;
            println!(
                "Rendered {} samples ({:.2}s) [{}] to {:?}",
                rendered.len(),
                rendered.len() as f32 / sr as f32,
                preset.version.slug(),
                out_wav
            );
        }
        Commands::Diagnose => {
            let (sr, golden) = load_or_build_golden_cache(&cli.golden_dir, &cli.out_dir, false)?;
            scorecard::print_detailed_diagnostics(&preset, &golden, sr);
        }
        Commands::Compare => {
            let (sr, golden) = load_or_build_golden_cache(&cli.golden_dir, &cli.out_dir, false)?;
            run_compare_all(&golden, sr, &cli.out_dir)?;
        }
    }

    Ok(())
}
