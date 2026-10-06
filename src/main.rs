mod assist;
mod experience;
mod screen_actions;
mod ui;
use axum::{
    extract::{Form, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rusqlite::{params, Connection};
use serde::Deserialize;
use spatial_atlas::audit::{append as audit, valid as audit_valid};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;
type Db = Arc<Mutex<Connection>>;
#[derive(Clone)]
struct App {
    db: Db,
    token: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Event {
    pub id: String,
    pub name: String,
    pub place: String,
    pub kind: String,
    pub time: String,
    pub lat: f64,
    pub lon: f64,
    pub version: i64,
    pub status: String,
}
fn id() -> String {
    format!("spid_{}", Uuid::new_v4())
}
fn init(c: &Connection) {
    c.execute_batch(include_str!("../assets/schema.sql"))
        .unwrap();
    c.execute_batch(spatial_atlas::delivery::SCHEMA).unwrap();
    let count: i64 = c
        .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
        .unwrap();
    if count == 0 {
        for (name, place, kind, time, lat, lon) in [
            (
                "光の余白 / Light Field",
                "秋葉原・万世橋",
                "ART",
                "2026-09-09T19:00",
                35.6972,
                139.771,
            ),
            (
                "電子音のある風景",
                "神田・川沿い",
                "MUSIC",
                "2026-09-10T20:00",
                35.695,
                139.769,
            ),
            (
                "記憶を重ねる街歩き",
                "秋葉原駅前",
                "WALK",
                "2026-09-12T16:00",
                35.6984,
                139.7731,
            ),
            (
                "透明な建築",
                "御茶ノ水",
                "AR",
                "2026-09-13T18:00",
                35.6997,
                139.765,
            ),
            (
                "街のかけら、収集室",
                "神田・アトリエ",
                "ART",
                "2026-09-14T13:00",
                35.6915,
                139.772,
            ),
            (
                "夜の音を編む",
                "浅草橋",
                "MUSIC",
                "2026-09-15T19:30",
                35.6974,
                139.785,
            ),
        ] {
            let eid = id();
            c.execute("INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,?7,1,'ACTIVE','studio','creator-local')",params![eid,name,place,kind,time,lat,lon]).unwrap();
            c.execute(
                "INSERT INTO edges VALUES('creator-local','CREATED',?1)",
                [&eid],
            )
            .unwrap();
        }
    }
}
fn events(c: &Connection) -> Vec<Event> {
    let mut s=c.prepare("SELECT id,name,place,kind,time,lat,lon,version,status FROM events WHERE tenant='studio' ORDER BY time").unwrap();
    s.query_map([], |r| {
        Ok(Event {
            id: r.get(0)?,
            name: r.get(1)?,
            place: r.get(2)?,
            kind: r.get(3)?,
            time: r.get(4)?,
            lat: r.get(5)?,
            lon: r.get(6)?,
            version: r.get(7)?,
            status: r.get(8)?,
        })
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}
fn distance(lat: f64, lon: f64) -> f64 {
    let a = (lat - 35.6984).to_radians();
    let b = (lon - 139.7731).to_radians();
    6371.0
        * 2.0
        * ((a / 2.0).sin().powi(2)
            + lat.to_radians().cos() * 35.6984f64.to_radians().cos() * (b / 2.0).sin().powi(2))
        .sqrt()
        .asin()
}
#[derive(Deserialize, Default)]
struct View {
    #[serde(default)]
    tab: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    radius: String,
    #[serde(default)]
    selected: String,
    #[serde(default)]
    notice: String,
}
async fn page(State(a): State<App>, Query(v): Query<View>) -> Response {
    let c = a.db.lock().unwrap();
    let html = experience::render(&c, &a.token, &v);
    let mut res = Html(html).into_response();
    res.headers_mut().insert(
        "set-cookie",
        format!(
            "atlas_session={}; HttpOnly; SameSite=Strict; Path=/",
            a.token
        )
        .parse()
        .unwrap(),
    );
    res.headers_mut().insert("content-security-policy","default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data: https://tile.openstreetmap.org; connect-src 'self'; media-src 'self' blob:; form-action 'self'; base-uri 'none'; frame-ancestors 'none'".parse().unwrap());
    res.headers_mut().insert(
        "permissions-policy",
        "geolocation=(self), microphone=(self), camera=()"
            .parse()
            .unwrap(),
    );
    res.headers_mut().insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    res.headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    res
}
#[derive(Deserialize)]
struct Action {
    token: String,
    key: String,
    op: String,
    #[serde(flatten)]
    fields: HashMap<String, String>,
}
fn apply(c: &mut Connection, f: &Action) -> Result<(), String> {
    if !audit_valid(c) {
        return Err("監査チェーンの不整合を検出しました。書込みを停止します".into());
    }
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM requests WHERE key=?1)",
            [&f.key],
            |r| r.get(0),
        )
        .unwrap();
    if exists {
        return Ok(());
    }
    let field = |k: &str| f.fields.get(k).map(String::as_str).unwrap_or("");
    match f.op.as_str() {
        "save" | "join" => {
            let valid: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1 AND tenant='studio')",
                    [field("id")],
                    |r| r.get(0),
                )
                .unwrap();
            if !valid || !["0", "1"].contains(&field("value")) {
                return Err("対象または状態が不正です".into());
            }
            tx.execute("INSERT INTO preferences(entity,kind,value) VALUES(?1,?2,?3) ON CONFLICT(entity,kind) DO UPDATE SET value=excluded.value", params![field("id"),f.op,field("value")]).map_err(|e|e.to_string())?;
            audit(&tx, &format!("{}.set:R2", f.op), field("id")).unwrap();
        }
        "follow" => {
            if field("id") != "creator-local" || !["0", "1"].contains(&field("value")) {
                return Err("対象または状態が不正です".into());
            }
            tx.execute("INSERT INTO preferences(entity,kind,value) VALUES(?1,'follow',?2) ON CONFLICT(entity,kind) DO UPDATE SET value=excluded.value", params![field("id"),field("value")]).map_err(|e|e.to_string())?;
            audit(&tx, "follow.set:R2", field("id")).unwrap();
        }
        "create" => {
            let name = field("name").trim();
            let place = field("place").trim();
            let lat: f64 = field("lat").parse().map_err(|_| "緯度が不正です")?;
            let lon: f64 = field("lon").parse().map_err(|_| "経度が不正です")?;
            if name.is_empty()
                || name.chars().count() > 100
                || place.is_empty()
                || place.chars().count() > 100
                || !lat.is_finite()
                || !lon.is_finite()
                || lat.abs() > 90.0
                || lon.abs() > 180.0
            {
                return Err("名前・場所・座標を確認してください".into());
            }
            if !["ART", "MUSIC", "WALK", "AR"].contains(&field("kind")) {
                return Err("分類が不正です".into());
            }
            if !valid_time(field("time")) {
                return Err("日時が不正です".into());
            }
            let eid = id();
            tx.execute("INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,?7,1,'PENDING_GEO','studio','creator-local')",params![eid,name,place,field("kind"),field("time"),lat,lon]).map_err(|e|e.to_string())?;
            tx.execute(
                "INSERT INTO edges VALUES('creator-local','CREATED',?1)",
                [&eid],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO outbox(entity,status) VALUES(?1,'PENDING')",
                [&eid],
            )
            .unwrap();
            audit(&tx, "event.create:R2", &eid).unwrap();
            spatial_atlas::delivery::enqueue(&tx, &eid).map_err(|e| e.to_string())?;
        }
        "sync" => {
            tx.execute("UPDATE events SET status='ACTIVE' WHERE id IN(SELECT entity FROM outbox WHERE status='PENDING')",[]).unwrap();
            tx.execute(
                "UPDATE outbox SET status='DELIVERED' WHERE status='PENDING'",
                [],
            )
            .unwrap();
            audit(&tx, "geo.sync:local-adapter", "outbox").unwrap();
        }
        "propose" => {
            if !valid_time(field("time")) {
                return Err("日時が不正です".into());
            }
            let version: i64 = field("version").parse().map_err(|_| "版が不正です")?;
            let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE id=?1 AND tenant='studio' AND owner='creator-local' AND version=?2)",params![field("id"),version],|r|r.get(0)).unwrap();
            if !owned {
                return Err("所有権または版が一致しません".into());
            }
            let aid = id();
            tx.execute(
                "INSERT INTO approvals VALUES(?1,?2,?3,?4,'PENDING')",
                params![aid, field("id"), field("time"), version],
            )
            .unwrap();
            audit(&tx, "event.update.proposed:R3", &aid).unwrap();
        }
        "approve" | "reject" => {
            let row=tx.query_row("SELECT entity,new_time,version FROM approvals WHERE id=?1 AND status='PENDING'",[field("id")],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).map_err(|_|"承認待ちの提案がありません")?;
            if f.op == "approve" {
                tx.execute("INSERT INTO history(entity,old_time,old_version) SELECT id,time,version FROM events WHERE id=?1 AND tenant='studio' AND owner='creator-local'", [&row.0]).unwrap();
                let changed=tx.execute("UPDATE events SET time=?1,version=version+1 WHERE id=?2 AND version=?3 AND tenant='studio' AND owner='creator-local'",params![row.1,row.0,row.2]).unwrap();
                if changed != 1 {
                    return Err("他の変更と競合しています。提案を作り直してください".into());
                }
                spatial_atlas::delivery::enqueue(&tx, &row.0).map_err(|e| e.to_string())?;
            }
            tx.execute(
                "UPDATE approvals SET status=?1 WHERE id=?2",
                params![
                    if f.op == "approve" {
                        "APPROVED"
                    } else {
                        "REJECTED"
                    },
                    field("id")
                ],
            )
            .unwrap();
            audit(&tx, &format!("event.update.{}:R3", f.op), field("id")).unwrap();
        }
        "retry_delivery" => {
            spatial_atlas::delivery::retry_dead(&tx, field("id")).map_err(|e| e.to_string())?;
            audit(&tx, "delivery.retry:R2", field("id")).unwrap();
        }
        _ => return Err("許可されていないToolです。R4操作は提供していません".into()),
    }
    tx.execute("INSERT INTO requests VALUES(?1)", [&f.key])
        .unwrap();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}
