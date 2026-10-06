//! Text tokens and permission-aware mention delivery.
use super::{model as m, store::Records};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn tokens(body: &str, marker: char) -> Vec<String> {
    let chars: Vec<_> = body.chars().collect();
    let mut out = BTreeSet::new();
    for (i, c) in chars.iter().enumerate() {
        if *c != marker && !(marker == '#' && *c == '＃') && !(marker == '@' && *c == '＠') { continue; }
        if i > 0 && (chars[i-1].is_alphanumeric() || chars[i-1] == '_') { continue; }
        let word: String = chars[i+1..].iter().take_while(|c| {
            if marker == '@' { c.is_ascii_alphanumeric() || **c == '_' }
            else { c.is_alphanumeric() || **c == '_' }
        }).collect();
        let length = word.chars().count();
        if (marker == '@' && (3..=30).contains(&length)) || (marker == '#' && (1..=40).contains(&length)) {
            out.insert(word.to_lowercase());
        }
        if out.len() >= 20 { break; }
    }
    out.into_iter().collect()
}

pub fn mention_recipients(r: &Records, who: &str, body: &str, scope: &str, id: &str) -> Vec<String> {
    let handles = tokens(body, '@');
    m::list(r, "account").into_iter().filter_map(|a| {
        let user = a["id"].as_str()?;
        if user == who || m::blocked(r, who, user) || !handles.iter().any(|h| a["handle"].as_str().map(|v| v.eq_ignore_ascii_case(h)).unwrap_or(false)) { return None; }
        let permitted = if scope == "room" {
            m::get(r, "room", id).map(|room| m::room_allowed(r, room, user)).unwrap_or(false)
        } else {
            m::get(r, "event", id).map(|e| super::completion::can_view(r, e, Some(user))).unwrap_or(false)
        };
        permitted.then(|| user.to_owned())
    }).collect()
}

pub fn notify_mentions(r: &mut Records, who: &str, body: &str, scope: &str, id: &str, talk: &str, exclude: &[String]) {
    let recipients = mention_recipients(r, who, body, scope, id);
    let name = m::get(r, if scope == "room" { "room" } else { "event" }, id)
        .map(|e| e["name"].clone()).unwrap_or(Value::Null);
    for person in recipients {
        if exclude.contains(&person) { continue; }
        m::push_alert(r, &person, json!({"type":"mention","event":if scope=="event"{id}else{""},"room":if scope=="room"{id}else{""},"name":name,"talk":talk,"body":body.chars().take(100).collect::<String>()}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokens_allow_japanese_and_reject_email_and_overlong_handles() {
        assert_eq!(tokens("#街歩き ＃音楽 #街歩き <script>", '#'), vec!["街歩き", "音楽"]);
        assert_eq!(tokens("@RyNat abc@example.com ＠guest @abcdefghijklmnopqrstuvwxyz123456789", '@'), vec!["guest", "rynat"]);
    }
    #[test]
    fn private_room_mentions_do_not_notify_outsiders_or_pending_members() {
        let mut r = Records::new();
        for user in ["owner", "member", "pending", "outsider"] {
            m::put(&mut r, "account", user, json!({"id":user,"handle":user}));
        }
        m::put(&mut r,"room","room",json!({"id":"room","members":["owner","member","pending"],"pending":["pending"]}));
        assert_eq!(mention_recipients(&r,"owner","@member @pending @outsider","room","room"),vec!["member"]);
        m::put(&mut r,"block:member","owner",json!(true));
        assert!(mention_recipients(&r,"owner","@member","room","room").is_empty());
    }
}
