//! Payload-free Web Push wake-ups. Event details stay in the authenticated inbox.
use super::{model as m,store::{Records,Result,Store}};
use base64::{Engine,engine::general_purpose::URL_SAFE_NO_PAD as B64};
use p256::ecdsa::{SigningKey,Signature,signature::Signer};
use serde_json::{Value,json};

pub fn endpoint_allowed(value:&str)->bool{
 let Ok(u)=reqwest::Url::parse(value)else{return false;};let host=u.host_str().unwrap_or("");
 u.scheme()=="https"&&u.port_or_known_default()==Some(443)&&u.username().is_empty()&&u.password().is_none()&&u.fragment().is_none()&&value.len()<2048&&
 (matches!(host,"fcm.googleapis.com"|"updates.push.services.mozilla.com"|"web.push.apple.com")||host.ends_with(".notify.windows.com"))
}
pub fn key(r:&mut Records)->Result<(String,String)>{
 if let Some(k)=m::get(r,"push_key","v1"){return Ok((k["private"].as_str().ok_or("通知鍵が不正です")?.into(),k["public"].as_str().ok_or("通知鍵が不正です")?.into()));}
 let k=SigningKey::random(&mut rand_core::OsRng);let private=B64.encode(k.to_bytes());let public=B64.encode(k.verifying_key().to_encoded_point(false).as_bytes());
 m::put(r,"push_key","v1",json!({"private":private,"public":public}));Ok((private,public))
}
pub fn authorization(private:&str,public:&str,endpoint:&str,origin:&str)->Result<String>{
 let url=reqwest::Url::parse(endpoint).map_err(|_|"通知先が不正です")?;
 let content=format!("{}.{}",B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#),B64.encode(json!({"aud":url.origin().ascii_serialization(),"exp":m::now()+3600,"sub":origin}).to_string()));
 let bytes=B64.decode(private).map_err(|_|"通知鍵が不正です")?;let key=SigningKey::from_slice(&bytes).map_err(|_|"通知鍵が不正です")?;
 let signature:Signature=key.sign(content.as_bytes());Ok(format!("vapid t={content}.{}, k={public}",B64.encode(signature.to_bytes())))
}
pub fn enqueue(r:&mut Records,who:&str,alert:&Value){
 let category=match alert["type"].as_str().unwrap_or(""){"event_change"|"booking_promoted"=>"changes","notice"=>"notices","message"=>"dm","talk"|"mention"=>if alert["room"].as_str().map(|s|!s.is_empty()).unwrap_or(false){"dm"}else{"talk"},_=>"dm"};
 let p=m::get(r,"preferences",who).cloned().unwrap_or(json!({}));if p[category]==false||p["frequency"]=="off"{return;}
 let now=m::now();let mut due=if p["frequency"]=="digest"{now+3600}else{now};
 let start=p["quiet_start"].as_i64().unwrap_or(22);let end=p["quiet_end"].as_i64().unwrap_or(8);let hour=(due/3600+9)%24;
 if (start<end&&hour>=start&&hour<end)||(start>end&&(hour>=start||hour<end)){due+=((end-hour+24)%24)*3600-due%3600;}
 let subs:Vec<_>=m::list(r,"push_subscription").into_iter().filter(|s|s["owner"]==who).cloned().collect();
 for sub in subs{let id=m::uid();m::put(r,"push_queue",&id,json!({"id":id,"subscription":sub["id"],"owner":who,"event":alert["event"],"alert":alert["id"],"category":category,"state":"pending","attempts":0,"due":due,"at":now}));}
}
pub async fn dispatch(store:&Store,origin:&str)->Result<usize>{
 let claim=store.transact(|r|{let key=key(r)?;let ids:Vec<_>=r.iter().filter(|((s,_),x)|s=="push_queue"&&(x["state"]=="pending"||(x["state"]=="sending"&&x["lease"].as_i64().unwrap_or(0)<m::now()))&&x["due"].as_i64().unwrap_or(0)<=m::now()).take(4).map(|((_,id),_)|id.clone()).collect();let mut work=vec![];for id in ids{let task=m::get(r,"push_queue",&id).cloned().unwrap();let prefs=m::get(r,"preferences",task["owner"].as_str().unwrap_or("")).cloned().unwrap_or(json!({}));let category=task["category"].as_str().unwrap_or("notices");let revoked=task["event"].as_str().filter(|e|!e.is_empty()).map(|id|m::get(r,"event",id).map(|e|!super::completion::can_view(r,e,task["owner"].as_str())).unwrap_or(true)).unwrap_or(false);if prefs["frequency"]=="off"||prefs[category]==false||revoked{r.get_mut(&("push_queue".into(),id.clone())).unwrap()["state"]=json!("canceled");continue;}let sub=m::get(r,"push_subscription",task["subscription"].as_str().unwrap_or("")).cloned();let x=r.get_mut(&("push_queue".into(),id.clone())).unwrap();x["state"]=json!("sending");x["lease"]=json!(m::now()+120);x["attempts"]=json!(x["attempts"].as_u64().unwrap_or(0)+1);work.push((id,task,sub));}Ok((key,work))}).await?;
 let ((private,public),work)=claim;let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(8)).build().map_err(|e|e.to_string())?;let count=work.len();
 for (id,task,sub) in work{let endpoint=sub.as_ref().and_then(|s|s["endpoint"].as_str()).unwrap_or("");let code=if endpoint_allowed(endpoint){match authorization(&private,&public,endpoint,origin){Ok(auth)=>client.post(endpoint).header("Authorization",auth).header("TTL","86400").header("Urgency","normal").header("Content-Length","0").send().await.map(|r|r.status().as_u16()).unwrap_or(0),Err(_)=>0}}else{410};
  store.transact(|r|{let attempts=m::get(r,"push_queue",&id).and_then(|x|x["attempts"].as_u64()).unwrap_or(1);let state=if (200..300).contains(&code){"accepted_by_push_service"}else if code==404||code==410{"expired_subscription"}else if attempts>=4{"failed"}else{"pending"};if let Some(x)=r.get_mut(&("push_queue".into(),id.clone())){x["state"]=json!(state);x["status_code"]=json!(code);x["due"]=json!(m::now()+60*(2_i64.pow(attempts as u32)));x["updated_at"]=json!(m::now());}if code==404||code==410{r.remove(&("push_subscription".into(),task["subscription"].as_str().unwrap_or("").into()));}m::put(r,"delivery",&format!("push:{id}"),json!({"id":id,"event":task["event"],"recipient":task["owner"],"channel":"web_push","status":state,"status_code":code,"attempts":attempts,"at":m::now()}));Ok(())}).await?;
 }Ok(count)
}
#[cfg(test)]mod tests{use super::*;use p256::ecdsa::{VerifyingKey,signature::Verifier};
 #[test]fn rejects_untrusted_endpoints(){for u in ["http://fcm.googleapis.com/x","https://fcm.googleapis.com.evil.test/x","https://127.0.0.1/x","https://fcm.googleapis.com:8443/x","https://name@fcm.googleapis.com/x"]{assert!(!endpoint_allowed(u));}assert!(endpoint_allowed("https://fcm.googleapis.com/fcm/send/token"));}
 #[test]fn signature_is_standard_es256(){let mut r=Records::new();let (private,public)=key(&mut r).unwrap();let a=authorization(&private,&public,"https://fcm.googleapis.com/test","https://example.com").unwrap();let jwt=a.trim_start_matches("vapid t=").split(',').next().unwrap();let p:Vec<_>=jwt.split('.').collect();let verify=VerifyingKey::from_sec1_bytes(&B64.decode(public).unwrap()).unwrap();verify.verify(format!("{}.{}",p[0],p[1]).as_bytes(),&Signature::from_slice(&B64.decode(p[2]).unwrap()).unwrap()).unwrap();let claims:Value=serde_json::from_slice(&B64.decode(p[1]).unwrap()).unwrap();assert_eq!(claims["aud"],"https://fcm.googleapis.com");}
}
