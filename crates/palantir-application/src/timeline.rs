//! Application: Timeline Engine — temporal ordering and analysis
//!
//! DDD Layer: Application Service — detects, corrects, and analyses time-series
//! data in ontology datasets.  Three responsibilities:
//!
//!   1. Detection  — auto-identify timestamp fields (`*_at`, `*_time`, `*_date`)
//!   2. Ordering   — detect disorder; sort records into chronological order
//!   3. Analysis   — gaps, bursts, velocity, cross-entity temporal latency
//!
//! Why no external deps?  ISO 8601 strings (YYYY-MM-DDTHH:MM:SS) sort correctly
//! as plain strings, so ordering needs no parsing.  Only gap/latency calculations
//! require conversion to seconds.

use crate::infrastructure::pipeline::dataset::{Dataset, Value};
use std::collections::HashMap;

// ─── Timestamp utilities ──────────────────────────────────────────────────────

/// Parse "YYYY-MM-DDTHH:MM:SS" (or "YYYY-MM-DD HH:MM:SS") to seconds since
/// a fixed epoch.  Returns None for any unparseable string.
pub fn parse_ts(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 10 {
        return None;
    }
    if s.chars().nth(4) != Some('-') || s.chars().nth(7) != Some('-') {
        return None;
    }

    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;

    let (h, m, sec) = if s.len() >= 19 {
        match s.chars().nth(10) {
            Some('T') | Some(' ') => (
                s[11..13].parse::<i64>().ok()?,
                s[14..16].parse::<i64>().ok()?,
                s[17..19].parse::<i64>().ok()?,
            ),
            _ => return None,
        }
    } else {
        (0, 0, 0)
    };

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // Days-in-month table (good enough for 2000-2099 with simple leap year rule)
    let dim: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);

    // Days from epoch (1970-01-01) to start of year
    let mut days: i64 =
        (year - 1970) * 365 + (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
    // Days from start of year to start of month
    for mi in 0..(month - 1) as usize {
        days += dim[mi];
        if mi == 1 && leap {
            days += 1;
        }
    }
    days += day - 1;

    Some(days * 86400 + h * 3600 + m * 60 + sec)
}

/// Human-readable duration: "45s", "14.3m", "2.1h", "3.0d"
pub fn fmt_duration(secs: i64) -> String {
    match secs.abs() {
        s if s < 60 => format!("{}s", s),
        s if s < 3600 => format!("{:.1}m", s as f64 / 60.0),
        s if s < 86400 => format!("{:.1}h", s as f64 / 3600.0),
        s => format!("{:.1}d", s as f64 / 86400.0),
    }
}

// ─── Detection ────────────────────────────────────────────────────────────────

const TS_SUFFIXES: &[&str] = &["_at", "_time", "_date", "_on"];

