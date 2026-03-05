use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageCoordinate {
    pub package_name: String,
    pub version_selector: Option<VersionSelector>,
}

impl PackageCoordinate {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        if let Some((package_name, selector_raw)) = trimmed.rsplit_once('@')
            && !package_name.trim().is_empty()
            && !selector_raw.trim().is_empty()
        {
            return Some(Self {
                package_name: package_name.trim().to_string(),
                version_selector: Some(VersionSelector::parse(selector_raw)),
            });
        }

        Some(Self {
            package_name: trimmed.to_string(),
            version_selector: None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionSelector {
    pub raw: String,
    pub atoms: Vec<String>,
    pub first_release_atom: Option<usize>,
}

impl VersionSelector {
    pub fn parse(raw: &str) -> Self {
        let normalized = raw.trim();
        let atoms = normalized
            .split('-')
            .map(str::trim)
            .filter(|atom| !atom.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let first_release_atom = atoms
            .iter()
            .position(|atom| atom.chars().any(|ch| ch.is_ascii_digit()));

        Self {
            raw: normalized.to_string(),
            atoms,
            first_release_atom,
        }
    }

    pub fn qualifier_atoms(&self) -> Vec<String> {
        match self.first_release_atom {
            Some(index) => self.atoms[..index].to_vec(),
            None => self.atoms.clone(),
        }
    }

    pub fn release_atoms(&self) -> Vec<String> {
        match self.first_release_atom {
            Some(index) => self.atoms[index..].to_vec(),
            None => Vec::new(),
        }
    }

    pub fn release_token(&self) -> Option<String> {
        let atoms = self.release_atoms();
        if atoms.is_empty() {
            None
        } else {
            Some(atoms.join("-"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PackageCoordinate;

    #[test]
    fn parses_package_coordinate_without_selector() {
        let parsed = PackageCoordinate::parse("python").expect("coordinate should parse");
        assert_eq!(parsed.package_name, "python");
        assert!(parsed.version_selector.is_none());
    }

    #[test]
    fn parses_package_coordinate_with_two_part_selector() {
        let parsed = PackageCoordinate::parse("python@mambaforge-24.11.0-1")
            .expect("coordinate should parse");
        let selector = parsed.version_selector.expect("selector should be present");
        assert_eq!(parsed.package_name, "python");
        assert_eq!(selector.atoms, vec!["mambaforge", "24.11.0", "1"]);
        assert_eq!(selector.qualifier_atoms(), vec!["mambaforge"]);
        assert_eq!(selector.release_token().as_deref(), Some("24.11.0-1"));
    }

    #[test]
    fn parses_package_coordinate_with_multi_part_selector() {
        let parsed = PackageCoordinate::parse("java@zulu-jre-javafx-8.92.0.21")
            .expect("coordinate should parse");
        let selector = parsed.version_selector.expect("selector should be present");
        assert_eq!(parsed.package_name, "java");
        assert_eq!(selector.atoms, vec!["zulu", "jre", "javafx", "8.92.0.21"]);
        assert_eq!(selector.qualifier_atoms(), vec!["zulu", "jre", "javafx"]);
        assert_eq!(selector.release_token().as_deref(), Some("8.92.0.21"));
    }

    #[test]
    fn handles_scoped_package_name_without_selector() {
        let parsed = PackageCoordinate::parse("@jdxcode/mise").expect("coordinate should parse");
        assert_eq!(parsed.package_name, "@jdxcode/mise");
        assert!(parsed.version_selector.is_none());
    }
}
