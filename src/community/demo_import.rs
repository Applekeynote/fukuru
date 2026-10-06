//! Explicit, idempotent administrator import. No HTTP route, auth credentials, or AI calls.
use super::{
    model as m,
    store::{Records, Result},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const BATCH: &str = "prefectures-2026-q4-v13";
#[derive(Deserialize)]
pub struct Manifest {
    pub batch: String,
    pub owner_handle: String,
    pub accounts: Vec<Value>,
    pub events: Vec<Entry>,
}
#[derive(Deserialize)]
pub struct Entry {
    pub event: Value,
    pub extra: Value,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if self.batch != BATCH
            || self.owner_handle != "rynat"
            || self.accounts.len() != 46
            || self.events.len() != 705
        {
            return Err("Unexpected import manifest".into());
        }
        let mut ids = BTreeSet::new();
        let mut handles = BTreeSet::new();
        for a in &self.accounts {
            let id = a["id"].as_str().ok_or("Missing account ID")?;
            if !id.starts_with("demo_region_")
                || a["sample"] != true
                || a["disabled"] != true
                || a["password_hash"] != ""
                || a["seed_batch"] != BATCH
                || !ids.insert(id)
                || !handles.insert(a["handle"].as_str().ok_or("Missing handle")?)
            {
                return Err("Unsafe or duplicate demo account".into());
            }
        }
        let mut event_ids = BTreeSet::new();
        let mut counts = std::collections::BTreeMap::new();
        for item in &self.events {
            let e = &item.event;
            let id = e["id"].as_str().ok_or("Missing event ID")?;
            let owner = e["owner"].as_str().ok_or("Missing owner")?;
            let start = chrono::DateTime::parse_from_rfc3339(e["start"].as_str().unwrap_or(""))
                .map_err(|_| "Invalid start")?;
            let end = chrono::DateTime::parse_from_rfc3339(e["end"].as_str().unwrap_or(""))
                .map_err(|_| "Invalid end")?;
            use chrono::Datelike;
            if !id.starts_with("spid_region_2026_")
                || !event_ids.insert(id)
                || e["sample"] != true
                || e["seed_batch"] != BATCH
                || (owner != "OWNER" && !ids.contains(owner))
                || start.year() != 2026
                || !(10..=12).contains(&start.month())
                || end <= start
                || e["location_precision"] != "approximate"
                || item.extra["visibility"] != "public"
                // Presentation text is independent of the internal fixture identity.
                || e["description"].as_str().unwrap_or("").trim().is_empty()
                || e["name"].as_str().unwrap_or("").trim().is_empty()
                || e["venue_verified"] != false
                || e["venue_rights"] != false
                || !e["lat"]
                    .as_f64()
                    .is_some_and(|v| (24.0..=46.0).contains(&v))
                || !e["lon"]
                    .as_f64()
                    .is_some_and(|v| (122.0..=146.0).contains(&v))
            {
                return Err("Unsafe or invalid demo event".into());
            }
            *counts.entry((owner, start.month())).or_insert(0) += 1;
        }
        if counts.len() != 141 || counts.values().any(|n| *n != 5) {
            return Err("Each prefecture needs five events per month".into());
        }
        Ok(())
    }
    pub fn check_target(&self, r: &Records) -> Result<String> {
        self.validate()?;
        let owner = m::list(r, "account")
            .into_iter()
            .find(|a| {
                a["handle"] == self.owner_handle && a["disabled"] != true && a["sample"] != true
            })
            .and_then(|a| a["id"].as_str())
            .ok_or("Active @rynat account is required")?;
        for a in &self.accounts {
            let id = a["id"].as_str().unwrap();
            if m::get(r, "account", id)
                .is_some_and(|old| old["seed_batch"] != BATCH || old["disabled"] != true)
                || m::list(r, "account")
                    .iter()
                    .any(|old| old["handle"] == a["handle"] && old["id"] != id)
            {
                return Err("Account collision; no changes made".into());
            }
        }
        for item in &self.events {
            if m::get(r, "event", item.event["id"].as_str().unwrap())
                .is_some_and(|old| old["seed_batch"] != BATCH)
            {
                return Err("Event collision; no changes made".into());
            }
        }
        Ok(owner.to_owned())
    }
    pub fn apply_chunk(&self, r: &mut Records, offset: usize, limit: usize) -> Result<usize> {
        let owner = self.check_target(r)?;
        let at = m::now();
        for a in &self.accounts {
            let id = a["id"].as_str().unwrap();
            if m::get(r, "account", id).is_none() {
                let mut a = a.clone();
                a["created"] = json!(at);
                m::put(r, "account", id, a);
            }
        }
        let mut added = 0;
        for item in self.events.iter().skip(offset).take(limit) {
            let id = item.event["id"].as_str().unwrap();
            if m::get(r, "event", id).is_some() {
                continue;
            }
            let mut e = item.event.clone();
            if e["owner"] == "OWNER" {
                e["owner"] = json!(owner);
            }
            e["created"] = json!(at);
            let host = e["owner"].as_str().unwrap().to_owned();
            m::put(r, "event", id, e);
            m::put(r, "event_extra", id, item.extra.clone());
            let room = format!("event-{id}");
            m::put(
                r,
                "room",
                &room,
                json!({"id":room,"kind":"event","event":id,"owner":host,"name":item.event["name"],"members":[],"at":at}),
            );
            m::put(
                r,
                &format!("rsvp:{id}"),
                &host,
                json!({"status":"going","at":at,"source":"creator"}),
            );
            added += 1;
        }
        Ok(added)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_is_complete_disabled_and_idempotent() {
        let manifest: Manifest =
            serde_json::from_str(include_str!("../../data/regional-events-2026-q4.json")).unwrap();
        let mut r = Records::new();
        m::put(
            &mut r,
            "account",
            "owner",
            json!({"id":"owner","handle":"rynat","disabled":false}),
        );
        assert_eq!(manifest.apply_chunk(&mut r, 0, 705).unwrap(), 705);
        assert_eq!(manifest.apply_chunk(&mut r, 0, 705).unwrap(), 0);
        assert_eq!(m::list(&r, "event").len(), 705);
        assert_eq!(m::list(&r, "account").len(), 47);
        assert_eq!(
            m::list(&r, "event")
                .iter()
                .filter(|e| e["owner"] == "owner")
                .count(),
            15
        );
        assert!(m::list(&r, "account")
            .iter()
            .filter(|a| a["sample"] == true)
            .all(|a| a["disabled"] == true && a["password_hash"] == ""));
    }
    #[test]
    fn import_rejects_missing_owner_and_collisions() {
        let manifest: Manifest =
            serde_json::from_str(include_str!("../../data/regional-events-2026-q4.json")).unwrap();
        let mut r = Records::new();
        assert!(manifest.apply_chunk(&mut r, 0, 705).is_err());
        assert!(r.is_empty());
        m::put(
            &mut r,
            "account",
            "owner",
            json!({"id":"owner","handle":"rynat"}),
        );
        m::put(
            &mut r,
            "event",
            "spid_region_2026_01_10_1",
            json!({"name":"Keep existing"}),
        );
        assert!(manifest.apply_chunk(&mut r, 0, 705).is_err());
        assert_eq!(m::list(&r, "event").len(), 1);
    }
}