fn valid_time(s: &str) -> bool {
    if s.len() != 16 {
        return false;
    }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' {
        return false;
    }
    let nums: Vec<u32> = s
        .split(['-', 'T', ':'])
        .map(|x| x.parse().unwrap_or(u32::MAX))
        .collect();
    if nums.len() != 5 {
        return false;
    }
    let (y, m, d, h, n) = (nums[0], nums[1], nums[2], nums[3], nums[4]);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    (2020..=2100).contains(&y) && d >= 1 && d <= days && h < 24 && n < 60
}
async fn action(State(a): State<App>, headers: HeaderMap, Form(f): Form<Action>) -> Response {
    let cookie = headers
        .get("cookie")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("");
    if f.token != a.token
        || !cookie
            .split(';')
            .any(|x| x.trim() == format!("atlas_session={}", a.token))
    {
        return (StatusCode::FORBIDDEN, "認証が必要です").into_response();
    }
    if f.key.len() > 80 || f.key.is_empty() {
        return (StatusCode::BAD_REQUEST, "冪等キーが必要です").into_response();
    }
    match apply(&mut a.db.lock().unwrap(), &f) {
        Ok(()) => axum::response::Redirect::to(
            if ["approve", "reject", "propose"].contains(&f.op.as_str()) {
                "/?tab=agent&notice=done"
            } else if f.op == "follow" {
                "/?tab=profile&notice=done"
            } else if ["save", "join"].contains(&f.op.as_str()) {
                "/?tab=saved&notice=done"
            } else {
                "/?notice=done"
            },
        )
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Html(format!(
                "<meta charset='utf-8'><p>{}</p><a href='/'>戻る</a>",
                ui::esc(&e)
            )),
        )
            .into_response(),
    }
}
#[tokio::main]
async fn main() {
    if std::env::var("ATLAS_MODE").unwrap_or_else(|_| "local".into()) != "local" {
        panic!("Cloud authentication and storage adapters must be configured before production mode is enabled")
    };
    let c =
        Connection::open(std::env::var("ATLAS_DB").unwrap_or_else(|_| "atlas.db".into())).unwrap();
    init(&c);
    c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    let a = App {
        db: Arc::new(Mutex::new(c)),
        token: Uuid::new_v4().to_string(),
    };
    let app = Router::new()
        .route("/", get(page))
        .route("/action", post(action))
        .route("/api/assist", post(assist::respond))
        .route("/api/context", get(assist::context))
        .route("/api/ui/propose", post(screen_actions::propose))
        .route("/api/ui/decide", post(screen_actions::decide))
        .route(
            "/location.js",
            get(|| async {
                (
                    [("content-type", "text/javascript")],
                    include_str!("../assets/location.js"),
                )
            }),
        )
        .route(
            "/presence.js",
            get(|| async {
                (
                    [("content-type", "text/javascript")],
                    include_str!("../assets/presence.js"),
                )
            }),
        )
        .route(
            "/device.js",
            get(|| async {
                (
                    [("content-type", "text/javascript")],
                    include_str!("../assets/device.js"),
                )
            }),
        )
        .route("/calendar", get(experience::calendar))
        .route(
            "/hero.png",
            get(|| async {
                (
                    [
                        ("content-type", "image/png"),
                        ("cache-control", "public, max-age=86400"),
                    ],
                    include_bytes!("../assets/hero.png").as_slice(),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [("content-type", "text/css")],
                    include_str!("../assets/style.css"),
                )
            }),
        )
        .route("/healthz", get(|| async { "ok" }))
        .with_state(a);
    let port: u16 = std::env::var("ATLAS_PORT")
        .unwrap_or_else(|_| "8787".into())
        .parse()
        .expect("ATLAS_PORT must be a port number");
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    println!("Spatial Atlas ready: http://127.0.0.1:{port} (private validation)");
    axum::serve(listener, app).await.unwrap()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init(&c);
        c
    }
    fn form(op: &str, fields: &[(&str, &str)]) -> Action {
        Action {
            token: "test".into(),
            key: id(),
            op: op.into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
    #[test]
    fn idempotent_outbox() {
        let mut c = db();
        let f = form(
            "create",
            &[
                ("name", "test"),
                ("place", "Tokyo"),
                ("lat", "35"),
                ("lon", "139"),
                ("kind", "ART"),
                ("time", "2026-10-01T20:00"),
            ],
        );
        apply(&mut c, &f).unwrap();
        apply(&mut c, &f).unwrap();
        assert_eq!(events(&c).len(), 7);
        assert_eq!(
            c.query_row("SELECT count(*) FROM managed_deliveries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(events(&c).iter().any(|e| e.status == "PENDING_GEO"));
        apply(&mut c, &form("sync", &[])).unwrap();
        assert!(events(&c).iter().all(|e| e.status == "ACTIVE"));
        assert!(audit_valid(&c));
    }
    #[test]
    fn approval_captures_immutable_cloud_snapshot() {
        let mut c = db();
        let e = events(&c)[0].clone();
        apply(
            &mut c,
            &form(
                "propose",
                &[
                    ("id", &e.id),
                    ("version", "1"),
                    ("time", "2026-10-01T20:00"),
                ],
            ),
        )
        .unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM managed_deliveries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let aid: String = c
            .query_row("SELECT id FROM approvals", [], |r| r.get(0))
            .unwrap();
        apply(&mut c, &form("approve", &[("id", &aid)])).unwrap();
        c.execute(
            "UPDATE events SET time='2026-11-01T12:00' WHERE id=?1",
            [e.id],
        )
        .unwrap();
        let payload: String = c
            .query_row("SELECT payload FROM managed_deliveries", [], |r| r.get(0))
            .unwrap();
        let snapshot: spatial_atlas::delivery::Snapshot = serde_json::from_str(&payload).unwrap();
        assert_eq!(snapshot.time, "2026-10-01T20:00");
        assert_eq!(snapshot.version, 2);
    }
    #[test]
    fn policy_and_conflict() {
        let mut c = db();
        assert!(apply(&mut c, &form("sql.execute", &[])).is_err());
        let e = events(&c)[0].clone();
        apply(
            &mut c,
            &form(
                "propose",
                &[
                    ("id", &e.id),
                    ("version", "1"),
                    ("time", "2026-10-01T20:00"),
                ],
            ),
        )
        .unwrap();
        assert_eq!(events(&c)[0].version, 1);
        let aid: String = c
            .query_row("SELECT id FROM approvals", [], |r| r.get(0))
            .unwrap();
        c.execute("UPDATE events SET version=2 WHERE id=?1", [e.id])
            .unwrap();
        assert!(apply(&mut c, &form("approve", &[("id", &aid)])).is_err());
        assert!(apply(
            &mut c,
            &form(
                "propose",
                &[
                    ("id", "other-tenant"),
                    ("version", "1"),
                    ("time", "2026-10-01T20:00")
                ]
            )
        )
        .is_err());
    }
    #[test]
    fn integrity() {
        let c = db();
        audit(&c, "read", "x").unwrap();
        assert!(audit_valid(&c));
        c.execute("UPDATE audit SET action='tampered'", []).unwrap();
        assert!(!audit_valid(&c));
        assert!(!valid_time("2026-02-30T12:00"));
        assert!(distance(35.6984, 139.7731) < 0.001);
    }
    #[test]
    fn saved_follow_and_attendance_survive_initialization() {
        let mut c = db();
        let eid = events(&c)[0].id.clone();
        for op in ["save", "join"] {
            let f = form(op, &[("id", &eid), ("value", "1")]);
            apply(&mut c, &f).unwrap();
            apply(&mut c, &f).unwrap();
        }
        apply(
            &mut c,
            &form("follow", &[("id", "creator-local"), ("value", "1")]),
        )
        .unwrap();
        init(&c);
        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM preferences WHERE value='1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
        apply(&mut c, &form("save", &[("id", &eid), ("value", "0")])).unwrap();
        assert!(apply(
            &mut c,
            &form("save", &[("id", "not-owned"), ("value", "1")])
        )
        .is_err());
        assert!(apply(
            &mut c,
            &form("follow", &[("id", "unknown"), ("value", "1")])
        )
        .is_err());
        assert!(audit_valid(&c));
        let html = experience::render(
            &c,
            "test",
            &View {
                tab: "saved".into(),
                ..Default::default()
            },
        );
        assert!(html.contains("参加予定"));
        assert!(html.contains(&eid));
    }
}
