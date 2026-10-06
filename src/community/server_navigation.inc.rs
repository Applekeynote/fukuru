async fn navigation_session(State(a):State<App>,h:HeaderMap,Json(v):Json<Value>)->Response {
    if !csrf(&a,&h,&v){return StatusCode::FORBIDDEN.into_response();}
    let user=match who(&a,&h).await{Ok(u)=>u,Err(e)=>return error(e)};
    if limited(&a,format!("navigation-session:{user}"),8,3600).await{return StatusCode::TOO_MANY_REQUESTS.into_response();}
    let q:super::navigation::SessionRequest=match serde_json::from_value(v){Ok(q)=>q,Err(_)=>return error("invalid route request")};
    let result=a.store.transact(|r|{let event=m::get(r,"event",&q.destination).ok_or("destination missing")?;if !super::completion::can_view(r,event,Some(&user)){return Err("destination unavailable".into());}let config=super::navigation::Config::load(r)?;super::navigation::create(r,&user,&q,m::now(),&config)}).await;
    match result{Ok(v)=>Json(v).into_response(),Err(e)=>error(e)}
}
async fn navigation_evidence(State(a):State<App>,h:HeaderMap,Json(v):Json<Value>)->Response {
    if !csrf(&a,&h,&v){return StatusCode::FORBIDDEN.into_response();}
    let user=match who(&a,&h).await{Ok(u)=>u,Err(e)=>return error(e)};
    if limited(&a,format!("navigation-evidence:{user}"),20,60).await{return StatusCode::TOO_MANY_REQUESTS.into_response();}
    let e:super::navigation::Evidence=match serde_json::from_value(v){Ok(e)=>e,Err(_)=>return error("invalid evidence")};
    match a.store.transact(|r|{let session=m::get(r,"navigation_session",&e.route_session_id).ok_or("session missing")?;let event=m::get(r,"event",session["destination"].as_str().unwrap_or("")).ok_or("destination missing")?;if !super::completion::can_view(r,event,Some(&user)){return Err("destination unavailable".into());}let config=super::navigation::Config::load(r)?;super::navigation::submit(r,&user,&e,m::now(),&config)}).await {Ok(v)=>Json(v).into_response(),Err(e)=>error(e)}
}
