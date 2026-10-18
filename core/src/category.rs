//! Choosing sites by category (tag): every picked tag is an alternative ("or"), whatever heading it is under; a site matches
//! when it carries at least one of them. Nothing selected matches nothing. Pure functions: no network, no clock.

use crate::feed::{Group, Site};
use serde::Serialize;
use std::collections::HashSet;

/// The slugs of the sites that carry at least one of the selected tags, in the order of `sites`.
pub fn matching(sites: &[Site], _groups: &[Group], selected: &[String]) -> Vec<String> {
    if selected.is_empty() {
        return vec![];
    }
    sites.iter().filter(|s| s.tags.iter().any(|t| selected.contains(t))).map(|s| s.slug.clone()).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub name: String,
    /// How many sites carry the tag.
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCount {
    pub name: String,
    pub tags: Vec<TagCount>,
}

/// What the picker shows: the headings in the site's order, each with the tags that at least one site carries and how many
/// sites carry each; tags that belong to no heading come last under "その他", sorted.
pub fn counts(sites: &[Site], groups: &[Group]) -> Vec<GroupCount> {
    let mut grouped: HashSet<&str> = HashSet::new();
    for g in groups {
        grouped.extend(g.tags.iter().map(String::as_str));
    }
    let n = |tag: &str| sites.iter().filter(|s| s.tags.iter().any(|t| t == tag)).count();
    let mut out: Vec<GroupCount> = vec![];
    for g in groups {
        let tags: Vec<TagCount> = g.tags.iter().filter(|t| n(t) > 0).map(|t| TagCount { name: t.clone(), count: n(t) }).collect();
        if !tags.is_empty() {
            out.push(GroupCount { name: g.name.clone(), tags });
        }
    }
    let mut rest: Vec<&str> = sites.iter().flat_map(|s| s.tags.iter().map(String::as_str)).filter(|t| !grouped.contains(t)).collect();
    rest.sort_unstable();
    rest.dedup();
    if !rest.is_empty() {
        out.push(GroupCount { name: "その他".into(), tags: rest.into_iter().map(|t| TagCount { name: t.into(), count: n(t) }).collect() });
    }
    out
}
