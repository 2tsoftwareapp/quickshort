//! Filtering and sorting: pure functions over the entry list.
//!
//! Kept free of I/O and UI so re-filtering on every keystroke cannot rescan the
//! folder or re-decode a thumbnail.

use crate::entry::{EntryKind, LibraryEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Name,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Sort::Newest => "Newest",
            Sort::Oldest => "Oldest",
            Sort::Name => "Name",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateFilter {
    #[default]
    All,
    Today,
    Last7Days,
    Last30Days,
}

impl DateFilter {
    pub fn label(self) -> &'static str {
        match self {
            DateFilter::All => "All time",
            DateFilter::Today => "Today",
            DateFilter::Last7Days => "Last 7 days",
            DateFilter::Last30Days => "Last 30 days",
        }
    }

    /// Oldest epoch day (UTC) the filter admits, if it admits anything bounded.
    fn cutoff_day(self, now_unix: i64) -> Option<i64> {
        let today = days_since_epoch(now_unix);
        match self {
            DateFilter::All => None,
            DateFilter::Today => Some(today),
            DateFilter::Last7Days => Some(today - 6),
            DateFilter::Last30Days => Some(today - 29),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FilterState {
    pub search: String,
    pub kind: Option<EntryKind>,
    pub date: DateFilter,
    pub tag: Option<String>,
    pub favorites_only: bool,
    pub sort: Sort,
}

/// Days since the Unix epoch. Dates are bucketed in UTC: the standard library
/// exposes no timezone data, and a wrong-by-hours "today" is better than a
/// dependency purely to shift a boundary.
pub fn days_since_epoch(unix: i64) -> i64 {
    unix.div_euclid(86_400)
}

/// `YYYY-MM-DD` (UTC) for display.
pub fn format_date(unix: i64) -> String {
    if unix <= 0 {
        return "—".to_string();
    }
    let (y, m, d) = civil_from_days(days_since_epoch(unix));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's civil-from-days: days since 1970-01-01 to a calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

impl FilterState {
    /// Entries admitted by the current search, kind, date and tag filters, in
    /// the selected order. `now_unix` is injected so date buckets are testable.
    pub fn apply<'a>(
        &self,
        entries: impl IntoIterator<Item = &'a LibraryEntry>,
        now_unix: i64,
    ) -> Vec<&'a LibraryEntry> {
        let cutoff = self.date.cutoff_day(now_unix);
        let needle = self.search.trim();
        let mut out: Vec<&LibraryEntry> = entries
            .into_iter()
            .filter(|e| e.matches(needle))
            .filter(|e| self.kind.is_none_or(|k| e.kind == k))
            .filter(|e| !self.favorites_only || e.favorite)
            .filter(|e| match &self.tag {
                Some(tag) => e.has_tag(tag),
                None => true,
            })
            .filter(|e| match cutoff {
                Some(min_day) => days_since_epoch(e.modified_unix) >= min_day,
                None => true,
            })
            .collect();

        match self.sort {
            Sort::Newest => out.sort_by(|a, b| {
                b.modified_unix
                    .cmp(&a.modified_unix)
                    .then_with(|| a.file_name.cmp(&b.file_name))
            }),
            Sort::Oldest => out.sort_by(|a, b| {
                a.modified_unix
                    .cmp(&b.modified_unix)
                    .then_with(|| a.file_name.cmp(&b.file_name))
            }),
            Sort::Name => {
                out.sort_by_key(|a| a.file_name.to_lowercase())
            }
        }
        out
    }
}
