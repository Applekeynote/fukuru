//! Model suggestions never execute UI commands. A separate, expiring human confirmation is required.
use super::*;
use axum::Json;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
fn session_hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Proposal {
    token: String,
    action: String,
    #[serde(default)]
    entity: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Decision {
    token: String,
    id: String,
    confirm: bool,
}
fn prepare(
    c: &mut Connection,
    session: &str,
    action: &str,
    entity: &str,
    time: i64,
) -> Result<Value, String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    if !audit_valid(&tx) {
        return Err("監査を確認できないため操作を停止しています。".into());
    }
    if ![
        "open_map",
        "open_saved",
        "open_event",
        "save_event",
        "join_event",
    ]
    .contains(&action)
    {
        return Err("この画面操作は許可されていません。".into());
    }
    let (name, version) = if ["open_map", "open_saved"].contains(&action) {
        (String::new(), 0)
    } else {
        tx.query_row("SELECT name,version FROM events WHERE id=?1 AND tenant='studio' AND owner='creator-local'",[entity],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(|_|"対象を確認できません。")?
    };
    let label = match action {
        "open_map" => "地図を開く".to_owned(),
        "open_saved" => "保存した体験を開く".to_owned(),
        "open_event" => format!("「{name}」の詳細を開く"),
        "save_event" => format!("「{name}」を保存する"),
        _ => format!("「{name}」を参加予定にする（予約・決済は行いません）"),
    };
    let id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO ui_proposals VALUES(?1,?2,?3,?4,?5,?6,'PENDING')",
        params![
            id,
            session_hash(session),
            action,
            entity,
            version,
            time + 300
        ],
    )
    .map_err(|e| e.to_string())?;
    audit(&tx, "ui.proposed:R0", &id).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(json!({"id":id,"label":label,"expires_in":300,"confirmation_required":true}))
}
fn execute(
    c: &mut Connection,
    session: &str,
    id: &str,
    confirm: bool,
    time: i64,
) -> Result<Value, String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    if !audit_valid(&tx) {
        return Err("監査を確認できないため操作を停止しています。".into());
    }
    let (action,entity,version,expiry)=tx.query_row("SELECT action,entity,entity_version,expires_at FROM ui_proposals WHERE id=?1 AND session_hash=?2 AND status='PENDING'",params![id,session_hash(session)],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).map_err(|_|"この提案は無効、または確認済みです。")?;
    if expiry <= time {
        return Err("提案の有効期限が切れました。もう一度提案してください。".into());
    }
    let mut url = String::new();
    if confirm {
        if version > 0 {
            let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE id=?1 AND version=?2 AND tenant='studio' AND owner='creator-local')",params![entity,version],|r|r.get(0)).map_err(|e|e.to_string())?;
            if !valid {
                return Err("対象が変更されました。新しい内容を確認してください。".into());
            }
        }
        match action.as_str() {
            "open_map" => url = "/?tab=map".into(),
            "open_saved" => url = "/?tab=saved".into(),
            "open_event" => {
                let mut query = reqwest::Url::parse("http://localhost/").unwrap();
                query.query_pairs_mut().append_pair("selected", &entity);
                url = format!("/?{}", query.query().unwrap());
            }
            "save_event" | "join_event" => {
                let kind = if action == "save_event" {
                    "save"
                } else {
                    "join"
                };
                tx.execute("INSERT INTO preferences(entity,kind,value) VALUES(?1,?2,'1') ON CONFLICT(entity,kind) DO UPDATE SET value='1'",params![entity,kind]).map_err(|e|e.to_string())?;
            }
            _ => return Err("許可されていない操作です。".into()),
        }
    }
    tx.execute(
        "UPDATE ui_proposals SET status=?1 WHERE id=?2",
        params![if confirm { "CONFIRMED" } else { "CANCELLED" }, id],
    )
    .map_err(|e| e.to_string())?;
    audit(
        &tx,
        if confirm {
            "ui.human-confirmed:R2"
        } else {
            "ui.cancelled:R0"
        },
        id,
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(
        json!({"confirmed":confirm,"url":url,"message":if confirm{"確認された操作を実行しました。"}else{"操作を取り消しました。"}}),
    )
}
pub(super) async fn propose(
    State(a): State<App>,
    headers: HeaderMap,
    Json(f): Json<Proposal>,
) -> Response {
    if !assist::authenticated(&a, &headers, Some(&f.token)) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match prepare(
        &mut a.db.lock().unwrap(),
        &a.token,
        &f.action,
        &f.entity,
        now(),
    ) {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error":e}))).into_response(),
    }
}
pub(super) async fn decide(
    State(a): State<App>,
    headers: HeaderMap,
    Json(f): Json<Decision>,
) -> Response {
    if !assist::authenticated(&a, &headers, Some(&f.token)) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match execute(&mut a.db.lock().unwrap(), &a.token, &f.id, f.confirm, now()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error":e}))).into_response(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proposal_cannot_mutate_and_confirmation_is_single_use() {
        let mut c = Connection::open_in_memory().unwrap();
        init(&c);
        let e = events(&c)[0].clone();
        let p = prepare(&mut c, "a", "save_event", &e.id, 100).unwrap();
        let id = p["id"].as_str().unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM preferences", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(execute(&mut c, "b", id, true, 101).is_err());
        execute(&mut c, "a", id, true, 102).unwrap();
        assert!(execute(&mut c, "a", id, true, 103).is_err());
        assert_eq!(
            c.query_row("SELECT value FROM preferences", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "1"
        );
    }
    #[test]
    fn expired_changed_and_unlisted_actions_fail_closed() {
        let mut c = Connection::open_in_memory().unwrap();
        init(&c);
        let e = events(&c)[0].clone();
        assert!(prepare(&mut c, "a", "delete_all", &e.id, 100).is_err());
        let p = prepare(&mut c, "a", "join_event", &e.id, 100).unwrap();
        let id = p["id"].as_str().unwrap();
        assert!(execute(&mut c, "a", id, true, 400).is_err());
        c.execute("UPDATE events SET version=version+1 WHERE id=?1", [&e.id])
            .unwrap();
        assert!(execute(&mut c, "a", id, true, 101).is_err());
        execute(&mut c, "a", id, false, 102).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM preferences", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
