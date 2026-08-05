use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use unicode_normalization::UnicodeNormalization;

/// Canonical advisory module schema version for serialization compatibility.
pub const ADVISORY_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Severity
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AdvisorySeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl AdvisorySeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Numeric weight for deterministic ordering (higher = more severe).
    pub fn weight(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

impl fmt::Display for AdvisorySeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Normalized package coordinates
// ---------------------------------------------------------------------------

/// Manager-agnostic, normalized package identity.
///
/// Preserves external package identity without locale-dependent transformations.
/// Normalization applies Unicode NFC and trims ASCII edge whitespace; case is
/// preserved per-ecosystem convention.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PackageCoordinates {
    /// Ecosystem or package-manager family identifier (e.g. "cargo", "npm",
    /// "pypi", "homebrew", "rubygems").
    pub ecosystem: String,
    /// Optional scope/namespace (e.g. "@types" for npm, "crates.io" is empty).
    #[serde(default)]
    pub scope: Option<String>,
    /// Canonical package name within the ecosystem.
    pub name: String,
}

impl PackageCoordinates {
    pub fn new(ecosystem: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            ecosystem: normalize_ecosystem(&ecosystem.into()),
            scope: None,
            name: normalize_package_name(&name.into()),
        }
    }

    pub fn with_scope(
        ecosystem: impl Into<String>,
        scope: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            ecosystem: normalize_ecosystem(&ecosystem.into()),
            scope: Some(normalize_package_name(&scope.into())),
            name: normalize_package_name(&name.into()),
        }
    }

    /// Return the fully-qualified identity string used for display and matching.
    /// Format: `scope/name` when scope is present, otherwise `name`.
    pub fn qualified_name(&self) -> String {
        if let Some(ref scope) = self.scope {
            format!("{}/{}", scope, self.name)
        } else {
            self.name.clone()
        }
    }

    /// Return the canonical identifier used for caching and matching.
    /// Format: `ecosystem:qualified_name`.
    pub fn canonical_id(&self) -> String {
        format!("{}:{}", self.ecosystem, self.qualified_name())
    }
}

impl fmt::Display for PackageCoordinates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical_id())
    }
}

// ---------------------------------------------------------------------------
// Source / provenance metadata
// ---------------------------------------------------------------------------

/// Advisory data source identifier.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorySource {
    /// Source provider identifier (e.g. "osv", "github_advisory", "nvd").
    pub provider: String,
    /// Optional data feed or sub-source identifier.
    #[serde(default)]
    pub feed: Option<String>,
    /// Source schema or format version.
    #[serde(default)]
    pub schema_version: Option<String>,
}

impl AdvisorySource {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: normalize_source_provider(&provider.into()),
            feed: None,
            schema_version: None,
        }
    }

    pub fn with_feed(provider: impl Into<String>, feed: impl Into<String>) -> Self {
        Self {
            provider: normalize_source_provider(&provider.into()),
            feed: Some(normalize_source_provider(&feed.into())),
            schema_version: None,
        }
    }

    /// Deterministic source key for cache identity.
    pub fn source_key(&self) -> String {
        if let Some(ref feed) = self.feed {
            format!("{}:{}", self.provider, feed)
        } else {
            self.provider.clone()
        }
    }
}

impl fmt::Display for AdvisorySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.source_key())
    }
}

// ---------------------------------------------------------------------------
// Affected range representation
// ---------------------------------------------------------------------------

/// Version range affected by an advisory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AffectedRange {
    /// Exact version affected.
    Exact { version: String },
    /// Range: `>=lower` (inclusive) and optionally `<upper` (exclusive).
    Range {
        lower: String,
        #[serde(default)]
        lower_inclusive: bool,
        #[serde(default)]
        upper: Option<String>,
        #[serde(default)]
        upper_inclusive: bool,
    },
    /// Wildcard: all versions.
    All,
    /// Raw ecosystem-specific range expression preserved as received.
    Raw { expression: String },
}

impl AffectedRange {
    pub fn exact(version: impl Into<String>) -> Self {
        Self::Exact {
            version: normalize_external_value(&version.into()),
        }
    }

    pub fn range_gte(lower: impl Into<String>) -> Self {
        Self::Range {
            lower: normalize_external_value(&lower.into()),
            lower_inclusive: true,
            upper: None,
            upper_inclusive: false,
        }
    }

    pub fn range(lower: impl Into<String>, upper: impl Into<String>) -> Self {
        Self::Range {
            lower: normalize_external_value(&lower.into()),
            lower_inclusive: true,
            upper: Some(normalize_external_value(&upper.into())),
            upper_inclusive: false,
        }
    }
}

impl fmt::Display for AffectedRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact { version } => write!(f, "{}", version),
            Self::Range {
                lower,
                lower_inclusive,
                upper,
                upper_inclusive,
            } => {
                let lo = if *lower_inclusive {
                    format!(">={}", lower)
                } else {
                    format!(">{}", lower)
                };
                if let Some(up) = upper {
                    let hi = if *upper_inclusive {
                        format!("<={}", up)
                    } else {
                        format!("<{}", up)
                    };
                    write!(f, "{} AND {}", lo, hi)
                } else {
                    write!(f, "{}", lo)
                }
            }
            Self::All => write!(f, "*"),
            Self::Raw { expression } => write!(f, "raw({})", expression),
        }
    }
}

// ---------------------------------------------------------------------------
// Fixed version metadata
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryFixedVersion {
    pub version: String,
    /// Optional verification URL (e.g. release tag, commit).
    #[serde(default)]
    pub verification_url: Option<String>,
}

impl AdvisoryFixedVersion {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: normalize_external_value(&version.into()),
            verification_url: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Advisory record (manager-agnostic)
// ---------------------------------------------------------------------------

/// Serializable, manager-agnostic security advisory record.
///
/// This is the canonical domain model consumed by matching, cache, and
/// future UI surfaces. It is deliberately separate from doctor findings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryRecord {
    /// Schema version of this record format.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// Stable advisory identifier from the source (e.g. "CVE-2024-1234",
    /// "GHSA-xxxx-xxxx-xxxx", or OSV advisory ID).
    pub advisory_id: String,

