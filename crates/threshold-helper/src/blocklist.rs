//! Which domains belong to which category.
//!
//! The seed list is embedded rather than read from a user-writable file,
//! because a limited user being able to edit it would defeat the commitment
//! lock: you could simply delete the entry instead of waiting. Sites the user
//! adds themselves travel in the request instead, and are recorded in the
//! admin-only lock so they cannot be dropped mid-commitment either.
//!
//! Nothing here is a judgement about the sites. The categories exist so the
//! user classifies their own distractions — a hardcoded list of "bad sites"
//! moralises and gets ignored, and alignment with the user's own categories is
//! what actually drives reduction.

use std::collections::BTreeMap;

/// One site, in the two shapes the two blocking layers need.
///
/// The hosts file has no wildcards, so it needs every host spelled out. Browser
/// policy matches a whole domain from the apex alone. Deriving both from one
/// table is the point: when these were separate lists they disagreed, and a
/// disagreement here is a hole nobody notices until a site loads.
pub struct Site {
    pub apex: &'static str,
    /// Subdomains beyond the `www.` and `m.` that every site gets.
    pub extra: &'static [&'static str],
}

const fn site(apex: &'static str, extra: &'static [&'static str]) -> Site {
    Site { apex, extra }
}

/// hosts has no wildcards, so every host is listed explicitly.
fn expand(apex: &str, extra: &[&str]) -> Vec<String> {
    let mut hosts = vec![apex.to_string(), format!("www.{apex}"), format!("m.{apex}")];
    hosts.extend(extra.iter().map(|sub| format!("{sub}.{apex}")));
    hosts
}

pub fn seed() -> BTreeMap<&'static str, Vec<Site>> {
    let mut map = BTreeMap::new();

    map.insert(
        "social",
        vec![
            site("facebook.com", &["web"]),
            site("instagram.com", &[]),
            // x.com is a progressive web app: its service worker answers a
            // navigation from cache before any name is ever resolved, and the
            // shell then rehydrates from these. Blocking the apex alone left a
            // fully working X behind a blocked address.
            site("x.com", &["api", "ton", "upload"]),
            site("twimg.com", &["abs", "abs-0", "pbs", "video"]),
            site("t.co", &[]),
            site("twitter.com", &["mobile", "api"]),
            site("threads.net", &[]),
            site("linkedin.com", &[]),
        ],
    );

    map.insert(
        "video",
        vec![
            site("youtube.com", &[]),
            site("youtu.be", &[]),
            site("tiktok.com", &[]),
            site("twitch.tv", &[]),
            site("netflix.com", &[]),
        ],
    );

    map.insert(
        "forums",
        vec![
            site("reddit.com", &["old", "new", "np"]),
            site("news.ycombinator.com", &[]),
        ],
    );

    map
}

fn selected(categories: &[String]) -> Vec<Site> {
    let mut seed = seed();
    categories
        .iter()
        .filter_map(|category| seed.remove(category.as_str()))
        .flatten()
        .collect()
}

fn tidy(mut hosts: Vec<String>) -> Vec<String> {
    hosts.sort();
    hosts.dedup();
    hosts
}

/// Every host for the requested categories and sites, deduplicated and ordered
/// so the hosts block is stable between writes.
///
/// A site the user named gets the same `expand` as a built-in: someone typing
/// `pinterest.com` means the site, and leaving `m.pinterest.com` reachable would
/// be a hole they would reasonably not expect. Validation happened at the
/// protocol boundary — by the time a name reaches here it is a checked apex.
pub fn hosts_for(categories: &[String], custom: &[String]) -> Vec<String> {
    let built_in = selected(categories)
        .into_iter()
        .flat_map(|site| expand(site.apex, site.extra));
    let theirs = custom.iter().flat_map(|apex| expand(apex, &[]));
    tidy(built_in.chain(theirs).collect())
}

/// The apexes, for browser policy.
///
/// Chromium's `URLBlocklist` matches a domain and everything under it, so one
/// entry per apex covers every subdomain — including the ones no hand-written
/// list would have thought of.
pub fn apexes_for(categories: &[String], custom: &[String]) -> Vec<String> {
    let built_in = selected(categories)
        .into_iter()
        .map(|site| site.apex.to_string());
    tidy(built_in.chain(custom.iter().cloned()).collect())
}

