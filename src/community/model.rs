use super::store::{Records, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
pub fn now() -> i64 {
    Utc::now().timestamp()
}
fn now_ms() -> i64 { Utc::now().timestamp_millis() }
pub fn uid() -> String {
    Uuid::new_v4().to_string()
}
pub fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
pub fn get<'a>(r: &'a Records, s: &str, k: &str) -> Option<&'a Value> {
    r.get(&(s.into(), k.into()))
}
pub fn put(r: &mut Records, s: &str, k: &str, v: Value) {
    r.insert((s.into(), k.into()), v);
}
pub fn list<'a>(r: &'a Records, s: &str) -> Vec<&'a Value> {
    r.iter()
        .filter(|((scope, _), _)| scope == s)
        .map(|(_, v)| v)
        .collect()
}
pub fn maintain(r:&mut Records){
    let at=now();if get(r,"maintenance","retention").and_then(|v|v["at"].as_i64()).map(|n|at-n<86400).unwrap_or(false){return;}
    r.retain(|(scope,_),v|{
        if scope=="navigation_session"{return v["expires_at"].as_i64().unwrap_or(0)>at;}
        if ["navigation_limit","navigation_daily","navigation_cooldown"].contains(&scope.as_str()){return v["at"].as_i64().is_some_and(|time|at-time<7*86400);}
        if scope=="session"||scope=="briefing_cache"{return v["expires"].as_i64().unwrap_or(at+1)>at;}
        if scope.starts_with("alert:")&&v["read_at"].as_i64().unwrap_or(0)==0{return true;}
        let days=if scope=="social_audit"||scope=="delivery"||scope=="push_queue"||scope.starts_with("alert:"){90}else if scope=="request"{7}else{return true;};
        v["at"].as_i64().map(|time|at-time<days*86400).unwrap_or(true)
    });
    put(r,"maintenance","retention",json!({"at":at}));
}
fn txt<'a>(v: &'a Value, k: &str, min: usize, max: usize) -> Result<&'a str> {
    let s = v[k]
        .as_str()
        .ok_or_else(|| format!("{k} を入力してください"))?
        .trim();
    if s.chars().count() < min || s.chars().count() > max || s.contains('\0') {
        Err(format!("{k} の長さが不正です"))
    } else {
        Ok(s)
    }
}
pub fn account(r: &Records, session: &str) -> Option<String> {
    let s = get(r, "session", &hash(session))?;
    if s["expires"].as_i64()? <= now() {
        return None;
    }
    let id = s["account"].as_str()?;
    let a = get(r, "account", id)?;
    if a["disabled"] == true {
        return None;
    }
    Some(id.into())
}
pub fn public_account(r: &Records, id: &str) -> Value {
    let Some(a) = get(r, "account", id) else {
        return Value::Null;
    };
    json!({"id":id,"handle":a["handle"],"name":a["name"],"bio":a["bio"],"theme":a["theme"],"layout":a["layout"],"avatar":a["avatar"],"banner":a["banner"],"banner_alt":a["banner_alt"],"ui_locale":a["ui_locale"],"interests":a["interests"],"sample":a["sample"],"demo":is_demo_account(a),"regional_profile":a["seed_batch"]==super::demo_import::BATCH,"region":a["region"],"host_verified":a["host_verified"]==true,"followers":list(r,&format!("follow:{id}")).len(),"following":r.iter().filter(|((s,k),_)|s.starts_with("follow:")&&k==id).count()})
}
pub fn export_personal_data(r: &Records, who: &str) -> Result<Value> {
    if get(r,"account",who).is_none() {return Err("アカウントが見つかりません".into());}
    let mut records=Vec::new();
    for ((scope,id),value) in r {
        let mine=match scope.as_str() {
            "event"|"media"|"report"|"verification"|"social_audit"=>value["owner"]==who||value["author"]==who||value["reporter"]==who||value["requester"]==who||value["actor"]==who,
            "account_delete"|"preferences"|"navigation_wallet"=>id==who,
            "navigation_session"|"navigation_ledger"=>value["owner"]==who,
            "navigation_limit"|"navigation_daily"|"navigation_cooldown"=>id.starts_with(&format!("{who}:")),
            _ if scope==&format!("note:{who}")||scope==&format!("search:{who}")=>true,
            _ if scope.starts_with("collab:")=>value["author"]==who,
            _ if scope==&format!("draft:{who}")||scope==&format!("alert:{who}")||scope==&format!("feed_dismiss:{who}")||scope==&format!("send_log:{who}")=>true,
            _ if scope.starts_with("rsvp:")||scope.starts_with("booking:")=>id==who,
            _ if scope.starts_with("post:")||scope.starts_with("message:")||scope.starts_with("notice:")=>value["author"]==who,
            _ if scope.starts_with("follow:")||scope.starts_with("block:")=>id==who||scope.ends_with(&format!(":{who}")),
            _ if scope.starts_with("room_read:")||scope.starts_with("talk_hide:")||scope.starts_with("inbox_room:")||scope.starts_with("inbox_alert:")=>scope.ends_with(&format!(":{who}")),
            _=>false,
        };
        if mine {
            let mut exported=value.clone();
            if scope=="media" {if let Some(obj)=exported.as_object_mut(){obj.remove("data");}}
            if scope=="navigation_session"{if let Some(obj)=exported.as_object_mut(){for key in ["nonce_hash","last_result","last_fingerprint"]{obj.remove(key);}}}
            records.push(json!({"scope":scope,"id":id,"data":exported}));
        }
    }
    records.sort_by(|a,b|a["scope"].as_str().cmp(&b["scope"].as_str()).then(a["id"].as_str().cmp(&b["id"].as_str())));
    Ok(json!({"format":"spatial-personal-export-v1","generated_at":now(),"account":public_account(r,who),"records":records,"media_note":"写真のメタデータと参照先を含みます。写真本体は別途、本人の閲覧権限がある間に開いて保存してください。","excluded":"パスワードハッシュ、セッション、CSRF、他者の投稿・会話、バックアップ、運用ログ"}))
}
pub fn is_demo_account(a: &Value) -> bool {
    a["sample"] == true
        || a["handle"].as_str().unwrap_or("").starts_with("spatial_test_")
        || a["name"].as_str().unwrap_or("").starts_with("検証専用")
}
pub fn is_demo_event(r: &Records, e: &Value) -> bool {
    e["sample"] == true
        || e["name"].as_str().unwrap_or("").starts_with("【検証")
        || get(r, "account", e["owner"].as_str().unwrap_or(""))
            .map(is_demo_account).unwrap_or(false)
}
pub fn register(
    r: &mut Records,
    input: &Value,
    password_hash: &str,
    session: &str,
) -> Result<String> {
    let handle = txt(input, "handle", 3, 30)?.to_ascii_lowercase();
    if !handle
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err("IDは英小文字・数字・_の3〜30文字です".into());
    }
    if get(r, "handle", &handle).is_some() {
        return Err("このIDは使用されています".into());
    }
    if list(r, "account").len() >= 1000 {
        return Err("新規登録の受付上限です".into());
    }
    let name = txt(input, "name", 1, 60)?;
    let id = uid();
    put(r, "handle", &handle, json!(id));
    put(
        r,
        "account",
        &id,
        json!({"id":id,"handle":handle,"name":name,"bio":"","theme":"sky","layout":"cards","avatar":"","interests":[],"password_hash":password_hash,"created":now(),"disabled":false,"sample":false,"notifications":{"enabled":true,"minutes":30,"messages":true}}),
    );
    new_session(r, &id, session);
    audit(r, &id, "account.register", &id);
    Ok(id)
}
pub fn new_session(r: &mut Records, id: &str, session: &str) {
    r.retain(|(s, _), v| s != "session" || v["expires"].as_i64().unwrap_or(0) > now());
    put(
        r,
        "session",
        &hash(session),
        json!({"account":id,"expires":now()+604800,"created":now()}),
    );
}
pub(crate) fn audit(r: &mut Records, actor: &str, op: &str, target: &str) {
    let at = now();
    put(
        r,
        "social_audit",
        &uid(),
        json!({"actor":actor,"op":op,"target":target,"at":at}),
    );
}
pub(crate) fn push_alert(r: &mut Records, person: &str, value: Value) {
    let scope=format!("alert:{person}");
    let mut old:Vec<_>=r.iter().filter(|((s,_),v)|s==&scope&&v["read_at"].as_i64().unwrap_or(0)>0).map(|((_,k),v)|(k.clone(),v["at"].as_i64().unwrap_or(0))).collect();
    if old.len()>=100 {old.sort_by_key(|x|x.1);let count=old.len().saturating_sub(99);for (id,_) in old.into_iter().take(count){r.remove(&(scope.clone(),id));}}
    let id=uid();
    let mut alert=value;
    alert["id"]=json!(id);
    alert["at"]=json!(now());
    alert["read_at"]=Value::Null;
    super::push::enqueue(r,person,&alert);
    let delivery_id=uid();
    put(r,"delivery",&delivery_id,json!({"id":delivery_id,"event":alert["event"],"recipient":person,"alert":id,"channel":"in_app","status":"delivered","at":now()}));
    put(r,&scope,&id,alert);
}
pub fn event_public(r: &Records, e: &Value, who: Option<&str>) -> Value {
    let id = e["id"].as_str().unwrap_or("");
    let rows = list(r, &format!("rsvp:{id}"));
    let mut out = e.clone();
    out["extra"]=super::completion::extra_public(r,e,who);
    out["host"] = public_account(r, e["owner"].as_str().unwrap_or(""));
    out["demo"] = json!(is_demo_event(r, e));
    out["interested"] = json!(rows.iter().filter(|v| v["status"] == "interested").count());
    out["going"] = json!(rows.iter().filter(|v| v["status"] == "going").count());
    let bookings=list(r,&format!("booking:{id}"));
    out["booked"] = json!(bookings.iter().filter(|v|v["status"]=="confirmed").count());
    out["waitlisted"] = json!(bookings.iter().filter(|v|v["status"]=="waitlisted").count());
    out["my_booking"] = who.and_then(|u|get(r,&format!("booking:{id}"),u)).map(|v|v["status"].clone()).unwrap_or(json!("none"));
    out["my_status"] = who
        .and_then(|u| get(r, &format!("rsvp:{id}"), u))
        .map(|v| v["status"].clone())
        .unwrap_or(json!("none"));
    out["fit"] = fit(r, e, who);
    let mut notices: Vec<_> = list(r, &format!("notice:{id}"))
        .into_iter()
        .cloned()
        .collect();
    notices.sort_by_key(|n| std::cmp::Reverse(n["at"].as_i64().unwrap_or(0)));
    out["notices"] = json!(notices);
    let mut changes: Vec<_> = list(r,&format!("event_change:{id}")).into_iter().cloned().collect();
    changes.sort_by_key(|v|std::cmp::Reverse(v["at"].as_i64().unwrap_or(0)));
    out["changes"] = json!(changes.into_iter().take(10).collect::<Vec<_>>());
    out
}
fn fit(r: &Records, e: &Value, who: Option<&str>) -> Value {
    let interests = who
        .and_then(|id| get(r, "account", id))
        .and_then(|v| v["interests"].as_array());
    let matched = interests.map(|v| v.contains(&e["kind"])).unwrap_or(false);
    let follow = who
        .map(|u| {
            get(
                r,
                &format!("follow:{}", e["owner"].as_str().unwrap_or("")),
                u,
            )
            .is_some()
        })
        .unwrap_or(false);
    let reason = match (matched,follow) {
        (true,true) => "選択した興味とLinked主催者が一致",
        (true,false) => "選択した興味に一致",
        (false,true) => "Linked主催者のイベント",
        (false,false) => "興味を設定すると理由を表示します",
    };
    json!({"score":if matched||follow {Some(50+if matched{35}else{0}+if follow{15}else{0})} else {None},"reason":reason,"source":"好みとの一致度・ルール計算"})
}
pub fn bootstrap(r: &Records, who: Option<&str>) -> Value {
    let me = who
        .map(|id| {
            let mut a = public_account(r, id);
            a["notifications"] = get(r, "account", id).unwrap()["notifications"].clone();
            a["google_only"]=json!(get(r,"account",id).unwrap()["password_hash"]=="" && get(r,"account",id).unwrap()["google_linked"]==true);
            a["google_linked"]=get(r,"account",id).unwrap()["google_linked"].clone();
            a["dm_policy"] = get(r, "account", id).unwrap()["dm_policy"].as_str().unwrap_or("everyone").into();
            a["deletion_requested_at"] = get(r, "account_delete", id).map(|v|v["requested_at"].clone()).unwrap_or(Value::Null);
            a
        })
        .unwrap_or(Value::Null);
    let mut ev: Vec<Value> = list(r, "event")
        .into_iter()
        .filter(|e| e["deleted"] != true)
        .filter(|e| super::completion::listed(r,e,who))
        .filter(|e| who.map(|u| !blocked(r,u,e["owner"].as_str().unwrap_or(""))).unwrap_or(true))
        .map(|e| event_public(r, e, who))
        .collect();
    ev.sort_by(|a, b| a["start"].as_str().cmp(&b["start"].as_str()));
    let rooms: Vec<_> = list(r, "room")
        .into_iter()
        .filter(|room| who.map(|u| room_allowed(r, room, u)).unwrap_or(false))
        .map(|room| room_view(r, room, who.unwrap_or("")))
        .collect();
    let follows: Vec<_> = r
        .keys()
        .filter(|(s, k)| s.starts_with("follow:") && Some(k.as_str()) == who)
        .map(|(s, _)| s.trim_start_matches("follow:"))
        .collect();
    let mut alerts:Vec<_>=who.map(|u|list(r,&format!("alert:{u}")).into_iter().cloned().collect()).unwrap_or_default();
    alerts.sort_by_key(|a| (a["read_at"].as_i64().unwrap_or(0)>0,std::cmp::Reverse(alert_priority(a)),std::cmp::Reverse(a["at"].as_i64().unwrap_or(0))));
    for alert in &mut alerts {let id=alert["id"].as_str().unwrap_or("");alert["list_state"]=json!(who.and_then(|u|get(r,&format!("inbox_alert:{u}"),id)).and_then(|v|v["state"].as_str()).unwrap_or("visible"));alert["priority"]=json!(alert_priority(alert));}
    let unread_alerts=alerts.iter().filter(|a|a["read_at"].as_i64().unwrap_or(0)==0 && a["list_state"]=="visible").count();
    // Bound each personal list separately so hidden rows cannot crowd out the inbox.
    let mut list_counts=std::collections::BTreeMap::<String,usize>::new();
    alerts.retain(|a|{let count=list_counts.entry(a["list_state"].as_str().unwrap_or("visible").to_owned()).or_default();*count+=1;*count<=100});

    let mut home_talks=Vec::new();
    if who.is_some(){for e in &ev {if e["demo"]==true || !["interested","going"].contains(&e["my_status"].as_str().unwrap_or("")){continue;}
        if let Ok(t)=thread(r,"event",e["id"].as_str().unwrap_or(""),who){if let Some(posts)=t["messages"].as_array(){for p in posts {let mut item=p.clone();item["event_id"]=e["id"].clone();item["event_name"]=e["name"].clone();home_talks.push(item);}}}
    }}
    home_talks.sort_by_key(|p|std::cmp::Reverse(p["edited_at"].as_i64().unwrap_or_else(||p["at"].as_i64().unwrap_or(0))));home_talks.truncate(100);
    let cases:Vec<_>=who.map(|u|list(r,"report").into_iter().filter(|x|x["reporter"]==u && x["kind"]=="emergency").cloned().collect()).unwrap_or_default();
    let invitations:Vec<_>=who.map(|u|list(r,"room").into_iter().filter(|room|room["kind"]!="event" && room["pending"].as_array().map(|p|p.contains(&json!(u))).unwrap_or(false)).map(|room|json!({"id":room["id"],"name":room["name"],"kind":room["kind"],"owner":room["owner"],"at":room["at"]})).collect()).unwrap_or_default();
    json!({"me":me,"events":ev,"home_talks":home_talks,"unread_alerts":unread_alerts,"drafts":who.map(|u|list(r,&format!("draft:{u}")).into_iter().cloned().collect::<Vec<_>>()).unwrap_or_default(),"cases":cases,"invitations":invitations,"accounts":list(r,"account").iter().filter(|a| who.map(|u| !blocked(r,u,a["id"].as_str().unwrap_or(""))).unwrap_or(true)).map(|a|public_account(r,a["id"].as_str().unwrap_or(""))).collect::<Vec<_>>(),"rooms":rooms,"following":follows,"blocked":who.map(|u|list(r,&format!("block:{u}")).into_iter().filter_map(Value::as_str).collect::<Vec<_>>()).unwrap_or_default(),"dismissed":who.map(|u|list(r,&format!("feed_dismiss:{u}")).into_iter().cloned().collect::<Vec<_>>()).unwrap_or_default(),"alerts":alerts})
}
pub fn blocked(r: &Records, a: &str, b: &str) -> bool {
    get(r,&format!("block:{a}"),b).is_some() || get(r,&format!("block:{b}"),a).is_some()
}
pub fn moderator(who: &str) -> bool {
    std::env::var("SPATIAL_MODERATOR_IDS").unwrap_or_default().split(',').map(str::trim).any(|id|!id.is_empty() && id==who)
}
pub fn moderation_queue(r: &Records, who: &str) -> Result<Value> {
    if !moderator(who) {return Err("モデレーション権限がありません".into());}
    let mut reports:Vec<_>=list(r,"report").into_iter().map(|report|{
        let mut out=report.clone();
        let kind=report["kind"].as_str().unwrap_or("");
        let target=report["target"].as_str().unwrap_or("");
        if kind=="post" || kind=="message" {
            if let Some(((scope,_),talk))=r.iter().find(|((scope,key),_)|key==target && scope.starts_with(if kind=="post" {"post:"} else {"message:"})) {
                out["target_scope"]=json!(scope);out["target_body"]=talk["body"].clone();out["target_author"]=talk["author"].clone();
            }
        } else if let Some(item)=get(r,kind,target) {out["target_name"]=item["name"].clone();}
        out
    }).collect();
    reports.sort_by_key(|v|(v["status"]!="open",v["kind"]!="emergency",std::cmp::Reverse(v["at"].as_i64().unwrap_or(0))));
    reports.truncate(100);
    let mut verifications:Vec<_>=list(r,"verification").into_iter().cloned().collect();
    verifications.sort_by_key(|v|(v["status"]!="pending",std::cmp::Reverse(v["at"].as_i64().unwrap_or(0))));
    verifications.truncate(100);
    let ar:Vec<_>=r.iter().filter(|((s,_),v)|s.starts_with("collab:")&&v["kind"]=="ar"&&v["visibility"]!="private"&&v["deleted"]!=true&&v["approved"]!=true).map(|(_,v)|v.clone()).collect();
    Ok(json!({"reports":reports,"verifications":verifications,"ar":ar}))
}
fn attendee(r: &Records, event: &str, who: &str) -> bool {
    if !get(r,"event",event).map(|e|super::completion::can_view(r,e,Some(who))).unwrap_or(false){return false;}
    get(r, "event", event)
        .map(|e| super::completion::can_edit(r,e,who))
        .unwrap_or(false)
        || get(r, &format!("rsvp:{event}"), who)
            .map(|v| v["status"] == "going")
            .unwrap_or(false)
}
pub fn room_allowed(r: &Records, room: &Value, who: &str) -> bool {
    if room["kind"] == "event" {
        attendee(r, room["event"].as_str().unwrap_or(""), who)
    } else {
        room["members"].as_array().map(|m| m.contains(&json!(who))).unwrap_or(false)
            && !room["pending"].as_array().map(|p|p.contains(&json!(who))).unwrap_or(false)
    }
}
fn alert_priority(a: &Value) -> u8 {
    if a["changed"].as_array().map(|v|v.iter().any(|x|x=="status" || x=="cancel_reason" || x=="deleted")).unwrap_or(false) {4}
    else if a["changed"].as_array().map(|v|v.iter().any(|x|x=="place" || x=="address" || x=="meeting" || x=="lat" || x=="lon")).unwrap_or(false) {3}
    else if a["type"]=="booking_promoted" || a["changed"].as_array().map(|v|v.iter().any(|x|x=="start" || x=="end")).unwrap_or(false) {2}
    else {1}
}
fn room_view(r: &Records, room: &Value, who: &str) -> Value {
    let mut out = room.clone();
    let rid = room["id"].as_str().unwrap_or("");
    let messages=list(r, &format!("message:{rid}"));
    let seen=get(r,&format!("room_read:{who}"),rid).map(|v|v["at_ms"].as_i64().unwrap_or_else(||v["at"].as_i64().unwrap_or(0)*1000)).unwrap_or(0);
    let members:std::collections::BTreeSet<_>=if room["kind"]=="event" {
        let eid=room["event"].as_str().unwrap_or("");
        list(r,"account").into_iter().filter_map(|a|a["id"].as_str()).filter(|u|attendee(r,eid,u)).map(str::to_owned).collect()
    } else {room["members"].as_array().map(|v|v.iter().filter_map(Value::as_str).filter(|u|room_allowed(r,room,u)).map(str::to_owned).collect()).unwrap_or_default()};
    out["active_members"]=json!(members);out["member_count"]=json!(members.len());
    out["message_count"] = json!(messages.len());
    out["list_state"]=json!(get(r,&format!("inbox_room:{who}"),rid).and_then(|v|v["state"].as_str()).unwrap_or("visible"));
    out["muted"] = json!(get(r,&format!("room_mute:{who}"),rid).is_some());
    out["unread"] = json!(messages.iter().filter(|m|m["author"]!=who && m["created_ms"].as_i64().unwrap_or_else(||m["at"].as_i64().unwrap_or(0)*1000)>seen && m["deleted"]!=true && m["moderation_hidden"]!=true && !blocked(r,who,m["author"].as_str().unwrap_or(""))).count());
    let latest=messages.iter().filter(|m|m["deleted"]!=true && m["moderation_hidden"]!=true && !blocked(r,who,m["author"].as_str().unwrap_or("")) && get(r,&format!("talk_hide:{who}"),m["id"].as_str().unwrap_or("")).is_none()).max_by_key(|m|m["edited_at"].as_i64().map(|t|t*1000).unwrap_or_else(||m["created_ms"].as_i64().unwrap_or_else(||m["at"].as_i64().unwrap_or(0)*1000)));
    out["latest_at"] = json!(latest.map(|m|m["edited_at"].as_i64().unwrap_or_else(||m["at"].as_i64().unwrap_or(0))).unwrap_or(room["at"].as_i64().unwrap_or(0)));
    out["latest_message_id"]=json!(latest.and_then(|m|m["id"].as_str()).unwrap_or(""));
    out["latest_talk"] = json!(latest.and_then(|m|m["body"].as_str()).unwrap_or("").chars().take(80).collect::<String>());
    out
}
pub fn thread(r: &Records, scope: &str, id: &str, who: Option<&str>) -> Result<Value> {
    let bucket = match scope {
        "room" => {
            let room = get(r, "room", id).ok_or("トークが見つかりません")?;
            if !who.map(|u| room_allowed(r, room, u)).unwrap_or(false) {
                return Err("このトークを閲覧する権限がありません".into());
            }
            format!("message:{id}")
        }
        "event" => {
            if !get(r, "event", id).map(|e|super::completion::can_view(r,e,who)).unwrap_or(false) {
                return Err("イベントが見つかりません".into());
            }
            format!("post:{id}")
        }
        _ => return Err("不正な表示先です".into()),
    };
    let visible = |m: &Value| m["deleted"]!=true && m["moderation_hidden"]!=true && who.map(|u| !blocked(r,u,m["author"].as_str().unwrap_or("")) && get(r,&format!("talk_hide:{u}"),m["id"].as_str().unwrap_or("")).is_none()).unwrap_or(true);
    let mut messages: Vec<Value> = list(r, &bucket)
        .iter()
        .filter(|m| visible(m))
        .map(|m| {
            let mut out = (*m).clone();
            out["author_profile"] = public_account(r, m["author"].as_str().unwrap_or(""));
            let talk_id=m["id"].as_str().unwrap_or("");
            out["reply_count"]=json!(list(r,&bucket).iter().filter(|x|x["reply_to"]==talk_id && visible(x)).count());
            if let Some(parent)=m["reply_to"].as_str().filter(|id|!id.is_empty()).and_then(|id|get(r,&bucket,id)).filter(|p|visible(p)) {
                out["reply_preview"]=json!(parent["body"].as_str().unwrap_or("").chars().take(100).collect::<String>());
            }
            if scope=="event" {
                out["host_answer"]=json!(m["reply_to"].as_str().map(|id|!id.is_empty()).unwrap_or(false) && get(r,"event",id).map(|e|e["owner"]==m["author"]).unwrap_or(false));
            }
            out
        })
        .collect();
    messages.sort_by_key(|m| m["created_ms"].as_i64().unwrap_or_else(||m["at"].as_i64().unwrap_or(0)*1000));
    Ok(
        json!({"messages":messages.into_iter().rev().take(200).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>()}),
    )
}
pub fn operate(r: &mut Records, who: &str, op: &str, v: &Value) -> Result<Value> {
    if get(r, "account", who).is_none() {
        return Err("ログインしてください".into());
    }
    if let Some(result)=super::completion::operate(r,who,op,v){return result;}
    let target = v["id"].as_str().unwrap_or("");
    let mut result = json!({"ok":true});
    match op {
        "emergency" => {
            let reason=txt(v,"reason",1,32)?;
            if !["danger","harassment","misinformation","other"].contains(&reason){return Err("連絡種別が不正です".into());}
            let detail=txt(v,"detail",20,1000)?;
            let event_id=v["event_id"].as_str().unwrap_or("");
            if !event_id.is_empty() && get(r,"event",event_id).is_none(){return Err("イベントが見つかりません".into());}
            let id=uid();
            put(r,"report",&id,json!({"id":id,"reporter":who,"kind":"emergency","target":event_id,"reason":reason,"detail":detail,"at":now(),"status":"open"}));
            result["id"]=json!(id);
        }
        "inbox_item" => {
            let kind=txt(v,"kind",1,12)?;
            let state=txt(v,"state",1,12)?;
            if !["visible","hidden","deleted"].contains(&state){return Err("一覧の状態が不正です".into());}
            match kind {
                "room"=>{let room=get(r,"room",target).ok_or("トークが見つかりません")?;if !room_allowed(r,room,who){return Err("このトークを操作できません".into());}},
                "alert"=>{if get(r,&format!("alert:{who}"),target).is_none(){return Err("通知が見つかりません".into());}},
                _=>return Err("一覧の種類が不正です".into()),
            }
            let scope=format!("inbox_{kind}:{who}");
            if state=="visible" {r.remove(&(scope,target.into()));}
            else {put(r,&scope,target,json!({"state":state,"at":now()}));}
        }
        "ack_alert" => {
            let alert=r.get_mut(&(format!("alert:{who}"),target.into())).ok_or("通知が見つかりません")?;
            alert["read_at"]=json!(now());
        }
        "read_room" => {
            let room=get(r,"room",target).ok_or("トークが見つかりません")?;
            if !room_allowed(r,room,who) {return Err("このトークを閲覧する権限がありません".into());}
            let latest=list(r,&format!("message:{target}")).iter().filter_map(|m|m["created_ms"].as_i64()).max().unwrap_or(0);
            put(r,&format!("room_read:{who}"),target,json!({"at":now(),"at_ms":now_ms().max(latest)}));
        }
        "hide_talk" | "delete_talk" => {
            let mut found=None;
            for ((scope,key),message) in r.iter() {
                if key==target && (scope.starts_with("post:")||scope.starts_with("message:")) {
                    found=Some((scope.clone(),message.clone()));break;
                }
            }
            let (scope,message)=found.ok_or("発言が見つかりません")?;
            if scope.starts_with("message:") {
                let room=get(r,"room",scope.trim_start_matches("message:")).ok_or("トークが見つかりません")?;
                if !room_allowed(r,room,who) {return Err("この発言を操作できません".into());}
            }
            if op=="hide_talk" {
                if v["active"]==false {r.remove(&(format!("talk_hide:{who}"),target.into()));}
                else {put(r,&format!("talk_hide:{who}"),target,json!({"at":now()}));}
            } else {
                if message["author"]!=who {return Err("自分の発言だけ削除できます".into());}
                let item=r.get_mut(&(scope.clone(),target.into())).unwrap();
                item["body"]=json!("");item["photo"]=json!("");item["deleted"]=json!(true);
                if let Some(mid)=message["photo"].as_str().and_then(|p|p.strip_prefix("/media/")) {r.remove(&("media".into(),mid.into()));}
            }
        }
        "report" => {
            let kind=txt(v,"kind",1,16)?;
            if !["event","account","post","message"].contains(&kind) { return Err("通報対象が不正です".into()); }
            let reason=txt(v,"reason",1,32)?;
            if !["misinformation","danger","impersonation","harassment","rights","other"].contains(&reason) { return Err("通報理由が不正です".into()); }
            let detail=txt(v,"detail",0,1000)?;
            let exists=if kind=="event"||kind=="account" { get(r,kind,target).is_some() } else {
                r.iter().any(|((scope,key),_)| key==target && if kind=="post" {scope.starts_with("post:")} else {scope.strip_prefix("message:").and_then(|id|get(r,"room",id)).map(|room|room_allowed(r,room,who)).unwrap_or(false)})
            };
            if !exists { return Err("通報対象が見つかりません".into()); }
            let id=uid();
            put(r,"report",&id,json!({"id":id,"reporter":who,"kind":kind,"target":target,"reason":reason,"detail":detail,"at":now(),"status":"open"}));
            result["id"]=json!(id);
        }
        "moderate" => {
            if !moderator(who) {return Err("モデレーション権限がありません".into());}
            let resolution=txt(v,"resolution",1,16)?;
            if !["hidden","dismissed","closed"].contains(&resolution) {return Err("処理結果が不正です".into());}
            let report=get(r,"report",target).ok_or("通報が見つかりません")?.clone();
            if report["status"]!="open" {return Err("この通報は処理済みです".into());}
            if report["kind"]=="emergency" && resolution!="closed" {return Err("緊急連絡には対応記録が必要です".into());}
            if resolution=="closed" && report["kind"]!="emergency" {return Err("この処理結果は緊急連絡専用です".into());}
            let review_note=if resolution=="closed" {txt(v,"review_note",10,1000)?.to_owned()} else {String::new()};
            if resolution=="hidden" {
                if report["kind"]!="post" && report["kind"]!="message" {return Err("発言の非表示はTalk通報だけに適用できます".into());}
                let talk=report["target"].as_str().unwrap_or("");
                let target_scope=r.keys().find(|(scope,key)|key==talk && (scope.starts_with("post:")||scope.starts_with("message:"))).map(|(scope,_)|scope.clone()).ok_or("発言が見つかりません")?;
                r.get_mut(&(target_scope,talk.into())).unwrap()["moderation_hidden"]=json!(true);
            }
            let item=r.get_mut(&("report".into(),target.into())).unwrap();
            item["status"]=json!(resolution);item["reviewer"]=json!(who);item["reviewed_at"]=json!(now());item["review_note"]=json!(review_note);
        }
        "verification_request" => {
            let kind=txt(v,"kind",1,12)?;
            if !["host","venue"].contains(&kind) {return Err("確認種別が不正です".into());}
            let evidence=txt(v,"evidence",20,1000)?;
            if kind=="host" && target!=who {return Err("本人だけが申請できます".into());}
            if kind=="venue" && get(r,"event",target).map(|e|e["owner"]!=who).unwrap_or(true) {return Err("主催者だけが申請できます".into());}
            let key=format!("{kind}:{target}");
            if get(r,"verification",&key).map(|x|x["status"]=="pending").unwrap_or(false) {return Err("申請を確認中です".into());}
            put(r,"verification",&key,json!({"id":key,"kind":kind,"target":target,"requester":who,"evidence":evidence,"at":now(),"status":"pending"}));
        }
        "review_verification" => {
            if !moderator(who) {return Err("モデレーション権限がありません".into());}
            let decision=txt(v,"decision",1,16)?;
            if !["approved","rejected"].contains(&decision) {return Err("審査結果が不正です".into());}
            let request=get(r,"verification",target).ok_or("申請が見つかりません")?.clone();
            if request["status"]!="pending" {return Err("申請は処理済みです".into());}
            if request["requester"]==who {return Err("自分の申請は審査できません".into());}
            let reviewer_note=txt(v,"reviewer_note",if decision=="approved" {20} else {5},1000)?;
            let kind=request["kind"].as_str().unwrap_or("");
            let id=request["target"].as_str().unwrap_or("");
            if decision=="approved" {
                let (scope,field)=if kind=="host" {("account","host_verified")} else {("event","venue_verified")};
                r.get_mut(&(scope.into(),id.into())).ok_or("確認対象が見つかりません")?[field]=json!(true);
            }
            let item=r.get_mut(&("verification".into(),target.into())).unwrap();
            item["status"]=json!(decision);item["reviewer"]=json!(who);item["reviewer_note"]=json!(reviewer_note);item["reviewed_at"]=json!(now());
        }
        "block" => {
            if target==who || get(r,"account",target).is_none() { return Err("ブロック先が不正です".into()); }
            let scope=format!("block:{who}");
            if v["active"]==true { put(r,&scope,target,json!(target));
                r.remove(&(format!("follow:{target}"),who.into()));
                r.remove(&(format!("follow:{who}"),target.into()));
            } else { r.remove(&(scope,target.into())); }
        }
        "dismiss" => {
            let kind=txt(v,"kind",1,16)?;
            if !["event","host"].contains(&kind) {return Err("非表示対象が不正です".into());}
            if get(r,if kind=="event" {"event"} else {"account"},target).is_none() {return Err("非表示対象が見つかりません".into());}
            let scope=format!("feed_dismiss:{who}");
            let key=format!("{kind}:{target}");
            if v["active"]==false {r.remove(&(scope,key));}
            else {
                let reason=txt(v,"reason",1,32)?;
                if !["not_interested","irrelevant","seen_enough","other"].contains(&reason) {return Err("理由が不正です".into());}
                put(r,&scope,&key,json!({"kind":kind,"id":target,"reason":reason,"at":now()}));
            }
        }
        "notice" => {
            let event = get(r, "event", target).ok_or("イベントが見つかりません")?;
            if !super::completion::can_edit(r,event,who) {
                return Err("お知らせは主催者だけが公開できます".into());
            }
            let event_name=event["name"].clone();
            let body = txt(v, "body", 1, 1000)?;
            let scope = format!("notice:{target}");
            if list(r, &scope).len() >= 20 {
                return Err("お知らせは20件までです".into());
            }
            let id = uid();
            put(r, &scope, &id, json!({"id":id,"body":body,"at":now()}));
            let audience:Vec<_>=r.iter().filter(|((s,_),v)|s==&format!("rsvp:{target}")&&["going","interested"].contains(&v["status"].as_str().unwrap_or(""))).map(|((_,person),_)|person.clone()).filter(|person|person!=who).collect();
            for person in audience {push_alert(r,&person,json!({"type":"notice","event":target,"name":event_name,"body":body}));}
            result["id"] = json!(id);
        }
        "edit_notice" => {
            let event_id=txt(v,"event_id",1,80)?;
            let event=get(r,"event",event_id).ok_or("イベントが見つかりません")?;
            if !super::completion::can_edit(r,event,who) {return Err("お知らせは主催者だけが訂正できます".into());}
            let event_name=event["name"].clone();
            let body=txt(v,"body",1,1000)?;
            let scope=format!("notice:{event_id}");
            let notice=get(r,&scope,target).ok_or("お知らせが見つかりません")?.clone();
            if notice["body"]==body {return Err("お知らせの内容が変わっていません".into());}
            let revisions=notice["revisions"].as_array().cloned().unwrap_or_default();
            if revisions.len()>=20 {return Err("お知らせの訂正は20回までです".into());}
            let mut revisions=revisions;
            revisions.push(json!({"body":notice["body"],"at":notice["edited_at"].as_i64().unwrap_or_else(||notice["at"].as_i64().unwrap_or_else(now))}));
            let item=r.get_mut(&(scope,target.into())).unwrap();
            item["body"]=json!(body);item["edited_at"]=json!(now());item["revisions"]=json!(revisions);
            let audience:Vec<_>=r.iter().filter(|((s,_),v)|s==&format!("rsvp:{event_id}")&&["going","interested"].contains(&v["status"].as_str().unwrap_or(""))).map(|((_,person),_)|person.clone()).filter(|person|person!=who).collect();
            for person in audience {push_alert(r,&person,json!({"type":"notice","event":event_id,"name":event_name,"body":body,"corrected":true}));}
        }
        "profile" => {
            let name = txt(v, "name", 1, 60)?;
            let bio = txt(v, "bio", 0, 1000)?;
            let theme = txt(v, "theme", 1, 16)?;
            let layout = txt(v, "layout", 1, 16)?;
            if !["sky", "violet", "mint", "sunset", "midnight"].contains(&theme)
                || !["cards", "journal"].contains(&layout)
            {
                return Err("プロフィールの形式が不正です".into());
            }
            let interests = v["interests"].as_array().ok_or("興味の形式が不正です")?;
            if interests.len() > 4
                || !interests
                    .iter()
                    .all(|x| [json!("ART"), json!("MUSIC"), json!("WALK"), json!("AR")].contains(x))
            {
                return Err("興味の形式が不正です".into());
            }
            let a = r.get_mut(&("account".into(), who.into())).unwrap();
            if a["name"]!=json!(name) {a["host_verified"]=json!(false);}
            a["name"] = json!(name);
            a["bio"] = json!(bio);
            a["theme"] = json!(theme);
            a["layout"] = json!(layout);
            a["interests"] = json!(interests);
            if let Some(banner)=v["banner"].as_str(){if !banner.is_empty(){let media=get_media_id(banner)?;if get(r,"media",media).map(|m|m["owner"]!=who||m["attached"]==true).unwrap_or(true){return Err("画像を利用できません".into());}}r.get_mut(&("account".into(),who.into())).unwrap()["banner"]=json!(banner);}
            if let Some(alt)=v["banner_alt"].as_str(){if alt.chars().count()>300{return Err("画像の説明は300文字以内です".into());}r.get_mut(&("account".into(),who.into())).unwrap()["banner_alt"]=json!(alt);}
            if let Some(avatar) = v["avatar"].as_str() {
                if !avatar.is_empty() {
                    let media = get_media_id(avatar)?;
                    if get(r, "media", media)
                        .map(|m| m["owner"] != who || m["attached"] == true)
                        .unwrap_or(true)
                    {
                        return Err("画像を利用できません".into());
                    }
                }
                r.get_mut(&("account".into(), who.into())).unwrap()["avatar"] = json!(avatar);
            }
        }
        "follow" => {
            if target == who || get(r, "account", target).is_none() {
                return Err("フォロー先が不正です".into());
            }
            if blocked(r,who,target) { return Err("この相手と接続できません".into()); }
            let scope = format!("follow:{target}");
            if v["active"] == true {
                put(r, &scope, who, json!({"at":now()}));
            } else {
                r.remove(&(scope, who.into()));
            }
        }
        "rsvp" => {
            let event=get(r,"event",target).ok_or("イベントが見つかりません")?;
            if !super::completion::can_view(r,event,Some(who)){return Err("このイベントにはアクセスできません".into());}
            let status = txt(v, "status", 1, 16)?;
            if !["none", "interested", "going"].contains(&status) {
                return Err("参加状態が不正です".into());
            }
            if status!="none" && is_demo_event(r,event) && event["owner"]!=who {
                return Err("操作体験用デモには実際の参加受付がありません".into());
            }
            if status!="none" && (DateTime::parse_from_rfc3339(event["end"].as_str().unwrap_or(""))
                .map(|d| d.timestamp()<=now()).unwrap_or(true) || event["status"]=="canceled") {
                return Err("このイベントの参加受付は終了しました".into());
            }
            if blocked(r,who,event["owner"].as_str().unwrap_or("")) { return Err("このイベントへ参加できません".into()); }
            let scope = format!("rsvp:{target}");
            if status == "none" {
                r.remove(&(scope, who.into()));
            } else {
                put(r, &scope, who, json!({"status":status,"at":now()}));
            }
        }
        "book" => {
            let event=get(r,"event",target).ok_or("イベントが見つかりません")?.clone();
            if !super::completion::can_view(r,&event,Some(who)){return Err("このイベントを利用できません".into());}
            let scope=format!("booking:{target}");
            if v["active"]==true {
                if is_demo_event(r,&event) || event["status"]=="canceled" {return Err("このイベントは予約できません".into());}
                if blocked(r,who,event["owner"].as_str().unwrap_or("")) {return Err("このイベントを予約できません".into());}
                let start=DateTime::parse_from_rfc3339(event["start"].as_str().unwrap_or("")).map_err(|_|"開催日時が不正です")?.timestamp();
                if start<=now() {return Err("予約受付は終了しました".into());}
                if event["booking_deadline"].as_str().and_then(|s|DateTime::parse_from_rfc3339(s).ok()).map(|d|d.timestamp()<=now()).unwrap_or(false) {return Err("予約受付の締切を過ぎました".into());}
                if event["price_yen"].as_u64()!=Some(0) {return Err("料金が未確定または有料のイベントは主催者の案内先で予約を確認してください".into());}
                let capacity=event["capacity"].as_u64().filter(|n|*n>0).ok_or("このイベントはSpatialで予約を受け付けていません")?;
                if get(r,&scope,who).is_some() {return Err("予約またはキャンセル待ちは登録済みです".into());}
                let booked=list(r,&scope).iter().filter(|b|b["status"]=="confirmed").count();
                let status=if booked<capacity as usize {"confirmed"} else {"waitlisted"};
                let at_ms=now_ms().max(list(r,&scope).iter().filter_map(|b|b["at_ms"].as_i64()).max().unwrap_or(0)+1);
                put(r,&scope,who,json!({"person":who,"status":status,"at":now(),"at_ms":at_ms}));
                result["status"]=json!(status);
            } else {
                let old=get(r,&scope,who).ok_or("予約・キャンセル待ちが見つかりません")?.clone();
                r.remove(&(scope.clone(),who.into()));
                if old["status"]=="confirmed" && event["status"]!="canceled" && DateTime::parse_from_rfc3339(event["start"].as_str().unwrap_or("")).map(|d|d.timestamp()>now()).unwrap_or(false) {
                    let next=list(r,&scope).iter().filter(|b|b["status"]=="waitlisted" && b["person"].as_str().map(|id|!blocked(r,id,event["owner"].as_str().unwrap_or(""))).unwrap_or(false)).min_by_key(|b|b["at_ms"].as_i64().unwrap_or(i64::MAX)).cloned().cloned();
                    if let Some(next)=next {
                        let person=next["person"].as_str().unwrap_or("").to_owned();
                        if !person.is_empty() {
                            r.get_mut(&(scope.clone(),person.clone())).unwrap()["status"]=json!("confirmed");
                            push_alert(r,&person,json!({"type":"booking_promoted","event":target,"name":event["name"],"body":"キャンセル待ちから予約確定に変わりました"}));
                        }
                    }
                }
                result["status"]=json!("none");
            }
        }
        "draft_event" => {
            let scope=format!("draft:{who}");
            let id=if target.is_empty(){if list(r,&scope).len()>=10{return Err("下書きは10件までです".into());}uid()}else{if get(r,&scope,target).is_none(){return Err("下書きが見つかりません".into());}target.to_owned()};
            let input=v["data"].as_object().ok_or("下書きの形式が不正です")?;
            let mut fields=serde_json::Map::new();
            for key in ["name","place","description","kind","start","end","address","meeting","lat","lon","floor","entrance","location_precision","venue_rights","location_confirmed","indoor","bring","status","cancel_reason","price_yen","capacity","booking_deadline","booking_url","payment_recipient","refund_policy","age_min","eligibility","wheelchair","elevator","step_free","preregistration"] {
                if let Some(value)=input.get(key){if !value.is_string() && !value.is_boolean() && !value.is_number() && !value.is_null(){return Err("下書きの値が不正です".into());}fields.insert(key.into(),value.clone());}
            }
            if let Some(options)=input.get("completion_options").filter(|v|v.is_object()){fields.insert("completion_options".into(),options.clone());}
            let serialized=serde_json::to_string(&fields).map_err(|_|"下書きの形式が不正です")?;
            if serialized.len()>10_000 {return Err("下書きは1万バイト以内にしてください".into());}
            put(r,&scope,&id,json!({"id":id,"data":fields,"at":now()}));
            result["id"]=json!(id);
        }
        "delete_draft" => {
            if r.remove(&(format!("draft:{who}"),target.into())).is_none(){return Err("下書きが見つかりません".into());}
        }
        "delete_event" => {
            let event=get(r,"event",target).ok_or("イベントが見つかりません")?.clone();
            if !super::completion::can_edit(r,&event,who) {return Err("作成者または承認済みの共同編集者だけがイベントを削除できます".into());}
            let audience:std::collections::HashSet<_>=r.iter().filter(|((s,_),_)|s==&format!("rsvp:{target}")||s==&format!("booking:{target}")).map(|((_,u),_)|u.clone()).collect();
            for person in audience {push_alert(r,&person,json!({"type":"event_change","deleted_event":target,"name":event["name"],"changed":["deleted"],"body":"主催者がイベントを削除しました"}));}
            let current=r.get_mut(&("event".into(),target.into())).unwrap();current["deleted"]=json!(true);current["deleted_at"]=json!(now());current["status"]=json!("canceled");
            if let Some(room)=r.get_mut(&("room".into(),format!("event-{target}"))){room["archived"]=json!(true);}
            let change=uid();put(r,&format!("event_change:{target}"),&change,json!({"id":change,"actor":who,"at":now(),"changed":["deleted"],"before":{"deleted":false},"after":{"deleted":true}}));
        }
        "event" => {
            let name = txt(v, "name", 1, 100)?;
            let place = txt(v, "place", 1, 100)?;
            let description = txt(v, "description", 1, 3000)?;
            let kind = txt(v, "kind", 1, 60)?;
            let start = DateTime::parse_from_rfc3339(txt(v, "start", 10, 40)?)
                .map_err(|_| "開始日時が不正です")?;
            let end = DateTime::parse_from_rfc3339(txt(v, "end", 10, 40)?)
                .map_err(|_| "終了日時が不正です")?;
            if end <= start || end.timestamp() - start.timestamp() > 604800 {
                return Err("終了は開始後、7日以内にしてください".into());
            }
            let online=v["completion_options"]["mode"]=="online";
            let lat = if online{0.}else{v["lat"].as_f64().ok_or("緯度が不正です")?};
            let lon = if online{0.}else{v["lon"].as_f64().ok_or("経度が不正です")?};
            if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90. || lon.abs() > 180. {
                return Err("位置が不正です".into());
            }
            let floor = match v.get("floor") {
                None | Some(Value::Null) => None,
                Some(n) => Some(
                    n.as_i64()
                        .filter(|n| (-20..=200).contains(n))
                        .ok_or("フロアは-20〜200の整数です")?,
                ),
            };
            let entrance = v.get("entrance").and_then(Value::as_str).unwrap_or("");
            if entrance.chars().count() > 600 || entrance.contains('\0') {
                return Err("入口案内は600文字以内です".into());
            }
            let eid = if target.is_empty() {
                format!("spid_{}", uid())
            } else {
                let old = get(r, "event", target).ok_or("イベントが見つかりません")?;
                if !super::completion::can_edit(r,old,who) {
                    return Err("主催者だけが変更できます".into());
                }
                target.to_owned()
            };
            let old = get(r,"event",&eid).cloned();
            let location_changed=old.as_ref().map(|o|o["lat"]!=json!(lat)||o["lon"]!=json!(lon)||o["place"]!=place).unwrap_or(true);
            let demo_owner=get(r,"account",who).map(is_demo_account).unwrap_or(false);
            let demo_event=old.as_ref().map(|e|is_demo_event(r,e)).unwrap_or(v["demo"]==true);
            if !online && !demo_owner && !demo_event && v["venue_rights"]!=true && old.as_ref().map(|o|o["venue_rights"]!=true).unwrap_or(true) {
                return Err("会場の利用権限を確認して申告してください".into());
            }
            if !online && location_changed && v["location_confirmed"]!=true {
                return Err("公開する会場の位置を地図で確認してください".into());
            }
            let precision=v["location_precision"].as_str().or_else(||old.as_ref().and_then(|o|o["location_precision"].as_str())).unwrap_or("approximate");
            if !["approximate","entrance","meeting"].contains(&precision) { return Err("位置の精度が不正です".into()); }
            let address=v["address"].as_str().or_else(||old.as_ref().and_then(|o|o["address"].as_str())).unwrap_or("");
            let meeting=v["meeting"].as_str().or_else(||old.as_ref().and_then(|o|o["meeting"].as_str())).unwrap_or("");
            if address.chars().count()>160||meeting.chars().count()>300 { return Err("住所・集合場所が長すぎます".into()); }
            let bring=v["bring"].as_str().or_else(||old.as_ref().and_then(|o|o["bring"].as_str())).unwrap_or("");
            if bring.chars().count()>300 || bring.contains('\0') {return Err("持ち物の条件は300文字以内です".into());}
            let price_yen=match v.get("price_yen") {
                Some(Value::Null)=>None,
                Some(value)=>Some(value.as_u64().filter(|n|*n<=1_000_000).ok_or("料金は0〜100万円の整数で入力してください")?),
                None=>old.as_ref().and_then(|o|o["price_yen"].as_u64()),
            };
            let capacity=match v.get("capacity") {
                Some(Value::Null)=>None,
                Some(value)=>Some(value.as_u64().filter(|n|(1..=1000).contains(n)).ok_or("定員は1〜1000人で入力してください")?),
                None=>old.as_ref().and_then(|o|o["capacity"].as_u64()),
            };
            let active_bookings=list(r,&format!("booking:{eid}"));
            if capacity.is_none() && !active_bookings.is_empty() {return Err("予約者またはキャンセル待ちがいるため定員予約を解除できません".into());}
            if let Some(limit)=capacity {if active_bookings.iter().filter(|b|b["status"]=="confirmed").count()>limit as usize {return Err("定員を現在の予約確定数より少なくできません".into());}}
            let booking_deadline=match v.get("booking_deadline") {
                Some(Value::Null)=>None,
                Some(Value::String(s)) if s.is_empty()=>None,
                Some(Value::String(s))=>Some(DateTime::parse_from_rfc3339(s).map_err(|_|"予約受付の締切日時が不正です")?.to_rfc3339()),
                Some(_)=>return Err("予約受付の締切日時が不正です".into()),
                None=>old.as_ref().and_then(|o|o["booking_deadline"].as_str()).map(str::to_owned),
            };
            if booking_deadline.as_deref().and_then(|s|DateTime::parse_from_rfc3339(s).ok()).map(|d|d.timestamp()>=start.timestamp()).unwrap_or(false) {return Err("予約受付の締切は開始前にしてください".into());}
            if old.is_none() && booking_deadline.as_deref().and_then(|s|DateTime::parse_from_rfc3339(s).ok()).map(|d|d.timestamp()<=now()).unwrap_or(false) {return Err("予約受付の締切は未来の日時にしてください".into());}
            let booking_url=v["booking_url"].as_str().or_else(||old.as_ref().and_then(|o|o["booking_url"].as_str())).unwrap_or("");
            if booking_url.len()>500 || (!booking_url.is_empty() && (!booking_url.starts_with("https://") || booking_url.chars().any(char::is_whitespace))) {return Err("外部予約URLはHTTPSのURLで入力してください".into());}
            if capacity.is_some() && price_yen!=Some(0) {return Err("Spatial内の定員予約は無料イベントで料金0円を設定してください。有料イベントは外部予約URLを使います".into());}
            let payment_recipient=v["payment_recipient"].as_str().or_else(||old.as_ref().and_then(|o|o["payment_recipient"].as_str())).unwrap_or("");
            let refund_policy=v["refund_policy"].as_str().or_else(||old.as_ref().and_then(|o|o["refund_policy"].as_str())).unwrap_or("");
            if payment_recipient.chars().count()>120 || refund_policy.chars().count()>600 {return Err("支払先・返金条件が長すぎます".into());}
            if price_yen.unwrap_or(0)>0 && (payment_recipient.trim().is_empty()||refund_policy.trim().is_empty()) {return Err("有料イベントは支払先と返金条件を入力してください".into());}
            let age_min=match v.get("age_min") {
                Some(Value::Null)=>None,
                Some(value)=>Some(value.as_u64().filter(|n|*n<=120).ok_or("対象年齢は0〜120歳で入力してください")?),
                None=>old.as_ref().and_then(|o|o["age_min"].as_u64()),
            };
            let eligibility=v["eligibility"].as_str().or_else(||old.as_ref().and_then(|o|o["eligibility"].as_str())).unwrap_or("");
            if eligibility.chars().count()>300 {return Err("参加条件は300文字以内です".into());}
            let wheelchair=v["wheelchair"].as_bool().or_else(||old.as_ref().and_then(|o|o["wheelchair"].as_bool())).unwrap_or(false);
            let elevator=v["elevator"].as_bool().or_else(||old.as_ref().and_then(|o|o["elevator"].as_bool())).unwrap_or(false);
            let step_free=v["step_free"].as_bool().or_else(||old.as_ref().and_then(|o|o["step_free"].as_bool())).unwrap_or(false);
            let preregistration=v["preregistration"].as_bool().or_else(||old.as_ref().and_then(|o|o["preregistration"].as_bool())).unwrap_or(false);
            let indoor=v["indoor"].as_bool().or_else(||old.as_ref().and_then(|o|o["indoor"].as_bool())).unwrap_or(false);
            let status=v["status"].as_str().or_else(||old.as_ref().and_then(|o|o["status"].as_str())).unwrap_or("active");
            if !["active","canceled"].contains(&status) { return Err("開催状態が不正です".into()); }
            let previous_start=old.as_ref().and_then(|o|o["start"].as_str()).and_then(|s|DateTime::parse_from_rfc3339(s).ok()).map(|d|d.timestamp());
            if status=="active" && start.timestamp()<now()-300 && previous_start!=Some(start.timestamp()) {return Err("過去の開始日時には公開・変更できません".into());}
            let cancel_reason=v["cancel_reason"].as_str().or_else(||old.as_ref().and_then(|o|o["cancel_reason"].as_str())).unwrap_or("");
            if status=="canceled" && (cancel_reason.trim().is_empty()||cancel_reason.chars().count()>1000) { return Err("中止理由を入力してください".into()); }
            let venue_changed=old.as_ref().map(|o|location_changed||o["address"]!=address||o["meeting"]!=meeting||o["entrance"]!=entrance).unwrap_or(true);
            let venue_verified=old.as_ref().map(|o|o["venue_verified"]==true && !venue_changed).unwrap_or(false);
            let version = get(r, "event", &eid)
                .and_then(|e| e["version"].as_i64())
                .unwrap_or(0)
                + 1;
            put(
                r,
                "event",
                &eid,
                json!({"id":eid,"owner":old.as_ref().map(|e|e["owner"].clone()).unwrap_or(json!(who)),"name":name,"place":place,"address":address,"meeting":meeting,"description":description,"kind":kind,"start":start.to_rfc3339(),"end":end.to_rfc3339(),"lat":lat,"lon":lon,"floor":floor,"entrance":entrance,"location_precision":precision,"bring":bring,"indoor":indoor,"price_yen":price_yen,"capacity":capacity,"booking_deadline":booking_deadline,"booking_url":booking_url,"payment_recipient":payment_recipient,"refund_policy":refund_policy,"age_min":age_min,"eligibility":eligibility,"wheelchair":wheelchair,"elevator":elevator,"step_free":step_free,"preregistration":preregistration,"venue_rights":v["venue_rights"]==true||old.as_ref().map(|o|o["venue_rights"]==true).unwrap_or(false),"venue_verified":venue_verified,"status":status,"cancel_reason":cancel_reason,"version":version,"sample":demo_event,"created":old.as_ref().and_then(|e|e["created"].as_i64()).unwrap_or_else(now)}),
            );
            if let Some(previous)=old.clone() {
                let changed=["name","description","kind","place","address","meeting","entrance","location_precision","start","end","lat","lon","floor","status","cancel_reason","price_yen","capacity","booking_deadline","booking_url","payment_recipient","refund_policy","age_min","eligibility","wheelchair","elevator","step_free","preregistration","bring","indoor","venue_verified"].iter().filter(|k| previous[**k]!=r.get(&("event".into(),eid.clone())).unwrap()[**k]).map(|k|*k).collect::<Vec<_>>();
                if !changed.is_empty() {
                    let current=get(r,"event",&eid).unwrap();
                    let before=changed.iter().map(|k|(k.to_string(),previous[*k].clone())).collect::<serde_json::Map<String,Value>>();
                    let after=changed.iter().map(|k|(k.to_string(),current[*k].clone())).collect::<serde_json::Map<String,Value>>();
                    let id=uid();put(r,&format!("event_change:{eid}"),&id,json!({"id":id,"at":now(),"changed":changed,"before":before,"after":after,"version":version}));
                    if changed.iter().any(|key| ["start","end","place","address","meeting","entrance","lat","lon","status","cancel_reason","price_yen","capacity","booking_deadline","booking_url","payment_recipient","refund_policy","age_min","eligibility","wheelchair","elevator","step_free","preregistration","venue_verified"].contains(key)) {
                        let audience:std::collections::HashSet<_>=r.iter().filter(|((scope,_),v)|(scope==&format!("rsvp:{eid}")&&["going","interested"].contains(&v["status"].as_str().unwrap_or("")))||(scope==&format!("booking:{eid}")&&["confirmed","waitlisted"].contains(&v["status"].as_str().unwrap_or(""))))
                            .map(|((_,person),_)|person.clone()).collect();
                        for person in audience {push_alert(r,&person,json!({"type":"event_change","event":eid,"name":name,"changed":changed,"version":version}));}
                    }
                }
            }
            let room = format!("event-{eid}");
            put(
                r,
                "room",
                &room,
                json!({"id":room,"kind":"event","event":eid,"name":name,"members":[],"owner":old.as_ref().map(|e|e["owner"].clone()).unwrap_or(json!(who)),"at":now(),"archived":get(r,"room",&room).map(|x|x["archived"]==true).unwrap_or(false)}),
            );
            if v["completion_options"].is_object(){let mut options=v["completion_options"].clone();options["id"]=json!(eid);super::completion::operate(r,who,"complete_event_options",&options).unwrap()?;}
            // Creating a plan also records the creator's attendance, in this transaction.
            // Editing must not undo a deliberate cancellation or join the co-editor.
            if old.is_none() && status == "active" {
                put(r, &format!("rsvp:{eid}"), who, json!({"status":"going","at":now(),"source":"creator"}));
            }
            if let Some(draft_id)=v["draft_id"].as_str().filter(|id|!id.is_empty()) {r.remove(&(format!("draft:{who}"),draft_id.into()));}
            result["id"] = json!(eid);
        }
        "post" | "message" => {
            let room_id=if op=="post" {format!("event-{target}")}else{target.to_owned()};
            if get(r,"room",&room_id).map(|x|x["archived"]==true).unwrap_or(false){return Err("アーカイブ済みの会話は閲覧のみです".into());}
            let body = txt(v, "body", 0, 2000)?;
            let photo = v["photo"].as_str().unwrap_or("");
            if body.is_empty() && photo.is_empty() {
                return Err("文章か写真を追加してください".into());
            }
            let scope = if op == "post" {
                if !attendee(r, target, who) {
                    return Err("参加予定にした人と主催者が投稿できます".into());
                }
                format!("post:{target}")
            } else {
                let room = get(r, "room", target).ok_or("トークが見つかりません")?;
                if !room_allowed(r, room, who) {
                    return Err("このトークには参加していません".into());
                }
                if room["kind"] != "event" && room["members"].as_array().map(|m| m.iter().any(|x| x.as_str().map(|id| blocked(r,who,id)).unwrap_or(false))).unwrap_or(false) {
                    return Err("ブロック中の相手を含むトークへは送信できません".into());
                }
                format!("message:{target}")
            };
            if list(r, &scope).len() >= 500 {
                return Err("このトークは500件に達しました。新しいトークを作成してください".into());
            }
            let reply_to=v["reply_to"].as_str().unwrap_or("");
            if !reply_to.is_empty() {
                let parent=get(r,&scope,reply_to).ok_or("返信先が見つかりません")?;
                if parent["deleted"]==true || parent["moderation_hidden"]==true || blocked(r,who,parent["author"].as_str().unwrap_or("")) {return Err("この発言には返信できません".into());}
            }
            if !photo.is_empty() {
                let mid = get_media_id(photo)?;
                if get(r, "media", mid)
                    .map(|m| m["owner"] != who)
                    .unwrap_or(true)
                {
                    return Err("画像を利用できません".into());
                }
                let m = r.get_mut(&("media".into(), mid.into())).unwrap();
                if m["attached"] == true {
                    return Err("この画像は既に投稿されています".into());
                }
                m["attached"] = json!(true);
                m["scope"] = json!(scope);
            }
            let send_scope=format!("send_log:{who}");
            let cutoff=now()-600;
            r.retain(|(s,_),v|s!=&send_scope||v["at"].as_i64().unwrap_or(0)>=cutoff);
            let recent=list(r,&send_scope);
            if recent.iter().filter(|x|x["at"].as_i64().unwrap_or(0)>=now()-60).count()>=8 {return Err("短時間の連続投稿を制限しています。少し待ってください".into());}
            let body_hash=if body.trim().is_empty(){String::new()}else{hash(&body.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase())};
            if !body_hash.is_empty() && recent.iter().filter(|x|x["body_hash"]==body_hash).count()>=3 {return Err("同じ内容の連続送信を制限しています".into());}
            put(r,&send_scope,&uid(),json!({"at":now(),"body_hash":body_hash}));
            let key = uid();
            let created_ms=now_ms().max(list(r,&scope).iter().filter_map(|m|m["created_ms"].as_i64()).max().unwrap_or(0)+1);
            put(
                r,
                &scope,
                &key,
                json!({"id":key,"author":who,"body":body,"tags":super::social::tokens(body,'#'),"mentions":super::social::tokens(body,'@'),"photo":photo,"reply_to":reply_to,"photo_alt":txt(v,"photo_alt",0,300).unwrap_or(""),"share_allowed":v["share_allowed"]==true,"summary_consent":v["summary_consent"]==true,"at":now(),"created_ms":created_ms}),
            );
            let audience:Vec<String>=if op=="post" {
                r.iter().filter(|((s,_),v)|s==&format!("rsvp:{target}")&&["going","interested"].contains(&v["status"].as_str().unwrap_or(""))).map(|((_,person),_)|person.clone()).collect()
            } else {
                let room=get(r,"room",target).unwrap();
                if room["kind"]=="event" {r.iter().filter(|((s,_),v)|s==&format!("rsvp:{}",room["event"].as_str().unwrap_or(""))&&v["status"]=="going").map(|((_,person),_)|person.clone()).chain(room["owner"].as_str().map(str::to_owned)).collect()}
                else {room["members"].as_array().unwrap().iter().filter_map(|m|m.as_str().filter(|id|room_allowed(r,room,id)).map(str::to_owned)).collect()}
            };
            let mentions=super::social::mention_recipients(r,who,body,if op=="post"{"event"}else{"room"},target);
            let notified=audience.clone();
            for person in audience {if person!=who && !blocked(r,who,&person) && !(op=="message" && get(r,&format!("room_mute:{person}"),target).is_some()) {push_alert(r,&person,json!({"type":if mentions.contains(&person){"mention"}else{"talk"},"event":if op=="post" {target} else {""},"room":if op=="message" {target} else {""},"name":if op=="post" {get(r,"event",target).and_then(|e|e["name"].as_str()).unwrap_or("イベント")} else {get(r,"room",target).and_then(|room|room["name"].as_str()).unwrap_or("トーク")},"talk":key,"body":body.chars().take(100).collect::<String>()}));}}
            super::social::notify_mentions(r,who,body,if op=="post"{"event"}else{"room"},target,&key,&notified);
            result["id"] = json!(key);
        }
        "room" => {
            let name = txt(v, "name", 1, 80)?;
            let members = v["members"].as_array().ok_or("メンバーを選んでください")?;
            if members.is_empty() || members.len() > 8 {
                return Err("作成者を含め最大9人です。他のメンバーを1〜8人選んでください".into());
            }
            let mut ids = vec![who.to_owned()];
            for m in members {
                let id = m.as_str().ok_or("メンバーが不正です")?;
                let a = get(r, "account", id).ok_or("メンバーが見つかりません")?;
                if a["sample"] == true {
                    return Err("サンプルアカウントにはメッセージを送れません".into());
                }
                if blocked(r,who,id) { return Err("この相手と会話できません".into()); }
                if !ids.contains(&id.to_owned()) {
                    ids.push(id.to_owned());
                }
            }
            if ids.len() < 2 {
                return Err("他のメンバーを選んでください".into());
            }
            ids.sort();
            if ids.len() == 2 {
                if let Some(old) = list(r, "room")
                    .iter()
                    .find(|room| room["kind"] == "dm" && room["members"] == json!(ids))
                {
                    return Ok(json!({"ok":true,"id":old["id"]}));
                }
                let recipient=ids.iter().find(|id|id.as_str()!=who).ok_or("相手が見つかりません")?;
                let policy=get(r,"account",recipient).and_then(|a|a["dm_policy"].as_str()).unwrap_or("everyone");
                if policy=="nobody" || (policy=="linked" && get(r,&format!("follow:{who}"),recipient).is_none()) {return Err("相手は新しいDMを受け付けていません".into());}
            }
            let rid = uid();
            put(
                r,
                "room",
                &rid,
                json!({"id":rid,"kind":if ids.len()==2{"dm"}else{"group"},"name":name,"pending":ids.iter().filter(|id|id.as_str()!=who).collect::<Vec<_>>(),"members":ids,"owner":who,"at":now()}),
            );
            result["id"] = json!(rid);
        }
        "room_invitation" => {
            let accept=v["accept"].as_bool().ok_or("招待の回答が不正です")?;
            let room=get(r,"room",target).ok_or("招待が見つかりません")?;
            if room["kind"]=="event" || !room["pending"].as_array().map(|p|p.contains(&json!(who))).unwrap_or(false) {return Err("未回答の招待が見つかりません".into());}
            if accept && blocked(r,who,room["owner"].as_str().unwrap_or("")) {return Err("ブロック中の相手の招待は受けられません".into());}
            let item=r.get_mut(&("room".into(),target.into())).unwrap();
            if let Some(pending)=item["pending"].as_array_mut(){pending.retain(|id|id!=who);}
            if !accept {if let Some(members)=item["members"].as_array_mut(){members.retain(|id|id!=who);}}
        }
        "leave_room" => {
            let room = get(r, "room", target).ok_or("トークが見つかりません")?;
            if room["kind"] == "event" {
                return Err("イベントの参加状態を変更してください".into());
            }
            if !room_allowed(r, room, who) {
                return Err("メンバーではありません".into());
            }
            let members: Vec<_> = room["members"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|id| *id != who)
                .cloned()
                .collect();
            r.get_mut(&("room".into(), target.into())).unwrap()["members"] = json!(members);
        }
        "mute_room" => {
            let room=get(r,"room",target).ok_or("トークが見つかりません")?;
            if !room_allowed(r,room,who) {return Err("このトークを設定できません".into());}
            let scope=format!("room_mute:{who}");
            if v["active"]==true {put(r,&scope,target,json!({"at":now()}));}
            else {r.remove(&(scope,target.into()));}
        }
        "notifications" => {
            let minutes = v["minutes"].as_i64().ok_or("通知時刻が不正です")?;
            if ![0, 5, 10, 30, 60, 1440].contains(&minutes) {
                return Err("通知時刻が不正です".into());
            }
            r.get_mut(&("account".into(), who.into())).unwrap()["notifications"] = json!({"enabled":v["enabled"]==true,"minutes":minutes,"messages":v["messages"]==true});
        }
        "dm_policy" => {
            let policy=txt(v,"policy",1,16)?;
            if !["everyone","linked","nobody"].contains(&policy) {return Err("DMの受信設定が不正です".into());}
            r.get_mut(&("account".into(),who.into())).unwrap()["dm_policy"]=json!(policy);
        }
        _ => return Err("未対応の操作です".into()),
    }
    audit(r, who, op, target);
    Ok(result)
}
fn get_media_id(url: &str) -> Result<&str> {
    let id = url.strip_prefix("/media/").ok_or("画像が不正です")?;
    Uuid::parse_str(id).map_err(|_| "画像IDが不正です")?;
    Ok(id)
}
fn m_id(m:&Value)->&str{m["id"].as_str().unwrap_or("")}
pub fn media_allowed(r: &Records, m: &Value, who: Option<&str>) -> bool {
    if m["owner"].as_str() == who {
        return true;
    }
    let scope = m["scope"].as_str().unwrap_or("");
    if scope.starts_with("post:") {
        if !get(r,"event",scope.trim_start_matches("post:")).map(|e|super::completion::can_view(r,e,who)).unwrap_or(false){return false;}
        return list(r,scope).iter().any(|post|post["photo"]==format!("/media/{}",m["id"].as_str().unwrap_or("")) && post["deleted"]!=true && post["moderation_hidden"]!=true && who.map(|u|get(r,&format!("talk_hide:{u}"),post["id"].as_str().unwrap_or("")).is_none()).unwrap_or(true));
    }
    if let Some(room) = scope.strip_prefix("message:") {
        return who
            .map(|u| {
                get(r, "room", room)
                    .map(|r2| room_allowed(r, r2, u))
                    .unwrap_or(false) && list(r,scope).iter().any(|post|post["photo"]==format!("/media/{}",m["id"].as_str().unwrap_or("")) && post["deleted"]!=true && post["moderation_hidden"]!=true && get(r,&format!("talk_hide:{u}"),post["id"].as_str().unwrap_or("")).is_none())
            })
            .unwrap_or(false);
    }
    if r.iter().any(|((scope,_),item)|scope.starts_with("collab:") && item["photo"]==format!("/media/{}",m_id(m)) && item["deleted"]!=true && (item["kind"]!="ar"||item["approved"]==true) && who.map(|u|!blocked(r,u,item["author"].as_str().unwrap_or(""))).unwrap_or(true) && get(r,"event",scope.trim_start_matches("collab:")).map(|e|super::completion::can_view(r,e,who) && (item["visibility"]=="public" || who.map(|u|super::completion::ids(&item["editors"]).iter().any(|v|v==u)&&attendee(r,scope.trim_start_matches("collab:"),u)).unwrap_or(false) || (item["visibility"]=="members" && who.map(|u|attendee(r,scope.trim_start_matches("collab:"),u)).unwrap_or(false)))).unwrap_or(false)){return true;}
    if list(r,"event_extra").iter().any(|x|x["cover"]==format!("/media/{}",m_id(m)) && get(r,"event",x["id"].as_str().unwrap_or("")).map(|e|super::completion::can_view(r,e,who)).unwrap_or(false)){return true;}
    list(r, "account")
        .iter()
        .any(|a| a["avatar"] == format!("/media/{}", m["id"].as_str().unwrap_or("")) || a["banner"] == format!("/media/{}", m["id"].as_str().unwrap_or("")))
}
pub fn delete_account(r: &mut Records, who: &str) -> Result<()> {
    let account=get(r,"account",who).ok_or("アカウントが見つかりません")?;
    if is_demo_account(account) {return Err("デモアカウントは削除できません".into());}
    let handle=account["handle"].as_str().unwrap_or("").to_owned();
    r.retain(|(s,_),v|s!="google_identity"||v["account"]!=who);
    let owned_events:Vec<String>=list(r,"event").iter().filter(|e|e["owner"]==who).filter_map(|e|e["id"].as_str().map(str::to_owned)).collect();
    let owned_rooms:Vec<String>=list(r,"room").iter().filter(|room|room["owner"]==who || owned_events.iter().any(|id|room["event"]==id.as_str())).filter_map(|room|room["id"].as_str().map(str::to_owned)).collect();
    let booked_events:Vec<String>=r.iter().filter(|((scope,key),_)|scope.starts_with("booking:") && key==who).map(|((scope,_),_)|scope.trim_start_matches("booking:").to_owned()).collect();
    r.retain(|(scope,key),value| {
        if (scope=="preferences" || scope=="recovery" || scope=="navigation_wallet") && key==who{return false;}
        if ["navigation_session","navigation_ledger"].contains(&scope.as_str())&&value["owner"]==who{return false;}
        if ["navigation_limit","navigation_daily","navigation_cooldown"].contains(&scope.as_str())&&key.starts_with(&format!("{who}:")){return false;}
        if scope==&format!("note:{who}") || scope==&format!("search:{who}"){return false;}
        if scope=="event_extra" && owned_events.contains(key){return false;}
        if scope=="delivery" && (value["recipient"]==who || owned_events.iter().any(|id|value["event"]==id.as_str())){return false;}
        if ["push_subscription","push_queue"].contains(&scope.as_str()) && value["owner"]==who{return false;}
        if scope.starts_with("collab:") && (value["author"]==who || owned_events.iter().any(|id|scope==&format!("collab:{id}"))){return false;}
        if (scope=="account" && key==who) || (scope=="handle" && key==&handle) {return false;}
        if scope=="session" && value["account"]==who {return false;}
        if scope=="media" && value["owner"]==who {return false;}
        if scope=="event" && owned_events.contains(key) {return false;}
        if scope=="room" && owned_rooms.contains(key) {return false;}
        if scope=="report" && value["reporter"]==who {return false;}
        if scope=="verification" && (value["requester"]==who || owned_events.iter().any(|id|value["target"]==id.as_str())) {return false;}
        if scope=="social_audit" && value["actor"]==who {return false;}
        if scope=="request" && key.starts_with(&format!("{who}:")) {return false;}
        if scope.starts_with("follow:") && (scope==&format!("follow:{who}") || key==who) {return false;}
        if scope.starts_with("block:") && (scope==&format!("block:{who}") || key==who) {return false;}
        if scope.starts_with("rsvp:") && (key==who || owned_events.iter().any(|id|scope==&format!("rsvp:{id}"))) {return false;}
        if scope.starts_with("booking:") && (key==who || owned_events.iter().any(|id|scope==&format!("booking:{id}"))) {return false;}
        if scope==&format!("send_log:{who}") {return false;}
        if scope.starts_with("alert:") && (scope==&format!("alert:{who}") || owned_events.iter().any(|id|value["event"]==id.as_str())) {return false;}
        if scope.starts_with("room_read:") && (scope==&format!("room_read:{who}") || owned_rooms.contains(key)) {return false;}
        if scope.starts_with("talk_hide:") && scope==&format!("talk_hide:{who}") {return false;}
        if scope==&format!("inbox_room:{who}")||scope==&format!("inbox_alert:{who}") {return false;}
        if scope.starts_with("feed_dismiss:") && (scope==&format!("feed_dismiss:{who}") || (value["kind"]=="host" && value["id"]==who) || (value["kind"]=="event" && owned_events.iter().any(|id|value["id"]==id.as_str()))) {return false;}
        if scope==&format!("draft:{who}") {return false;}
        if scope=="account_delete" && key==who {return false;}
        if scope.starts_with("notice:") || scope.starts_with("event_change:") || scope.starts_with("post:") {
            if owned_events.iter().any(|id|scope.ends_with(&format!(":{id}"))) || value["author"]==who {return false;}
        }
        if scope.starts_with("message:") && (owned_rooms.iter().any(|id|scope==&format!("message:{id}")) || value["author"]==who) {return false;}
        true
    });
    for ((scope,_),value) in r.iter_mut(){if scope=="event_extra"{for field in ["cohosts","cohost_invites","invitees"]{if let Some(ids)=value[field].as_array_mut(){ids.retain(|id|id!=who);}}}}
    for device in r.iter().filter(|((scope,_),d)|scope=="device" && d["accounts"].as_array().map(|a|a.contains(&json!(who))).unwrap_or(false)).map(|((_,key),_)|key.clone()).collect::<Vec<_>>() {
        if let Some(v)=r.get_mut(&("device".into(),device)) {if let Some(ids)=v["accounts"].as_array_mut(){ids.retain(|id|id!=who);}}
    }
    for room in list(r,"room").into_iter().filter(|room|room["members"].as_array().map(|a|a.contains(&json!(who))).unwrap_or(false)).filter_map(|room|room["id"].as_str().map(str::to_owned)).collect::<Vec<_>>() {
        if let Some(v)=r.get_mut(&("room".into(),room)) {if let Some(m)=v["members"].as_array_mut(){m.retain(|id|id!=who);}if let Some(p)=v["pending"].as_array_mut(){p.retain(|id|id!=who);}}
    }
    for eid in booked_events.into_iter().filter(|id|!owned_events.contains(id)) {
        let Some(event)=get(r,"event",&eid).cloned() else {continue};
        let scope=format!("booking:{eid}");
        let capacity=event["capacity"].as_u64().unwrap_or(0) as usize;
        let confirmed=list(r,&scope).iter().filter(|b|b["status"]=="confirmed").count();
        if confirmed>=capacity || event["status"]=="canceled" || DateTime::parse_from_rfc3339(event["start"].as_str().unwrap_or("")).map(|d|d.timestamp()<=now()).unwrap_or(true) {continue;}
        if let Some(next)=list(r,&scope).iter().filter(|b|b["status"]=="waitlisted" && b["person"]!=who).min_by_key(|b|b["at_ms"].as_i64().unwrap_or(i64::MAX)).cloned().cloned() {
            if let Some(person)=next["person"].as_str() {
                let person=person.to_owned();
                if let Some(booking)=r.get_mut(&(scope,person.clone())) {booking["status"]=json!("confirmed");}
                push_alert(r,&person,json!({"type":"booking_promoted","event":eid,"name":event["name"],"body":"キャンセル待ちから予約確定に変わりました"}));
            }
        }
    }
    Ok(())
}
pub fn seed(r: &mut Records) {
    if get(r, "meta", "seed-v1").is_some() {
        return;
    }
    for (i, handle, name, bio, kind) in [
        (
            0,
            "nagi_sample",
            "Nagi Studio",
            "街の余白とアートを見つける、架空の主催者。",
            "ART",
        ),
        (
            1,
            "sora_sample",
            "Sora Walks",
            "歩いて出会う風景を紹介する、架空の主催者。",
            "WALK",
        ),
        (
            2,
            "mina_sample",
            "Mina Sound",
            "音楽と街の接点を探す、架空の主催者。",
            "MUSIC",
        ),
    ] {
        let id = format!("00000000-0000-4000-8000-{:012}", i + 1);
        put(
            r,
            "account",
            &id,
            json!({"id":id,"handle":handle,"name":name,"bio":bio,"theme":(["sky","mint","violet"][i]),"layout":"cards","avatar":"","interests":[kind],"password_hash":"","disabled":true,"sample":true}),
        );
        put(r, "handle", handle, json!(id));
        for j in 0..2 {
            let eid = format!("spid_00000000-0000-4000-9000-{:012}", i * 2 + j + 1);
            let start = (Utc::now() + chrono::Duration::days((i * 2 + j + 3) as i64))
                .date_naive()
                .and_hms_opt(9, 0, 0)
                .unwrap()
                .and_utc();
            let title = [
                "光の余白、街の展示室",
                "夕暮れのフォトウォーク",
                "川沿いの小さな発見",
                "路地と建築をめぐる",
                "街の音を集める夜",
                "屋上のリスニング会",
            ][i * 2 + j];
            put(
                r,
                "event",
                &eid,
                json!({"id":eid,"owner":id,"name":title,"place":(["秋葉原・万世橋","神田川沿い","御茶ノ水"][i]),"description":format!("{title}。その場所の空気を楽しみ、参加者と発見を分かち合う体験です。動きやすい服装でお越しください。これは操作体験用のサンプルで、実際の開催予定ではありません。"),"kind":kind,"start":start.to_rfc3339(),"end":(start+chrono::Duration::hours(2)).to_rfc3339(),"lat":35.6972+i as f64*0.001,"lon":139.771+j as f64*0.003,"version":1,"sample":true,"created":now()}),
            );
            let room = format!("event-{eid}");
            put(
                r,
                "room",
                &room,
                json!({"id":room,"kind":"event","event":eid,"name":title,"members":[],"owner":id,"at":now()}),
            );
        }
    }
    put(r, "meta", "seed-v1", json!(true));
}
#[cfg(test)]
mod community_tests {
    use super::*;
    fn fixture() -> Records {
        let mut r=Records::new();
        for id in ["a","b","c"] {put(&mut r,"account",id,json!({"id":id,"handle":id,"name":id,"disabled":false,"interests":[]}));}
        put(&mut r,"event","e",json!({"id":"e","owner":"a","name":"Test event","kind":"WALK","start":"2030-01-01T00:00:00Z","end":"2030-01-01T02:00:00Z"}));
        put(&mut r,"rsvp:e","b",json!({"status":"going"}));
        put(&mut r,"room","r",json!({"id":"r","kind":"dm","name":"DM","members":["a","b"],"owner":"a","at":now()}));
        r
    }
    #[test]
    fn inbox_actions_are_private_reversible_and_do_not_delete_talks() {
        let mut r=fixture();
        operate(&mut r,"a","message",&json!({"id":"r","body":"Keep shared history"})).unwrap();
        let initial=bootstrap(&r,Some("b"));
        let aid=initial["alerts"][0]["id"].as_str().unwrap().to_owned();
        assert!(operate(&mut r,"c","inbox_item",&json!({"kind":"room","id":"r","state":"deleted"})).is_err());
        assert!(operate(&mut r,"a","inbox_item",&json!({"kind":"alert","id":aid,"state":"hidden"})).is_err());
        assert!(operate(&mut r,"b","inbox_item",&json!({"kind":"room","id":"r","state":"purge"})).is_err());
        for state in ["hidden","deleted","visible"] {
            operate(&mut r,"b","inbox_item",&json!({"kind":"room","id":"r","state":state})).unwrap();
            operate(&mut r,"b","inbox_item",&json!({"kind":"alert","id":aid,"state":state})).unwrap();
            let b=bootstrap(&r,Some("b"));
            assert_eq!(b["rooms"][0]["list_state"],state);
            assert_eq!(b["alerts"][0]["list_state"],state);
            assert_eq!(b["unread_alerts"],if state=="visible"{1}else{0});
            assert_eq!(bootstrap(&r,Some("a"))["rooms"][0]["list_state"],"visible");
            assert_eq!(thread(&r,"room","r",Some("b")).unwrap()["messages"].as_array().unwrap().len(),1);
        }
        assert!(get(&r,"inbox_room:b","r").is_none());
        assert!(get(&r,"inbox_alert:b",&aid).is_none());
        operate(&mut r,"b","inbox_item",&json!({"kind":"room","id":"r","state":"hidden"})).unwrap();
        let own=export_personal_data(&r,"b").unwrap();
        assert!(own.to_string().contains("inbox_room:b"));
        assert!(!export_personal_data(&r,"a").unwrap().to_string().contains("inbox_room:b"));
        delete_account(&mut r,"b").unwrap();
        assert!(get(&r,"inbox_room:b","r").is_none());
    }
    #[test]
    fn talk_alerts_unread_and_personal_hide() {
        let mut r=fixture();
        let id=operate(&mut r,"a","message",&json!({"id":"r","body":"hello"})).unwrap()["id"].as_str().unwrap().to_owned();
        let b=bootstrap(&r,Some("b"));
        assert_eq!(b["rooms"][0]["unread"],1);
        assert!(b["alerts"].as_array().unwrap().iter().any(|a|a["type"]=="talk"));
        assert!(operate(&mut r,"c","hide_talk",&json!({"id":id})).is_err());
        operate(&mut r,"b","hide_talk",&json!({"id":id})).unwrap();
        assert_eq!(thread(&r,"room","r",Some("b")).unwrap()["messages"].as_array().unwrap().len(),0);
        assert_eq!(thread(&r,"room","r",Some("a")).unwrap()["messages"].as_array().unwrap().len(),1);
        operate(&mut r,"b","read_room",&json!({"id":"r"})).unwrap();
        assert_eq!(bootstrap(&r,Some("b"))["rooms"][0]["unread"],0);
        assert!(operate(&mut r,"b","delete_talk",&json!({"id":id})).is_err());
        operate(&mut r,"a","delete_talk",&json!({"id":id})).unwrap();
        assert_eq!(thread(&r,"room","r",Some("a")).unwrap()["messages"].as_array().unwrap().len(),0);
    }
    #[test]
    fn private_report_requires_room_access_and_deletion_revokes_session() {
        let mut r=fixture();
        let id=operate(&mut r,"a","message",&json!({"id":"r","body":"hello"})).unwrap()["id"].as_str().unwrap().to_owned();
        let report=json!({"id":id,"kind":"message","reason":"harassment","detail":""});
        assert!(operate(&mut r,"c","report",&report).is_err());
        assert!(operate(&mut r,"b","report",&report).is_ok());
        new_session(&mut r,"a","secret");
        assert_eq!(account(&r,"secret"),Some("a".into()));
        delete_account(&mut r,"a").unwrap();
        assert_eq!(account(&r,"secret"),None);
        assert!(get(&r,"event","e").is_none());
        assert!(get(&r,"message:r",&id).is_none());
    }
    #[test]
    fn alerts_prioritize_cancellation_and_verification_needs_review() {
        let mut r=fixture();
        push_alert(&mut r,"b",json!({"type":"talk","name":"DM","room":"r"}));
        push_alert(&mut r,"b",json!({"type":"event_change","name":"Test event","event":"e","changed":["status"]}));
        assert_eq!(bootstrap(&r,Some("b"))["alerts"][0]["changed"][0],"status");
        assert!(operate(&mut r,"b","verification_request",&json!({"id":"e","kind":"venue","evidence":"I have permission from the venue manager."})).is_err());
        operate(&mut r,"a","verification_request",&json!({"id":"e","kind":"venue","evidence":"I have permission from the venue manager."})).unwrap();
        assert_ne!(get(&r,"event","e").unwrap()["venue_verified"],true);
    }
    #[test]
    fn replies_stay_in_scope_and_respect_hidden_parents() {
        let mut r=fixture();
        let parent=operate(&mut r,"b","post",&json!({"id":"e","body":"meeting point?"})).unwrap()["id"].as_str().unwrap().to_owned();
        let answer=operate(&mut r,"a","post",&json!({"id":"e","body":"front gate","reply_to":parent})).unwrap()["id"].as_str().unwrap().to_owned();
        assert!(operate(&mut r,"a","message",&json!({"id":"r","body":"wrong scope","reply_to":parent})).is_err());
        let visible=thread(&r,"event","e",Some("b")).unwrap();
        let posts=visible["messages"].as_array().unwrap();
        assert_eq!(posts[0]["reply_count"],1);
        assert_eq!(posts[1]["host_answer"],true);
        assert_eq!(posts[1]["reply_preview"],"meeting point?");
        operate(&mut r,"b","hide_talk",&json!({"id":parent})).unwrap();
        let hidden=thread(&r,"event","e",Some("b")).unwrap();
        assert_eq!(hidden["messages"][0]["id"],answer);
        assert!(hidden["messages"][0].get("reply_preview").is_none());
    }
    #[test]
    fn feed_feedback_is_personal_and_reversible() {
        let mut r=fixture();
        operate(&mut r,"b","dismiss",&json!({"kind":"event","id":"e","reason":"not_interested","active":true})).unwrap();
        assert_eq!(bootstrap(&r,Some("b"))["dismissed"][0]["id"],"e");
        assert_eq!(bootstrap(&r,Some("a"))["dismissed"].as_array().unwrap().len(),0);
        assert!(operate(&mut r,"b","dismiss",&json!({"kind":"event","id":"e","reason":"bad","active":true})).is_err());
        operate(&mut r,"b","dismiss",&json!({"kind":"event","id":"e","active":false})).unwrap();
        assert_eq!(bootstrap(&r,Some("b"))["dismissed"].as_array().unwrap().len(),0);
    }
    #[test]
    fn home_feed_respects_watch_visibility_and_full_unread_count() {
        let mut r=fixture();
        operate(&mut r,"b","rsvp",&json!({"id":"e","status":"interested"})).unwrap();
        let post=operate(&mut r,"a","post",&json!({"id":"e","body":"Home feed public talk"})).unwrap()["id"].as_str().unwrap().to_owned();
        assert!(bootstrap(&r,Some("b"))["home_talks"].as_array().unwrap().iter().any(|p|p["id"]==post));
        assert!(bootstrap(&r,None)["home_talks"].as_array().unwrap().is_empty());
        operate(&mut r,"b","hide_talk",&json!({"id":post})).unwrap();
        assert!(bootstrap(&r,Some("b"))["home_talks"].as_array().unwrap().is_empty());
        for i in 0..105 {push_alert(&mut r,"b",json!({"name":format!("badge-{i}"),"type":"talk"}));}
        let count=bootstrap(&r,Some("b"))["unread_alerts"].as_u64().unwrap();
        assert!(count>=105);
        assert_eq!(bootstrap(&r,Some("b"))["alerts"].as_array().unwrap().len(),100);
        let first=bootstrap(&r,Some("b"))["alerts"][0]["id"].clone();
        operate(&mut r,"b","ack_alert",&json!({"id":first})).unwrap();
        assert_eq!(bootstrap(&r,Some("b"))["unread_alerts"].as_u64().unwrap(),count-1);
        assert!(bootstrap(&r,Some("b"))["alerts"].as_array().unwrap().iter().all(|a|a["read_at"].as_i64().unwrap_or(0)==0));
    }
    #[test]
    fn paid_event_requires_payment_and_refund_terms() {
        let mut r=fixture();
        let base=json!({"name":"Paid walk","place":"Tokyo","description":"Guided walk","kind":"WALK","start":"2030-01-02T00:00:00Z","end":"2030-01-02T02:00:00Z","lat":35.7,"lon":139.7,"venue_rights":true,"location_confirmed":true,"price_yen":1500,"age_min":12,"wheelchair":true});
        assert!(operate(&mut r,"a","event",&base).is_err());
        let mut complete=base;
        complete["payment_recipient"]=json!("主催者の受付窓口");
        complete["refund_policy"]=json!("中止時は主催者が返金します");
        let id=operate(&mut r,"a","event",&complete).unwrap()["id"].as_str().unwrap().to_owned();
        let visible=bootstrap(&r,Some("b"));
        let event=visible["events"].as_array().unwrap().iter().find(|e|e["id"]==id).unwrap();
        assert_eq!(event["price_yen"],1500);
        assert_eq!(event["wheelchair"],true);
        assert_eq!(event["age_min"],12);
    }
    #[test]
    fn incomplete_drafts_remain_private_until_published() {
        let mut r=fixture();
        let draft_id=operate(&mut r,"a","draft_event",&json!({"data":{"name":"Unfinished","place":"Tokyo"}})).unwrap()["id"].as_str().unwrap().to_owned();
        assert_eq!(bootstrap(&r,Some("a"))["drafts"][0]["data"]["name"],"Unfinished");
        assert!(bootstrap(&r,Some("b"))["drafts"].as_array().unwrap().is_empty());
        assert!(operate(&mut r,"b","delete_draft",&json!({"id":draft_id})).is_err());
        let published=operate(&mut r,"a","event",&json!({"draft_id":draft_id,"name":"Finished","place":"Tokyo","description":"A walk","kind":"WALK","start":"2030-01-03T00:00:00Z","end":"2030-01-03T02:00:00Z","lat":35.7,"lon":139.7,"venue_rights":true,"location_confirmed":true})).unwrap();
        assert!(published["id"].as_str().is_some());
        assert!(bootstrap(&r,Some("a"))["drafts"].as_array().unwrap().is_empty());
    }
    #[test]
    fn notice_correction_keeps_history_and_notifies_attendees() {
        let mut r=fixture();
        let id=operate(&mut r,"a","notice",&json!({"id":"e","body":"北口に集合"})).unwrap()["id"].as_str().unwrap().to_owned();
        assert!(operate(&mut r,"b","edit_notice",&json!({"id":id,"event_id":"e","body":"南口に集合"})).is_err());
        operate(&mut r,"a","edit_notice",&json!({"id":id,"event_id":"e","body":"南口に集合"})).unwrap();
        let visible=event_public(&r,get(&r,"event","e").unwrap(),Some("b"));
        assert_eq!(visible["notices"][0]["body"],"南口に集合");
        assert_eq!(visible["notices"][0]["revisions"][0]["body"],"北口に集合");
        assert!(bootstrap(&r,Some("b"))["alerts"].as_array().unwrap().iter().any(|a|a["corrected"]==true));
    }
    #[test]
    fn dm_policy_controls_new_invites_without_revoking_existing_room() {
        let mut r=fixture();
        operate(&mut r,"b","dm_policy",&json!({"policy":"nobody"})).unwrap();
        assert!(operate(&mut r,"c","room",&json!({"name":"New DM","members":["b"]})).is_err());
        assert_eq!(operate(&mut r,"a","room",&json!({"name":"Existing DM","members":["b"]})).unwrap()["id"],"r");
        operate(&mut r,"b","dm_policy",&json!({"policy":"linked"})).unwrap();
        assert!(operate(&mut r,"c","room",&json!({"name":"New DM","members":["b"]})).is_err());
        operate(&mut r,"b","follow",&json!({"id":"c","active":true})).unwrap();
        assert!(operate(&mut r,"c","room",&json!({"name":"New DM","members":["b"]})).is_ok());
    }
    #[test]
    fn muted_room_keeps_unread_but_suppresses_talk_alerts_and_spam() {
        let mut r=fixture();
        operate(&mut r,"b","mute_room",&json!({"id":"r","active":true})).unwrap();
        let before=bootstrap(&r,Some("b"))["alerts"].as_array().unwrap().len();
        operate(&mut r,"a","message",&json!({"id":"r","body":"one"})).unwrap();
        let b=bootstrap(&r,Some("b"));
        assert_eq!(b["rooms"][0]["muted"],true);
        assert_eq!(b["rooms"][0]["unread"],1);
        assert_eq!(b["alerts"].as_array().unwrap().len(),before);
        for _ in 0..2 {operate(&mut r,"a","message",&json!({"id":"r","body":"repeat"})).unwrap();}
        assert!(operate(&mut r,"a","message",&json!({"id":"r","body":"repeat"})).is_ok());
        assert!(operate(&mut r,"a","message",&json!({"id":"r","body":"repeat"})).is_err());
        for i in 0..4 {operate(&mut r,"a","message",&json!({"id":"r","body":format!("unique-{i}")})).unwrap();}
        assert!(operate(&mut r,"a","message",&json!({"id":"r","body":"ninth"})).is_err());
    }
    #[test]
    fn free_booking_waitlist_promotes_oldest_and_stays_distinct_from_join() {
        let mut r=fixture();
        let data=json!({"name":"Free walk","place":"Tokyo","description":"Guided walk","kind":"WALK","start":"2030-01-03T00:00:00Z","end":"2030-01-03T02:00:00Z","lat":35.7,"lon":139.7,"venue_rights":true,"location_confirmed":true,"price_yen":0,"capacity":1,"booking_deadline":"2030-01-02T00:00:00Z"});
        let id=operate(&mut r,"a","event",&data).unwrap()["id"].as_str().unwrap().to_owned();
        assert_eq!(operate(&mut r,"b","book",&json!({"id":id,"active":true})).unwrap()["status"],"confirmed");
        assert_eq!(operate(&mut r,"c","book",&json!({"id":id,"active":true})).unwrap()["status"],"waitlisted");
        let visible=event_public(&r,get(&r,"event",&id).unwrap(),Some("c"));
        assert_eq!(visible["booked"],1);assert_eq!(visible["waitlisted"],1);
        assert_eq!(visible["my_booking"],"waitlisted");assert_eq!(visible["my_status"],"none");
        operate(&mut r,"b","book",&json!({"id":id,"active":false})).unwrap();
        assert_eq!(get(&r,&format!("booking:{id}"),"c").unwrap()["status"],"confirmed");
        assert!(bootstrap(&r,Some("c"))["alerts"].as_array().unwrap().iter().any(|a|a["type"]=="booking_promoted"));
        let mut no_capacity=data.clone();no_capacity["id"]=json!(id);no_capacity["capacity"]=Value::Null;
        assert!(operate(&mut r,"a","event",&no_capacity).is_err());
        assert_eq!(operate(&mut r,"b","book",&json!({"id":id,"active":true})).unwrap()["status"],"waitlisted");
        delete_account(&mut r,"c").unwrap();
        assert_eq!(get(&r,&format!("booking:{id}"),"b").unwrap()["status"],"confirmed");
        let mut paid=data.clone();paid["price_yen"]=json!(2000);
        assert!(operate(&mut r,"a","event",&paid).is_err());
    }
    #[test]
    fn personal_export_excludes_credentials_other_people_and_photo_bytes() {
        let mut r=fixture();
        get_mut_for_test(&mut r,"account","a")["password_hash"]=json!("secret-hash");
        put(&mut r,"media","mine",json!({"owner":"a","data":"base64-private","scope":"post:e"}));
        put(&mut r,"media","theirs",json!({"owner":"b","data":"other-private"}));
        put(&mut r,"message:r","own-message",json!({"author":"a","body":"my words"}));
        put(&mut r,"message:r","other-message",json!({"author":"b","body":"their words"}));
        let exported=export_personal_data(&r,"a").unwrap().to_string();
        assert!(exported.contains("own-message"));
        assert!(exported.contains("mine"));
        for absent in ["secret-hash","base64-private","other-private","other-message","their words"] {assert!(!exported.contains(absent),"{absent} leaked");}
    }
    #[test]
    fn event_history_records_values_and_venue_review_survives_unrelated_edit() {
        let mut r=fixture();
        let mut data=json!({"name":"Walk","place":"Tokyo","address":"1-1","description":"Original","kind":"WALK","start":"2030-01-03T00:00:00Z","end":"2030-01-03T02:00:00Z","lat":35.7,"lon":139.7,"venue_rights":true,"location_confirmed":true});
        let id=operate(&mut r,"a","event",&data).unwrap()["id"].as_str().unwrap().to_owned();
        get_mut_for_test(&mut r,"event",&id)["venue_verified"]=json!(true);
        data["id"]=json!(id);data["description"]=json!("Revised");
        operate(&mut r,"a","event",&data).unwrap();
        assert_eq!(get(&r,"event",&id).unwrap()["venue_verified"],true);
        let history=list(&r,&format!("event_change:{id}"));
        assert_eq!(history[0]["before"]["description"],"Original");
        assert_eq!(history[0]["after"]["description"],"Revised");
        data["address"]=json!("2-2");
        operate(&mut r,"a","event",&data).unwrap();
        assert_eq!(get(&r,"event",&id).unwrap()["venue_verified"],false);
        assert!(operate(&mut r,"a","event",&json!({"name":"Past","place":"Tokyo","description":"Past walk","kind":"WALK","start":"2020-01-01T00:00:00Z","end":"2020-01-01T02:00:00Z","lat":35.7,"lon":139.7,"venue_rights":true,"location_confirmed":true})).is_err());
    }
    #[test]
    fn creator_attendance_is_atomic_and_edits_keep_cancellation() {
        let mut r=fixture();
        let mut data=json!({"name":"New walk","place":"Tokyo","description":"A short walk","kind":"WALK","start":"2030-01-03T00:00:00Z","end":"2030-01-03T02:00:00Z","lat":35.7,"lon":139.7,"venue_rights":true,"location_confirmed":true});
        let id=operate(&mut r,"a","event",&data).unwrap()["id"].as_str().unwrap().to_owned();
        assert_eq!(get(&r,&format!("rsvp:{id}"),"a").unwrap()["status"],"going");
        operate(&mut r,"a","rsvp",&json!({"id":id,"status":"none"})).unwrap();
        data["id"]=json!(id);operate(&mut r,"a","event",&data).unwrap();
        assert!(get(&r,&format!("rsvp:{id}"),"a").is_none());
        put(&mut r,"event_extra",&id,json!({"cohosts":["c"]}));
        operate(&mut r,"c","event",&data).unwrap();
        assert!(get(&r,&format!("rsvp:{id}"),"c").is_none());
        assert!(get(&r,&format!("rsvp:{id}"),"a").is_none());
        let mut demo=data.clone();demo.as_object_mut().unwrap().remove("id");demo["demo"]=json!(true);
        let did=operate(&mut r,"a","event",&demo).unwrap()["id"].as_str().unwrap().to_owned();
        operate(&mut r,"a","rsvp",&json!({"id":did,"status":"none"})).unwrap();
        assert!(operate(&mut r,"a","rsvp",&json!({"id":did,"status":"going"})).is_ok());
        assert!(operate(&mut r,"b","rsvp",&json!({"id":did,"status":"going"})).is_err());
    }
    fn get_mut_for_test<'a>(r:&'a mut Records,scope:&str,id:&str)->&'a mut Value{r.get_mut(&(scope.into(),id.into())).unwrap()}
}
