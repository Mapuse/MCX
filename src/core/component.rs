use std::path::PathBuf;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComponentPriority {
    Required,
    Recommended,
    Optional,
    Development,
}

impl std::str::FromStr for ComponentPriority {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "required" | "req" | "mandatory" => Ok(Self::Required),
            "recommended" | "rec" | "default" => Ok(Self::Recommended),
            "optional" | "opt" => Ok(Self::Optional),
            "development" | "dev" | "debug" => Ok(Self::Development),
            _ => Err(()),
        }
    }
}

impl ComponentPriority {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Recommended => "recommended",
            Self::Optional => "optional",
            Self::Development => "development",
        }
    }

    pub fn should_install(&self, minimal: bool, include_dev: bool) -> bool {
        match self {
            Self::Required => true,
            Self::Recommended => !minimal,
            Self::Optional => false,
            Self::Development => include_dev,
        }
    }
}

impl std::fmt::Display for ComponentPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentDependency {
    pub package: String,
    pub component: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub priority: ComponentPriority,
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub dependencies: Vec<ComponentDependency>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ComponentFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub minimal: bool,
    pub include_dev: bool,
}

impl ComponentFilter {
    pub fn none() -> Self {
        Self { include: Vec::new(), exclude: Vec::new(), minimal: false, include_dev: false }
    }

    pub fn should_install_component(&self, component: &Component) -> bool {
        if self.exclude.contains(&component.name) {
            return false;
        }
        if !self.include.is_empty() {
            return self.include.contains(&component.name)
                || component.priority == ComponentPriority::Required;
        }
        component.priority.should_install(self.minimal, self.include_dev)
    }
}

pub fn filter_files_by_components(
    all_files: &[PathBuf],
    components: &[Component],
    filter: &ComponentFilter,
) -> Vec<PathBuf> {
    if components.is_empty() {
        return all_files.to_vec();
    }

    let mut selected: Vec<PathBuf> = Vec::new();
    for component in components {
        if filter.should_install_component(component) {
            selected.extend(component.files.iter().cloned());
        }
    }
    selected.sort();
    selected.dedup();
    selected
}

pub fn validate_components(components: &[Component]) -> Result<(), String> {
    let names: Vec<&str> = components.iter().map(|c| c.name.as_str()).collect();
    for name in &names {
        if names.iter().filter(|n| *n == name).count() > 1 {
            return Err(format!("Duplicate component name: {}", name));
        }
    }

    for component in components {
        for dep in &component.dependencies {
            if !names.contains(&dep.component.as_str()) {
                return Err(format!(
                    "Component '{}' depends on '{}' which does not exist in this package",
                    component.name, dep.component
                ));
            }
        }
    }

    let has_required = components.iter().any(|c| c.priority == ComponentPriority::Required);
    if !has_required && !components.is_empty() {
        return Err("Package must have at least one Required component".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(ComponentPriority::Required < ComponentPriority::Recommended);
        assert!(ComponentPriority::Recommended < ComponentPriority::Optional);
        assert!(ComponentPriority::Optional < ComponentPriority::Development);
    }

    #[test]
    fn test_should_install() {
        assert!(ComponentPriority::Required.should_install(false, false));
        assert!(ComponentPriority::Recommended.should_install(false, false));
        assert!(!ComponentPriority::Optional.should_install(false, false));
        assert!(!ComponentPriority::Development.should_install(false, false));

        assert!(ComponentPriority::Required.should_install(true, false));
        assert!(!ComponentPriority::Recommended.should_install(true, false));

        assert!(ComponentPriority::Development.should_install(false, true));
    }

    #[test]
    fn test_component_filter() {
        let filter = ComponentFilter::none();
        let req = Component {
            name: "core".into(), priority: ComponentPriority::Required,
            files: vec![], dependencies: vec![], description: String::new(),
        };
        let opt = Component {
            name: "extras".into(), priority: ComponentPriority::Optional,
            files: vec![], dependencies: vec![], description: String::new(),
        };
        assert!(filter.should_install_component(&req));
        assert!(!filter.should_install_component(&opt));
    }

    #[test]
    fn test_validate_components() {
        let components = vec![
            Component {
                name: "core".into(), priority: ComponentPriority::Required,
                files: vec![], dependencies: vec![], description: String::new(),
            },
            Component {
                name: "dev".into(), priority: ComponentPriority::Development,
                files: vec![], dependencies: vec![ComponentDependency {
                    package: "self".into(), component: "core".into(),
                }], description: String::new(),
            },
        ];
        assert!(validate_components(&components).is_ok());

        let bad = vec![
            Component {
                name: "core".into(), priority: ComponentPriority::Optional,
                files: vec![], dependencies: vec![], description: String::new(),
            },
        ];
        assert!(validate_components(&bad).is_err());
    }
}
