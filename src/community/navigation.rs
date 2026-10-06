//! Session-scoped, server-authoritative soft currency. Unreviewed geometry earns nothing.
//! Browser sensor claims are evidence, never proof of device integrity or a wallet command.
use super::{
    model as m,
    store::{Records, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    pub lat: f64,
    pub lon: f64,
    pub accuracy: f64,
    pub timestamp: i64,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub route_session_id: String,
    pub nonce: String,
    pub sequence: u32,
    pub request_id: String,
    pub position: Position,
    #[serde(default)]
    pub imu: Option<Value>,
    #[serde(default)]
    pub device_integrity: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionRequest {
    pub destination: String,
    pub mode: String,
    pub path: Vec<[f64; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub session_ttl: i64,
    pub max_sessions_per_day: u64,
    pub coin_radius: f64,
    pub max_accuracy: f64,
    pub coin_spacing: f64,
    pub cooldown: i64,
    pub daily_cap: u64,
    pub soft: u32,
    pub pending: u32,
    pub reject: u32,
    pub arrival_dwell: i64,
    pub max_path_points: usize,
    pub diminishing_after: u64,
    pub evidence_window: i64,
    pub max_sensor_payload_bytes: usize,
    pub ledger_retention_seconds: i64,
    pub ledger_limit_per_account: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            session_ttl: 7200,
            max_sessions_per_day: 8,
            coin_radius: 4.,
            max_accuracy: 12.,
            coin_spacing: 100.,
            cooldown: 86400,
            daily_cap: 100,
            soft: 21,
            pending: 51,
            reject: 81,
            arrival_dwell: 10,
            max_path_points: 2500,
            diminishing_after: 50,
            evidence_window: 15,
            max_sensor_payload_bytes: 4096,
            ledger_retention_seconds: 604800,
            ledger_limit_per_account: 32,
        }
    }
}
impl Config {
    pub fn load(r: &Records) -> Result<Self> {
        let cfg: Self = match m::get(r, "navigation_config", "runtime") {
            Some(v) => {
                serde_json::from_value(v.clone()).map_err(|_| "invalid navigation configuration")?
            }
            None => Self::default(),
        };
        if cfg.session_ttl <= 0
            || cfg.session_ttl > 86400
            || cfg.coin_radius < 3.
            || cfg.coin_radius > 5.
            || cfg.max_accuracy <= 0.
            || cfg.max_accuracy > 20.
            || cfg.coin_spacing < 30.
            || cfg.max_sessions_per_day == 0
            || cfg.cooldown < 3600
            || cfg.cooldown > 604800
            || cfg.daily_cap > 1000
            || cfg.max_sessions_per_day > 32
            || cfg.soft >= cfg.pending
            || cfg.pending >= cfg.reject
            || cfg.reject > 100
            || cfg.arrival_dwell < 10
            || cfg.max_path_points > 2500
            || cfg.max_path_points < 2
            || cfg.max_sensor_payload_bytes > 8192
            || cfg.max_sensor_payload_bytes < 512
            || cfg.ledger_retention_seconds < 86400
            || cfg.ledger_retention_seconds > 604800
            || cfg.ledger_limit_per_account < 1
            || cfg.ledger_limit_per_account > 64
            || cfg.evidence_window > 30
            || cfg.evidence_window < 1
        {
            return Err("invalid navigation configuration".into());
        }
        Ok(cfg)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardType {
    RouteCoin,
    DiscoveryToken,
    JourneyBonus,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyClassification {
    VerifiedSidewalk,
    Crossing,
    Railway,
    PrivateProperty,
    ProhibitedArea,
    DangerousRoad,
    Construction,
    Water,
    Cliff,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyCorridor {
    pub reviewed_by: String,
    pub expires_at: i64,
    pub classification: SafetyClassification,
    pub safety_zone: Vec<[f64; 2]>,
    pub points: Vec<[f64; 2]>,
    pub hazards: Vec<Value>,
    #[serde(default)]
    pub allowed_modes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionRule {
    pub radius: f64,
    pub max_accuracy: f64,
    pub segment: String,
    pub safety_zone: Vec<[f64; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Coin {
    pub coin_id: String,
    pub position: [f64; 2],
    pub collection_rule: CollectionRule,
    pub status: String,
    pub reward_type: RewardType,
}
pub fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    let r = std::f64::consts::PI / 180.;
    let x = (b[0] - a[0]) * r;
    let y = (b[1] - a[1]) * r;
    12742000.
        * ((x / 2.).sin().powi(2) + (a[0] * r).cos() * (b[0] * r).cos() * (y / 2.).sin().powi(2))
            .sqrt()
            .min(1.)
            .asin()
}
fn valid_point(p: [f64; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite() && p[0].abs() <= 90. && p[1].abs() <= 180.
}
fn geometry(path: &[[f64; 2]], p: [f64; 2]) -> (f64, f64, f64) {
    let mut total = 0.;
    let mut best = (f64::INFINITY, 0.);
    for pair in path.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let scale = p[0].to_radians().cos();
        let ax = (a[1] - p[1]) * scale;
        let ay = a[0] - p[0];
        let dx = (b[1] - a[1]) * scale;
        let dy = b[0] - a[0];
        let t = (-(ax * dx + ay * dy) / (dx * dx + dy * dy).max(1e-20)).clamp(0., 1.);
        let length = distance(a, b);
        let off = distance(p, [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        if off < best.0 {
            best = (off, total + length * t);
        }
        total += length;
    }
    (best.0, best.1, total)
}
fn nonce() -> String {
    format!("{}{}", m::uid(), m::uid())
}
// Only trusted, unexpired corridor records may mint a manifest. Client safety flags are ignored.
// A future sidewalk/railway/private-property provider implements this record boundary.
fn inside_zone(p: [f64; 2], zone: &[[f64; 2]], margin: f64) -> bool {
    if zone.len() < 3 || zone.len() > 128 || zone.iter().any(|p| !valid_point(*p)) {
        return false;
    }
    let mut inside = false;
    let mut j = zone.len() - 1;
    for i in 0..zone.len() {
        let a = zone[i];
        let b = zone[j];
        if (a[0] > p[0]) != (b[0] > p[0])
            && p[1] < (b[1] - a[1]) * (p[0] - a[0]) / (b[0] - a[0]) + a[1]
        {
            inside = !inside;
        }
        if geometry(&[a, b], p).0 < margin {
            return false;
        }
        j = i;
    }
    inside
}
fn manifest(
    r: &Records,
    path: &[[f64; 2]],
    mode: &str,
    now: i64,
    c: &Config,
) -> (Vec<Coin>, Vec<Value>) {
    let mut coins = Vec::new();
    let mut hazards = Vec::new();
    let mut cells = std::collections::BTreeSet::new();
    for record in m::list(r, "navigation_corridor") {
        if record["reviewed_by"]
            .as_str()
            .is_none_or(|name| name.trim().is_empty())
            || record["expires_at"].as_i64().unwrap_or(0) <= now
        {
            continue;
        }
        if let Some(items) = record["hazards"].as_array() {
            hazards.extend(items.iter().cloned());
        }
        let permitted = record["allowed_modes"]
            .as_array()
            .map(|modes| modes.iter().any(|m| m == mode))
            .unwrap_or(mode == "WALKING");
        if !permitted {
            continue;
        }
        if record["classification"] != "verified_sidewalk"
            || record["hazards"].as_array().is_none_or(|v| !v.is_empty())
        {
            continue;
        }
        let zone: Vec<[f64; 2]> =
            serde_json::from_value(record["safety_zone"].clone()).unwrap_or_default();
        let Some(points) = record["points"].as_array() else {
            continue;
        };
        let mut previous = None;
        for raw in points {
            let Some(lat) = raw[0].as_f64() else { continue };
            let Some(lon) = raw[1].as_f64() else { continue };
            let p = [lat, lon];
            if !valid_point(p)
                || !inside_zone(p, &zone, c.coin_radius)
                || geometry(path, p).0 > 5.
                || previous
                    .map(|q| distance(q, p) < c.coin_spacing)
                    .unwrap_or(false)
            {
                continue;
            }
            let segment = m::hash(&format!("{lat:.4}:{lon:.4}"));
            if !cells.insert(segment.clone()) {
                continue;
            }
            previous = Some(p);
            coins.push(Coin {
                coin_id: m::uid(),
                position: p,
                collection_rule: CollectionRule {
                    radius: c.coin_radius,
                    max_accuracy: c.max_accuracy,
                    segment,
                    safety_zone: zone.clone(),
                },
                status: "available".into(),
                reward_type: RewardType::RouteCoin,
            });
            if coins.len() >= 30 {
                break;
            }
        }
    }
    coins.retain(|coin| {
        !hazards.iter().any(|h| {
            h["point"]["lat"]
                .as_f64()
                .zip(h["point"]["lon"].as_f64())
                .is_some_and(|(lat, lon)| distance(coin.position, [lat, lon]) < 35.)
        })
    });
    (coins, hazards)
}
pub fn create(
    r: &mut Records,
    user: &str,
    q: &SessionRequest,
    now: i64,
    c: &Config,
) -> Result<Value> {
    if !["WALKING", "BICYCLING", "DRIVING"].contains(&q.mode.as_str())
        || q.path.len() < 2
        || q.path.len() > c.max_path_points
        || q.path.iter().any(|p| !valid_point(*p))
    {
        return Err("invalid route".into());
    }
    let dest = m::get(r, "event", &q.destination).ok_or("destination missing")?;
    let target = [
        dest["lat"].as_f64().ok_or("destination position")?,
        dest["lon"].as_f64().ok_or("destination position")?,
    ];
    let demo = dest["demo"] == true || m::is_demo_event(r, dest);
    if distance(*q.path.last().unwrap(), target) > 100.
        || geometry(&q.path, q.path[0]).2 > 100000.
        || q.path.windows(2).any(|p| distance(p[0], p[1]) > 2000.)
    {
        return Err("invalid route geometry".into());
    }
    let day = now / 86400;
    let counter = format!("{user}:{day}");
    let count = m::get(r, "navigation_limit", &counter)
        .and_then(|v| v["count"].as_u64())
        .unwrap_or(0);
    if count >= c.max_sessions_per_day {
        return Err("route session limit".into());
    }
    // Bound counters and purge expired exact coordinates on normal navigation use.
    r.retain(|(scope, _), v| {
        ![
            "navigation_limit",
            "navigation_daily",
            "navigation_cooldown",
        ]
        .contains(&scope.as_str())
            || v["at"].as_i64().is_some_and(|at| now - at < 604800)
    });
    // Reward ledger keeps aggregates, never exact coordinates.
    r.retain(|(scope, _), v| {
        scope != "navigation_session" || v["expires_at"].as_i64().unwrap_or(0) > now
    });
    for session in r
        .iter_mut()
        .filter(|((scope, _), v)| scope == "navigation_session" && v["owner"] == user)
    {
        session.1["closed"] = json!(true);
    }
    r.retain(|(scope, _), v| {
        scope != "navigation_ledger"
            || v["at"]
                .as_i64()
                .is_some_and(|at| now - at < c.ledger_retention_seconds)
    });
    let mut journal: Vec<_> = r
        .iter()
        .filter(|((scope, _), v)| scope == "navigation_ledger" && v["owner"] == user)
        .map(|(key, v)| (key.clone(), v["at"].as_i64().unwrap_or(0)))
        .collect();
    journal.sort_by_key(|(_, at)| *at);
    let excess = journal
        .len()
        .saturating_sub(c.ledger_limit_per_account.saturating_sub(1));
    for (key, _) in journal.into_iter().take(excess) {
        r.remove(&key);
    }
    let (mut coins, hazards) = manifest(r, &q.path, &q.mode, now, c);
    if demo {
        coins.clear();
    }
    let id = m::uid();
    let token = nonce();
    let session = json!({"id":id,"owner":user,"destination":q.destination,"mode":q.mode,"path":q.path,"coins":coins,"hazards":hazards,"nonce_hash":m::hash(&token),"expires_at":now+c.session_ttl,"created_at":now,"sequence":0,"risk_score":0,"samples":0,"reward_count":0,"closed":false});
    m::put(r, "navigation_session", &id, session);
    m::put(
        r,
        "navigation_limit",
        &counter,
        json!({"count":count+1,"at":now}),
    );
    Ok(
        json!({"route_session_id":id,"nonce":token,"expires_at":now+c.session_ttl,"coins":coins,"hazards":hazards,"reward_count":0,"transferable":false}),
    )
}
pub fn submit(r: &mut Records, user: &str, e: &Evidence, now: i64, c: &Config) -> Result<Value> {
    if e.request_id.len() > 64 || uuid::Uuid::parse_str(&e.request_id).is_err() {
        return Err("invalid request id".into());
    }
    let mut s = m::get(r, "navigation_session", &e.route_session_id)
        .cloned()
        .ok_or("session missing")?;
    if s["owner"] != user {
        return Err("session owner mismatch".into());
    }
    if s["expires_at"].as_i64().unwrap_or(0) <= now || s["closed"] == true {
        return Err("session expired".into());
    }
    // Idempotent retry must match the entire request. Never repeat a ledger credit.
    let encoded = serde_json::to_string(e).map_err(|_| "invalid evidence")?;
    if encoded.len() > c.max_sensor_payload_bytes {
        return Err("sensor evidence too large".into());
    }
    let fingerprint = m::hash(&encoded);
    if s["last_request"] == e.request_id {
        return if s["last_fingerprint"] == fingerprint {
            Ok(s["last_result"].clone())
        } else {
            Err("replay mismatch".into())
        };
    }
    if s["nonce_hash"] != m::hash(&e.nonce)
        || e.sequence as u64 != s["sequence"].as_u64().unwrap_or(0) + 1
    {
        return Err("replayed evidence".into());
    }
    let p = &e.position;
    let pos = [p.lat, p.lon];
    if !valid_point(pos)
        || !p.accuracy.is_finite()
        || p.accuracy < 0.
        || p.accuracy > 10000.
        || p.speed.is_some_and(|v| !v.is_finite() || v < 0.)
        || p.heading
            .is_some_and(|v| !v.is_finite() || !(0.0..=360.).contains(&v))
    {
        return Err("invalid sensor evidence".into());
    }
    let path: Vec<[f64; 2]> =
        serde_json::from_value(s["path"].clone()).map_err(|_| "route unavailable")?;
    let (off, progress, total) = geometry(&path, pos);
    let mut risk = s["risk_score"].as_u64().unwrap_or(0) as u32;
    let mut anomaly = 0;
    let maxspeed = match s["mode"].as_str() {
        Some("DRIVING") => 60.,
        Some("BICYCLING") => 16.,
        _ => 4.5,
    };
    if p.speed.is_some_and(|speed| speed > maxspeed) {
        anomaly += 35;
    }
    let timestamp = p.timestamp / 1000;
    if (timestamp - now).abs() > c.evidence_window {
        anomaly += 35;
    }
    if s["samples"].as_u64().unwrap_or(0) == 0 && distance(pos, path[0]) > 25. + p.accuracy.min(15.)
    {
        anomaly += 60;
    }
    let mut continuous = false;
    if let Some(last) = s["last_position"].as_array() {
        let lastpos = [
            last[0].as_f64().unwrap_or(0.),
            last[1].as_f64().unwrap_or(0.),
        ];
        let elapsed = now - s["last_at"].as_i64().unwrap_or(now);
        if elapsed <= 0 {
            return Err("evidence too frequent".into());
        }
        if timestamp <= s["last_timestamp"].as_i64().unwrap_or(0) {
            anomaly += 40;
        }
        if (distance(lastpos, pos) - p.accuracy - s["last_accuracy"].as_f64().unwrap_or(0.)).max(0.)
            / elapsed as f64
            > maxspeed
        {
            anomaly += 60;
        }
        if progress + 30. < s["progress"].as_f64().unwrap_or(0.) {
            anomaly += 15;
        }
        continuous = elapsed <= 20 && anomaly == 0;
    }
    if off > 40. + p.accuracy.min(20.) {
        anomaly += 25;
    }
    risk = (risk + anomaly).min(100);
    if anomaly == 0 && continuous {
        risk = risk.saturating_sub(2);
    }
    let samples = s["samples"].as_u64().unwrap_or(0) + 1;
    let status = if risk >= c.reject {
        "rejected"
    } else if risk >= c.pending {
        "pending"
    } else if risk >= c.soft {
        "verification"
    } else {
        "normal"
    };
    let mut collected = Vec::new();
    let mut count = s["reward_count"].as_u64().unwrap_or(0);
    let daykey = format!("{user}:{}", now / 86400);
    let mut daily = m::get(r, "navigation_daily", &daykey)
        .and_then(|v| v["amount"].as_u64())
        .unwrap_or(0);
    let mut claims = m::get(r, "navigation_daily", &daykey)
        .and_then(|v| v["claims"].as_u64())
        .unwrap_or(0);
    let in_hazard = s["hazards"].as_array().is_some_and(|hs| {
        hs.iter().any(|h| {
            let x = h["point"]["lat"].as_f64();
            let y = h["point"]["lon"].as_f64();
            x.zip(y)
                .is_some_and(|(lat, lon)| distance(pos, [lat, lon]) < 30. + p.accuracy)
        })
    });
    if continuous
        && samples >= 3
        && risk < c.soft
        && p.accuracy <= c.max_accuracy
        && off <= 10.
        && !in_hazard
    {
        if let Some(coins) = s["coins"].as_array_mut() {
            for coin in coins {
                if coin["status"] != "available" {
                    continue;
                }
                let cp = [
                    coin["position"][0].as_f64().unwrap_or(0.),
                    coin["position"][1].as_f64().unwrap_or(0.),
                ];
                let radius = c.coin_radius + p.accuracy.min(c.coin_radius);
                let zone: Vec<[f64; 2]> =
                    serde_json::from_value(coin["collection_rule"]["safety_zone"].clone())
                        .unwrap_or_default();
                if !inside_zone(pos, &zone, p.accuracy) || distance(pos, cp) > radius {
                    continue;
                }
                let key = format!(
                    "{}:{}",
                    user,
                    coin["collection_rule"]["segment"].as_str().unwrap_or("")
                );
                if m::get(r, "navigation_cooldown", &key)
                    .and_then(|v| v["at"].as_i64())
                    .is_some_and(|at| now - at < c.cooldown)
                    || daily >= c.daily_cap
                {
                    coin["status"] = json!("cooldown");
                    continue;
                }
                coin["status"] = json!("collected");
                claims += 1;
                let amount = u64::from(claims <= c.diminishing_after || claims % 2 == 0);
                count += amount;
                daily += amount;
                collected.push(coin["coin_id"].clone());
                m::put(r, "navigation_cooldown", &key, json!({"at":now}));
            }
        }
    }
    let near_end = continuous
        && samples >= 3
        && risk < c.soft
        && p.accuracy <= c.max_accuracy
        && !in_hazard
        && p.speed.is_none_or(|speed| speed <= 0.7)
        && total - progress < 15.
        && distance(pos, *path.last().unwrap()) < 15.;
    let since = if near_end {
        s["arrival_since"].as_i64().unwrap_or(now)
    } else {
        now
    };
    s["arrival_since"] = if near_end { json!(since) } else { Value::Null };
    let has_verified = s["coins"].as_array().is_some_and(|coins| {
        coins.iter().any(|coin| {
            let zone: Vec<[f64; 2]> =
                serde_json::from_value(coin["collection_rule"]["safety_zone"].clone())
                    .unwrap_or_default();
            inside_zone(pos, &zone, p.accuracy)
        })
    });
    let mut journey_credit = 0;
    if near_end
        && now - since >= c.arrival_dwell
        && has_verified
        && s["journey_bonus"] != true
        && s["max_progress"].as_f64().unwrap_or(0.) >= total * 0.8
        && daily < c.daily_cap
    {
        s["journey_bonus"] = json!(true);
        let bonus = 5.min(c.daily_cap - daily);
        journey_credit = bonus;
        count += bonus;
        daily += bonus;
    }
    let token = nonce();
    let result = json!({"route_session_id":e.route_session_id,"nonce":token,"sequence":e.sequence,"risk_score":risk,"status":status,"collected":collected,"reward_count":count,"journey_bonus":s["journey_bonus"]==true});
    s["sequence"] = json!(e.sequence);
    s["nonce_hash"] = json!(m::hash(&token));
    s["risk_score"] = json!(risk);
    s["samples"] = json!(samples);
    s["last_position"] = json!(pos);
    s["last_accuracy"] = json!(p.accuracy);
    s["last_timestamp"] = json!(timestamp);
    s["last_at"] = json!(now);
    s["progress"] = json!(progress);
    s["max_progress"] = json!(s["max_progress"].as_f64().unwrap_or(0.).max(progress));
    s["reward_count"] = json!(count);
    s["last_request"] = json!(e.request_id);
    s["last_fingerprint"] = json!(fingerprint);
    s["last_result"] = result.clone();
    let prior = m::get(r, "navigation_daily", &daykey)
        .and_then(|v| v["amount"].as_u64())
        .unwrap_or(0);
    let credit = daily.saturating_sub(prior);
    if credit > 0 {
        let balance = m::get(r, "navigation_wallet", user)
            .and_then(|v| v["balance"].as_u64())
            .unwrap_or(0);
        m::put(
            r,
            "navigation_wallet",
            user,
            json!({"balance":balance+credit,"transferable":false}),
        );
        let previous = m::get(r, "navigation_ledger", &e.route_session_id)
            .cloned()
            .unwrap_or(json!({}));
        m::put(
            r,
            "navigation_ledger",
            &e.route_session_id,
            json!({"owner":user,"amount":previous["amount"].as_u64().unwrap_or(0)+credit,"at":now,"sequence":e.sequence,"reward_types":{"route_coin":previous["reward_types"]["route_coin"].as_u64().unwrap_or(0)+credit-journey_credit,"journey_bonus":previous["reward_types"]["journey_bonus"].as_u64().unwrap_or(0)+journey_credit,"discovery_token":0}}),
        );
    }
    m::put(
        r,
        "navigation_daily",
        &daykey,
        json!({"amount":daily,"claims":claims,"at":now}),
    );
    m::put(r, "navigation_session", &e.route_session_id, s);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Records, SessionRequest, Config) {
        let mut r = Records::new();
        m::put(&mut r, "event", "venue", json!({"lat":35.002,"lon":139.}));
        let q = SessionRequest {
            destination: "venue".into(),
            mode: "WALKING".into(),
            path: vec![[35., 139.], [35.001, 139.], [35.002, 139.]],
        };
        (r, q, Config::default())
    }
    fn evidence(s: &Value, seq: u32, lat: f64, at: i64) -> Evidence {
        Evidence {
            route_session_id: s["route_session_id"].as_str().unwrap().into(),
            nonce: s["nonce"].as_str().unwrap().into(),
            sequence: seq,
            request_id: m::uid(),
            position: Position {
                lat,
                lon: 139.,
                accuracy: 3.,
                timestamp: at * 1000,
                speed: Some(1.),
                heading: None,
            },
            imu: None,
            device_integrity: None,
        }
    }
    #[test]
    fn unknown_safety_is_empty() {
        let (mut r, q, c) = fixture();
        let s = create(&mut r, "a", &q, 1000, &c).unwrap();
        assert!(s["coins"].as_array().unwrap().is_empty());
        assert_eq!(s["transferable"], false);
    }
    #[test]
    fn demo_and_unreviewed_transport_cannot_earn() {
        let (mut r, mut q, c) = fixture();
        m::put(
            &mut r,
            "navigation_corridor",
            "safe",
            json!({
                "reviewed_by":"operator","classification":"verified_sidewalk",
                "expires_at":10000,"hazards":[],
                "safety_zone":[[34.999,138.999],[35.003,138.999],[35.003,139.001],[34.999,139.001]],
                "points":[[35.00005,139.]]
            }),
        );
        q.mode = "BICYCLING".into();
        assert!(create(&mut r, "a", &q, 1000, &c).unwrap()["coins"]
            .as_array()
            .unwrap()
            .is_empty());
        q.mode = "WALKING".into();
        m::put(
            &mut r,
            "event",
            "venue",
            json!({"lat":35.002,"lon":139.,"demo":true}),
        );
        assert!(create(&mut r, "b", &q, 1000, &c).unwrap()["coins"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn owner_expiry_replay_and_teleport() {
        let (mut r, q, c) = fixture();
        let s = create(&mut r, "a", &q, 1000, &c).unwrap();
        let e = evidence(&s, 1, 35., 1001);
        assert!(submit(&mut r, "b", &e, 1001, &c).is_err());
        let v = submit(&mut r, "a", &e, 1001, &c).unwrap();
        assert_eq!(submit(&mut r, "a", &e, 1001, &c).unwrap(), v);
        let mut replay = e.clone();
        replay.request_id = m::uid();
        assert!(submit(&mut r, "a", &replay, 1002, &c).is_err());
        let t = evidence(&v, 2, 36., 1002);
        let v = submit(&mut r, "a", &t, 1002, &c).unwrap();
        assert!(v["risk_score"].as_u64().unwrap() >= 81);
        assert_eq!(v["reward_count"], 0);
        assert!(submit(&mut r, "a", &e, 9000, &c).is_err());
    }
    #[test]
    fn verified_corridor_credits_once() {
        let (mut r, q, c) = fixture();
        m::put(
            &mut r,
            "navigation_corridor",
            "safe",
            json!({"reviewed_by":"operator","classification":"verified_sidewalk","expires_at":10000,"hazards":[],"safety_zone":[[34.999,138.999],[35.001,138.999],[35.001,139.001],[34.999,139.001]],"points":[[35.00005,139.]]}),
        );
        let mut s = create(&mut r, "a", &q, 1000, &c).unwrap();
        assert_eq!(s["coins"].as_array().unwrap().len(), 1);
        for i in 1..=3 {
            let e = evidence(&s, i, 35.00005, 1000 + i as i64 * 5);
            s = submit(&mut r, "a", &e, 1000 + i as i64 * 5, &c).unwrap();
            assert_eq!(submit(&mut r, "a", &e, 1000 + i as i64 * 5, &c).unwrap(), s);
        }
        assert_eq!(s["reward_count"], 1);
        assert_eq!(m::get(&r, "navigation_wallet", "a").unwrap()["balance"], 1);
        let s2 = create(&mut r, "a", &q, 1100, &c).unwrap();
        let mut s = s2;
        for i in 1..=3 {
            let e = evidence(&s, i, 35.00005, 1100 + i as i64 * 5);
            s = submit(&mut r, "a", &e, 1100 + i as i64 * 5, &c).unwrap();
        }
        assert_eq!(s["reward_count"], 0);
    }
    #[test]
    fn hazardous_corridor_cannot_mint() {
        let (mut r, q, c) = fixture();
        m::put(
            &mut r,
            "navigation_corridor",
            "unsafe",
            json!({"reviewed_by":"operator","classification":"verified_sidewalk","expires_at":10000,"hazards":[{"point":{"lat":35.,"lon":139.}}],"points":[[35.,139.]]}),
        );
        assert!(create(&mut r, "a", &q, 1000, &c).unwrap()["coins"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn uncertainty_must_fit_pedestrian_zone() {
        let zone = [
            [34.999, 138.999],
            [35.001, 138.999],
            [35.001, 139.001],
            [34.999, 139.001],
        ];
        assert!(inside_zone([35., 139.], &zone, 5.));
        assert!(!inside_zone([35., 139.00099], &zone, 5.));
        assert!(!inside_zone([35., 139.002], &zone, 1.));
        assert!(!inside_zone([35., 139.], &[], 1.));
    }
    #[test]
    fn new_session_cannot_start_at_destination_for_bonus() {
        let (mut r, q, c) = fixture();
        let s = create(&mut r, "a", &q, 1000, &c).unwrap();
        let e = evidence(&s, 1, 35.002, 1005);
        let v = submit(&mut r, "a", &e, 1005, &c).unwrap();
        assert!(v["risk_score"].as_u64().unwrap() >= 51);
        assert_eq!(v["reward_count"], 0);
    }
    #[test]
    fn config_fails_closed_and_limits_sessions() {
        let (mut r, q, mut c) = fixture();
        c.max_sessions_per_day = 1;
        assert!(create(&mut r, "a", &q, 1000, &c).is_ok());
        assert!(create(&mut r, "a", &q, 1001, &c).is_err());
        m::put(
            &mut r,
            "navigation_config",
            "runtime",
            json!({"coin_radius":100.}),
        );
        assert!(Config::load(&r).is_err());
    }
    #[test]
    fn arrival_bonus_once_and_journal_is_aggregated() {
        let (mut r, q, c) = fixture();
        m::put(
            &mut r,
            "navigation_corridor",
            "safe",
            json!({"reviewed_by":"operator","classification":"verified_sidewalk","expires_at":10000,"hazards":[],"safety_zone":[[34.999,138.999],[35.003,138.999],[35.003,139.001],[34.999,139.001]],"points":[[35.00005,139.]]}),
        );
        let mut s = create(&mut r, "a", &q, 1000, &c).unwrap();
        let mut seq = 0;
        let mut at = 1000;
        for lat in [
            35., 35.00005, 35.00005, 35.0004, 35.0008, 35.0012, 35.0016, 35.002, 35.002, 35.002,
            35.002,
        ] {
            seq += 1;
            at += 15;
            let mut e = evidence(&s, seq, lat, at);
            e.position.speed = Some(0.);
            s = submit(&mut r, "a", &e, at, &c).unwrap();
        }
        assert_eq!(s["reward_count"], 6);
        assert_eq!(s["journey_bonus"], true);
        let rows = m::list(&r, "navigation_ledger");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["reward_types"]["route_coin"], 1);
        assert_eq!(rows[0]["reward_types"]["journey_bonus"], 5);
    }
}
