const assert=require('node:assert/strict'),{spawn}=require('node:child_process'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),{randomUUID}=require('node:crypto');
const fixture=process.argv.includes('--fixture'),port=8794,base=fixture?'http://127.0.0.1:8792':`http://127.0.0.1:${port}`,dir=fs.mkdtempSync(path.join(os.tmpdir(),'spatial-refine-'));
const server=fixture?null:spawn(path.resolve('target/debug/spatial_community.exe'),[],{env:{...process.env,PORT:String(port),PUBLIC_ORIGIN:base,ATLAS_PUBLIC:'false',ATLAS_STORE:'local',ATLAS_COMMUNITY_DB:path.join(dir,'test.db')},windowsHide:true,stdio:'ignore'});
const client=()=>({cookies:{},csrf:'',id:''});
async function req(c,url,body){const r=await fetch(base+url,{method:body?'POST':'GET',headers:{Origin:base,Cookie:Object.entries(c.cookies).map(([k,v])=>`${k}=${v}`).join('; '),...(body?{'Content-Type':'application/json'}:{})},body:body?JSON.stringify({csrf:c.csrf,...body}):undefined});for(const h of r.headers.getSetCookie()){const[k,v]=h.split(';')[0].split('=');c.cookies[k]=v;}const text=await r.text();let data;try{data=JSON.parse(text)}catch{data={error:text||String(r.status)}}return{status:r.status,data};}
const ok=r=>{assert.equal(r.status,200,JSON.stringify(r.data));return r.data;},deny=r=>assert.notEqual(r.status,200);
async function boot(c){const d=ok(await req(c,'/api/bootstrap'));c.csrf=d.csrf;c.id=d.me?.id||'';return d;}
const act=(c,op,v={})=>req(c,'/api/action',{op,request_id:randomUUID(),...v});
(async()=>{try{
for(let n=0;n<80;n++){try{if((await fetch(base+'/api/health')).ok)break;}catch{}await new Promise(r=>setTimeout(r,100));}
const a=client(),b=client(),d=client(),suffix=randomUUID().slice(0,8),password='LocalAcceptanceOnly!2026';
for(const [name,c] of [['host',a],['member',b],['other',d]]){await boot(c);c.handle='rf_'+name+'_'+suffix;ok(await req(c,'/api/auth',{mode:'register',handle:c.handle,name:{host:'街歩き主催者',member:'みな',other:'りく'}[name],password}));await boot(c);}
const start=new Date();start.setDate(start.getDate()+1);start.setHours(10,0,0,0);const end=new Date(+start+7200000);
const input={name:'まちの喫茶店と写真さんぽ',kind:'写真と喫茶店',place:'東京駅 丸の内側',address:'東京都千代田区丸の内1丁目',description:'古い喫茶店をめぐりながら、気になった風景を写真に残します。 #街歩き #写真',start:start.toISOString(),end:end.toISOString(),lat:35.6812,lon:139.7671,venue_rights:true,location_confirmed:true,price_yen:500,bring:'歩きやすい靴・カメラ',eligibility:'途中参加可・撮影前にお店へ確認',indoor:false,completion_options:{mode:'onsite',visibility:'public',languages:'Japanese English',contact:'参加者トーク',duration_minutes:120}};
input.payment_recipient='店舗へ当日直接支払い';input.refund_policy='参加費は現地で支払い。中止時の請求はありません。';
const demo=ok(await act(a,'event',{...input,name:'勝川で、ぽよと寄り道',demo:true,venue_rights:false})).id;
assert((await boot(a)).events.find(e=>e.id===demo)?.demo);
deny(await act(b,'rsvp',{id:demo,status:'going'}));
ok(await act(a,'event',{...input,id:demo,name:'勝川で、ぽよと寄り道（編集）',demo:false,venue_rights:false}));
assert((await boot(a)).events.find(e=>e.id===demo)?.demo);
const event=ok(await act(a,'event',input)).id;ok(await act(b,'rsvp',{id:event,status:'going'}));
const talk=ok(await act(a,'post',{id:event,body:`集合場所は丸の内側です。 @${b.handle} #街歩き`})).id;
const thread=ok(await req(b,'/api/thread?scope=event&id='+event));assert(thread.messages.find(x=>x.id===talk).tags.includes('街歩き'));assert(thread.messages.find(x=>x.id===talk).mentions.includes(b.handle));
const alerts=(await boot(b)).alerts;assert(alerts.some(x=>x.type==='mention'&&x.talk===talk));
const room=(await boot(b)).rooms.find(x=>x.event===event);assert.equal(room.member_count,2);
const item=ok(await act(b,'complete_collab',{id:event,kind:'record',title:'写真のメモ',body:'雨が上がったあとに訪問',visibility:'private',editors:[a.id]})).id;
assert(ok(await req(a,'/api/workspace?id='+event)).entries.find(x=>x.id===item)?.can_edit);
assert(!ok(await req(d,'/api/workspace?id='+event)).entries.some(x=>x.id===item));
ok(await act(a,'complete_collab',{id:event,item_id:item,kind:'record',title:'写真のメモ',body:'撮影前に店員へ確認',visibility:'private'}));
deny(await act(a,'complete_collab',{id:event,item_id:item,kind:'record',title:'Unauthorized scope',visibility:'public'}));
deny(await act(a,'complete_collab',{id:event,item_id:item,deleted:true}));
ok(await act(b,'complete_collab',{id:event,item_id:item,kind:'record',title:'編集許可を解除',visibility:'private',editors:[]}));
deny(await act(a,'complete_collab',{id:event,item_id:item,kind:'record',title:'Denied',visibility:'private'}));
deny(await act(b,'delete_event',{id:event}));
if(fixture){ok(await act(a,'rsvp',{id:event,status:'going'}));ok(await act(a,'message',{id:room.id,body:`おはようございます。 @${b.handle} 10時に丸の内側で集合です。 #待ち合わせ`}));ok(await act(b,'message',{id:room.id,body:'了解です！カメラを持っていきます 📷'}));
fs.writeFileSync('target/refinement-fixture.json',JSON.stringify({handle:a.handle,password,event,room:room.id,member:b.id},null,2));console.log('Local UI fixture ready:',a.handle,event);
}else{ok(await act(a,'complete_event_options',{id:event,cohosts:[b.id]}));deny(await act(b,'delete_event',{id:event}));ok(await act(b,'complete_cohost_reply',{id:event,accept:true}));ok(await act(b,'complete_event_cover',{id:event,cover:'',cover_alt:''}));ok(await act(a,'complete_event_options',{id:event,cohosts:[]}));deny(await act(b,'delete_event',{id:event}));ok(await act(a,'complete_event_options',{id:event,cohosts:[b.id]}));ok(await act(b,'complete_cohost_reply',{id:event,accept:true}));ok(await act(b,'delete_event',{id:event}));assert(!(await boot(b)).events.some(x=>x.id===event));deny(await req(b,'/api/event?id='+event));deny(await req(b,'/api/workspace?id='+event));deny(await req(b,'/api/thread?scope=room&id='+room.id));assert((await boot(b)).alerts.some(x=>x.deleted_event===event&&x.body));}
console.log('PASS: free category, persisted tags/mentions, participant count, record editor grants/revocation, pending/revoked cohost delete rejection, accepted cohost image edit and deletion, deleted event/thread ACL');
}finally{server?.kill();}})().catch(e=>{console.error(e);process.exitCode=1;});

