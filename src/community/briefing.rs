use super::{
    model as m,
    store::{Records, Result},
};
use serde_json::{json, Value};

pub fn recent(r: &Records, now: i64) -> Vec<Value> {
    let mut events: Vec<_> = m::list(r, "event")
        .into_iter()
        .filter(|e| !m::is_demo_event(r,e) && e["status"]!="canceled" && super::completion::listed(r,e,None))
        .filter(|e| {
            let parse = |k: &str| {
                chrono::DateTime::parse_from_rfc3339(e[k].as_str().unwrap_or(""))
                    .ok()
                    .map(|d| d.timestamp())
            };
            matches!((parse("start"),parse("end")), (Some(s),Some(t)) if s < now+72*3600 && t > now)
        })
        .collect();
    events.sort_by_key(|e| {
        chrono::DateTime::parse_from_rfc3339(e["start"].as_str().unwrap_or(""))
            .map(|d| d.timestamp())
            .unwrap_or(i64::MAX)
    });
    events.into_iter().take(6).map(|e| {
        let id=e["id"].as_str().unwrap_or("");
        let mut notices:Vec<_>=m::list(r,&format!("notice:{id}")).into_iter().filter(|n|n["at"].as_i64().unwrap_or(0)>=now-72*3600).cloned().collect();
        notices.sort_by_key(|n|std::cmp::Reverse(n["at"].as_i64().unwrap_or(0)));
        notices.truncate(3);
        json!({"id":id,"name":e["name"],"description":e["description"],"place":e["place"],"start":e["start"],"end":e["end"],"sample":e["sample"],"notices":notices})
    }).collect()
}

pub fn charge(r: &mut Records, user: &str) -> Result<()> {
    super::lifecycle::charge(r,user,m::now())
}

pub fn validate(v: &Value, source: &[Value]) -> bool {
    let Some(rows) = v["items"].as_array() else {
        return false;
    };
    if rows.len() != source.len() {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    rows.iter().all(|x| {
        let id = x["id"].as_str().unwrap_or("");
        source.iter().any(|e| e["id"] == id)
            && ids.insert(id)
            && ["title", "summary", "notice"].iter().all(|k| {
                x[k].as_str()
                    .map(|s| s.chars().count() <= 800 && (*k != "title" || !s.trim().is_empty()))
                    .unwrap_or(false)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_and_private_content() {
        let mut r = Records::new();
        let now = 1_800_000_000;
        let date = |n| chrono::DateTime::from_timestamp(n, 0).unwrap().to_rfc3339();
        for (id, s, e) in [
            ("live", now - 100, now + 100),
            ("ended", now - 100, now),
            ("edge", now + 72 * 3600, now + 72 * 3600 + 100),
        ] {
            m::put(
                &mut r,
                "event",
                id,
                json!({"id":id,"start":date(s),"end":date(e)}),
            );
        }
        m::put(
            &mut r,
            "message:event-live",
            "private",
            json!({"body":"PRIVATE"}),
        );
        m::put(
            &mut r,
            "notice:live",
            "old",
            json!({"body":"OLD","at":now-72*3600-1}),
        );
        let rows = recent(&r, now);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "live");
        assert!(!json!(rows).to_string().contains("PRIVATE"));
        assert!(!json!(rows).to_string().contains("OLD"));
    }
    #[test]
    fn reject_invented_or_duplicate_ids() {
        let sources = vec![json!({"id":"a"}), json!({"id":"b"})];
        let row = |id| json!({"id":id,"title":"title","summary":"body","notice":""});
        assert!(validate(&json!({"items":[row("a"),row("b")]}), &sources));
        assert!(!validate(&json!({"items":[row("a"),row("a")]}), &sources));
        assert!(!validate(
            &json!({"items":[row("a"),row("invented")]}),
            &sources
        ));
    }
}