    /// Normalized package identity.
    pub package: PackageCoordinates,

    /// Affected version range.
    pub affected_range: AffectedRange,

    /// Severity classification.
    pub severity: AdvisorySeverity,

    /// Optional CVSS score (0.0 – 10.0).
    #[serde(default)]
    pub cvss_score: Option<f32>,

    /// Human-readable summary.
    pub summary: String,

    /// Optional detailed description.
    #[serde(default)]
    pub description: Option<String>,

    /// Optional fixed version information.
    #[serde(default)]
    pub fixed_version: Option<AdvisoryFixedVersion>,

    /// Optional references (URLs).
    #[serde(default)]
    pub references: Vec<String>,

    /// Advisory source and provenance.
    pub source: AdvisorySource,

    /// Epoch milliseconds when this record was fetched.
    pub fetched_at_epoch_ms: i64,

    /// Epoch milliseconds when this record expires from cache.
    pub expires_at_epoch_ms: i64,
}

fn default_schema_version() -> u32 {
    ADVISORY_SCHEMA_VERSION
}

impl AdvisoryRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        advisory_id: impl Into<String>,
        package: PackageCoordinates,
        affected_range: AffectedRange,
        severity: AdvisorySeverity,
        summary: impl Into<String>,
        source: AdvisorySource,
        fetched_at: i64,
        expires_at: i64,
    ) -> Self {
        Self {
            schema_version: ADVISORY_SCHEMA_VERSION,
            advisory_id: normalize_advisory_id(&advisory_id.into()),
            package,
            affected_range,
            severity,
            cvss_score: None,
            summary: summary.into(),
            description: None,
            fixed_version: None,
            references: Vec::new(),
            source,
            fetched_at_epoch_ms: fetched_at,
            expires_at_epoch_ms: expires_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

/// Normalize a package name by trimming ASCII edge whitespace and applying
/// Unicode NFC. Does not apply locale-dependent case conversion.
pub fn normalize_package_name(name: &str) -> String {
    let trimmed = name.trim_matches(|c: char| c.is_ascii_whitespace());
    trimmed.nfc().collect()
}

/// Normalize an ecosystem identifier to lowercase ASCII.
pub fn normalize_ecosystem(ecosystem: &str) -> String {
    ecosystem
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .to_ascii_lowercase()
}

/// Normalize a source provider identifier to lowercase ASCII.
pub fn normalize_source_provider(provider: &str) -> String {
    provider
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .to_ascii_lowercase()
}

pub fn normalize_advisory_id(advisory_id: &str) -> String {
    normalize_external_value(advisory_id)
}

fn normalize_external_value(value: &str) -> String {
    value
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .nfc()
        .collect()
}

pub fn contains_control_chars(s: &str) -> bool {
    s.chars().any(char::is_control)
}

pub fn contains_forbidden_prose_control_chars(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

pub fn affected_range_is_canonical(range: &AffectedRange) -> bool {
    let valid_value = |value: &str| {
        !value.trim().is_empty()
            && value == normalize_external_value(value)
            && !contains_control_chars(value)
    };
    match range {
        AffectedRange::Exact { version } => valid_value(version),
        AffectedRange::Range { lower, upper, .. } => {
            valid_value(lower) && upper.as_deref().is_none_or(valid_value)
        }
        AffectedRange::Raw { expression } => valid_value(expression),
        AffectedRange::All => true,
    }
}

/// Validate that an advisory record has the required fields populated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvisoryValidationError {
    EmptyAdvisoryId,
    EmptyPackageName,
    EmptyEcosystem,
    InvalidSchemaVersion,
    MissingTimestamps,
    NegativeTimestamps,
    InvalidTimestampOrder,
    InvalidCvssScore,
    EmptySummary,
    EmptySourceProvider,
    NonCanonicalAdvisoryId,
    NonCanonicalEcosystem,
    NonCanonicalPackageName,
    NonCanonicalScope,
    NonCanonicalSourceProvider,
    NonCanonicalSourceFeed,
    ControlCharacterPresent,
    InvalidAffectedRange,
    InvalidFixedVersion,
    InvalidSourceMetadata,
    InvalidReference,
}

impl fmt::Display for AdvisoryValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyAdvisoryId => write!(f, "advisory_id is empty"),
            Self::EmptyPackageName => write!(f, "package name is empty"),
            Self::EmptyEcosystem => write!(f, "ecosystem is empty"),
            Self::InvalidSchemaVersion => write!(f, "unsupported schema_version"),
            Self::MissingTimestamps => write!(f, "fetched_at or expires_at is zero"),
            Self::NegativeTimestamps => write!(f, "timestamp is negative"),
            Self::InvalidTimestampOrder => write!(f, "expires_at must be after fetched_at"),
            Self::InvalidCvssScore => write!(f, "cvss_score must be finite and between 0 and 10"),
            Self::EmptySummary => write!(f, "summary is empty"),
            Self::EmptySourceProvider => write!(f, "source provider is empty"),
            Self::NonCanonicalAdvisoryId => write!(f, "advisory ID is not canonical"),
            Self::NonCanonicalEcosystem => write!(f, "ecosystem is not canonical"),
            Self::NonCanonicalPackageName => write!(f, "package name is not canonical"),
            Self::NonCanonicalScope => write!(f, "scope is not canonical"),
            Self::NonCanonicalSourceProvider => write!(f, "source provider is not canonical"),
            Self::NonCanonicalSourceFeed => write!(f, "source feed is not canonical"),
            Self::ControlCharacterPresent => write!(f, "control character present in text field"),
            Self::InvalidAffectedRange => write!(f, "affected range is invalid or empty"),
            Self::InvalidFixedVersion => write!(f, "fixed version is invalid or empty"),
            Self::InvalidSourceMetadata => write!(f, "source metadata is invalid or empty"),
            Self::InvalidReference => write!(f, "advisory reference is invalid or empty"),
        }
    }
}

