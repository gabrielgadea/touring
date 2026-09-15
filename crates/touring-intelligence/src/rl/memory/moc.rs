// #tags: kind:module lang:rust domain:memory purpose:retrieval process:explore status:experimental
//! Emergent communities + Maps of Content (F5 of the hashtag library).
//!
//! Layers L1 (tag↔item bipartite) and L3 (typed links) already exist; this
//! module is L4 — the structure that EMERGES from them instead of being
//! imposed (GraphRAG's lesson: hierarchy should be discovered by community
//! detection, not declared). Communities are found by weighted label
//! propagation over entries, where an edge means "share a *strong* tag"
//! (domain/purpose/artifact/process/lang — never `status`, which is
//! universal, nor `kind`, which is too coarse) or "are directly linked".
//!
//! A MOC renders one topic's communities as an OKF markdown document with
//! `[[key]]` links, so a domain becomes browsable knowledge instead of a
//! flat hit list — the answer to "I built a map artifact months ago; where is
//! everything related to it?".

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::tags;

/// Facets strong enough to connect entries into communities. `status` is
/// universal and `kind` is too coarse — including them would collapse the
/// graph into one giant component.
const STRONG_FACETS: &[&str] = &["domain", "purpose", "artifact", "process", "lang"];

/// Weight of a direct typed link relative to one shared strong tag.
const LINK_WEIGHT: u32 = 2;

/// One detected community: a stable label and its member entry keys.
#[derive(Debug, Clone)]
pub struct Community {
    /// The community's label after convergence (initially the smallest key).
    pub label: String,
    /// Member entry keys, sorted.
    pub members: Vec<String>,
}

/// The weighted entry↔entry graph: adjacency map with edge weights.
type Graph = HashMap<String, BTreeMap<String, u32>>;

/// Adds one weight unit per unordered pair sharing a strong tag.
fn add_shared_tag_edges(graph: &mut Graph, by_tag: &BTreeMap<String, Vec<String>>) {
    for members in by_tag.values() {
        for (i, a) in members.iter().enumerate() {
            for b in &members[i + 1..] {
                *graph
                    .entry(a.clone())
                    .or_default()
                    .entry(b.clone())
                    .or_insert(0) += 1;
                *graph
                    .entry(b.clone())
                    .or_default()
                    .entry(a.clone())
                    .or_insert(0) += 1;
            }
        }
    }
}

/// Adds `LINK_WEIGHT` per typed link, both directions (the neighbourhood is
/// undirected for community purposes). When `within` is given, only links
/// with both ends in the set count.
fn add_link_edges(
    graph: &mut Graph,
    links: &[(String, String)],
    within: Option<&BTreeSet<String>>,
) {
    for (src, dst) in links {
        if let Some(set) = within
            && !(set.contains(src) && set.contains(dst))
        {
            continue;
        }
        *graph
            .entry(src.clone())
            .or_default()
            .entry(dst.clone())
            .or_insert(0) += LINK_WEIGHT;
        *graph
            .entry(dst.clone())
            .or_default()
            .entry(src.clone())
            .or_insert(0) += LINK_WEIGHT;
    }
}

