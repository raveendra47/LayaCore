use crate::batch::BatchScanReport;
use crate::error::AnalyzerError;
use crate::AudioAnalysis;

pub fn format_json(analysis: &AudioAnalysis, pretty: bool) -> Result<String, AnalyzerError> {
    if pretty {
        serde_json::to_string_pretty(analysis)
            .map_err(|e| AnalyzerError::InvalidData(format!("JSON serialization error: {}", e)))
    } else {
        serde_json::to_string(analysis)
            .map_err(|e| AnalyzerError::InvalidData(format!("JSON serialization error: {}", e)))
    }
}

pub fn format_batch_json(report: &BatchScanReport, pretty: bool) -> Result<String, AnalyzerError> {
    if pretty {
        serde_json::to_string_pretty(report)
            .map_err(|e| AnalyzerError::InvalidData(format!("JSON serialization error: {}", e)))
    } else {
        serde_json::to_string(report)
            .map_err(|e| AnalyzerError::InvalidData(format!("JSON serialization error: {}", e)))
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn format_batch_csv(report: &BatchScanReport) -> String {
    let mut out = String::from(
        "Track,RelativePath,Format,Codec,SampleRateHz,BitDepth,Channels,DurationSec,IntegratedLUFS,LRA,TruePeakDBTP,SamplePeakDBFS,ClippedSamples,DynamicRangeDB,PhaseCorrelation,BPM,Key,AuthenticityVerdict,CutoffHz,ConfidencePct,Error\n",
    );

    for t in &report.tracks {
        if let Some(a) = &t.analysis {
            let m = &a.metadata;
            let bit_depth = m
                .bits_per_sample
                .map(|b| b.to_string())
                .unwrap_or_default();
            let dur = m
                .duration_seconds
                .map(|d| format!("{:.2}", d))
                .unwrap_or_default();
            let cutoff = a
                .quality
                .detected_cutoff_hz
                .map(|f| format!("{:.0}", f))
                .unwrap_or_default();
            let row = format!(
                "{},{},{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{},{:.2},{:.3},{:.0},{},{:?},{},{:.0},\n",
                csv_escape(&t.file_name),
                csv_escape(&t.relative_path),
                csv_escape(&m.format_name),
                csv_escape(&m.codec_name),
                m.sample_rate,
                bit_depth,
                m.channels,
                dur,
                a.lufs.integrated_lufs,
                a.lufs.loudness_range_lra,
                a.true_peak.max_true_peak_dbtp,
                a.time_domain.overall_peak_dbfs,
                a.time_domain.total_clipped_samples,
                a.time_domain.dynamic_range_db,
                a.stereo.phase_correlation,
                a.musical.estimated_bpm,
                csv_escape(&a.musical.estimated_key),
                a.quality.verdict,
                cutoff,
                a.quality.confidence_pct,
            );
            out.push_str(&row);
        } else {
            let err = t.error.as_deref().unwrap_or("Decode error");
            let row = format!(
                "{},{},,,,,,,,,,,,,,,,,,,{}\n",
                csv_escape(&t.file_name),
                csv_escape(&t.relative_path),
                csv_escape(err),
            );
            out.push_str(&row);
        }
    }

    out
}
