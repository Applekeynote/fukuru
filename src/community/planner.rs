//! AI selects only from authorized canonical candidates. Constraints are checked again after inference.
use super::{
    model as m,
    store::{Records, Result},
};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
pub struct Input {
    pub from: String,
    pub until: String,
    pub region: String,
    pub budget: f64,
    pub travel: f64,
    pub transport: String,
    pub interest: String,
    pub request: String,
}
impl Input {
    pub fn window(&self, now: i64) -> Result<(i64, i64)> {
        let parse = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.timestamp())
                .map_err(|_| "日時を指定してください".to_string())
        };
        let a = parse(&self.from)?;
        let z = parse(&self.until)?;
        if a < now - 60
            || z <= a
            || z - a > 7 * 86400
            || z > super::lifecycle::STOP_AT
            || !self.budget.is_finite()
            || !(0.0..=1_000_000.0).contains(&self.budget)
            || !self.travel.is_finite()
            || !(0.0..=240.0).contains(&self.travel)
            || !["walk", "transit", "cycle", "drive"].contains(&self.transport.as_str())
            || self.region.is_empty()
            || self.region.chars().count() > 80
            || self.request.chars().count() > 1000
            || self.interest.chars().count() > 60
        {
            return Err("期間は今から7日以内の幅、地域・予算・移動条件を指定してください".into());
        }
        Ok((a, z))
    }
}
pub fn candidates(r: &Records, user: &str, input: &Input, now: i64) -> Result<Vec<Value>> {
    let (a, z) = input.window(now)?;
    let mut rows:Vec<_>=m::list(r,"event").into_iter().filter(|e| {
        let time=|k:&str|chrono::DateTime::parse_from_rfc3339(e[k].as_str().unwrap_or("")).map(|d|d.timestamp()).ok();
        let text=format!("{} {}",e["place"].as_str().unwrap_or(""),e["address"].as_str().unwrap_or(""));
        super::completion::can_view(r,e,Some(user)) && super::completion::listed(r,e,Some(user))
            && !m::blocked(r,user,e["owner"].as_str().unwrap_or(""))
            && (!m::is_demo_event(r,e) || e["seed_batch"]==super::demo_import::BATCH)
            && e["status"]!="canceled" && matches!((time("start"),time("end")),(Some(s),Some(t)) if s>=a && t<=z)
            && text.contains(input.region.trim())
            && e["price_yen"].as_f64().is_some_and(|p|p>=0. && p<=input.budget)
            && (input.interest=="all" || e["kind"]==input.interest)
    }).map(|e|json!({"id":e["id"],"name":e["name"],"kind":e["kind"],"description":e["description"],"start":e["start"],"end":e["end"],"place":e["place"],"price_yen":e["price_yen"],"lat":e["lat"],"lon":e["lon"]})).collect();
    rows.sort_by(|a, b| a["start"].as_str().cmp(&b["start"].as_str()));
    rows.truncate(40);
    Ok(rows)
}
fn gap(a: &Value, b: &Value, input: &Input) -> Option<f64> {
    let rad = |n: f64| n.to_radians();
    let la = rad(a["lat"].as_f64()?);
    let lb = rad(b["lat"].as_f64()?);
    let dl = rad(b["lon"].as_f64()? - a["lon"].as_f64()?);
    let d = 6371.0
        * 2.0
        * (((lb - la) / 2.).sin().powi(2) + la.cos() * lb.cos() * (dl / 2.).sin().powi(2))
            .sqrt()
            .min(1.)
            .asin();
    let (speed, extra) = match input.transport.as_str() {
        "cycle" => (12., 0.),
        "drive" => (25., 10.),
        "transit" => (20., 10.),
        _ => (4., 0.),
    };
    Some(input.travel.max(d / speed * 60. + extra))
}
pub fn validate(answer: &Value, source: &[Value], input: &Input) -> Result<Value> {
    let choices = answer["items"]
        .as_array()
        .ok_or("行程案を検証できませんでした")?;
    if choices.is_empty() || choices.len() > 6 {
        return Err("条件に合う行程案を作れませんでした".into());
    }
    let mut unique = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for choice in choices {
        let id = choice["id"].as_str().ok_or("候補が不正です")?;
        let reason = choice["reason"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.chars().count() <= 240)
            .ok_or("推薦理由が不正です")?;
        let mut e = source
            .iter()
            .find(|e| e["id"] == id)
            .ok_or("登録されていない候補です")?
            .clone();
        if !unique.insert(id) {
            return Err("候補が重複しています".into());
        }
        e["reason"] = json!(reason);
        rows.push(e);
    }
    rows.sort_by(|a, b| a["start"].as_str().cmp(&b["start"].as_str()));
    let cost: f64 = rows.iter().filter_map(|e| e["price_yen"].as_f64()).sum();
    if cost > input.budget {
        return Err("提案が予算上限を超えています。条件を変更してください".into());
    }
    for i in 0..rows.len() {
        let minutes = if i == 0 {
            0.
        } else {
            gap(&rows[i - 1], &rows[i], input).ok_or("移動距離を検証できませんでした")?
        };
        if i > 0 {
            let start = chrono::DateTime::parse_from_rfc3339(rows[i]["start"].as_str().unwrap())
                .unwrap()
                .timestamp();
            let end = chrono::DateTime::parse_from_rfc3339(rows[i - 1]["end"].as_str().unwrap())
                .unwrap()
                .timestamp();
            if ((start - end) as f64) < minutes * 60. {
                return Err("提案の移動時間が不足しています。条件を変更してください".into());
            }
        }
        rows[i]["travel_minutes"] = json!(minutes.ceil());
    }
    Ok(
        json!({"items":rows,"cost_yen":cost,"source":"Gemini","transport":input.transport,"travel_estimate":true}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        Input {
            from: "2026-10-12T00:00:00Z".into(),
            until: "2026-10-12T12:00:00Z".into(),
            region: "愛知県".into(),
            budget: 500.,
            travel: 30.,
            transport: "walk".into(),
            interest: "all".into(),
            request: "写真".into(),
        }
    }
    #[test]
    fn reject_fabrications_over_budget_overlap_and_duplicate() {
        let src = vec![
            json!({"id":"a","start":"2026-10-12T01:00:00Z","end":"2026-10-12T02:00:00Z","lat":35.,"lon":136.,"price_yen":300}),
            json!({"id":"b","start":"2026-10-12T02:10:00Z","end":"2026-10-12T03:00:00Z","lat":35.,"lon":136.,"price_yen":300}),
        ];
        let row = |id| json!({"id":id,"reason":"写真の条件に合う"});
        assert!(validate(&json!({"items":[row("a")]}), &src, &input()).is_ok());
        for ids in [
            vec![row("invented")],
            vec![row("a"), row("a")],
            vec![row("a"), row("b")],
        ] {
            assert!(validate(&json!({"items":ids}), &src, &input()).is_err());
        }
        let mut x = input();
        x.budget = 1000.;
        assert!(validate(&json!({"items":[row("a"),row("b")]}), &src, &x).is_err());
    }
}
