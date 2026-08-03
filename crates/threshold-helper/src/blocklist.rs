//! Which domains belong to which category.
//!
//! The seed list is embedded rather than read from a user-writable file,
//! because a limited user being able to edit it would defeat the commitment
//! lock: you could simply delete the entry instead of waiting. An override may
//! live in ProgramData, which is admin-writable only.
//!
//! Nothing here is a judgement about the sites. The categories exist so the
//! user classifies their own distractions — a hardcoded list of "bad sites"
//! moralises and gets ignored, and alignment with the user's own categories is
//! what actually drives reduction.

use std::collections::BTreeMap;

/// hosts has no wildcards, so every host is listed explicitly.
fn expand(apex: &str, extra: &[&str]) -> Vec<String> {
    let mut hosts = vec![apex.to_string(), format!("www.{apex}"), format!("m.{apex}")];
    hosts.extend(extra.iter().map(|sub| format!("{sub}.{apex}")));
    hosts
}

pub fn seed() -> BTreeMap<&'static str, Vec<String>> {
    let mut map = BTreeMap::new();

    map.insert(
        "social",
        [
            expand("facebook.com", &["web"]),
            expand("instagram.com", &[]),
            expand("x.com", &[]),
            expand("twitter.com", &["mobile"]),
            expand("threads.net", &[]),
            expand("linkedin.com", &[]),
        ]
        .concat(),
    );

    map.insert(
        "video",
        [
            expand("youtube.com", &["m"]),
            vec!["youtu.be".to_string()],
            expand("tiktok.com", &[]),
            expand("twitch.tv", &[]),
            expand("netflix.com", &[]),
        ]
        .concat(),
    );

    map.insert(
        "forums",
        [
            expand("reddit.com", &["old", "new", "np"]),
            expand("news.ycombinator.com", &[]),
        ]
        .concat(),
    );

    map
}

/// Every host for the requested categories, deduplicated and ordered so the
/// hosts block is stable between writes.
pub fn hosts_for(categories: &[String]) -> Vec<String> {
    let seed = seed();
    let mut hosts: Vec<String> = categories
        .iter()
        .filter_map(|category| seed.get(category.as_str()))
        .flatten()
        .cloned()
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

/// Category names in the seed table that the request did not match — surfaced
/// so a typo is visible rather than silently blocking nothing.
pub fn unknown_categories(categories: &[String]) -> Vec<String> {
    let seed = seed();
    categories
        .iter()
        .filter(|category| !seed.contains_key(category.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_each_apex_to_its_common_hosts() {
        let hosts = hosts_for(&["social".to_string()]);
        assert!(hosts.contains(&"facebook.com".to_string()));
        assert!(hosts.contains(&"www.facebook.com".to_string()));
        assert!(hosts.contains(&"m.facebook.com".to_string()));
    }

    #[test]
    fn combines_categories_without_duplicates() {
        let hosts = hosts_for(&["social".to_string(), "video".to_string()]);
        let mut sorted = hosts.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(hosts.len(), sorted.len());
        assert!(hosts.contains(&"youtu.be".to_string()));
    }

    #[test]
    fn an_unmatched_category_blocks_nothing_but_is_reported() {
        assert!(hosts_for(&["nonsense".to_string()]).is_empty());
        assert_eq!(unknown_categories(&["nonsense".to_string()]).len(), 1);
    }

    #[test]
    fn output_is_stable_between_calls() {
        let a = hosts_for(&["video".to_string(), "social".to_string()]);
        let b = hosts_for(&["social".to_string(), "video".to_string()]);
        assert_eq!(a, b);
    }
}
