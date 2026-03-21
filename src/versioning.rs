use std::cmp::Ordering;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionParts(pub [u32; 4]);

impl Ord for VersionParts {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for VersionParts {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn compare_versions(left: Option<&str>, right: Option<&str>) -> Option<Ordering> {
    let left = parse_version(left?)?;
    let right = parse_version(right?)?;
    Some(left.cmp(&right))
}

pub fn normalize_version_string(value: &str) -> Option<String> {
    let characters = value.chars().collect::<Vec<_>>();

    for start in 0..characters.len() {
        if !characters[start].is_ascii_digit() {
            continue;
        }

        let mut candidate = String::new();

        for ch in characters.iter().skip(start) {
            if ch.is_ascii_digit() {
                candidate.push(*ch);
                continue;
            }

            if *ch == '.' && !candidate.is_empty() && !candidate.ends_with('.') {
                candidate.push(*ch);
                continue;
            }

            break;
        }

        while candidate.ends_with('.') {
            candidate.pop();
        }

        if candidate.matches('.').count() > 0 {
            return Some(candidate);
        }
    }

    None
}

pub fn parse_version(value: &str) -> Option<VersionParts> {
    let normalized = normalize_version_string(value)?;
    let mut parts = [0_u32; 4];

    for (index, part) in normalized.split('.').take(4).enumerate() {
        parts[index] = part.parse().ok()?;
    }

    Some(VersionParts(parts))
}
