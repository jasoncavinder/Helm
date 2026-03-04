use crate::models::ManagerInstallInstance;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiInstanceState {
    None,
    AttentionNeeded,
    Acknowledged,
}

impl MultiInstanceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AttentionNeeded => "attention_needed",
            Self::Acknowledged => "acknowledged",
        }
    }
}

pub fn install_instance_fingerprint(instances: &[ManagerInstallInstance]) -> Option<String> {
    let ids = instances
        .iter()
        .map(|instance| instance.instance_id.as_str());
    instance_ids_fingerprint(ids)
}

pub fn instance_ids_fingerprint<'a>(
    instance_ids: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let mut sorted = instance_ids
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if sorted.len() <= 1 {
        return None;
    }
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() <= 1 {
        return None;
    }
    let canonical = sorted.join("\n");
    Some(format!("{:016x}", stable_hash64(canonical.as_str())))
}

pub fn resolve_multi_instance_state<'a>(
    instance_ids: impl IntoIterator<Item = &'a str>,
    acknowledged_fingerprint: Option<&str>,
) -> (MultiInstanceState, Option<String>, bool) {
    let fingerprint = instance_ids_fingerprint(instance_ids);
    match fingerprint {
        None => (MultiInstanceState::None, None, false),
        Some(value) => {
            let acknowledged = acknowledged_fingerprint
                .map(str::trim)
                .filter(|stored| !stored.is_empty())
                .is_some_and(|stored| stored == value);
            if acknowledged {
                (MultiInstanceState::Acknowledged, Some(value), true)
            } else {
                (MultiInstanceState::AttentionNeeded, Some(value), false)
            }
        }
    }
}

fn stable_hash64(input: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{MultiInstanceState, instance_ids_fingerprint, resolve_multi_instance_state};

    #[test]
    fn fingerprint_is_order_independent() {
        let first =
            instance_ids_fingerprint(["rustup-homebrew", "rustup-user"]).expect("fingerprint");
        let second =
            instance_ids_fingerprint(["rustup-user", "rustup-homebrew"]).expect("fingerprint");
        assert_eq!(first, second);
    }

    #[test]
    fn fingerprint_requires_multiple_unique_instances() {
        assert_eq!(instance_ids_fingerprint(["only-one"]), None);
        assert_eq!(instance_ids_fingerprint(["same", "same"]), None);
    }

    #[test]
    fn resolve_state_defaults_to_attention_when_unacknowledged() {
        let (state, fingerprint, acknowledged) = resolve_multi_instance_state(["a", "b"], None);
        assert_eq!(state, MultiInstanceState::AttentionNeeded);
        assert!(fingerprint.is_some());
        assert!(!acknowledged);
    }

    #[test]
    fn resolve_state_is_acknowledged_when_fingerprint_matches() {
        let fingerprint = instance_ids_fingerprint(["a", "b"]).expect("fingerprint");
        let (state, resolved, acknowledged) =
            resolve_multi_instance_state(["b", "a"], Some(fingerprint.as_str()));
        assert_eq!(state, MultiInstanceState::Acknowledged);
        assert_eq!(resolved.as_deref(), Some(fingerprint.as_str()));
        assert!(acknowledged);
    }

    #[test]
    fn resolve_state_returns_none_for_single_instance() {
        let (state, fingerprint, acknowledged) =
            resolve_multi_instance_state(["only"], Some("abc"));
        assert_eq!(state, MultiInstanceState::None);
        assert!(fingerprint.is_none());
        assert!(!acknowledged);
    }
}
