// Google ID tokens are verified locally against Google's public RSA keys.
// A one-use nonce binds every sign-in/link attempt to this browser's CSRF cookie.
fn google_client_id() -> String { std::env::var("GOOGLE_SIGNIN_CLIENT_ID").or_else(|_|std::env::var("GOOGLE_CALENDAR_CLIENT_ID")).unwrap_or_default() }
fn google_claims_valid(v:&Value, client:&str, nonce:&str, now:i64)->bool {
    ["https://accounts.google.com","accounts.google.com"].contains(&v["iss"].as_str().unwrap_or(""))
    && v["aud"].as_str()==Some(client) && !client.is_empty()
    && v["exp"].as_i64().map(|n|n>now).unwrap_or(false)
    && v["iat"].as_i64().map(|n|n<=now+60 && n>=now-3600).unwrap_or(false)
    && v["nonce"].as_str()==Some(nonce) && nonce.len()==64
    && v["sub"].as_str().map(|s|!s.is_empty()&&s.len()<=255).unwrap_or(false)
    && v.get("azp").map(|s|s.as_str()==Some(client)).unwrap_or(true)
}
async fn google_nonce(State(a):State<App>,h:HeaderMap,Json(v):Json<Value>)->Response {
    if !csrf(&a,&h,&v){return StatusCode::FORBIDDEN.into_response();}
    if google_client_id().is_empty(){return error("Googleログインの接続設定がありません");}
    if limited(&a,"google-challenge".into(),120,60).await{return StatusCode::TOO_MANY_REQUESTS.into_response();}
    let link=if v["mode"]=="link" {match who(&a,&h).await{Ok(id)=>id,Err(e)=>return error(e)}} else{String::new()};
    let nonce=secret();let key=m::hash(&cookie(&h,"spatial_csrf"));
    match a.store.transact(|r|{r.retain(|(s,_),v|s!="google_nonce"||v["expires"].as_i64().unwrap_or(0)>m::now());m::put(r,"google_nonce",&key,json!({"nonce":nonce,"link":link,"expires":m::now()+300}));Ok(())}).await {
        Ok(())=>Json(json!({"nonce":nonce,"client_id":google_client_id()})).into_response(),Err(e)=>error(e)
    }
}
async fn google_auth(State(a):State<App>,h:HeaderMap,Json(v):Json<Value>)->Response {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
    if !csrf(&a,&h,&v){return StatusCode::FORBIDDEN.into_response();}
    if limited(&a,"google-auth".into(),60,60).await{return StatusCode::TOO_MANY_REQUESTS.into_response();}
    let credential=v["credential"].as_str().unwrap_or("");if credential.len()>16384{return error("Googleの認証をやり直してください");}
    let parts:Vec<_>=credential.split('.').collect();if parts.len()!=3{return error("Googleの認証をやり直してください");}
    let decode=|s:&str|B64.decode(s).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok());
    let head=match decode(parts[0]){Some(v)=>v,None=>return error("Googleの認証をやり直してください")};
    let claims=match decode(parts[1]){Some(v)=>v,None=>return error("Googleの認証をやり直してください")};
    if head["alg"]!="RS256"{return error("Googleの認証をやり直してください");}
    let key=m::hash(&cookie(&h,"spatial_csrf"));
    let challenge=match a.store.transact(|r|Ok(m::get(r,"google_nonce",&key).cloned())).await{Ok(Some(v)) if v["expires"].as_i64().unwrap_or(0)>m::now()=>v,_=>return error("認証の期限が切れました。もう一度ログインしてください")};
    if !google_claims_valid(&claims,&google_client_id(),challenge["nonce"].as_str().unwrap_or(""),m::now()){return error("Googleの認証をやり直してください");}
    let keys={let mut cache=a.google_keys.lock().await;if cache.as_ref().map(|(at,_)|m::now()-*at>3600||(m::now()-*at>60&&!cache.as_ref().unwrap().1["keys"].as_array().map(|ks|ks.iter().any(|k|k["kid"]==head["kid"])).unwrap_or(false))).unwrap_or(true){let response=match a.client.get("https://www.googleapis.com/oauth2/v3/certs").send().await{Ok(v) if v.status().is_success()=>v,_=>return error("Googleの本人確認を利用できません")};let data=match response.json::<Value>().await{Ok(v)=>v,Err(_)=>return error("Googleの本人確認を利用できません")};*cache=Some((m::now(),data));}cache.as_ref().unwrap().1.clone()};
    let jwk=match keys["keys"].as_array().and_then(|ks|ks.iter().find(|k|k["kid"]==head["kid"]&&k["kty"]=="RSA"&&k["alg"]=="RS256")){Some(k)=>k,None=>return error("Googleの認証をやり直してください")};
    let n=B64.decode(jwk["n"].as_str().unwrap_or(""));let e=B64.decode(jwk["e"].as_str().unwrap_or(""));let signature=B64.decode(parts[2]);
    let verified=match(n,e,signature){(Ok(n),Ok(e),Ok(sig))=>ring::signature::RsaPublicKeyComponents{n:&n,e:&e}.verify(&ring::signature::RSA_PKCS1_2048_8192_SHA256,format!("{}.{}",parts[0],parts[1]).as_bytes(),&sig).is_ok(),_=>false};
    if !verified{return error("Googleの認証をやり直してください");}
    let subject=m::hash(claims["sub"].as_str().unwrap());let session=secret();let current=who(&a,&h).await.ok();
    let locale=claims["locale"].as_str().filter(|s|s.len()<=35&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')).unwrap_or("").to_owned();
    let result=a.store.transact(|r|{
        let stored=m::get(r,"google_nonce",&key).ok_or("認証は使用済みです")?;
        if stored!=&challenge||stored["expires"].as_i64().unwrap_or(0)<=m::now(){return Err("認証の期限が切れました".into());}
        let linked=challenge["link"].as_str().unwrap_or("");let known=m::get(r,"google_identity",&subject).and_then(|x|x["account"].as_str()).map(str::to_owned);
        let id=if !linked.is_empty(){
            if current.as_deref()!=Some(linked){return Err("連携するアカウントを確認してください".into());}
            if known.as_deref().map(|id|id!=linked).unwrap_or(false){return Err("このGoogleアカウントは別のSpatialアカウントに連携済みです".into());}
            if r.iter().any(|((s,k),v)|s=="google_identity"&&v["account"]==linked&&k!=&subject){return Err("別のGoogleアカウントが連携済みです".into());}
            linked.to_owned()
        }else if let Some(id)=known {id} else {
            let handle=format!("owl_{}",&m::uid().replace('-',"")[..16]);let name=claims["name"].as_str().unwrap_or("フクロウ").chars().take(60).collect::<String>();
            m::register(r,&json!({"handle":handle,"name":if name.trim().is_empty(){"フクロウ"}else{&name}}),"",&session)?
        };
        if m::get(r,"account",&id).map(|x|x["disabled"]==true).unwrap_or(true){return Err("このアカウントではログインできません".into());}
        r.remove(&("google_nonce".into(),key.clone()));m::put(r,"google_identity",&subject,json!({"account":id}));
        if let Some(account)=r.get_mut(&("account".into(),id.clone())){account["google_linked"]=json!(true);if !locale.is_empty(){account["ui_locale"]=json!(locale);}}
        m::new_session(r,&id,&session);r.get_mut(&("session".into(),m::hash(&session))).unwrap()["google_reauth_at"]=json!(m::now());m::audit(r,&id,if linked.is_empty(){"account.google_login"}else{"account.google_link"},&id);Ok(id)
    }).await;
    match result{Ok(id)=>finish_login(&a,&h,id,session).await,Err(e)=>error(e)}
}
#[cfg(test)] mod google_auth_tests{
    use super::*;
    #[test] fn rejects_wrong_audience_issuer_expiry_and_nonce(){let nonce="a".repeat(64);let good=json!({"iss":"https://accounts.google.com","aud":"client","iat":900,"exp":1100,"nonce":nonce,"sub":"123"});assert!(google_claims_valid(&good,"client",&nonce,1000));for(k,bad)in[("iss",json!("https://evil.example")),("aud",json!("other")),("exp",json!(999)),("iat",json!(1200)),("nonce",json!("other")),("sub",json!("")),("azp",json!("other"))]{let mut v=good.clone();v[k]=bad;assert!(!google_claims_valid(&v,"client",&nonce,1000),"{k}");}}
}
