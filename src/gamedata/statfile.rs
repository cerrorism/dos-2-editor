//! Parser for Larian generated-stat files and their `using` inheritance.
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Default)]
struct Entry {
    parent: Option<String>,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub struct StatDatabase {
    entries: HashMap<String, Entry>,
}

impl StatDatabase {
    pub fn parse_and_merge(&mut self, source: &str) {
        let mut current: Option<(String, Entry)> = None;
        for line in source.lines().map(str::trim) {
            if let Some(name) = quoted_after(line, "new entry ") {
                if let Some((name, entry)) = current.take() {
                    self.entries.insert(name, entry);
                }
                current = Some((name.to_owned(), Entry::default()));
            } else if let Some(parent) = quoted_after(line, "using ") {
                if let Some((_, entry)) = &mut current {
                    entry.parent = Some(parent.to_owned());
                }
            } else if line.starts_with("data ") {
                let values = quoted_values(line);
                if let ([key, value], Some((_, entry))) = (values.as_slice(), &mut current) {
                    entry.fields.insert((*key).to_owned(), (*value).to_owned());
                }
            }
        }
        if let Some((name, entry)) = current {
            self.entries.insert(name, entry);
        }
    }

    pub fn resolved_fields(&self, name: &str) -> BTreeMap<String, String> {
        let mut resolved = BTreeMap::new();
        self.resolve_into(name, &mut HashSet::new(), &mut resolved);
        resolved
    }

    fn resolve_into(
        &self,
        name: &str,
        visited: &mut HashSet<String>,
        resolved: &mut BTreeMap<String, String>,
    ) {
        if !visited.insert(name.to_owned()) {
            return;
        }
        let Some(entry) = self.entries.get(name) else {
            return;
        };
        if let Some(parent) = &entry.parent {
            self.resolve_into(parent, visited, resolved);
        }
        resolved.extend(entry.fields.clone());
    }
}

fn quoted_after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?.trim_start();
    let rest = rest.strip_prefix('"')?;
    rest.split_once('"').map(|(value, _)| value)
}

fn quoted_values(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

#[cfg(test)]
mod tests {
    use super::StatDatabase;

    #[test]
    fn inherits_and_overrides_stat_fields() {
        let mut stats = StatDatabase::default();
        stats.parse_and_merge(
            r#"
new entry "Base"
data "Armor Defense Value" "10"
data "FinesseBoost" "1"
new entry "Child"
using "Base"
data "Armor Defense Value" "15"
"#,
        );
        let fields = stats.resolved_fields("Child");
        assert_eq!(fields.get("Armor Defense Value"), Some(&"15".to_owned()));
        assert_eq!(fields.get("FinesseBoost"), Some(&"1".to_owned()));
    }
}