impl std::error::Error for AdvisoryValidationError {}

/// Validate an advisory record. Returns `Ok(())` or the first validation error.
pub fn validate_advisory(record: &AdvisoryRecord) -> Result<(), AdvisoryValidationError> {
    if record.advisory_id.trim().is_empty() {
        return Err(AdvisoryValidationError::EmptyAdvisoryId);
    }
    if record.advisory_id != normalize_advisory_id(&record.advisory_id) {
        return Err(AdvisoryValidationError::NonCanonicalAdvisoryId);
    }
    if contains_control_chars(&record.advisory_id) {
        return Err(AdvisoryValidationError::ControlCharacterPresent);
    }

    if record.package.name.trim().is_empty() {
        return Err(AdvisoryValidationError::EmptyPackageName);
    }
    if record.package.name != normalize_package_name(&record.package.name) {
        return Err(AdvisoryValidationError::NonCanonicalPackageName);
    }
    if contains_control_chars(&record.package.name) {
        return Err(AdvisoryValidationError::ControlCharacterPresent);
    }

    if record.package.ecosystem.trim().is_empty() {
        return Err(AdvisoryValidationError::EmptyEcosystem);
    }
    if record.package.ecosystem != normalize_ecosystem(&record.package.ecosystem) {
        return Err(AdvisoryValidationError::NonCanonicalEcosystem);
    }
    if contains_control_chars(&record.package.ecosystem) {
        return Err(AdvisoryValidationError::ControlCharacterPresent);
    }

    if let Some(ref scope) = record.package.scope {
        if scope.is_empty() || scope != &normalize_package_name(scope) {
            return Err(AdvisoryValidationError::NonCanonicalScope);
        }
        if contains_control_chars(scope) {
            return Err(AdvisoryValidationError::ControlCharacterPresent);
        }
    }

    if record.summary.trim().is_empty() {
        return Err(AdvisoryValidationError::EmptySummary);
    }
    if contains_forbidden_prose_control_chars(&record.summary) {
        return Err(AdvisoryValidationError::ControlCharacterPresent);
    }
    if record
        .description
        .as_ref()
        .is_some_and(|desc| contains_forbidden_prose_control_chars(desc))
    {
        return Err(AdvisoryValidationError::ControlCharacterPresent);
    }

    if record.source.provider.trim().is_empty() {
        return Err(AdvisoryValidationError::EmptySourceProvider);
    }
    if record.source.provider != normalize_source_provider(&record.source.provider) {
        return Err(AdvisoryValidationError::NonCanonicalSourceProvider);
    }
    if contains_control_chars(&record.source.provider) {
        return Err(AdvisoryValidationError::ControlCharacterPresent);
    }

    if let Some(feed) = record.source.feed.as_ref() {
        if feed.is_empty() || feed != &normalize_source_provider(feed) {
            return Err(AdvisoryValidationError::NonCanonicalSourceFeed);
        }
        if contains_control_chars(feed) {
            return Err(AdvisoryValidationError::ControlCharacterPresent);
        }
    }
    if record
        .source
        .schema_version
        .as_ref()
        .is_some_and(|version| version.trim().is_empty() || contains_control_chars(version))
    {
        return Err(AdvisoryValidationError::InvalidSourceMetadata);
    }

    if !affected_range_is_canonical(&record.affected_range) {
        return Err(AdvisoryValidationError::InvalidAffectedRange);
    }

    if let Some(fixed) = record.fixed_version.as_ref()
        && (fixed.version.trim().is_empty()
            || fixed.version != normalize_external_value(&fixed.version)
            || contains_control_chars(&fixed.version)
            || fixed
                .verification_url
                .as_ref()
                .is_some_and(|url| url.trim().is_empty() || contains_control_chars(url)))
    {
        return Err(AdvisoryValidationError::InvalidFixedVersion);
    }
    if record
        .references
        .iter()
        .any(|reference| reference.trim().is_empty() || contains_control_chars(reference))
    {
        return Err(AdvisoryValidationError::InvalidReference);
    }
    if record.schema_version != ADVISORY_SCHEMA_VERSION {
        return Err(AdvisoryValidationError::InvalidSchemaVersion);
    }
    if record.fetched_at_epoch_ms == 0 || record.expires_at_epoch_ms == 0 {
        return Err(AdvisoryValidationError::MissingTimestamps);
    }
    if record.fetched_at_epoch_ms < 0 || record.expires_at_epoch_ms < 0 {
        return Err(AdvisoryValidationError::NegativeTimestamps);
    }
    if record.expires_at_epoch_ms <= record.fetched_at_epoch_ms {
        return Err(AdvisoryValidationError::InvalidTimestampOrder);
    }
    if record
        .cvss_score
        .is_some_and(|score| !score.is_finite() || !(0.0..=10.0).contains(&score))
    {
        return Err(AdvisoryValidationError::InvalidCvssScore);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Deterministic cache keys
// ---------------------------------------------------------------------------

/// Build a deterministic cache key for a single advisory record.
/// Format: `advisory:<source_key>:<advisory_id>`
///
/// All components are lowercase ASCII. This key is stable across fetches
/// for the same advisory from the same source.
pub fn build_advisory_cache_key(record: &AdvisoryRecord) -> String {
    format!(
        "advisory:{}:{}",
        record.source.source_key().to_lowercase(),
        record.advisory_id.to_lowercase()
    )
}

/// Build a deterministic cache key for package-scoped advisory lookup.
/// Format: `pkg_advisory:<ecosystem>:<qualified_name>`
pub fn build_package_advisory_key(coords: &PackageCoordinates) -> String {
    format!(
        "pkg_advisory:{}:{}",
        coords.ecosystem.to_lowercase(),
        coords.qualified_name()
    )
}

// ---------------------------------------------------------------------------
// Ordering and deduplication
// ---------------------------------------------------------------------------

/// Primary sort key for advisory records: severity descending, then
/// advisory_id ascending for deterministic output.
pub fn advisory_sort_key(record: &AdvisoryRecord) -> (u8, String) {
    // Negate severity weight so higher severity sorts first.
    let severity_order = 255 - record.severity.weight();
    (severity_order, record.advisory_id.to_lowercase())
}

/// Sort a slice of advisory records deterministically in-place.
pub fn sort_advisories(records: &mut [AdvisoryRecord]) {
    records.sort_by(|a, b| {
        advisory_sort_key(a)
            .cmp(&advisory_sort_key(b))
            .then_with(|| a.package.canonical_id().cmp(&b.package.canonical_id()))
    });
}

/// Deduplicate advisory records by `(source_key, advisory_id)`, keeping the
/// first occurrence (typically the freshest after sort).
pub fn deduplicate_advisories(records: &[AdvisoryRecord]) -> Vec<AdvisoryRecord> {
    let mut seen = HashSet::new();
    records
        .iter()
        .filter(|record| {
            let key = format!(
                "{}:{}",
                record.source.source_key().to_lowercase(),
                record.advisory_id.to_lowercase()
            );
            seen.insert(key)
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Freshness / TTL / cache behavior
// ---------------------------------------------------------------------------

/// Freshness state of cached advisory data relative to the current time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    /// Cache data is within TTL.
    Fresh,
    /// Cache data has exceeded TTL but is still usable offline.
    Stale,
    /// No cache data available.
    Missing,
}

impl FreshnessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Missing => "missing",
        }
    }

    /// Whether this state permits offline-safe reads.
    pub fn can_use_offline(self) -> bool {
        matches!(self, Self::Fresh | Self::Stale)
    }
}

/// TTL thresholds for advisory cache freshness evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FreshnessThresholds {
    /// Default TTL in milliseconds for advisory cache entries.
    pub default_ttl_ms: i64,
}