/// Every apex this app could possibly have blocked, plus the ones named here.
///
/// The fallback for undoing browser policy when the record of what was applied
/// is gone. Over-removing is the right direction to err in: an entry we take out
/// that was never ours only ever unblocks something.
pub fn every_apex(custom: &[String]) -> Vec<String> {
    let all: Vec<String> = seed().keys().map(|name| name.to_string()).collect();
    apexes_for(&all, custom)
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

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn expands_each_apex_to_its_common_hosts() {
        let hosts = hosts_for(&list(&["social"]), &[]);
        assert!(hosts.contains(&"facebook.com".to_string()));
        assert!(hosts.contains(&"www.facebook.com".to_string()));
        assert!(hosts.contains(&"m.facebook.com".to_string()));
    }

    /// The bug that started this: x.com resolved to 0.0.0.0 and loaded anyway,
    /// because its service worker served the shell and these fed it.
    #[test]
    fn covers_the_hosts_that_kept_x_alive_behind_a_blocked_apex() {
        let hosts = hosts_for(&list(&["social"]), &[]);
        for host in [
            "x.com",
            "www.x.com",
            "api.x.com",
            "t.co",
            "abs.twimg.com",
            "pbs.twimg.com",
            "video.twimg.com",
        ] {
            assert!(hosts.contains(&host.to_string()), "{host} should be blocked");
        }
    }

    #[test]
    fn combines_categories_without_duplicates() {
        let hosts = hosts_for(&list(&["social", "video"]), &[]);
        let mut sorted = hosts.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(hosts.len(), sorted.len());
        assert!(hosts.contains(&"youtu.be".to_string()));
    }

    #[test]
    fn an_unmatched_category_blocks_nothing_but_is_reported() {
        assert!(hosts_for(&list(&["nonsense"]), &[]).is_empty());
        assert_eq!(unknown_categories(&list(&["nonsense"])).len(), 1);
    }

    #[test]
    fn output_is_stable_between_calls() {
        let a = hosts_for(&list(&["video", "social"]), &[]);
        let b = hosts_for(&list(&["social", "video"]), &[]);
        assert_eq!(a, b);
    }

    #[test]
    fn a_custom_site_gets_the_same_treatment_as_a_built_in() {
        let hosts = hosts_for(&[], &list(&["pinterest.com"]));
        assert_eq!(
            hosts,
            list(&["m.pinterest.com", "pinterest.com", "www.pinterest.com"])
        );
    }

    #[test]
    fn a_custom_site_that_repeats_a_built_in_does_not_double_up() {
        let hosts = hosts_for(&list(&["social"]), &list(&["x.com"]));
        assert_eq!(hosts.iter().filter(|host| *host == "x.com").count(), 1);
    }

    #[test]
    fn categories_and_custom_sites_can_be_blocked_together() {
        let hosts = hosts_for(&list(&["forums"]), &list(&["pinterest.com"]));
        assert!(hosts.contains(&"reddit.com".to_string()));
        assert!(hosts.contains(&"pinterest.com".to_string()));
    }

    #[test]
    fn policy_patterns_are_apexes_because_chromium_covers_subdomains() {
        let apexes = apexes_for(&list(&["forums"]), &list(&["pinterest.com"]));
        assert!(apexes.contains(&"reddit.com".to_string()));
        assert!(apexes.contains(&"pinterest.com".to_string()));
        // The whole point: no subdomain fan-out at this layer.
        assert!(!apexes.iter().any(|apex| apex.starts_with("old.")));
        assert!(!apexes.iter().any(|apex| apex.starts_with("www.")));
    }

    #[test]
    fn every_seeded_host_sits_under_a_seeded_apex() {
        // Guards the one-table promise: if the two derivations ever diverge, a
        // host would be null-routed that policy never blocks, or the reverse.
        for category in ["social", "video", "forums"] {
            let cats = list(&[category]);
            let apexes = apexes_for(&cats, &[]);
            for host in hosts_for(&cats, &[]) {
                assert!(
                    apexes
                        .iter()
                        .any(|apex| host == *apex || host.ends_with(&format!(".{apex}"))),
                    "{host} has no apex in {category}"
                );
            }
        }
    }
}
