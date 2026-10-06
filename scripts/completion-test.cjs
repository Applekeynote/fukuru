const assert=require('node:assert/strict'),{spawn}=require('node:child_process'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),{randomUUID}=require('node:crypto');
const port=8793,base=process.env.COMPLETION_BASE_URL||`http://127.0.0.1:${port}`,dir=fs.mkdtempSync(path.join(os.tmpdir(),'spatial-complete-'));
const server=process.env.COMPLETION_BASE_URL?null:spawn(path.resolve(process.platform==='win32'?'target/debug/spatial_community.exe':'target/debug/spatial_community'),[],{env:{...process.env,PORT:String(port),PUBLIC_ORIGIN:base,ATLAS_PUBLIC:'false',ATLAS_STORE:'local',ATLAS_COMMUNITY_DB:path.join(dir,'test.db')},windowsHide:true,stdio:'ignore'});
const client=()=>({cookies:{},csrf:'',id:''});
async function req(c,url,body){const r=await fetch(base+url,{method:body?'POST':'GET',headers:{Origin:base,Cookie:Object.entries(c.cookies).map(([k,v])=>`${k}=${v}`).join('; '),...(body?{'Content-Type':'application/json'}:{})},body:body?JSON.stringify({csrf:c.csrf,...body}):undefined});for(const h of r.headers.getSetCookie()){const[k,v]=h.split(';')[0].split('=');c.cookies[k]=v;}const text=await r.text();let data;try{data=JSON.parse(text)}catch{data=text}return{status:r.status,data};}
async function boot(c){const r=await req(c,'/api/bootstrap');assert.equal(r.status,200);c.csrf=r.data.csrf;c.id=r.data.me?.id||'';return r.data;}
const act=(c,op,v={})=>req(c,'/api/action',{op,request_id:randomUUID(),...v});
const ok=r=>{assert.equal(r.status,200,JSON.stringify(r.data));return r.data;};const denied=r=>assert.notEqual(r.status,200,JSON.stringify(r.data));
(async()=>{try{
for(let i=0;i<100;i++){try{if((await fetch(base+'/api/health')).ok)break;}catch{}await new Promise(r=>setTimeout(r,100));}
const a=client(),b=client(),d=client(),anon=client(),suffix=randomUUID().slice(0,8),password='LocalAcceptanceOnly!2026';
for(const [name,u]of[['host',a],['member',b],['outsider',d]]){await boot(u);u.handle='qa_'+name+'_'+suffix;ok(await req(u,'/api/auth',{mode:'register',handle:u.handle,name:'検証 '+name,password}));await boot(u);}await boot(anon);
const pushEndpoint='https://fcm.googleapis.com/fcm/send/local-test-'+suffix;ok(await req(b,'/api/push/subscribe',{endpoint:pushEndpoint}));denied(await req(anon,'/api/push/test',{endpoint:pushEndpoint}));denied(await req(a,'/api/push/test',{endpoint:pushEndpoint}));ok(await req(b,'/api/push/subscribe',{endpoint:pushEndpoint,remove:true}));
const input={name:'東京・週末の街歩き',description:'ローカル環境の受入検証用イベントです。',place:'東京駅周辺',address:'東京都千代田区丸の内',kind:'WALK',start:new Date(Date.now()+86400000).toISOString(),end:new Date(Date.now()+90000000).toISOString(),lat:35.681,lon:139.767,venue_rights:true,location_confirmed:true,price_yen:0,capacity:5};
const event=ok(await act(a,'event',{...input,completion_options:{mode:'onsite',visibility:'invite',invitees:[b.id],cohosts:[b.id],contact:'イベントトークで連絡',weather_policy:'雨天中止',duration_minutes:60}})).id;
assert.equal((await boot(anon)).events.some(e=>e.id===event),false);assert.equal((await boot(d)).events.some(e=>e.id===event),false);
denied(await req(d,'/api/event?id='+event));denied(await req(d,'/api/workspace?id='+event));denied(await req(d,'/api/thread?scope=event&id='+event));denied(await act(d,'rsvp',{id:event,status:'going'}));denied(await act(d,'book',{id:event,active:true}));
const memberEvent=ok(await req(b,'/api/event?id='+event));assert.equal(memberEvent.extra.can_edit,false);assert.equal(JSON.stringify(memberEvent.changes).includes(b.id),false,'change history must not expose invitation IDs');
denied(await act(b,'event',{...input,id:event}));ok(await act(b,'complete_cohost_reply',{id:event,accept:true}));ok(await act(b,'event',{...input,id:event,name:input.name+'（共同主催が編集）'}));assert.equal(ok(await req(b,'/api/event?id='+event)).owner,a.id);
const privateItem=ok(await act(b,'complete_collab',{id:event,kind:'memory',title:'本人のみの記録',body:'private note',visibility:'private',lat:35.6812345,lon:139.7671234})).id;
assert.equal(ok(await req(a,'/api/workspace?id='+event)).entries.some(x=>x.id===privateItem),false);assert.equal(ok(await req(b,'/api/workspace?id='+event)).entries[0].lat,35.681);
denied(await act(a,'complete_collab',{id:event,item_id:privateItem,kind:'memory',title:'forged',visibility:'public'}));
const publicItem=ok(await act(b,'complete_collab',{id:event,kind:'memory',title:'参加者の公開記録',body:'本人の記録',visibility:'public'})).id;
denied(await act(a,'complete_collab',{id:event,item_id:publicItem,kind:'memory',title:'主催者による改変',body:'偽の記録',visibility:'public'}));
denied(await act(a,'complete_collab',{id:event,item_id:publicItem,deleted:true}));
assert.equal(ok(await req(b,'/api/workspace?id='+event)).entries.find(x=>x.id===publicItem).title,'参加者の公開記録');
ok(await act(b,'complete_note',{id:event,body:'自分だけの準備'}));assert.equal(ok(await req(a,'/api/workspace?id='+event)).note,null);
ok(await act(b,'complete_search',{name:'週末東京',query:{filters:{time:'weekend',region:'東京'}}}));assert.equal(ok(await req(a,'/api/personal')).searches.length,0);
const talk=ok(await act(b,'post',{id:event,body:'集合は駅の北口です。'})).id;ok(await act(b,'complete_talk',{id:talk,body:'集合は駅の南口です。'}));denied(await act(a,'complete_talk',{id:talk,body:'他人の本文変更'}));ok(await act(a,'complete_talk',{id:talk,pinned:true}));
ok(await act(a,'complete_room',{id:'event-'+event,archived:true}));denied(await act(b,'post',{id:event,body:'アーカイブ後'}));ok(await act(a,'complete_room',{id:'event-'+event,archived:false}));
const copies=ok(await act(a,'complete_duplicate',{id:event,dates:[new Date(Date.now()+7*86400000).toISOString(),new Date(Date.now()+14*86400000).toISOString()]}));assert.equal(copies.drafts.length,2);assert.equal((await boot(b)).drafts.length,0);
ok(await act(a,'complete_event_options',{id:event,mode:'onsite',visibility:'unlisted'}));assert.equal((await boot(anon)).events.some(e=>e.id===event),false);ok(await req(anon,'/api/event?id='+event));
ok(await act(a,'complete_event_options',{id:event,mode:'onsite',visibility:'public'}));assert.equal((await boot(anon)).events.some(e=>e.id===event),true);
const p=ok(await req(b,'/api/personal'));assert.ok(p.sessions.some(x=>x.current));assert.equal(JSON.stringify(p).includes(b.cookies.spatial_session),false);
const code=ok(await req(d,'/api/recovery',{mode:'generate',password})).code;assert.equal(code.length,64);denied(await req(d,'/api/recovery',{mode:'reset',handle:d.handle,code:'0'.repeat(64),password:'NewLocalPassword!2026'}));ok(await req(d,'/api/recovery',{mode:'reset',handle:d.handle,code,password:'NewLocalPassword!2026'}));assert.equal((await boot(d)).me,null);denied(await req(d,'/api/recovery',{mode:'reset',handle:d.handle,code,password:'AnotherLocalPassword!2026'}));
assert.equal(ok(await req(b,'/api/export')).records.some(x=>x.scope.startsWith('note:')),true);
console.log('PASS: invite/unlisted visibility, booking and thread ACL, consent before cohost editing, private records and history, personal notes/searches, talk ownership/pinning/archive, recurring drafts, session redaction, single-use recovery');
if(process.env.COMPLETION_BASE_URL)fs.writeFileSync('completion-preview-fixture.json',JSON.stringify({event,handle:a.handle,password,base},null,2));
}finally{server?.kill();}})().catch(e=>{console.error(e);process.exitCode=1});
