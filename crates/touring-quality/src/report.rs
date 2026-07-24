//! Report emission — `Human`, `Json`, `Toon`, `Badge` formats.
//!
//! `emit_report(score, format, writer)` writes a single `EliteScore` to any
//! `Write` implementer. Used by:
//!
//! - `touring elite check --format=human` (CLI)
//! - `touring_elite_check` MCP tool (returns JSON via `serde_json::to_string`)
//! - `touring elite badge` (CLI, returns Badge line)

use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::gate::GateStatus;
use crate::score::EliteScore;

/// Format of an emitted report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ReportFormat {
    /// Multi-line, human-readable (default for the CLI).
    #[default]
    Human,
    /// Pretty-printed JSON.
    Json,
    /// TOON (Token-Oriented Object Notation) — compact, line-based.
    Toon,
    /// Single-line badge: `💎 DIAMOND (composite=0.97)`.
    Badge,
}

impl std::str::FromStr for ReportFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "human" | "h" | "text" => Ok(Self::Human),
            "json" | "j" => Ok(Self::Json),
            "toon" | "t" => Ok(Self::Toon),
            "badge" | "b" => Ok(Self::Badge),
            other => Err(format!("unknown report format: {other}")),
        }
    }
}

/// Emit a report of `score` in `format` to `writer`.
///
/// # Errors
///
/// Returns a `std::io::Error` if writing the formatted report to `writer` fails.
pub fn emit_report<W: Write>(
    score: &EliteScore,
    format: ReportFormat,
    writer: &mut W,
) -> std::io::Result<()> {
    match format {
        ReportFormat::Human => emit_human(score, writer),
        ReportFormat::Json => emit_json(score, writer),
        ReportFormat::Toon => emit_toon(score, writer),
        ReportFormat::Badge => emit_badge(score, writer),
    }
}

fn emit_badge<W: Write>(score: &EliteScore, w: &mut W) -> std::io::Result<()> {
    writeln!(w, "{}", score.badge())
}

fn emit_human<W: Write>(score: &EliteScore, w: &mut W) -> std::io::Result<()> {
    writeln!(
        w,
        "═══════════════════════════════════════════════════════════════"
    )?;
    writeln!(w, " Touring Elite — 13-gate Composite Audit")?;
    writeln!(
        w,
        "═══════════════════════════════════════════════════════════════"
    )?;
    writeln!(w)?;
    writeln!(w, "  Badge:        {}", score.tier)?;
    writeln!(w, "  Composite:    {:.4}", score.composite)?;
    writeln!(w, "  Tier:         {}", score.tier.label())?;
    writeln!(
        w,
        "  Weights:      {:.2} total, {:.4} weighted_sum",
        score.total_weight, score.weighted_sum
    )?;
    writeln!(w)?;
    writeln!(w, "  Gates:")?;
    for g in &score.gates {
        let status_glyph = match g.status {
            GateStatus::Pass => "✓",
            GateStatus::Warn => "⚠",
            GateStatus::Advisory => "○",
            GateStatus::Fail => "✗",
            GateStatus::External => "·",
            GateStatus::Missing => "?",
        };
        writeln!(
            w,
            "    {}  {:<20}  score={:.2} weight={:.2}  {}",
            status_glyph,
            g.gate_id.slug(),
            g.score,
            g.weight,
            g.message
        )?;
    }
    writeln!(w)?;
    writeln!(
        w,
        "  Block reasons: {} | Warn reasons: {}",
        score.block_reasons().len(),
        score.warn_reasons().len()
    )?;
    writeln!(
        w,
        "  Release-ready: {}",
        if score.is_release_ready() {
            "✓ YES"
        } else {
            "✗ NO — BLOCK"
        }
    )?;
    Ok(())
}

fn emit_json<W: Write>(score: &EliteScore, w: &mut W) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(score).map_err(std::io::Error::other)?;
    writeln!(w, "{s}")
}

fn emit_toon<W: Write>(score: &EliteScore, w: &mut W) -> std::io::Result<()> {
    // Minimal TOON v0.1 — flat key=value with section headers.
    writeln!(w, "composite: {:.4}", score.composite)?;
    writeln!(w, "tier: {}", score.tier.label())?;
    writeln!(w, "release_ready: {}", score.is_release_ready())?;
    writeln!(w, "total_weight: {:.4}", score.total_weight)?;
    writeln!(w, "weighted_sum: {:.4}", score.weighted_sum)?;
    writeln!(w, "gates[{}]:", score.gates.len())?;
    for g in &score.gates {
        writeln!(
            w,
            "  - id={} status={:?} severity={:?} score={:.2} weight={:.2} elapsed_ms={}",
            g.gate_id.slug(),
            g.status,
            g.severity,
            g.score,
            g.weight,
            g.elapsed_ms
        )?;
        if !g.message.is_empty() {
            writeln!(w, "    message: \"{}\"", g.message.replace('"', "\\\""))?;
        }
        if !g.evidence.is_empty() {
            writeln!(w, "    evidence[{}]:", g.evidence.len())?;
            for e in &g.evidence {
                let line = e.line.map_or_else(|| "-".to_string(), |l| l.to_string());
                writeln!(
                    w,
                    "      - kind={:?} path={} line={} excerpt={:?}",
                    e.kind, e.path, line, e.excerpt
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Change;
    use crate::gate::{GateId, GateOutcome, GateSeverity};

    #[test]
    fn badge_format() {
        let s = EliteScore::from_gates(vec![GateOutcome::pass(
            GateId::CodeQuality,
            GateSeverity::Block,
        )]);
        let mut buf = Vec::new();
        emit_report(&s, ReportFormat::Badge, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("DIAMOND"));
    }

    #[test]
    fn parse_format() {
        assert_eq!(
            "human".parse::<ReportFormat>().unwrap(),
            ReportFormat::Human
        );
        assert_eq!("JSON".parse::<ReportFormat>().unwrap(), ReportFormat::Json);
        assert_eq!("toon".parse::<ReportFormat>().unwrap(), ReportFormat::Toon);
        assert_eq!(
            "badge".parse::<ReportFormat>().unwrap(),
            ReportFormat::Badge
        );
        assert!("foo".parse::<ReportFormat>().is_err());
    }

    #[test]
    fn human_emits_all_gates() {
        let _change = Change::new();
        let s = EliteScore::from_gates(vec![
            GateOutcome::pass(GateId::Architecture, GateSeverity::Block),
            GateOutcome::fail(GateId::Security, GateSeverity::Block, "denied"),
        ]);
        let mut buf = Vec::new();
        emit_report(&s, ReportFormat::Human, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("architecture"));
        assert!(out.contains("security"));
        assert!(out.contains("denied"));
    }
}