impl Default for FreshnessThresholds {
    fn default() -> Self {
        Self {
            default_ttl_ms: 24 * 60 * 60 * 1000, // 24 hours
        }
    }
}

impl FreshnessThresholds {
    /// Evaluate freshness of a single advisory record against the current time.
    pub fn evaluate(&self, now_ms: i64, record: &AdvisoryRecord) -> FreshnessState {
        if record.expires_at_epoch_ms <= 0 {
            return FreshnessState::Missing;
        }
        if now_ms < record.expires_at_epoch_ms {
            FreshnessState::Fresh
        } else {
            FreshnessState::Stale
        }
    }

    /// Evaluate freshness for a collection. Returns the most degraded state.
    pub fn evaluate_batch(&self, now_ms: i64, records: &[AdvisoryRecord]) -> FreshnessState {
        if records.is_empty() {
            return FreshnessState::Missing;
        }
        let mut worst = FreshnessState::Fresh;
        for record in records {
            let state = self.evaluate(now_ms, record);
            worst = std::cmp::min(worst, state);
        }
        worst
    }

    /// Compute an expires_at timestamp from a fetched_at timestamp.
    pub fn compute_expiry(&self, fetched_at_ms: i64) -> i64 {
        fetched_at_ms + self.default_ttl_ms
    }
}

impl PartialOrd for FreshnessState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FreshnessState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let to_order = |s: Self| match s {
            FreshnessState::Fresh => 2,
            FreshnessState::Stale => 1,
            FreshnessState::Missing => 0,
        };
        to_order(*self).cmp(&to_order(*other))
    }
}

// ---------------------------------------------------------------------------
// Storage-facing cache record
// ---------------------------------------------------------------------------

/// Flat storage record for SQLite persistence. This type is designed to map
/// directly to the `security_advisories` table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdvisoryCacheRecord {
    /// Deterministic cache key.
    pub cache_key: String,
    /// Advisory ID from source.
    pub advisory_id: String,
    /// Ecosystem identifier.
    pub ecosystem: String,
    /// Optional scope.
    #[serde(default)]
    pub scope: Option<String>,
    /// Package name.
    pub package_name: String,
    /// Affected range serialized as JSON.
    pub affected_range_json: String,
    /// Severity as string.
    pub severity: String,
    /// Optional CVSS score.
    #[serde(default)]
    pub cvss_score: Option<f32>,
    /// Summary text.
    pub summary: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Fixed version or null.
    #[serde(default)]
    pub fixed_version: Option<String>,
    /// Source provider.
    pub source_provider: String,
    /// Optional source feed.
    #[serde(default)]
    pub source_feed: Option<String>,
    /// Fetched timestamp (epoch ms).
    pub fetched_at_epoch_ms: i64,
    /// Expiry timestamp (epoch ms).
    pub expires_at_epoch_ms: i64,
}

