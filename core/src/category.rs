//! Choosing sites by category (tag), the way a faceted search does: the tags of one heading are alternatives ("or"), the
//! headings together are all required ("and"). Tags that belong to no heading form one more heading. Nothing selected
//! matches nothing. Pure functions: no network, no clock.

use crate::feed::{Group, Site};
use serde::Serialize;
use std::collections::HashSet;

/// The slugs of the sites that match the selected tags, in the order of `sites`.
pub fn matching(sites: &[Site], groups: &[Group], selected: &[String]) -> Vec<String> {
    if selected.is_empty() {
        return vec![];
    }
    let mut facets: Vec<Vec<&str>> = vec![];
    let mut grouped: HashSet<&str> = HashSet::new();
    for g in groups {
        let chosen: Vec<&str> = g.tags.iter().filter(|t| selected.contains(t)).map(String::as_str).collect();
        if !chosen.is_empty() {
            facets.push(chosen);
        }
        grouped.extend(g.tags.iter().map(String::as_str));
    }
    let rest: Vec<&str> = selected.iter().map(String::as_str).filter(|t| !grouped.contains(t)).collect();
    if !rest.is_empty() {
        facets.push(rest);
    }
    sites.iter().filter(|s| facets.iter().all(|f| s.tags.iter().any(|t| f.contains(&t.as_str())))).map(|s| s.slug.clone()).collect()
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
/// sites that is; tags that belong to no heading come last under "その他", sorted. With tags already picked the number of a tag
/// is what it would give next to the picks of the OTHER headings (the usual faceted count), so a chip that would leave nothing
/// shows 0 instead of looking harmless: "職能・業界団体" + "がん" is 0 because no such body carries an area tag.
pub fn counts(sites: &[Site], groups: &[Group], selected: &[String]) -> Vec<GroupCount> {
    let mut grouped: HashSet<&str> = HashSet::new();
    for g in groups {
        grouped.extend(g.tags.iter().map(String::as_str));
    }
    let mut out: Vec<GroupCount> = vec![];
    for g in groups {
        // the picks of the other headings (the picks outside every heading count as one more heading)
        let others: Vec<String> = selected.iter().filter(|t| !g.tags.contains(t)).cloned().collect();
        let n = |tag: &str| {
            let mut sel = others.clone();
            sel.push(tag.to_string());
            matching(sites, groups, &sel).len()
        };
        let present = |tag: &str| sites.iter().any(|s| s.tags.iter().any(|t| t == tag));
        let tags: Vec<TagCount> = g.tags.iter().filter(|t| present(t)).map(|t| TagCount { name: t.clone(), count: n(t) }).collect();
        if !tags.is_empty() {
            out.push(GroupCount { name: g.name.clone(), tags });
        }
    }
    let mut rest: Vec<&str> = sites.iter().flat_map(|s| s.tags.iter().map(String::as_str)).filter(|t| !grouped.contains(t)).collect();
    rest.sort_unstable();
    rest.dedup();
    if !rest.is_empty() {
        let others: Vec<String> = selected.iter().filter(|t| grouped.contains(t.as_str())).cloned().collect();
        let tags = rest
            .into_iter()
            .map(|t| {
                let mut sel = others.clone();
                sel.push(t.to_string());
                TagCount { name: t.into(), count: matching(sites, groups, &sel).len() }
            })
            .collect();
        out.push(GroupCount { name: "その他".into(), tags });
    }
    out
}
