'use strict';
// OAuth tokens live only in this dialog closure. Only tagged Fukuru events are reconciled.
(function(host){
const BASE='https://www.googleapis.com/calendar/v3/calendars/primary/events';
function owned(e,owner){return e.extendedProperties?.private?.app==='fukuru'&&e.extendedProperties.private.owner===owner;}
function legacy(e,body){return e.id===body.id&&/^[0-9a-f]{64}$/.test(body.id)&&!e.extendedProperties?.private?.app;}
function operations(plan,existing){
 const expected=new Map(plan.events.map(e=>[e.id,e])),ops=[];
 for(const body of plan.events){const old=existing.find(e=>e.id===body.id);if(old&&!owned(old,plan.owner)&&!legacy(old,body))throw Error('同じIDの予定を更新できません');ops.push({kind:old?'update':'insert',body,etag:old?.etag});}
 for(const e of existing){if(!owned(e,plan.owner)||expected.has(e.id)||e.status==='cancelled')continue;
  const props=e.extendedProperties.private;if(props.attendance==='withdrawn')continue;
  ops.push({kind:'withdraw',etag:e.etag,body:{id:e.id,summary:'参加取消・中止 · '+(props.original_name||e.summary||''),extendedProperties:{private:{...props,original_name:props.original_name||e.summary||'',attendance:'withdrawn'}}}});
 }
 return ops;
}
async function send(token,url,options={}){
 const r=await fetch(url,{...options,headers:{Authorization:'Bearer '+token,'Content-Type':'application/json',...options.headers}});
 if(!r.ok){const detail=r.status===401?'Googleの認証が切れました':r.status===403?'Googleカレンダーの権限を確認してください':r.status===412?'Google側で予定が変更されました。再読み込みしてください':'Googleカレンダーと連携できませんでした';const error=Error(detail);error.status=r.status;throw error;}
 return r.status===204?{}:r.json();
}
async function list(token,owner,at){let page='',rows=[];do{const u=new URL(BASE);u.searchParams.set('privateExtendedProperty','owner='+owner);u.searchParams.append('privateExtendedProperty','app=fukuru');u.searchParams.set('timeMin',new Date(at*1000).toISOString());u.searchParams.set('maxResults','250');if(page)u.searchParams.set('pageToken',page);const x=await send(token,u);rows.push(...(x.items||[]));page=x.nextPageToken||'';if(rows.length>1000)throw Error('連携する予定が多すぎます');}while(page);return rows;}
async function open(c){
 if(!c.needLogin())return;
 if(!c.state().google_client_id)throw Error('Google連携の設定がありません');
 const E=c.esc,plan=await c.request('/api/google-calendar-plan');
 c.dialog(`<section class="cal-connect"><h2>Googleカレンダー</h2><div class="cal-connect-count"><b>${plan.events.length}</b><span>参加予定</span></div><p>参加予定の追加・変更を反映します。参加取消・中止は、連携した予定に表示します。</p><button class="primary" type="button" data-calendar-connect>Googleで連携する</button><p role="status" data-calendar-result></p></section>`);
 if(!host.google?.accounts?.oauth2)await new Promise((resolve,reject)=>{const script=document.createElement('script');script.src='https://accounts.google.com/gsi/client';script.onload=resolve;script.onerror=()=>reject(Error('Googleの認証を読み込めません'));document.head.append(script);});
 const button=document.querySelector('[data-calendar-connect]'),status=document.querySelector('[data-calendar-result]');let token='',closed=false;
 const dialog=document.querySelector('#cx-dialog');dialog.addEventListener('close',()=>{closed=true;token='';},{once:true});
 const client=host.google.accounts.oauth2.initTokenClient({client_id:c.state().google_client_id,scope:'https://www.googleapis.com/auth/calendar.events',error_callback:()=>{button.disabled=false;status.textContent='認証が完了していません';},callback:async response=>{
  if(closed)return;if(!response.access_token){button.disabled=false;status.textContent='認証が完了していません';return;}token=response.access_token;
  try{const latest=await c.request('/api/google-calendar-plan');if(latest.owner!==plan.owner)throw Error('アカウントが切り替わりました');const existing=await list(token,latest.owner,latest.at);if(closed)return;
   // Previous releases used the same server-derived IDs without ownership tags.
   const missing=latest.events.filter(e=>!existing.some(old=>old.id===e.id));
   for(let i=0;i<missing.length;i+=3){if(closed)return;const found=await Promise.all(missing.slice(i,i+3).map(async e=>{try{return await send(token,BASE+'/'+encodeURIComponent(e.id));}catch(error){if(error.status===404||error.status===410)return null;throw error;}}));existing.push(...found.filter(Boolean));}
   if(closed)return;const ops=operations(latest,existing);
   button.disabled=false;button.textContent='予定を反映する';status.textContent=ops.filter(o=>o.kind!=='withdraw').length+'件の予定 · '+ops.filter(o=>o.kind==='withdraw').length+'件の参加取消・中止';
   button.onclick=async()=>{button.disabled=true;let done=0;try{for(const o of ops){if(closed)return;const body={...o.body};delete body.id;if(o.kind==='insert'){try{await send(token,BASE,{method:'POST',body:JSON.stringify(o.body)});}catch(e){throw Error(e.message+'（重複の場合は再読み込み）');}}else await send(token,BASE+'/'+encodeURIComponent(o.body.id),{method:'PATCH',headers:o.etag?{'If-Match':o.etag}:{},body:JSON.stringify(body)});done++;status.textContent=done+' / '+ops.length;}
    const email=existing.find(e=>e.organizer?.email)?.organizer.email||'選択したGoogleアカウント';status.textContent=done+'件を反映しました · '+email;button.textContent='反映しました';token='';c.toast('Googleカレンダーに反映しました');
   }catch(e){token='';status.textContent=done+'件を反映 · '+e.message;button.textContent='再接続して確認';button.disabled=false;button.onclick=()=>{button.disabled=true;client.requestAccessToken({prompt:''});};}};
  }catch(e){token='';status.textContent=e.message;button.disabled=false;}
 }});
 button.onclick=()=>{button.disabled=true;client.requestAccessToken({prompt:''});};
}
if(typeof module!=='undefined'&&module.exports)module.exports={owned,operations};else host.SpatialCalendar={open};
})(typeof window==='undefined'?{}:window);