impl AdvisoryCacheRecord {
    /// Convert a domain `AdvisoryRecord` to a flat cache record.
    pub fn from_advisory(record: &AdvisoryRecord) -> Self {
        Self {
            cache_key: build_advisory_cache_key(record),
            advisory_id: record.advisory_id.clone(),
            ecosystem: record.package.ecosystem.clone(),
            scope: record.package.scope.clone(),
            package_name: record.package.name.clone(),
            affected_range_json: serde_json::to_string(&record.affected_range).unwrap_or_default(),
            severity: record.severity.as_str().to_string(),
            cvss_score: record.cvss_score,
            summary: record.summary.clone(),
            description: record.description.clone(),
            fixed_version: record.fixed_version.as_ref().map(|fv| fv.version.clone()),
            source_provider: record.source.provider.clone(),
            source_feed: record.source.feed.clone(),
            fetched_at_epoch_ms: record.fetched_at_epoch_ms,
            expires_at_epoch_ms: record.expires_at_epoch_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Storage trait (interface for SQLite persistence)
// ---------------------------------------------------------------------------

/// Storage trait for advisory cache records.
///
/// This trait is the integration contract for the SQLite-backed persistence
/// layer.
pub trait AdvisoryCacheStore: Send + Sync {
    /// Insert or replace advisory cache records.
    fn upsert_advisories(&self, records: &[AdvisoryCacheRecord]) -> Result<usize, String>;

    /// Retrieve all cached advisories for a package.
    fn get_advisories_for_package(
        &self,
        ecosystem: &str,
        package_name: &str,
    ) -> Result<Vec<AdvisoryCacheRecord>, String>;

    /// Retrieve all cached advisories from a specific source.
    fn get_advisories_by_source(
        &self,
        source_provider: &str,
    ) -> Result<Vec<AdvisoryCacheRecord>, String>;

    /// Remove expired records before a given timestamp.
    fn prune_expired(&self, before_epoch_ms: i64) -> Result<usize, String>;

    /// Remove all cached advisories.
    fn clear_all(&self) -> Result<(), String>;

    /// Count total cached advisory records.
    fn count(&self) -> Result<usize, String>;
}

// ---------------------------------------------------------------------------
// Future refresh and evaluation contracts
// ---------------------------------------------------------------------------

/// Request to refresh advisory data from a source.
///
/// This is a future-facing contract. No network access is performed in this
/// module; the request is routed to an orchestration task by the coordinator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryRefreshRequest {
    /// Source provider to refresh.
    pub source: AdvisorySource,
    /// Optional package filter.
    #[serde(default)]
    pub packages: Option<Vec<PackageCoordinates>>,
    /// Whether to force refresh even if cache is fresh.
    #[serde(default)]
    pub force: bool,
}

/// Result of an advisory refresh operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryRefreshResult {
    /// Source that was refreshed.
    pub source: AdvisorySource,
    /// Number of records fetched.
    pub records_fetched: usize,
    /// Number of records upserted to cache.
    pub records_upserted: usize,
    /// Whether the operation completed successfully.
    pub success: bool,
    /// Optional error message.
    #[serde(default)]
    pub error: Option<String>,
    /// Timestamp of completion (epoch ms).
    pub completed_at_epoch_ms: i64,
}

/// Request to evaluate installed packages against cached advisories.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryEvaluationRequest {
    /// Packages to evaluate.
    pub packages: Vec<PackageCoordinates>,
    /// Optional minimum severity threshold.
    #[serde(default)]
    pub min_severity: Option<AdvisorySeverity>,
    /// Whether to allow stale cache data.
    #[serde(default)]
    pub allow_stale: bool,
}

/// Result of an advisory evaluation operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryEvaluationResult {
    /// Matched advisories.
    pub matches: Vec<AdvisoryRecord>,
    /// Packages that were evaluated but had no matches.
    #[serde(default)]
    pub clean_packages: Vec<PackageCoordinates>,
    /// Overall freshness state.
    pub freshness: FreshnessState,
    /// Evaluation timestamp (epoch ms).
    pub evaluated_at_epoch_ms: i64,
}

/// Task hook contract for advisory operations.
///
/// This is used by the orchestration layer to schedule advisory refresh and
/// evaluation as background tasks. The hook is invoked by the coordinator
/// and does not perform network access in this module.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "action")]
pub enum AdvisoryTaskHook {
    /// Trigger advisory data refresh.
    Refresh { request: AdvisoryRefreshRequest },
    /// Evaluate installed packages against cache.
    Evaluate { request: AdvisoryEvaluationRequest },
}

