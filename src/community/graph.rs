//! Public identity/relationship projection. Private conversations never enter the graph.
use super::store::Records;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn enqueue(old: &Records, records: &mut Records) {
    let updates: Vec<_> = records
        .iter()
        .filter(|((s, id), v)| s == "event" && old.get(&(s.clone(), id.clone())) != Some(v))
        .map(|((_, id), v)| (id.clone(), v.clone()))
        .collect();
    for (id, event) in updates {
        let key = format!("{}:{}", id, event["version"]);
        records
            .entry(("geo_outbox".into(), key.clone()))
            .or_insert_with(
                || json!({"id":key,"event":event,"state":"PENDING","attempts":0,"lease":0}),
            );
    }
}

pub fn project(r: &Records) -> BTreeMap<(String, String), Value> {
    let mut g = BTreeMap::new();
    for ((scope, id), v) in r {
        if scope=="event" && !super::completion::listed(r,v,None){continue;}
        if let Some(eid)=scope.strip_prefix("rsvp:"){if super::model::get(r,"event",eid).map(|e|!super::completion::listed(r,e,None)).unwrap_or(true){continue;}}
        if scope == "account" || scope == "event" {
            g.insert(
                ("node".into(), id.clone()),
                json!([id, scope, v["name"], v["owner"].as_str().unwrap_or(id)]),
            );
        }
        let edge = if scope == "event" {
            Some((v["owner"].as_str().unwrap_or(""), id.as_str(), "HOSTS"))
        } else if let Some(target) = scope.strip_prefix("follow:") {
            Some((id.as_str(), target, "FOLLOWS"))
        } else if let Some(target) = scope.strip_prefix("rsvp:") {
            Some((
                id.as_str(),
                target,
                if v["status"] == "going" {
                    "GOING"
                } else {
                    "INTERESTED"
                },
            ))
        } else {
            None
        };
        if let Some((source, target, relation)) = edge {
            g.insert(
                ("edge".into(), format!("{source}:{target}:{relation}")),
                json!([source, target, relation]),
            );
        }
    }
    g
}
pub fn mutations(old: &Records, new: &Records) -> Vec<Value> {
    let before = project(old);
    let after = project(new);
    let mut result = Vec::new();
    for (key, row) in &after {
        if before.get(key) == Some(row) {
            continue;
        }
        let (table, columns) = if key.0 == "node" {
            ("CommunityIdentity", vec!["id", "kind", "name", "owner"])
        } else {
            (
                "CommunityRelationship",
                vec!["source", "target", "relation"],
            )
        };
        result.push(json!({"insertOrUpdate":{"table":table,"columns":columns,"values":[row]}}));
    }
    for (key, row) in &before {
        if after.contains_key(key) {
            continue;
        }
        let (table, keys) = if key.0 == "node" {
            ("CommunityIdentity", json!([row[0]]))
        } else {
            ("CommunityRelationship", row.clone())
        };
        result.push(json!({"delete":{"table":table,"keySet":{"keys":[keys]}}}));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_private_content_and_removes_old_edges() {
        let mut r = Records::new();
        r.insert(
            ("account".into(), "a".into()),
            json!({"name":"A","password_hash":"secret"}),
        );
        r.insert(("message:r".into(), "m".into()), json!({"body":"private"}));
        r.insert(("follow:b".into(), "a".into()), json!({}));
        let g = project(&r);
        assert_eq!(g.len(), 2);
        assert!(!serde_json::to_string(&g.values().collect::<Vec<_>>())
            .unwrap()
            .contains("secret"));
        let mut after = r.clone();
        after.remove(&("follow:b".into(), "a".into()));
        let m = mutations(&r, &after);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0]["delete"]["table"], "CommunityRelationship");
    }
}
