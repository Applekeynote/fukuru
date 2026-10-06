//! Event operations, collaborative records and personal controls.
use super::{model as m, store::{Records, Result}};
use serde_json::{json, Value};

fn text(v:&Value,key:&str,max:usize)->Result<String>{
    let s=v[key].as_str().unwrap_or("").trim();
    if s.chars().count()>max || s.contains('\0'){return Err(format!("{key} は{max}文字以内です"));}
    Ok(s.into())
}
pub(crate) fn ids(v:&Value)->Vec<String>{v.as_array().map(|a|a.iter().filter_map(|x|x.as_str().map(str::to_owned)).collect()).unwrap_or_default()}
pub fn can_edit(r:&Records,e:&Value,who:&str)->bool{
    e["deleted"]!=true && (e["owner"]==who || m::get(r,"event_extra",e["id"].as_str().unwrap_or("")).map(|x|ids(&x["cohosts"]).iter().any(|id|id==who)).unwrap_or(false))
}
pub fn can_view(r:&Records,e:&Value,who:Option<&str>)->bool{
    if e["deleted"]==true || e["moderation_hidden"]==true{return false;}
    if who.map(|u|m::blocked(r,u,e["owner"].as_str().unwrap_or(""))).unwrap_or(false){return false;}
    let id=e["id"].as_str().unwrap_or("");
    let extra=m::get(r,"event_extra",id);
    if extra.map(|x|x["visibility"]!="invite").unwrap_or(true){return true;}
    who.map(|u|can_edit(r,e,u)||extra.map(|x|ids(&x["invitees"]).iter().any(|id|id==u)).unwrap_or(false)).unwrap_or(false)
}
pub fn listed(r:&Records,e:&Value,who:Option<&str>)->bool{
    can_view(r,e,who) && (m::get(r,"event_extra",e["id"].as_str().unwrap_or("")).map(|x|x["visibility"]=="public"||x["visibility"].is_null()).unwrap_or(true)
      || who.map(|u|can_edit(r,e,u)||m::get(r,&format!("rsvp:{}",e["id"].as_str().unwrap_or("")),u).is_some()||m::get(r,"event_extra",e["id"].as_str().unwrap_or("")).map(|x|ids(&x["invitees"]).iter().any(|id|id==u)).unwrap_or(false)).unwrap_or(false))
}
pub fn extra_public(r:&Records,e:&Value,who:Option<&str>)->Value{
    let mut x=m::get(r,"event_extra",e["id"].as_str().unwrap_or("")).cloned().unwrap_or(json!({}));
    if let Some(o)=x.as_object_mut(){o.remove("invitees");o.remove("cohost_invites");}
    x["can_edit"]=json!(who.map(|u|can_edit(r,e,u)).unwrap_or(false)); x
}
fn checked_url(v:&Value,key:&str)->Result<String>{
    let s=text(v,key,500)?;
    if !s.is_empty(){let url=reqwest::Url::parse(&s).map_err(|_|"HTTPSのURLを入力してください")?;
        if url.scheme()!="https"||!url.username().is_empty()||url.password().is_some(){return Err("HTTPSのURLを入力してください".into());}}
    Ok(s)
}
fn ensure_event<'a>(r:&'a Records,id:&str,who:&str)->Result<&'a Value>{
    let e=m::get(r,"event",id).ok_or("イベントが見つかりません")?;
    if !can_view(r,e,Some(who)){return Err("このイベントを利用できません".into());} Ok(e)
}
fn participant(r:&Records,e:&Value,who:&str)->bool{
    can_edit(r,e,who)||m::get(r,&format!("rsvp:{}",e["id"].as_str().unwrap_or("")),who).map(|x|x["status"]=="going").unwrap_or(false)
}
pub fn workspace(r:&Records,id:&str,who:Option<&str>)->Result<Value>{
    let e=m::get(r,"event",id).ok_or("イベントが見つかりません")?;
    if !can_view(r,e,who){return Err("このイベントを閲覧できません".into());}
    let editor=who.map(|u|can_edit(r,e,u)).unwrap_or(false);
    let member=who.map(|u|participant(r,e,u)).unwrap_or(false);
    let mut entries:Vec<_>=m::list(r,&format!("collab:{id}")).into_iter().filter(|x|x["deleted"]!=true)
       .filter(|x|x["visibility"]=="public"||who.map(|u|x["author"]==u||(member&&ids(&x["editors"]).iter().any(|v|v==u))).unwrap_or(false)||(x["visibility"]=="members"&&member))
       .filter(|x|x["kind"]!="ar"||x["approved"]==true||editor||who.map(|u|x["author"]==u).unwrap_or(false))
       .filter(|x|who.map(|u|!m::blocked(r,u,x["author"].as_str().unwrap_or(""))).unwrap_or(true)).cloned().collect();
    for entry in &mut entries {entry["can_edit"]=json!(who.map(|u|member&&(entry["author"]==u||ids(&entry["editors"]).iter().any(|v|v==u))).unwrap_or(false));}
    entries.sort_by_key(|x|(x["order"].as_i64().unwrap_or(0),x["at"].as_i64().unwrap_or(0)));
    let extra=if editor {m::get(r,"event_extra",id).cloned().unwrap_or(json!({}))}else{extra_public(r,e,who)};
    let delivery=if editor {m::list(r,"delivery").into_iter().filter(|x|x["event"]==id).cloned().collect::<Vec<_>>()}else{vec![]};
    Ok(json!({"event":id,"can_edit":editor,"can_contribute":member,"extra":extra,"entries":entries,"delivery":delivery,"collaborators":m::list(r,"account").into_iter().filter(|a|who.is_some()&&member&&participant(r,e,a["id"].as_str().unwrap_or(""))&&who.map(|u|!m::blocked(r,u,a["id"].as_str().unwrap_or(""))).unwrap_or(false)).map(|a|json!({"id":a["id"],"name":a["name"]})).collect::<Vec<_>>(),"note":who.and_then(|u|m::get(r,&format!("note:{u}"),id)).cloned().unwrap_or(Value::Null)}))
}
pub fn personal(r:&Records,who:&str,session:&str)->Value{
    let current=m::hash(session);
    let sessions:Vec<_>=r.iter().filter(|((s,_),v)|s=="session"&&v["account"]==who&&v["expires"].as_i64().unwrap_or(0)>m::now())
      .map(|((_,k),v)|json!({"id":m::hash(&format!("revoke:{k}")),"current":k==&current,"created":v["created"],"expires":v["expires"],"device":v["label"]})).collect();
    let invites:Vec<_>=m::list(r,"event_extra").into_iter().filter(|x|ids(&x["cohost_invites"]).iter().any(|u|u==who)).map(|x|json!({"id":x["id"],"name":m::get(r,"event",x["id"].as_str().unwrap_or("")).map(|e|e["name"].clone())})).collect();
    json!({"sessions":sessions,"searches":m::list(r,&format!("search:{who}")),"notes":m::list(r,&format!("note:{who}")),"settings":m::get(r,"preferences",who),"cases":m::list(r,"report").into_iter().filter(|x|x["reporter"]==who).collect::<Vec<_>>(),"cohost_invites":invites})
}
pub fn operate(r:&mut Records,who:&str,op:&str,v:&Value)->Option<Result<Value>>{
    if !op.starts_with("complete_"){return None;} Some(run(r,who,op,v))
}
fn run(r:&mut Records,who:&str,op:&str,v:&Value)->Result<Value>{
    let id=v["id"].as_str().unwrap_or("");let mut out=json!({"ok":true});
    match op {
        "complete_service_status"=>{if !m::moderator(who){return Err("運営権限が必要です".into());}let message=text(v,"message",1000)?;if message.is_empty(){return Err("告知内容を入力してください".into());}m::put(r,"service_status","current",json!({"message":message,"incident":v["incident"]==true,"at":m::now(),"actor":who}));}
        "complete_event_cover"=>{
            let e=ensure_event(r,id,who)?.clone();
            if !can_edit(r,&e,who){return Err("作成者または承認済みの共同編集者だけが画像を変更できます".into());}
            let mut x=m::get(r,"event_extra",id).cloned().unwrap_or(json!({"id":id}));
            let cover=if v.get("cover").is_some(){text(v,"cover",200)?}else{x["cover"].as_str().unwrap_or("").to_owned()};let alt=text(v,"cover_alt",300)?;
            if !cover.is_empty(){
                let mid=cover.strip_prefix("/media/").ok_or("写真をアップロードしてください")?;
                if v.get("cover").is_some() && m::get(r,"media",mid).map(|a|a["owner"]!=who).unwrap_or(true){return Err("写真を利用できません".into());}
                if alt.is_empty(){return Err("画像の説明を入力してください".into());}
            }
            x["cover"]=json!(cover);x["cover_alt"]=json!(alt);x["updated_at"]=json!(m::now());m::put(r,"event_extra",id,x);
        }
        "complete_event_options"=>{
            let e=ensure_event(r,id,who)?.clone();if !can_edit(r,&e,who){return Err("主催者だけが設定できます".into());}
            let old=m::get(r,"event_extra",id).cloned().unwrap_or(json!({}));let mut x=old.clone();x["id"]=json!(id);
            for key in ["weather_policy","weather_decision","alternative","contact","emergency_contact","access","languages","clothing","recruitment","preparation","official_hours","official_conditions"] {if v.get(key).is_some(){x[key]=json!(text(v,key,1000)?);}}
            for key in ["online_url","official_url","audio_url"]{if v.get(key).is_some(){x[key]=json!(checked_url(v,key)?);}}
            let mode=v["mode"].as_str().unwrap_or("onsite");if !["onsite","online","hybrid"].contains(&mode){return Err("開催方法が不正です".into());}
            if mode!="onsite" && x["online_url"].as_str().unwrap_or("").is_empty(){return Err("オンライン参加URLを入力してください".into());}x["mode"]=json!(mode);
            x["duration_minutes"]=json!(v["duration_minutes"].as_u64().unwrap_or(60).clamp(5,10080));x["late_join"]=json!(v["late_join"]==true);
            x["crowding"]=json!(match v["crowding"].as_str().unwrap_or("unknown"){s@("unknown"|"quiet"|"moderate"|"busy")=>s,_=>return Err("混雑度が不正です".into())});x["crowding_at"]=json!(m::now());
            if e["owner"]==who {
                let vis=v["visibility"].as_str().unwrap_or("public");if !["public","unlisted","invite"].contains(&vis){return Err("公開範囲が不正です".into());}x["visibility"]=json!(vis);
                let invited=ids(v.get("invitees").unwrap_or(&old["invitees"]));let cohosts=if let Some(x)=v.get("cohosts"){ids(x)}else{let mut all=ids(&old["cohosts"]);all.extend(ids(&old["cohost_invites"]));all};
                if invited.len()>100||cohosts.len()>8{return Err("招待は100人、共同主催は8人までです".into());}
                for u in invited.iter().chain(cohosts.iter()){if m::get(r,"account",u).is_none()||m::blocked(r,who,u){return Err("招待できないアカウントです".into());}}
                for u in &cohosts {if u!=who && !ids(&old["cohosts"]).contains(u) && !ids(&old["cohost_invites"]).contains(u){m::push_alert(r,u,json!({"type":"cohost_invite","invitation_event":id,"name":e["name"],"body":"共同編集に招待されました"}));}}
                x["invitees"]=json!(invited);let accepted=ids(&old["cohosts"]);x["cohosts"]=json!(cohosts.iter().filter(|u|accepted.contains(u)).collect::<Vec<_>>());x["cohost_invites"]=json!(cohosts.iter().filter(|u|!accepted.contains(u)&&u.as_str()!=who).collect::<Vec<_>>());
            }
            if let Some(cover)=v["cover"].as_str(){if !cover.is_empty(){let mid=cover.strip_prefix("/media/").ok_or("写真をアップロードしてください")?;if m::get(r,"media",mid).map(|a|a["owner"]!=who).unwrap_or(true){return Err("写真を利用できません".into());}}x["cover"]=json!(cover);x["cover_alt"]=json!(text(v,"cover_alt",300)?);if !cover.is_empty()&&x["cover_alt"]==""{return Err("カバー画像の説明を入力してください".into());}}
            if v.get("cover_alt").is_some()&&v.get("cover").is_none(){x["cover_alt"]=json!(text(v,"cover_alt",300)?);if x["cover"].as_str().map(|s|!s.is_empty()).unwrap_or(false)&&x["cover_alt"]==""{return Err("カバー画像の説明を入力してください".into());}}
            x["updated_at"]=json!(m::now());m::put(r,"event_extra",id,x.clone());
            if old!=x {let cid=m::uid();m::put(r,&format!("event_change:{id}"),&cid,json!({"id":cid,"at":m::now(),"changed":["開催方法・参加案内"],"before":{"開催方法・参加案内":"更新前"},"after":{"開催方法・参加案内":"更新済み"},"actor":who}));
                let recipients:Vec<_>=r.keys().filter(|(s,_)|s==&format!("rsvp:{id}")).map(|(_,u)|u.clone()).collect();for u in recipients{m::push_alert(r,&u,json!({"type":"notice","event":id,"name":e["name"],"body":"開催方法・参加案内が更新されました。詳細をご確認ください。"}));}}
        }
        "complete_cohost_reply"=>{let e=m::get(r,"event",id).ok_or("イベントがありません")?;if m::blocked(r,who,e["owner"].as_str().unwrap_or("")){return Err("招待を受諾できません".into());}let mut x=m::get(r,"event_extra",id).cloned().ok_or("招待がありません")?;let mut pending=ids(&x["cohost_invites"]);if !pending.iter().any(|u|u==who){return Err("招待がありません".into());}pending.retain(|u|u!=who);x["cohost_invites"]=json!(pending);if v["accept"]==true{let mut accepted=ids(&x["cohosts"]);accepted.push(who.into());x["cohosts"]=json!(accepted);}m::put(r,"event_extra",id,x);}
        "complete_duplicate"=>{
            let e=ensure_event(r,id,who)?.clone();if !can_edit(r,&e,who){return Err("主催者だけが複製できます".into());}
            let dates=v["dates"].as_array().ok_or("開催日を入力してください")?;if dates.is_empty()||dates.len()>12{return Err("開催日は1〜12日です".into());}
            let start=chrono::DateTime::parse_from_rfc3339(e["start"].as_str().unwrap_or("")).map_err(|_|"日時が不正です")?;let end=chrono::DateTime::parse_from_rfc3339(e["end"].as_str().unwrap_or("")).map_err(|_|"日時が不正です")?;
            let mut created=vec![];for date in dates{let at=chrono::DateTime::parse_from_rfc3339(date.as_str().unwrap_or("")).map_err(|_|"日時が不正です")?;if at.timestamp()<=m::now(){return Err("次回の開始日時を指定してください".into());}
                let mut copy=e.clone();copy["id"]=json!("");copy["start"]=json!(at.to_rfc3339());copy["end"]=json!((at+(end-start)).to_rfc3339());copy["status"]=json!("active");copy["cancel_reason"]=json!("");copy["booking_deadline"]=Value::Null;copy["location_confirmed"]=json!(true);copy["draft_id"]=json!("");
                let mut options=m::get(r,"event_extra",id).cloned().unwrap_or(json!({}));
                for key in ["id","cohosts","cohost_invites","invitees","cover","cover_alt"]{if let Some(o)=options.as_object_mut(){o.remove(key);}}
                copy["completion_options"]=options;
                let draft=m::uid();m::put(r,&format!("draft:{who}"),&draft,json!({"id":draft,"data":copy,"at":m::now()}));created.push(draft);}
            out["drafts"]=json!(created);
        }
        "complete_collab"=>{
            let e=ensure_event(r,id,who)?.clone();if !participant(r,&e,who){return Err("主催者またはJoinした参加者が編集できます".into());}
            let key=v["item_id"].as_str().filter(|x|!x.is_empty()).map(str::to_owned).unwrap_or_else(m::uid);let scope=format!("collab:{id}");let old=m::get(r,&scope,&key).cloned();
            if old.as_ref().map(|x|x["deleted"]==true).unwrap_or(false){return Err("削除済みの記録は編集できません".into());}
            let is_author=old.as_ref().map(|x|x["author"]==who).unwrap_or(true);
            if !is_author && !old.as_ref().map(|x|ids(&x["editors"]).iter().any(|u|u==who)).unwrap_or(false){return Err("この記録の共同編集は許可されていません".into());}
            if v["deleted"]==true && !is_author{return Err("記録の削除は作成者だけが行えます".into());}
            if v["deleted"]==true{let x=r.get_mut(&(scope,key)).ok_or("記録がありません")?;x["deleted"]=json!(true);return Ok(out);}
            let kind=v["kind"].as_str().unwrap_or("record");if !["route","guide","ar","record","access","recruitment","memory"].contains(&kind){return Err("記録の種類が不正です".into());}
            if !is_author && old.as_ref().map(|x|x["kind"]!=kind).unwrap_or(false){return Err("種類の変更は作成者だけが行えます".into());}
            let visibility=v["visibility"].as_str().unwrap_or("members");if !["private","members","public"].contains(&visibility){return Err("公開範囲が不正です".into());}
            // Other people's consent and visibility cannot be widened by an editor.
            if old.as_ref().map(|x|x["author"]!=who&&(x["visibility"]!=visibility||x["summary_consent"]!=(v["summary_consent"]==true))).unwrap_or(false){return Err("公開範囲とAI同意は投稿者本人が変更してください".into());}
            let title=text(v,"title",120)?;let body=text(v,"body",3000)?;if title.is_empty(){return Err("見出しを入力してください".into());}
            let lat=v["lat"].as_f64();let lon=v["lon"].as_f64();if lat.is_some()!=lon.is_some()||lat.map(|n|!n.is_finite()||n.abs()>90.).unwrap_or(false)||lon.map(|n|!n.is_finite()||n.abs()>180.).unwrap_or(false){return Err("地点が不正です".into());}
            if kind=="ar"&&v["stationary_safe"]!=true{return Err("立ち止まれる安全な設置地点を確認してください".into());}
            let photo=text(v,"photo",100)?;if !photo.is_empty(){let mid=photo.strip_prefix("/media/").ok_or("写真が不正です")?;if m::get(r,"media",mid).map(|x|x["owner"]!=who && old.as_ref().map(|o|o["photo"]!=photo).unwrap_or(true)).unwrap_or(true){return Err("写真を利用できません".into());}}
            let alt=text(v,"photo_alt",300)?;if !photo.is_empty()&&alt.is_empty(){return Err("写真の説明を入力してください".into());}
            let editors=if is_author {ids(v.get("editors").unwrap_or(&old.as_ref().map(|x|x["editors"].clone()).unwrap_or(Value::Null)))} else {ids(&old.as_ref().unwrap()["editors"])};
            if editors.len()>8{return Err("共同編集者は8人までです".into());}
            for u in &editors{if m::blocked(r,who,u)||!participant(r,&e,u){return Err("共同編集者は参加者から選んでください".into());}}
            let author=old.as_ref().and_then(|x|x["author"].as_str()).unwrap_or(who);let x=json!({"id":key,"event":id,"kind":kind,"title":title,"body":body,"lat":lat.map(|n|(n*1000.).round()/1000.),"lon":lon.map(|n|(n*1000.).round()/1000.),"photo":photo,"photo_alt":alt,"audio_url":checked_url(v,"audio_url")?,"visibility":visibility,"author":author,"editors":editors,"summary_consent":v["summary_consent"]==true,"approved":false,"stationary_safe":v["stationary_safe"]==true,"order":v["order"].as_i64().unwrap_or(0).clamp(0,1000),"at":old.as_ref().and_then(|x|x["at"].as_i64()).unwrap_or_else(m::now),"updated_at":m::now(),"version":old.as_ref().and_then(|x|x["version"].as_u64()).unwrap_or(0)+1});
            if old.is_none()&&m::list(r,&scope).len()>=200{return Err("共同記録は200件までです".into());}m::put(r,&scope,&key,x);out["id"]=json!(key);
        }
        "complete_ar_review"=>{if !m::moderator(who){return Err("審査権限がありません".into());}let event=text(v,"event_id",100)?;let key=(format!("collab:{event}"),id.into());let x=r.get_mut(&key).ok_or("作品がありません")?;if x["author"]==who||x["kind"]!="ar"{return Err("本人の作品は審査できません".into());}x["review_note"]=json!(text(v,"review_note",1000)?);if x["review_note"].as_str().unwrap_or("").chars().count()<20{return Err("安全な設置場所を確認した根拠を20文字以上で記録してください".into());}x["approved"]=json!(v["approved"]==true);x["reviewer"]=json!(who);}
        "complete_note"=>{let e=ensure_event(r,id,who)?;let _=e;let body=text(v,"body",4000)?;m::put(r,&format!("note:{who}"),id,json!({"id":id,"body":body,"at":m::now()}));}
        "complete_search"=>{let scope=format!("search:{who}");if v["remove"]==true{r.remove(&(scope,id.into()));}else{if m::list(r,&scope).len()>=20{return Err("検索保存は20件までです".into());}let key=m::uid();let query=v["query"].clone();if query.to_string().len()>4000{return Err("検索条件が長すぎます".into());}m::put(r,&scope,&key,json!({"id":key,"name":text(v,"name",80)?,"query":query,"at":m::now()}));}}
        "complete_preferences"=>{let frequency=v["frequency"].as_str().unwrap_or("instant");if !["instant","digest","off"].contains(&frequency){return Err("通知頻度が不正です".into());}m::put(r,"preferences",who,json!({"frequency":frequency,"quiet_start":v["quiet_start"].as_u64().unwrap_or(22).min(23),"quiet_end":v["quiet_end"].as_u64().unwrap_or(8).min(23),"changes":v["changes"]!=false,"notices":v["notices"]!=false,"dm":v["dm"]!=false,"talk":v["talk"]!=false,"ai_translation":v["ai_translation"]!=false,"ai_summary":v["ai_summary"]!=false,"ai_recommendation":v["ai_recommendation"]!=false}));}
        "complete_contact"=>{let kind=v["kind"].as_str().unwrap_or("support");if !["support","bug","privacy","appeal"].contains(&kind){return Err("問い合わせ種別が不正です".into());}let detail=text(v,"detail",2000)?;if detail.len()<10{return Err("問い合わせ内容を入力してください".into());}let key=m::uid();m::put(r,"report",&key,json!({"id":key,"reporter":who,"kind":kind,"target":id,"detail":detail,"status":"open","at":m::now()}));out["id"]=json!(key);}
        "complete_talk"=>{
            let found=r.iter().find(|((s,k),_)|k==id&&(s.starts_with("post:")||s.starts_with("message:"))).map(|((s,_),x)|(s.clone(),x.clone())).ok_or("発言がありません")?;let(scope,old)=found;
            if let Some(eid)=scope.strip_prefix("post:"){ensure_event(r,eid,who)?;}
            if let Some(rid)=scope.strip_prefix("message:"){if !m::get(r,"room",rid).map(|x|m::room_allowed(r,x,who)).unwrap_or(false){return Err("この会話を編集できません".into());}}
            if old["deleted"]==true||old["moderation_hidden"]==true{return Err("この発言は変更できません".into());}
            if v.get("pinned").is_some(){let owner=if let Some(eid)=scope.strip_prefix("post:"){m::get(r,"event",eid).map(|e|can_edit(r,e,who)).unwrap_or(false)}else{m::get(r,"room",scope.trim_start_matches("message:")).map(|x|x["owner"]==who).unwrap_or(false)};if !owner{return Err("固定は主催者・会話作成者が行えます".into());}r.get_mut(&(scope,id.into())).unwrap()["pinned"]=json!(v["pinned"]==true);}else{
                if old["author"]!=who{return Err("自分の投稿だけ編集できます".into());}let body=text(v,"body",2000)?;if body.is_empty()&&old["photo"].as_str().unwrap_or("").is_empty(){return Err("本文または写真が必要です".into());}let x=r.get_mut(&(scope,id.into())).unwrap();x["tags"]=json!(super::social::tokens(&body,'#'));x["mentions"]=json!(super::social::tokens(&body,'@'));x["body"]=json!(body);x["photo_alt"]=json!(text(v,"photo_alt",300)?);x["share_allowed"]=json!(v["share_allowed"]==true);x["summary_consent"]=json!(v["summary_consent"]==true);x["edited_at"]=json!(m::now());}
        }
        "complete_room"=>{
            let room=m::get(r,"room",id).ok_or("会話がありません")?.clone();if room["owner"]!=who{return Err("会話作成者だけが管理できます".into());}
            if let Some(active)=v["archived"].as_bool(){r.get_mut(&("room".into(),id.into())).unwrap()["archived"]=json!(active);}else{
                if room["kind"]!="group"{return Err("グループ会話だけメンバーを変更できます".into());}let person=text(v,"person",100)?;if person==who{return Err("作成者は退出操作を使ってください".into());}let mut members=ids(&room["members"]);let mut pending=ids(&room["pending"]);
                if v["remove"]==true{members.retain(|u|u!=&person);pending.retain(|u|u!=&person);}else{if members.len()>=9||m::get(r,"account",&person).is_none()||m::blocked(r,who,&person){return Err("このメンバーを招待できません".into());}if !members.contains(&person){members.push(person.clone());pending.push(person.clone());}}
                let x=r.get_mut(&("room".into(),id.into())).unwrap();x["members"]=json!(members);x["pending"]=json!(pending);let mut history=x["history"].as_array().cloned().unwrap_or_default();history.push(json!({"person":person,"removed":v["remove"]==true,"at":m::now()}));x["history"]=json!(history.into_iter().rev().take(50).collect::<Vec<_>>());
            }
        }
        "complete_support_review"=>{if !m::moderator(who){return Err("審査権限がありません".into());}let note=text(v,"note",2000)?;if note.chars().count()<10{return Err("対応記録を10文字以上で入力してください".into());}let x=r.get_mut(&("report".into(),id.into())).ok_or("問い合わせがありません")?;x["review_note"]=json!(note);x["status"]=json!(if v["closed"]==true{"closed"}else{"reviewing"});x["reviewed_at"]=json!(m::now());x["reviewer"]=json!(who);}
        _=>return Err("未対応の操作です".into())
    }
    let audit=m::uid();m::put(r,"social_audit",&audit,json!({"actor":who,"op":op,"target":id,"at":m::now()}));Ok(out)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn explicit_edit_permission_can_be_revoked_and_never_widens_consent(){
        let mut r=records();for u in ["b","c"]{m::put(&mut r,"rsvp:e",u,json!({"status":"going"}));}
        let item=run(&mut r,"b","complete_collab",&json!({"id":"e","kind":"record","title":"Original","body":"Private","visibility":"private","editors":["a"],"summary_consent":false})).unwrap()["id"].as_str().unwrap().to_owned();
        let view=workspace(&r,"e",Some("a")).unwrap();assert_eq!(view["entries"][0]["can_edit"],true);
        assert_eq!(workspace(&r,"e",Some("c")).unwrap()["entries"].as_array().unwrap().len(),0);
        assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"kind":"record","title":"Edited","body":"Authorized","visibility":"private","summary_consent":false})).is_ok());
        assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"kind":"record","title":"Leak","visibility":"public"})).is_err());
        assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"kind":"record","title":"Consent","visibility":"private","summary_consent":true})).is_err());
        assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"deleted":true})).is_err());
        assert!(run(&mut r,"b","complete_collab",&json!({"id":"e","item_id":item,"kind":"record","title":"Revoked","visibility":"private","editors":[]})).is_ok());
        assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"kind":"record","title":"Denied","visibility":"private"})).is_err());
        assert_eq!(workspace(&r,"e",Some("a")).unwrap()["entries"].as_array().unwrap().len(),0);
    }
    #[test]fn grant_rejects_outsiders_and_deleted_records(){
        let mut r=records();assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","kind":"record","title":"Bad grant","editors":["c"]})).is_err());
        let item=run(&mut r,"a","complete_collab",&json!({"id":"e","kind":"record","title":"Record"})).unwrap()["id"].as_str().unwrap().to_owned();
        assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"deleted":true})).is_ok());
        assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"kind":"record","title":"Resurrect"})).is_err());
    }
    fn records()->Records{let mut r=Records::new();for id in ["a","b","c"]{m::put(&mut r,"account",id,json!({"id":id,"name":id}));}m::put(&mut r,"event","e",json!({"id":"e","owner":"a"}));r}
    #[test]fn event_delete_requires_owner_or_accepted_current_cohost(){
        let mut r=records();m::put(&mut r,"event_extra","e",json!({"cohost_invites":["b"]}));
        assert!(m::operate(&mut r,"b","delete_event",&json!({"id":"e"})).is_err());
        assert!(m::operate(&mut r,"c","delete_event",&json!({"id":"e"})).is_err());
        run(&mut r,"b","complete_cohost_reply",&json!({"id":"e","accept":true})).unwrap();
        let mut revoked=r.clone();revoked.get_mut(&("event_extra".into(),"e".into())).unwrap()["cohosts"]=json!([]);
        assert!(m::operate(&mut revoked,"b","delete_event",&json!({"id":"e"})).is_err());
        m::put(&mut r,"rsvp:e","c",json!({"status":"going"}));m::put(&mut r,"room","event-e",json!({"id":"event-e"}));
        m::operate(&mut r,"b","delete_event",&json!({"id":"e"})).unwrap();
        assert_eq!(m::get(&r,"event","e").unwrap()["deleted"],true);assert_eq!(m::get(&r,"room","event-e").unwrap()["archived"],true);
        assert!(m::list(&r,"alert:c").iter().any(|a|a["deleted_event"]=="e"));
        assert!(!can_view(&r,m::get(&r,"event","e").unwrap(),Some("a")));
        assert!(m::operate(&mut r,"a","delete_event",&json!({"id":"e"})).is_err());
    }
    #[test]fn image_update_preserves_event_settings_and_checks_media_ownership(){
        let mut r=records();m::put(&mut r,"event_extra","e",json!({"cohosts":["b"],"visibility":"invite","invitees":["c"],"mode":"hybrid","duration_minutes":125,"cover":"/media/a-photo","cover_alt":"Original"}));
        m::put(&mut r,"media","a-photo",json!({"owner":"a"}));m::put(&mut r,"media","b-photo",json!({"owner":"b"}));
        assert!(run(&mut r,"c","complete_event_cover",&json!({"id":"e","cover":"/media/b-photo","cover_alt":"Denied"})).is_err());
        assert!(run(&mut r,"b","complete_event_cover",&json!({"id":"e","cover":"/media/a-photo","cover_alt":"Denied"})).is_err());
        run(&mut r,"b","complete_event_cover",&json!({"id":"e","cover_alt":"Revised description"})).unwrap();
        assert_eq!(m::get(&r,"event_extra","e").unwrap()["cover"],"/media/a-photo");
        run(&mut r,"b","complete_event_cover",&json!({"id":"e","cover":"/media/b-photo","cover_alt":"My image"})).unwrap();
        let x=m::get(&r,"event_extra","e").unwrap();assert_eq!(x["visibility"],"invite");assert_eq!(x["duration_minutes"],125);assert_eq!(x["mode"],"hybrid");assert_eq!(x["invitees"],json!(["c"]));assert_eq!(x["cohosts"],json!(["b"]));
        run(&mut r,"b","complete_event_cover",&json!({"id":"e","cover":"","cover_alt":""})).unwrap();assert_eq!(m::get(&r,"event_extra","e").unwrap()["cover"],"");
    }
    #[test]fn invitation_is_enforced_everywhere(){let mut r=records();m::put(&mut r,"event_extra","e",json!({"visibility":"invite","invitees":["b"]}));let e=m::get(&r,"event","e").unwrap();assert!(!can_view(&r,e,None));assert!(can_view(&r,e,Some("b")));assert!(!can_view(&r,e,Some("c")));assert!(workspace(&r,"e",Some("c")).is_err());}
    #[test]fn pending_cohost_has_no_edit_access(){let mut r=records();m::put(&mut r,"event_extra","e",json!({"cohost_invites":["b"]}));assert!(!can_edit(&r,m::get(&r,"event","e").unwrap(),"b"));assert!(run(&mut r,"b","complete_cohost_reply",&json!({"id":"e","accept":true})).is_ok());assert!(can_edit(&r,m::get(&r,"event","e").unwrap(),"b"));}
    #[test]fn private_collaboration_is_not_disclosed(){let mut r=records();let x=run(&mut r,"a","complete_collab",&json!({"id":"e","kind":"memory","title":"Private","visibility":"private","lat":35.123456,"lon":139.123456})).unwrap();assert_eq!(workspace(&r,"e",Some("b")).unwrap()["entries"].as_array().unwrap().len(),0);let data=workspace(&r,"e",Some("a")).unwrap();assert_eq!(data["entries"][0]["lat"],35.123);assert!(run(&mut r,"b","complete_collab",&json!({"id":"e","item_id":x["id"],"deleted":true})).is_err());}
    #[test]fn organizer_cannot_rewrite_participant_record(){let mut r=records();m::put(&mut r,"rsvp:e","b",json!({"status":"going"}));for visibility in ["public","members"]{let created=run(&mut r,"b","complete_collab",&json!({"id":"e","kind":"route","title":"Original","body":"Participant route","visibility":visibility})).unwrap();let item=created["id"].as_str().unwrap();assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"kind":"route","title":"Forged","body":"Organizer text","visibility":visibility})).is_err());assert!(run(&mut r,"a","complete_collab",&json!({"id":"e","item_id":item,"deleted":true})).is_err());assert_eq!(m::get(&r,"collab:e",item).unwrap()["body"],"Participant route");assert!(run(&mut r,"b","complete_collab",&json!({"id":"e","item_id":item,"kind":"route","title":"Corrected","body":"Participant correction","visibility":visibility})).is_ok());}}
    #[test]fn actor_cannot_change_others_talk(){let mut r=records();m::put(&mut r,"post:e","p",json!({"author":"a","body":"original"}));assert!(run(&mut r,"b","complete_talk",&json!({"id":"p","body":"forged"})).is_err());assert!(run(&mut r,"a","complete_talk",&json!({"id":"p","body":"corrected"})).is_ok());assert_eq!(m::get(&r,"post:e","p").unwrap()["body"],"corrected");}
}
