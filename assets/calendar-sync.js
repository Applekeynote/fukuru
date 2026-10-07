'use strict';
// Only app-created calendars are accessible. OAuth tokens never leave this closure.
(function(host){
const ROOT='https://www.googleapis.com/calendar/v3';
const CAL_SCOPE='https://www.googleapis.com/auth/calendar.app.created';
const SCOPE='openid '+CAL_SCOPE;
function owned(e,owner){return e.extendedProperties?.private?.app==='fukuru'&&e.extendedProperties.private.owner===owner;}
function operations(plan,existing){
 const expected=new Map(plan.events.map(e=>[e.id,e])),ops=[];
 for(const body of plan.events){const old=existing.find(e=>e.id===body.id);if(old&&!owned(old,plan.owner))throw Error('ふくる以外の予定は変更できません');ops.push({kind:old?'update':'insert',body,etag:old?.etag});}
 if(!plan.partial)for(const e of existing){if(!owned(e,plan.owner)||expected.has(e.id)||e.status==='cancelled')continue;
  const props=e.extendedProperties.private;if(props.attendance==='withdrawn')continue;
  ops.push({kind:'withdraw',etag:e.etag,body:{id:e.id,summary:'参加取消・中止 · '+(props.original_name||e.summary||''),extendedProperties:{private:{...props,original_name:props.original_name||e.summary||'',attendance:'withdrawn'}}}});
 }
 return ops;
}
function calendarPath(id){if(typeof id!=='string'||!id.endsWith('@group.calendar.google.com')||id.length>256)throw Error('専用カレンダーを確認できません');return ROOT+'/calendars/'+encodeURIComponent(id);}
function description(owner,fp){return 'fukuru:v16:'+owner+':'+fp;}
function scopeAllowed(scope){const scopes=new Set((scope||'').split(/\s+/));return scopes.has(CAL_SCOPE)&&scopes.has('openid')&&![...scopes].some(s=>s.startsWith('https://www.googleapis.com/auth/calendar')&&s!==CAL_SCOPE);}
function authMessage(error){return error==='access_denied'?'Googleで連携が許可されませんでした':error==='popup_closed'?'Googleの許可画面が閉じられました':'Googleの認証を再試行してください';}
async function send(token,url,options={}){
 const r=await fetch(url,{...options,headers:{Authorization:'Bearer '+token,'Content-Type':'application/json',...options.headers}});
 if(!r.ok){const detail=r.status===401?'Googleの認証が切れました':r.status===403?'専用カレンダーの権限を確認してください':r.status===412?'Google側で予定が変更されました。再接続してください':'Googleカレンダーと連携できませんでした';const error=Error(detail);error.status=r.status;throw error;}
 return r.status===204?{}:r.json();
}
async function fingerprint(token,clientId){const info=await send(token,'https://openidconnect.googleapis.com/v1/userinfo');if(typeof info.sub!=='string'||!info.sub)throw Error('Googleアカウントを確認できません');const bytes=await host.crypto.subtle.digest('SHA-256',new TextEncoder().encode(clientId+':'+info.sub));return [...new Uint8Array(bytes)].map(x=>x.toString(16).padStart(2,'0')).join('');}
async function linked(c,token,fp,owner){const ref=await c.request('/api/google-calendar-link',{op:'get',fingerprint:fp});if(!ref.calendar_id)return null;
 try{const calendar=await send(token,calendarPath(ref.calendar_id));if(calendar.description!==description(owner,fp))throw Error('専用カレンダーの識別情報が一致しません');return calendar;}catch(e){if(e.status===404||e.status===410)return null;throw e;}}
async function create(c,token,fp,owner){const current=await linked(c,token,fp,owner);if(current)return current;
 const calendar=await send(token,ROOT+'/calendars',{method:'POST',body:JSON.stringify({summary:'ふくる',description:description(owner,fp),timeZone:'Asia/Tokyo'})});calendarPath(calendar.id);
 await c.request('/api/google-calendar-link',{op:'set',fingerprint:fp,calendar_id:calendar.id});return calendar;}
async function list(token,calendarId,owner,at){let page='',rows=[];do{const u=new URL(calendarPath(calendarId)+'/events');u.searchParams.set('privateExtendedProperty','owner='+owner);u.searchParams.append('privateExtendedProperty','app=fukuru');u.searchParams.set('timeMin',new Date(at*1000).toISOString());u.searchParams.set('maxResults','250');if(page)u.searchParams.set('pageToken',page);const x=await send(token,u);rows.push(...(x.items||[]));page=x.nextPageToken||'';if(rows.length>1000)throw Error('連携する予定が多すぎます');}while(page);return rows;}
async function loadPlan(c,eventId){const plan=await c.request('/api/google-calendar-plan');if(!eventId)return plan;const body=await c.request('/api/google-event',{id:eventId});if(body.extendedProperties?.private?.owner!==plan.owner)throw Error('アカウントを確認してください');return {...plan,events:[body],partial:true};}
async function open(c,options={}){
 if(!c.needLogin())return;
 if(!c.state().google_client_id)throw Error('Google連携の設定がありません');
 const plan=await loadPlan(c,options.eventId);
 c.dialog(`<section class="cal-connect"><h2>Googleカレンダー</h2><div class="cal-connect-count"><b>${plan.events.length}</b><span>参加予定</span></div><p>「ふくる」専用カレンダーへ反映します。個人の予定にはアクセスしません。</p><button class="primary" type="button" data-calendar-connect>Googleで連携する</button><p role="status" data-calendar-result></p></section>`);
 if(!host.google?.accounts?.oauth2)await new Promise((resolve,reject)=>{const script=document.createElement('script');script.src='https://accounts.google.com/gsi/client';script.onload=resolve;script.onerror=()=>reject(Error('Googleの認証を読み込めません'));document.head.append(script);});
 const button=document.querySelector('[data-calendar-connect]'),status=document.querySelector('[data-calendar-result]');let token='',closed=false;
 const dialog=document.querySelector('#cx-dialog');dialog.addEventListener('close',()=>{closed=true;token='';},{once:true});
 const retry=()=>{button.disabled=false;button.textContent='Googleで再接続する';button.onclick=authorize;};
 const client=host.google.accounts.oauth2.initTokenClient({client_id:c.state().google_client_id,scope:SCOPE,include_granted_scopes:false,error_callback:error=>{if(closed)return;token='';status.textContent=authMessage(error.type);retry();},callback:async response=>{
  if(closed)return;if(!response.access_token||response.error){status.textContent=authMessage(response.error);retry();return;}
  if(!scopeAllowed(response.scope)){status.textContent='専用カレンダーの権限だけを許可してください';retry();return;}token=response.access_token;
  try{const latest=await loadPlan(c,options.eventId);if(latest.owner!==plan.owner)throw Error('アカウントが切り替わりました');const fp=await fingerprint(token,c.state().google_client_id);if(closed)return;
   const calendar=await linked(c,token,fp,plan.owner);const existing=calendar?await list(token,calendar.id,plan.owner,latest.at):[];if(closed)return;
   const ops=operations(latest,existing);button.disabled=false;button.textContent=calendar?'予定を反映する':'専用カレンダーを作成して反映';status.textContent=ops.filter(o=>o.kind!=='withdraw').length+'件の予定 · '+ops.filter(o=>o.kind==='withdraw').length+'件の参加取消・中止';
   button.onclick=async()=>{button.disabled=true;let done=0;try{if(closed)return;const check=await loadPlan(c,options.eventId);if(check.owner!==plan.owner)throw Error('アカウントが切り替わりました');if(JSON.stringify(check.events)!==JSON.stringify(latest.events))throw Error('予定が変更されました。再接続してください');
    const apply=async()=>{if(closed)return;const target=await create(c,token,fp,plan.owner);if(closed)return;const base=calendarPath(target.id)+'/events';
     const fresh=operations(latest,await list(token,target.id,plan.owner,latest.at));
     for(const o of fresh){if(closed)return;const body={...o.body};delete body.id;if(o.kind==='insert')await send(token,base,{method:'POST',body:JSON.stringify(o.body)});else await send(token,base+'/'+encodeURIComponent(o.body.id),{method:'PATCH',headers:o.etag?{'If-Match':o.etag}:{},body:JSON.stringify(body)});done++;status.textContent=done+' / '+fresh.length;}
    };
    if(host.navigator?.locks)await host.navigator.locks.request('fukuru-calendar:'+fp+':'+plan.owner,apply);else await apply();if(closed)return;
    status.textContent=done+'件を反映しました · ふくる';button.textContent='反映しました';token='';c.toast('ふくる専用カレンダーに反映しました');
   }catch(e){token='';if(closed)return;status.textContent=done+'件を反映 · '+e.message;retry();}};
  }catch(e){token='';if(closed)return;status.textContent=e.message;retry();}
 }});
 function authorize(){button.disabled=true;token='';client.requestAccessToken({prompt:'select_account',include_granted_scopes:false});}
 button.onclick=authorize;
}
if(typeof module!=='undefined'&&module.exports)module.exports={owned,operations,calendarPath,description,scopeAllowed,authMessage,linked,create,list};else host.SpatialCalendar={open};
})(typeof window==='undefined'?{}:window);