impl AdvisoryTaskHook {
    /// Return a deterministic task label for orchestration.
    pub fn task_label(&self) -> String {
        match self {
            Self::Refresh { request } => {
                format!("advisory_refresh:{}", request.source.source_key())
            }
            Self::Evaluate { request } => {
                format!("advisory_evaluate:{}_packages", request.packages.len())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Severity ---

    #[test]
    fn severity_as_str() {
        assert_eq!(AdvisorySeverity::Low.as_str(), "low");
        assert_eq!(AdvisorySeverity::Medium.as_str(), "medium");
        assert_eq!(AdvisorySeverity::High.as_str(), "high");
        assert_eq!(AdvisorySeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn severity_weight_ordering() {
        assert!(AdvisorySeverity::Low.weight() < AdvisorySeverity::Medium.weight());
        assert!(AdvisorySeverity::Medium.weight() < AdvisorySeverity::High.weight());
        assert!(AdvisorySeverity::High.weight() < AdvisorySeverity::Critical.weight());
    }

    #[test]
    fn severity_ord_matches_weight() {
        assert!(AdvisorySeverity::Low < AdvisorySeverity::Critical);
    }

    // --- PackageCoordinates ---

    #[test]
    fn coordinates_qualified_name_without_scope() {
        let coords = PackageCoordinates::new("cargo", "serde");
        assert_eq!(coords.qualified_name(), "serde");
    }

    #[test]
    fn coordinates_qualified_name_with_scope() {
        let coords = PackageCoordinates::with_scope("npm", "@types", "node");
        assert_eq!(coords.qualified_name(), "@types/node");
    }

    #[test]
    fn coordinates_canonical_id() {
        let coords = PackageCoordinates::new("pypi", "requests");
        assert_eq!(coords.canonical_id(), "pypi:requests");
    }

    #[test]
    fn coordinates_canonical_id_with_scope() {
        let coords = PackageCoordinates::with_scope("npm", "@types", "node");
        assert_eq!(coords.canonical_id(), "npm:@types/node");
    }

    #[test]
    fn coordinates_preserves_unicode_nfc() {
        // Package name with non-ASCII characters is preserved as-is.
        let coords = PackageCoordinates::new("npm", "café-cli");
        assert_eq!(coords.name, "café-cli");
    }

    // --- AdvisorySource ---

    #[test]
    fn source_key_without_feed() {
        let source = AdvisorySource::new("osv");
        assert_eq!(source.source_key(), "osv");
    }

    #[test]
    fn source_key_with_feed() {
        let source = AdvisorySource {
            provider: "github".to_string(),
            feed: Some("advisory".to_string()),
            schema_version: None,
        };
        assert_eq!(source.source_key(), "github:advisory");
    }

    // --- AffectedRange ---

    #[test]
    fn affected_range_exact_display() {
        let range = AffectedRange::exact("1.0.0");
        assert_eq!(format!("{}", range), "1.0.0");
    }

    #[test]
    fn affected_range_gte_display() {
        let range = AffectedRange::range_gte("1.0.0");
        assert_eq!(format!("{}", range), ">=1.0.0");
    }

    #[test]
    fn affected_range_bounded_display() {
        let range = AffectedRange::range("1.0.0", "2.0.0");
        assert_eq!(format!("{}", range), ">=1.0.0 AND <2.0.0");
    }

    #[test]
    fn affected_range_all_display() {
        let range = AffectedRange::All;
        assert_eq!(format!("{}", range), "*");
    }

    #[test]
    fn affected_range_raw_display() {
        let range = AffectedRange::Raw {
            expression: ">=1.0 <2.0".to_string(),
        };
        assert_eq!(format!("{}", range), "raw(>=1.0 <2.0)");
    }

    // --- Normalization ---

    #[test]
    fn normalize_package_name_trims_whitespace() {
        assert_eq!(normalize_package_name("  serde  "), "serde");
    }

    #[test]
    fn normalize_package_name_preserves_internal_spaces() {
        assert_eq!(normalize_package_name("some package"), "some package");
    }

    #[test]
    fn normalize_ecosystem_lowercases() {
        assert_eq!(normalize_ecosystem("Cargo"), "cargo");
        assert_eq!(normalize_ecosystem("  NPM  "), "npm");
    }

    #[test]
    fn normalize_source_provider_lowercases() {
        assert_eq!(normalize_source_provider("OSV"), "osv");
    }

    #[test]
    fn constructors_normalize_external_identity_fields() {
        let record = AdvisoryRecord::new(
            "  OSV-1  ",
            PackageCoordinates::with_scope(" NPM ", " @types ", " node "),
            AffectedRange::exact(" 1.0.0 "),
            AdvisorySeverity::High,
            "summary",
            AdvisorySource::with_feed(" OSV ", " PRIMARY "),
            1_000,
            2_000,
        );
        assert_eq!(record.advisory_id, "OSV-1");
        assert_eq!(record.package.canonical_id(), "npm:@types/node");
        assert_eq!(record.source.source_key(), "osv:primary");
        assert_eq!(record.affected_range, AffectedRange::exact("1.0.0"));
    }

    // --- Validation ---

    #[test]
    fn validate_rejects_empty_advisory_id() {
        let record = make_test_advisory("", "serde");
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::EmptyAdvisoryId
        );
    }

    #[test]
    fn validate_rejects_empty_package_name() {
        let record = make_test_advisory("CVE-2024-0001", "");
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::EmptyPackageName
        );
    }

    #[test]
    fn validate_rejects_empty_ecosystem() {
        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.package.ecosystem.clear();
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::EmptyEcosystem
        );
    }

    #[test]
    fn validate_rejects_invalid_schema_version() {
        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.schema_version = 99;
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::InvalidSchemaVersion
        );
    }

    #[test]
    fn validate_rejects_missing_timestamps() {
        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.fetched_at_epoch_ms = 0;
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::MissingTimestamps
        );
    }

