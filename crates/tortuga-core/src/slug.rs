//! File names derived from display names (R-GIT-5).

/// Names Windows does not allow for files.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Turns a display name into a file/directory name: lower-case letters and
/// digits (any script), everything else collapsed to single `-`.
/// Never empty and never a name Windows refuses (`con`, `nul`, …).
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut slug: String = slug.trim_end_matches('-').chars().take(60).collect();
    slug = slug.trim_end_matches('-').to_owned();
    if slug.is_empty() {
        return "untitled".to_owned();
    }
    if RESERVED.contains(&slug.as_str()) {
        slug.push('-');
    }
    slug
}

/// `base`, or `base-2`, `base-3`, … — the first candidate `taken` rejects.
pub fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Create user"), "create-user");
        assert_eq!(slugify("  GET /users/{id}  "), "get-users-id");
        assert_eq!(slugify("Søg på æbler"), "søg-på-æbler");
        assert_eq!(slugify("???"), "untitled");
        assert_eq!(slugify("CON"), "con-");
        assert_eq!(slugify(&"a".repeat(100)).len(), 60);
    }

    #[test]
    fn unique_appends_numbers() {
        let taken = ["a", "a-2"];
        assert_eq!(unique("a", |c| taken.contains(&c)), "a-3");
        assert_eq!(unique("b", |c| taken.contains(&c)), "b");
    }
}
