//! One-time profile-only migration of the existing disabled regional identities.
use super::{
    demo_import::BATCH,
    model as m,
    store::{Records, Result},
};
use serde_json::{json, Value};
pub fn apply(r: &mut Records) -> Result<usize> {
    let profiles: Vec<Value> =
        serde_json::from_str(include_str!("../../data/regional-profiles-v15.json"))
            .map_err(|e| e.to_string())?;
    if profiles.len() != 46 {
        return Err("Unexpected regional profile manifest".into());
    }
    let mut changed = 0;
    for p in profiles {
        let id = p["id"].as_str().ok_or("Missing regional ID")?;
        let Some(old) = m::get(r, "account", id) else {
            continue;
        };
        if old["seed_batch"] != BATCH
            || old["handle"] != p["handle"]
            || old["disabled"] != true
            || old["password_hash"] != ""
        {
            return Err("Regional identity mismatch".into());
        }
        if old["profile_version"].as_u64().unwrap_or(0) >= 15 {
            continue;
        }
        let mut updated = old.clone();
        for key in ["name", "bio", "region", "interests", "profile_version"] {
            updated[key] = p[key].clone();
        }
        m::put(r, "account", id, updated);
        changed += 1;
    }
    if changed > 0 {
        m::put(
            r,
            "migration",
            "regional-profiles-v15",
            json!({"changed":changed,"at":m::now()}),
        );
    }
    Ok(changed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_never_enable_accounts_and_are_idempotent() {
        let mut r = Records::new();
        let p: Vec<Value> =
            serde_json::from_str(include_str!("../../data/regional-profiles-v15.json")).unwrap();
        let a = &p[0];
        let id = a["id"].as_str().unwrap();
        m::put(
            &mut r,
            "account",
            id,
            json!({"id":id,"handle":a["handle"],"disabled":true,"password_hash":"","sample":true,"seed_batch":BATCH}),
        );
        assert_eq!(apply(&mut r).unwrap(), 1);
        let after = m::get(&r, "account", id).unwrap();
        assert_eq!(after["disabled"], true);
        assert_eq!(after["password_hash"], "");
        assert_eq!(after["sample"], true);
        assert_eq!(after["interests"].as_array().unwrap().len(), 2);
        assert_eq!(apply(&mut r).unwrap(), 0);
    }
}