/// Auto-detect timestamp fields in a dataset.
/// A field qualifies if:  name ends with a temporal suffix  AND
///                        ≥ 80% of non-empty String values parse as ISO 8601.
pub fn detect_ts_fields(dataset: &Dataset) -> Vec<String> {
    if dataset.records.is_empty() {
        return vec![];
    }

    // Collect candidate field names (ends with temporal suffix)
    let mut candidates: Vec<String> = dataset
        .records
        .first()
        .map(|r| {
            let mut names: Vec<_> = r
                .fields
                .keys()
                .filter(|k| TS_SUFFIXES.iter().any(|s| k.ends_with(s)) || k.contains("timestamp"))
                .cloned()
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();

    // Verify parseable ratio
    candidates.retain(|field| {
        let mut total = 0usize;
        let mut ok = 0usize;
        for rec in &dataset.records {
            if let Some(Value::String(s)) = rec.get(field) {
                if !s.is_empty() {
                    total += 1;
                }
                if parse_ts(s).is_some() {
                    ok += 1;
                }
            }
        }
        total > 0 && ok * 100 / total >= 80
    });

    candidates
}

// ─── Disorder analysis ────────────────────────────────────────────────────────

/// Evidence that a specific pair of adjacent records is out of order.
pub struct Violation {
    pub row_a: usize,
    pub id_a: String,
    pub ts_a: String,
    pub row_b: usize,
    pub id_b: String,
    pub ts_b: String,
}

pub struct DisorderReport {
    pub field: String,
    pub total: usize,
    pub oo_count: usize, // consecutive pairs where ts[i] > ts[i+1]
    pub oo_pct: f64,
    pub violations: Vec<Violation>, // first few examples
}

/// Count out-of-order adjacent transitions and collect examples.
pub fn analyse_disorder(dataset: &Dataset, field: &str) -> DisorderReport {
    let mut oo_count = 0usize;
    let mut violations = Vec::new();

    for i in 0..dataset.records.len().saturating_sub(1) {
        let a = &dataset.records[i];
        let b = &dataset.records[i + 1];
        let ts_a = a.get(field).and_then(Value::as_str).unwrap_or("");
        let ts_b = b.get(field).and_then(Value::as_str).unwrap_or("");
        if !ts_a.is_empty() && !ts_b.is_empty() && ts_a > ts_b {
            oo_count += 1;
            if violations.len() < 4 {
                violations.push(Violation {
                    row_a: i + 1,
                    id_a: a.id.clone(),
                    ts_a: ts_a.to_string(),
                    row_b: i + 2,
                    id_b: b.id.clone(),
                    ts_b: ts_b.to_string(),
                });
            }
        }
    }

    let total = dataset.records.len();
    let pairs = total.saturating_sub(1).max(1);
    DisorderReport {
        field: field.to_string(),
        total,
        oo_count,
        oo_pct: oo_count as f64 / pairs as f64 * 100.0,
        violations,
    }
}

// ─── Sorting ──────────────────────────────────────────────────────────────────

/// Sort a Dataset's records in-place by a timestamp field (ascending).
///
/// ISO 8601 strings sort correctly lexicographically, so no parsing needed.
/// Records with missing/unparseable timestamps are placed at the end.
pub fn sort_dataset(dataset: &mut Dataset, field: &str) {
    dataset.records.sort_by(|a, b| {
        let ta = a.get(field).and_then(Value::as_str).unwrap_or("");
        let tb = b.get(field).and_then(Value::as_str).unwrap_or("");
        ta.cmp(tb)
    });
}

// ─── Timeline statistics ──────────────────────────────────────────────────────

pub struct TimelineStats {
    pub field: String,
    pub first_id: String,
    pub first_ts: String,
    pub last_id: String,
    pub last_ts: String,
    pub span_secs: i64,
    pub count: usize,
    pub avg_interval_secs: f64,
    pub min_interval_secs: i64,
    pub max_interval_secs: i64,
}

/// Compute basic timeline statistics from a (sorted) dataset.
pub fn compute_stats(dataset: &Dataset, field: &str) -> Option<TimelineStats> {
    // Collect (ts_secs, id) pairs for records that have a valid timestamp
    let mut pts: Vec<(i64, String, String)> = dataset
        .records
        .iter()
        .filter_map(|r| {
            let ts_str = r.get(field).and_then(Value::as_str)?;
            let secs = parse_ts(ts_str)?;
            Some((secs, r.id.clone(), ts_str.to_string()))
        })
        .collect();

    if pts.len() < 2 {
        return None;
    }
    pts.sort_by_key(|p| p.0);

    let intervals: Vec<i64> = pts.windows(2).map(|w| w[1].0 - w[0].0).collect();

    let avg = intervals.iter().sum::<i64>() as f64 / intervals.len() as f64;
    let min = *intervals.iter().min().unwrap();
    let max = *intervals.iter().max().unwrap();

    Some(TimelineStats {
        field: field.to_string(),
        first_id: pts.first().unwrap().1.clone(),
        first_ts: pts.first().unwrap().2.clone(),
        last_id: pts.last().unwrap().1.clone(),
        last_ts: pts.last().unwrap().2.clone(),
        span_secs: pts.last().unwrap().0 - pts.first().unwrap().0,
        count: pts.len(),
        avg_interval_secs: avg,
        min_interval_secs: min,
        max_interval_secs: max,
    })
}

// ─── Gap detection ────────────────────────────────────────────────────────────

pub struct Gap {
    pub prev_id: String,
    pub next_id: String,
    pub prev_ts: String,
    pub next_ts: String,
    pub gap_secs: i64,
}

/// Find consecutive pairs with a gap larger than `min_gap_secs`.
/// Dataset must already be sorted.
pub fn detect_gaps(dataset: &Dataset, field: &str, min_gap_secs: i64) -> Vec<Gap> {
    let pts: Vec<_> = dataset
        .records
        .iter()
        .filter_map(|r| {
            let ts_str = r.get(field).and_then(Value::as_str)?;
            let secs = parse_ts(ts_str)?;
            Some((secs, r.id.as_str(), ts_str))
        })
        .collect();

    let mut gaps = Vec::new();
    for w in pts.windows(2) {
        let gap = w[1].0 - w[0].0;
        if gap >= min_gap_secs {
            gaps.push(Gap {
                prev_id: w[0].1.to_string(),
                next_id: w[1].1.to_string(),
                prev_ts: w[0].2.to_string(),
                next_ts: w[1].2.to_string(),
                gap_secs: gap,
            });
        }
    }
    gaps
}

// ─── Daily histogram ──────────────────────────────────────────────────────────

pub struct DayBucket {
    pub day: String, // "2024-01-02"
    pub count: usize,
    pub ids: Vec<String>,
}

/// Group records by calendar day (first 10 chars of timestamp).
/// Returns buckets sorted chronologically.
pub fn daily_histogram(dataset: &Dataset, field: &str) -> Vec<DayBucket> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for rec in &dataset.records {
        if let Some(Value::String(ts)) = rec.get(field) {
            if ts.len() >= 10 {
                map.entry(ts[..10].to_string())
                    .or_default()
                    .push(rec.id.clone());
            }
        }
    }
    let mut buckets: Vec<DayBucket> = map
        .into_iter()
        .map(|(day, ids)| DayBucket {
            count: ids.len(),
            day,
            ids,
        })
        .collect();
    buckets.sort_by(|a, b| a.day.cmp(&b.day));
    buckets
}

// ─── Cross-entity latency ─────────────────────────────────────────────────────

pub struct LatencyEntry {
    pub anchor_id: String,
    pub anchor_ts: String,
    pub event_id: String,
    pub event_ts: String,
    pub latency_secs: i64,
}

/// Compute per-anchor temporal latency to a related event dataset.
///
/// Example:  anchor = orders (ordered_at)
///           events = payments (paid_at, has order_id FK)
///           join_field = "order_id"
/// Returns one row per anchor that has a matching event.
/// Results sorted by latency descending (largest delays first).
pub fn compute_latency(
    anchor: &Dataset,
    anchor_ts_field: &str,
    events: &Dataset,
    join_field: &str,
    event_ts_field: &str,
) -> Vec<LatencyEntry> {
    // Index events by their FK value
    let mut event_index: HashMap<String, (&str, String)> = HashMap::new(); // fk_val → (event_id, event_ts)
    for ev in &events.records {
        if let Some(fk_val) = ev.get(join_field).map(|v| v.to_string()) {
            if let Some(Value::String(ets)) = ev.get(event_ts_field) {
                event_index
                    .entry(fk_val)
                    .or_insert_with(|| (&ev.id, ets.clone()));
            }
        }
    }

    let mut rows: Vec<LatencyEntry> = anchor
        .records
        .iter()
        .filter_map(|anc| {
            let ats = anc.get(anchor_ts_field).and_then(Value::as_str)?;
            let (ev_id, ev_ts) = event_index.get(&anc.id)?;
            let a_secs = parse_ts(ats)?;
            let e_secs = parse_ts(ev_ts)?;
            Some(LatencyEntry {
                anchor_id: anc.id.clone(),
                anchor_ts: ats.to_string(),
                event_id: ev_id.to_string(),
                event_ts: ev_ts.clone(),
                latency_secs: e_secs - a_secs,
            })
        })
        .collect();

    rows.sort_by(|a, b| b.latency_secs.cmp(&a.latency_secs));
    rows
}