/// Reads the typed link pairs of the store.
fn read_links(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT src, dst FROM memory_links")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Builds the entry graph over the whole store (or one `domain:<value>`
/// slice): nodes are entries carrying at least one strong tag or one link;
/// edges accumulate one point per shared strong tag and `LINK_WEIGHT` per
/// typed link.
fn build_graph(conn: &rusqlite::Connection, domain: Option<&str>) -> rusqlite::Result<Graph> {
    let mut graph: Graph = HashMap::new();
    let mut by_tag: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT entry_key, facet, value, full_tag FROM memory_tags")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // A domain slice restricts the NODE SET (entries carrying `domain:<d>`),
    // never the EDGE vocabulary: keeping only the `domain:<d>` tag itself
    // left `by_tag` with a single key, so the slice degenerated into one
    // complete-graph community — the opposite of emergent structure (D2,
    // cross-audit 2026-08-23). Membership first, then edges from every
    // strong facet those members carry.
    let members: Option<std::collections::BTreeSet<String>> = domain.map(|d| {
        rows.iter()
            .filter(|(_, facet, value, _)| facet == "domain" && value == d)
            .map(|(key, ..)| key.clone())
            .collect()
    });
    for (key, facet, value, full_tag) in rows {
        if !STRONG_FACETS.contains(&facet.as_str()) {
            continue;
        }
        if let Some(m) = &members
            && !m.contains(&key)
        {
            continue;
        }
        graph.entry(key.clone()).or_default();
        // The slicing tag itself is shared by EVERY member by definition —
        // as an edge it carries zero information and would glue the slice
        // back into one complete graph. Membership only, never an edge.
        if let Some(d) = domain
            && facet == "domain"
            && value == d
        {
            continue;
        }
        by_tag.entry(full_tag).or_default().push(key);
    }
    add_shared_tag_edges(&mut graph, &by_tag);
    let links = read_links(conn)?;
    // In a sliced graph a link only counts when both ends belong to the slice.
    let node_set: Option<BTreeSet<String>> = domain.map(|_| graph.keys().cloned().collect());
    add_link_edges(&mut graph, &links, node_set.as_ref());
    Ok(graph)
}

/// Weighted label propagation (Raghavan et al. 2007): every node starts with
/// its own key as label; each round a node adopts the label carrying the most
/// accumulated neighbour weight, ties broken by the smaller label so the
/// result is deterministic (REGRA #17 discipline: same store → same
/// communities). Caps at 25 rounds.
pub(crate) fn label_propagate(graph: &Graph) -> Vec<Community> {
    let mut label_of: BTreeMap<String, String> =
        graph.keys().map(|k| (k.clone(), k.clone())).collect();
    // Iteration order changes the winner in oscillating clusters, and HashMap
    // order is per-process random — without a fixed order the same store can
    // yield a different label per run. Sorted keys make the result a pure
    // function of the graph (REGRA #17 discipline).
    let ordered: Vec<&String> = {
        let mut v: Vec<&String> = graph.keys().collect();
        v.sort();
        v
    };
    for _ in 0..25 {
        let mut changed = false;
        for node in &ordered {
            let mut weight_by_label: BTreeMap<String, u32> = BTreeMap::new();
            for (neigh, w) in &graph[*node] {
                if let Some(l) = label_of.get(neigh) {
                    *weight_by_label.entry(l.clone()).or_insert(0) += w;
                }
            }
            let Some(best) = weight_by_label
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(l, _)| l.clone())
            else {
                continue;
            };
            if label_of[*node] != best {
                label_of.insert((*node).clone(), best);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut communities: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (node, label) in label_of {
        communities.entry(label).or_default().push(node);
    }
    communities
        .into_iter()
        .map(|(label, members)| Community { label, members })
        .collect()
}

/// Detects the communities of the whole store (or of one `domain:<value>`
/// slice when `domain` is given).
pub fn detect_communities(
    conn: &rusqlite::Connection,
    domain: Option<&str>,
) -> rusqlite::Result<Vec<Community>> {
    let graph = build_graph(conn, domain)?;
    Ok(label_propagate(&graph))
}

/// Renders a Map of Content for `topic` as an OKF markdown document:
/// communities of the topic's corpus (entries whose tags or text match),
/// per-facet coverage, and the typed links between members.
pub fn render_moc(conn: &rusqlite::Connection, topic: &str) -> rusqlite::Result<String> {
    let corpus = topic_corpus(conn, topic)?;
    let graph = subgraph_for(conn, &corpus)?;
    let communities = label_propagate(&graph);

    let mut md = String::new();
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%z");
    md.push_str(&format!(
        "---\ntype: MOC\ntitle: \"MOC — {topic}\"\ndescription: \"Mapa de conteúdo emergente do tópico {topic}: comunidades detectadas por label propagation sobre tags+links (F5).\"\ntags: [moc, {topic}]\ntimestamp: {ts}\nokf_version: \"0.1\"\n---\n\n"
    ));
    md.push_str(&format!("# Mapa de Conteúdo — {topic}\n\n"));
    // Per-community sections, largest first. Singletons are not "communities"
    // of one — they collapse into a single honest "no strong ties" bucket at
    // the end (the alternative is N one-line sections that group nothing).
    let (connected, singletons): (Vec<&Community>, Vec<&Community>) =
        communities.iter().partition(|c| c.members.len() >= 2);

    md.push_str(&format!(
        "Corpus: {} entradas · {} comunidades emergentes ({} entradas conectadas) · gerado por `touring memory moc {topic}`\n\n",
        corpus.len(),
        connected.len(),
        corpus.len() - singletons.len()
    ));

    let mut summaries: Vec<(usize, String)> = Vec::new();
    for (idx, community) in connected.iter().enumerate() {
        let mut section = format!(
            "## Comunidade {} — `{}` ({} entradas)\n\n",
            idx + 1,
            community.label,
            community.members.len()
        );
        for key in &community.members {
            let (value, tags_line) = entry_summary(conn, key);
            section.push_str(&format!("- [[{key}]] — {value}\n  `{tags_line}`\n"));
        }
        section.push('\n');
        summaries.push((community.members.len(), section));
    }
    summaries.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    for (_, s) in summaries {
        md.push_str(&s);
    }
    if !singletons.is_empty() {
        md.push_str(&format!(
            "## Sem conexões fortes ({} entradas)\n\nEstas entradas do corpus não compartilham tag forte nem link tipado com outra — candidatas a tagging mais rico.\n\n",
            singletons.len()
        ));
        for community in singletons {
            for key in &community.members {
                let (value, tags_line) = entry_summary(conn, key);
                md.push_str(&format!("- [[{key}]] — {value}\n  `{tags_line}`\n"));
            }
        }
        md.push('\n');
    }

    md.push_str(
        "## Cobertura por faceta\n\n| faceta | valores distintos | entradas |\n|---|---|---|\n",
    );
    for (facet, values, count) in facet_coverage(conn, &corpus)? {
        md.push_str(&format!("| {facet} | {values} | {count} |\n"));
    }

    let links = links_within(conn, &corpus)?;
    if !links.is_empty() {
        md.push_str("\n## Links tipados\n\n");
        for (src, rel, dst) in links {
            md.push_str(&format!("- [[{src}]] —`{rel}`→ [[{dst}]]\n"));
        }
    }
    Ok(md)
}

/// The MOC corpus: entries tagged `domain:<topic>` OR carrying the topic in
/// a tag value OR matching it in the FTS index — three honest retrieval
/// paths, unioned and sorted.
fn topic_corpus(conn: &rusqlite::Connection, topic: &str) -> rusqlite::Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT entry_key FROM memory_tags
         WHERE (facet = 'domain' AND value = ?1) OR value LIKE ?2",
    )?;
    let by_tag = stmt
        .query_map(rusqlite::params![topic, format!("%{topic}%")], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    keys.extend(by_tag);
    if conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories_fts'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0
    {
        let fts_query = format!("\"{}\"", topic.replace('"', "\"\""));
        let mut fstmt = conn.prepare("SELECT key FROM memories_fts WHERE memories_fts MATCH ?1")?;
        if let Ok(iter) =
            fstmt.query_map(rusqlite::params![fts_query], |row| row.get::<_, String>(0))
        {
            for k in iter.flatten() {
                keys.insert(k);
            }
        }
    }
    Ok(keys)
}

/// The induced subgraph over one corpus (edges as in [`build_graph`], but
/// only between corpus members).
fn subgraph_for(conn: &rusqlite::Connection, corpus: &BTreeSet<String>) -> rusqlite::Result<Graph> {
    let mut graph: Graph = corpus
        .iter()
        .map(|k| (k.clone(), BTreeMap::new()))
        .collect();
    let mut by_tag: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT entry_key, facet, full_tag FROM memory_tags")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (key, facet, full_tag) in rows {
        if corpus.contains(&key) && STRONG_FACETS.contains(&facet.as_str()) {
            by_tag.entry(full_tag).or_default().push(key);
        }
    }
    add_shared_tag_edges(&mut graph, &by_tag);
    let links = read_links(conn)?;
    add_link_edges(&mut graph, &links, Some(corpus));
    Ok(graph)
}

/// One line per entry for the MOC: truncated value + its `facet:value` tags.
fn entry_summary(conn: &rusqlite::Connection, key: &str) -> (String, String) {
    let value: String = conn
        .query_row(
            "SELECT value FROM memory_entries WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let one_line = value.lines().next().unwrap_or("").trim().to_string();
    let truncated = if one_line.chars().count() > 100 {
        format!("{}…", one_line.chars().take(100).collect::<String>())
    } else {
        one_line
    };
    let tags_line = tags::fetch_tags(conn, key)
        .map(|ts| {
            ts.iter()
                .map(|t| t.full_tag.clone())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    (truncated, tags_line)
}

/// Facet coverage table rows: (facet, distinct values, entry count) over the
/// corpus, ordered by coverage.
fn facet_coverage(
    conn: &rusqlite::Connection,
    corpus: &BTreeSet<String>,
) -> rusqlite::Result<Vec<(String, usize, usize)>> {
    let mut by_facet: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
    for key in corpus {
        let mut stmt = conn.prepare("SELECT facet, value FROM memory_tags WHERE entry_key = ?1")?;
        let rows = stmt
            .query_map(rusqlite::params![key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (facet, value) in rows {
            let slot = by_facet.entry(facet).or_default();
            slot.0.insert(value);
            slot.1 += 1;
        }
    }
    let mut out: Vec<(String, usize, usize)> = by_facet
        .into_iter()
        .map(|(f, (values, count))| (f, values.len(), count))
        .collect();
    out.sort_by_key(|(_, _, c)| std::cmp::Reverse(*c));
    Ok(out)
}

/// Typed links with both ends inside the corpus.
fn links_within(
    conn: &rusqlite::Connection,
    corpus: &BTreeSet<String>,
) -> rusqlite::Result<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare("SELECT src, rel, dst FROM memory_links ORDER BY created_at")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|(src, _, dst)| corpus.contains(src) && corpus.contains(dst))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn fixture_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        tags::ensure_tag_schema(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT PRIMARY KEY, value TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'local',
                entry_type TEXT NOT NULL DEFAULT 'lesson',
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    fn tag(conn: &rusqlite::Connection, key: &str, raw: &str) {
        let t = tags::parse_tag(raw).unwrap();
        tags::upsert_tag(conn, key, &t, tags::TagSource::Explicit).unwrap();
    }

    #[rstest]
    fn communities_emerge_from_shared_strong_tags() {
        let conn = fixture_conn();
        // Cluster A shares domain:wiring; cluster B shares domain:ceg;
        // status:stable everywhere must NOT merge them.
        for k in ["a1", "a2", "a3"] {
            tag(&conn, k, "#domain:wiring");
            tag(&conn, k, "#status:stable");
        }
        for k in ["b1", "b2"] {
            tag(&conn, k, "#domain:ceg");
            tag(&conn, k, "#status:stable");
        }
        let communities = detect_communities(&conn, None).unwrap();
        assert_eq!(communities.len(), 2, "status must not glue the graph");
        let sizes: Vec<usize> = communities.iter().map(|c| c.members.len()).collect();
        assert!(sizes.contains(&3) && sizes.contains(&2));
    }

    #[rstest]
    fn links_bridge_tag_islands() {
        let conn = fixture_conn();
        tag(&conn, "x", "#domain:wiring");
        tag(&conn, "y", "#domain:ceg");
        // No shared strong tag, but a typed link must pull them together.
        tags::upsert_link(&conn, "x", tags::LinkRel::Extends, "y").unwrap();
        let communities = detect_communities(&conn, None).unwrap();
        assert_eq!(communities.len(), 1, "link weight bridges the islands");
    }

    /// D2 (cross-audit 2026-08-23): a domain slice restricts membership, not
    /// the edge vocabulary. Keeping only the `domain:<d>` tag itself made
    /// every slice one complete-graph community — structure inside the
    /// domain (here two `purpose:` clusters) must still emerge. `kind` is NOT
    /// a valid cluster signal here: it is deliberately outside STRONG_FACETS.
    #[rstest]
    fn domain_slice_preserves_internal_structure() {
        let conn = fixture_conn();
        for k in ["w1", "w2"] {
            tag(&conn, k, "#domain:wiring");
            tag(&conn, k, "#purpose:orphan-detection");
        }
        for k in ["w3", "w4"] {
            tag(&conn, k, "#domain:wiring");
            tag(&conn, k, "#purpose:blast-radius");
        }
        // Outside the slice: must not appear at all.
        tag(&conn, "other", "#domain:ceg");
        tag(&conn, "other", "#purpose:orphan-detection");

        let communities = detect_communities(&conn, Some("wiring")).unwrap();
        let all: Vec<&String> = communities.iter().flat_map(|c| &c.members).collect();
        assert!(
            !all.iter().any(|m| *m == "other"),
            "slice keeps only the domain"
        );
        assert_eq!(
            communities.len(),
            2,
            "the two kind-clusters inside the domain must stay distinct: {communities:?}"
        );
    }

    #[rstest]
    fn detection_is_deterministic() {
        let conn = fixture_conn();
        for k in ["a", "b", "c", "d"] {
            tag(&conn, k, "#domain:memory");
        }
        let first = detect_communities(&conn, None).unwrap();
        let second = detect_communities(&conn, None).unwrap();
        let sig = |cs: &[Community]| {
            cs.iter()
                .map(|c| (c.label.clone(), c.members.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(sig(&first), sig(&second), "same store → same communities");
    }

    #[rstest]
    fn moc_renders_okf_with_links() {
        let conn = fixture_conn();
        conn.execute(
            "INSERT INTO memory_entries(key, value) VALUES ('m1', 'render the diorama map')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entries(key, value) VALUES ('m2', 'map projection lessons')",
            [],
        )
        .unwrap();
        tag(&conn, "m1", "#domain:maps");
        tag(&conn, "m1", "#artifact:map");
        tag(&conn, "m2", "#domain:maps");
        tags::upsert_link(&conn, "m1", tags::LinkRel::Exemplifies, "m2").unwrap();

        let moc = render_moc(&conn, "maps").unwrap();
        assert!(moc.contains("type: MOC"), "OKF frontmatter");
        assert!(moc.contains("[[m1]]"), "wikilink to member");
        assert!(moc.contains("exemplifies"), "typed link section");
        assert!(moc.contains("| domain |"), "facet coverage table");
    }
}