    #[test]
    fn validate_rejects_negative_timestamps() {
        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.fetched_at_epoch_ms = -1;
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::NegativeTimestamps
        );
    }

    #[test]
    fn validate_rejects_controls_in_identity_and_allows_formatted_prose() {
        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.advisory_id = "CVE-2024-0001\nforged".to_string();
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::ControlCharacterPresent
        );

        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.package.scope = Some("scope\tforged".to_string());
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::ControlCharacterPresent
        );

        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.summary = "First line\nSecond line".to_string();
        record.description = Some("Details:\n\titem".to_string());
        assert!(validate_advisory(&record).is_ok());
    }

    #[test]
    fn validate_accepts_valid_record() {
        let record = make_test_advisory("CVE-2024-0001", "serde");
        assert!(validate_advisory(&record).is_ok());
    }

    // --- Cache keys ---

    #[test]
    fn advisory_cache_key_is_deterministic() {
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let key = build_advisory_cache_key(&record);
        assert_eq!(key, "advisory:osv:cve-2024-0001");
    }

    #[test]
    fn advisory_cache_key_lowercases_components() {
        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.source.provider = "OSV".to_string();
        record.advisory_id = "CVE-2024-0001".to_string();
        let key = build_advisory_cache_key(&record);
        assert_eq!(key, "advisory:osv:cve-2024-0001");
    }

    #[test]
    fn package_advisory_key_without_scope() {
        let coords = PackageCoordinates::new("cargo", "serde");
        let key = build_package_advisory_key(&coords);
        assert_eq!(key, "pkg_advisory:cargo:serde");
    }

    #[test]
    fn package_advisory_key_with_scope() {
        let coords = PackageCoordinates::with_scope("npm", "@types", "node");
        let key = build_package_advisory_key(&coords);
        assert_eq!(key, "pkg_advisory:npm:@types/node");
    }

    // --- Ordering ---

    #[test]
    fn advisory_sort_key_orders_by_severity_desc() {
        let mut low = make_test_advisory("CVE-2024-0001", "serde");
        let mut crit = make_test_advisory("CVE-2024-0002", "serde");
        low.severity = AdvisorySeverity::Low;
        crit.severity = AdvisorySeverity::Critical;
        assert!(
            advisory_sort_key(&crit) < advisory_sort_key(&low),
            "critical should sort before low"
        );
    }

    #[test]
    fn advisory_sort_key_breaks_ties_by_id() {
        let mut a = make_test_advisory("CVE-2024-0002", "serde");
        let mut b = make_test_advisory("CVE-2024-0001", "serde");
        a.severity = AdvisorySeverity::High;
        b.severity = AdvisorySeverity::High;
        assert!(
            advisory_sort_key(&b) < advisory_sort_key(&a),
            "0001 should sort before 0002"
        );
    }

    #[test]
    fn sort_advisories_deterministic_order() {
        let mut records = vec![
            make_test_advisory("CVE-2024-0003", "a"),
            make_test_advisory("CVE-2024-0001", "b"),
            make_test_advisory("CVE-2024-0002", "c"),
        ];
        records[0].severity = AdvisorySeverity::Low;
        records[1].severity = AdvisorySeverity::Critical;
        records[2].severity = AdvisorySeverity::High;

        sort_advisories(&mut records);

        assert_eq!(records[0].severity, AdvisorySeverity::Critical);
        assert_eq!(records[1].severity, AdvisorySeverity::High);
        assert_eq!(records[2].severity, AdvisorySeverity::Low);
    }

    // --- Deduplication ---

    #[test]
    fn deduplicate_removes_exact_duplicates() {
        let mut a = make_test_advisory("CVE-2024-0001", "serde");
        let mut b = make_test_advisory("CVE-2024-0001", "serde");
        a.source.provider = "osv".to_string();
        b.source.provider = "osv".to_string();
        let records = vec![a, b];
        let deduped = deduplicate_advisories(&records);
        assert_eq!(deduped.len(), 1);
    }

    #[test]
    fn deduplicate_keeps_different_sources() {
        let mut a = make_test_advisory("CVE-2024-0001", "serde");
        let mut b = make_test_advisory("CVE-2024-0001", "serde");
        a.source.provider = "osv".to_string();
        b.source.provider = "nvd".to_string();
        let records = vec![a, b];
        let deduped = deduplicate_advisories(&records);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn deduplicate_preserves_order_of_first_occurrence() {
        let records = vec![
            make_test_advisory("CVE-2024-0002", "a"),
            make_test_advisory("CVE-2024-0001", "b"),
            make_test_advisory("CVE-2024-0002", "a"),
        ];
        let deduped = deduplicate_advisories(&records);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].advisory_id, "CVE-2024-0002");
        assert_eq!(deduped[1].advisory_id, "CVE-2024-0001");
    }

    // --- Freshness ---

    #[test]
    fn freshness_fresh_before_expiry() {
        let thresholds = FreshnessThresholds::default();
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let now = record.fetched_at_epoch_ms + 1000;
        assert_eq!(thresholds.evaluate(now, &record), FreshnessState::Fresh);
    }

    #[test]
    fn freshness_stale_after_expiry() {
        let thresholds = FreshnessThresholds::default();
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let now = record.expires_at_epoch_ms + 1;
        assert_eq!(thresholds.evaluate(now, &record), FreshnessState::Stale);
    }

    #[test]
    fn freshness_missing_for_zero_expiry() {
        let thresholds = FreshnessThresholds::default();
        let mut record = make_test_advisory("CVE-2024-0001", "serde");
        record.expires_at_epoch_ms = 0;
        assert_eq!(thresholds.evaluate(1000, &record), FreshnessState::Missing);
    }

    #[test]
    fn freshness_batch_empty_returns_missing() {
        let thresholds = FreshnessThresholds::default();
        assert_eq!(
            thresholds.evaluate_batch(1000, &[]),
            FreshnessState::Missing
        );
    }

    #[test]
    fn freshness_batch_returns_worst_state() {
        let thresholds = FreshnessThresholds::default();
        let fresh = make_test_advisory("CVE-2024-0001", "a");
        let mut stale = make_test_advisory("CVE-2024-0002", "b");
        stale.expires_at_epoch_ms = 100;
        let now = 500_000;
        let records = vec![fresh, stale];
        assert_eq!(
            thresholds.evaluate_batch(now, &records),
            FreshnessState::Stale
        );
    }

    #[test]
    fn freshness_stale_can_use_offline() {
        assert!(FreshnessState::Stale.can_use_offline());
    }

    #[test]
    fn freshness_fresh_can_use_offline() {
        assert!(FreshnessState::Fresh.can_use_offline());
    }

    #[test]
    fn freshness_missing_cannot_use_offline() {
        assert!(!FreshnessState::Missing.can_use_offline());
    }

    #[test]
    fn freshness_thresholds_compute_expiry() {
        let thresholds = FreshnessThresholds {
            default_ttl_ms: 3600000,
        };
        assert_eq!(thresholds.compute_expiry(1000), 3601000);
    }

    #[test]
    fn freshness_state_as_str() {
        assert_eq!(FreshnessState::Fresh.as_str(), "fresh");
        assert_eq!(FreshnessState::Stale.as_str(), "stale");
        assert_eq!(FreshnessState::Missing.as_str(), "missing");
    }

    // --- Cache record ---

    #[test]
    fn cache_record_from_advisory() {
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let cache = AdvisoryCacheRecord::from_advisory(&record);
        assert_eq!(cache.advisory_id, "CVE-2024-0001");
        assert_eq!(cache.ecosystem, "cargo");
        assert_eq!(cache.package_name, "serde");
        assert_eq!(cache.severity, "high");
        assert_eq!(cache.source_provider, "osv");
        assert_eq!(cache.cache_key, "advisory:osv:cve-2024-0001");
    }

    // --- Task hooks ---

    #[test]
    fn task_hook_refresh_label() {
        let hook = AdvisoryTaskHook::Refresh {
            request: AdvisoryRefreshRequest {
                source: AdvisorySource::new("osv"),
                packages: None,
                force: false,
            },
        };
        assert_eq!(hook.task_label(), "advisory_refresh:osv");
    }

    #[test]
    fn task_hook_evaluate_label() {
        let hook = AdvisoryTaskHook::Evaluate {
            request: AdvisoryEvaluationRequest {
                packages: vec![PackageCoordinates::new("cargo", "serde")],
                min_severity: None,
                allow_stale: true,
            },
        };
        assert_eq!(hook.task_label(), "advisory_evaluate:1_packages");
    }

    // --- Serialization round-trip ---

    #[test]
    fn advisory_record_serializes_and_deserializes() {
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let json = serde_json::to_string(&record).expect("serialize");
        let deserialized: AdvisoryRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(record, deserialized);
    }

    #[test]
    fn advisory_record_with_all_fields_serializes() {
        let record = AdvisoryRecord {
            schema_version: ADVISORY_SCHEMA_VERSION,
            advisory_id: "GHSA-xxxx-xxxx-xxxx".to_string(),
            package: PackageCoordinates::with_scope("npm", "@types", "node"),
            affected_range: AffectedRange::range("1.0.0", "2.0.0"),
            severity: AdvisorySeverity::Critical,
            cvss_score: Some(9.8),
            summary: "Remote code execution".to_string(),
            description: Some("Detailed description here".to_string()),
            fixed_version: Some(AdvisoryFixedVersion::new("2.0.0")),
            references: vec!["https://example.com".to_string()],
            source: AdvisorySource {
                provider: "github".to_string(),
                feed: Some("advisory".to_string()),
                schema_version: Some("1.0".to_string()),
            },
            fetched_at_epoch_ms: 1000,
            expires_at_epoch_ms: 2000,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        let deserialized: AdvisoryRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(record, deserialized);
    }

    #[test]
    fn cache_record_serializes() {
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let cache = AdvisoryCacheRecord::from_advisory(&record);
        let json = serde_json::to_string(&cache).expect("serialize");
        let _: AdvisoryCacheRecord = serde_json::from_str(&json).expect("deserialize");
    }

    // --- Deterministic output ---

    #[test]
    fn sort_then_dedup_produces_deterministic_output() {
        let mut records = vec![
            make_test_advisory("CVE-2024-0002", "b"),
            make_test_advisory("CVE-2024-0001", "a"),
            make_test_advisory("CVE-2024-0002", "b"),
            make_test_advisory("CVE-2024-0003", "c"),
        ];
        records[0].severity = AdvisorySeverity::High;
        records[1].severity = AdvisorySeverity::Critical;
        records[2].severity = AdvisorySeverity::High;
        records[3].severity = AdvisorySeverity::Low;

        sort_advisories(&mut records);
        let deduped = deduplicate_advisories(&records);

        assert_eq!(deduped.len(), 3);
        assert_eq!(deduped[0].advisory_id, "CVE-2024-0001");
        assert_eq!(deduped[1].advisory_id, "CVE-2024-0002");
        assert_eq!(deduped[2].advisory_id, "CVE-2024-0003");
    }

    // --- Invalid data boundaries ---

    #[test]
    fn validate_whitespace_only_advisory_id_rejected() {
        let record = make_test_advisory("   ", "serde");
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::EmptyAdvisoryId
        );
    }

    #[test]
    fn validate_whitespace_only_package_name_rejected() {
        let record = make_test_advisory("CVE-2024-0001", "   ");
        assert_eq!(
            validate_advisory(&record).unwrap_err(),
            AdvisoryValidationError::EmptyPackageName
        );
    }

    // --- Freshness boundary at exact expiry ---

    #[test]
    fn freshness_at_exact_expiry_is_stale() {
        let thresholds = FreshnessThresholds::default();
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let now = record.expires_at_epoch_ms;
        assert_eq!(thresholds.evaluate(now, &record), FreshnessState::Stale);
    }

    #[test]
    fn freshness_one_ms_before_expiry_is_fresh() {
        let thresholds = FreshnessThresholds::default();
        let record = make_test_advisory("CVE-2024-0001", "serde");
        let now = record.expires_at_epoch_ms - 1;
        assert_eq!(thresholds.evaluate(now, &record), FreshnessState::Fresh);
    }

    // --- Freshness batch all fresh ---

    #[test]
    fn freshness_batch_all_fresh() {
        let thresholds = FreshnessThresholds::default();
        let now = 500_000;
        let records = vec![
            make_test_advisory("CVE-2024-0001", "a"),
            make_test_advisory("CVE-2024-0002", "b"),
        ];
        assert_eq!(
            thresholds.evaluate_batch(now, &records),
            FreshnessState::Fresh
        );
    }

    // --- Freshness batch all stale ---

    #[test]
    fn freshness_batch_all_stale() {
        let thresholds = FreshnessThresholds::default();
        let now = 999_999_999;
        let records = vec![
            make_test_advisory("CVE-2024-0001", "a"),
            make_test_advisory("CVE-2024-0002", "b"),
        ];
        assert_eq!(
            thresholds.evaluate_batch(now, &records),
            FreshnessState::Stale
        );
    }

    // --- AdvisoryFixedVersion ---

    #[test]
    fn fixed_version_new() {
        let fv = AdvisoryFixedVersion::new("2.0.0");
        assert_eq!(fv.version, "2.0.0");
        assert!(fv.verification_url.is_none());
    }

    // --- AdvisoryValidationError display ---

    #[test]
    fn validation_error_display() {
        assert_eq!(
            format!("{}", AdvisoryValidationError::EmptyAdvisoryId),
            "advisory_id is empty"
        );
    }

    // --- Helper ---

    fn make_test_advisory(id: &str, pkg: &str) -> AdvisoryRecord {
        AdvisoryRecord {
            schema_version: ADVISORY_SCHEMA_VERSION,
            advisory_id: id.to_string(),
            package: PackageCoordinates::new("cargo", pkg),
            affected_range: AffectedRange::exact("1.0.0"),
            severity: AdvisorySeverity::High,
            cvss_score: None,
            summary: "Test advisory".to_string(),
            description: None,
            fixed_version: None,
            references: Vec::new(),
            source: AdvisorySource::new("osv"),
            fetched_at_epoch_ms: 1000,
            expires_at_epoch_ms: 1000 + 24 * 60 * 60 * 1000,
        }
    }
}
