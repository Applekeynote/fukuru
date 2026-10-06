'use strict';
const assert=require('node:assert/strict'),{spawn,spawnSync}=require('node:child_process'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),http=require('node:http'),{randomUUID}=require('node:crypto');
const fixture=process.argv.includes('--fixture'),port=8796,base=`http://127.0.0.1:${port}`,dir=fs.mkdtempSync(path.join(os.tmpdir(),'spatial-v13-')),db=path.join(dir,'isolated.db');
const env={...process.env,PORT:String(port),PUBLIC_ORIGIN:base,ATLAS_PUBLIC:'false',ATLAS_STORE:'local',ATLAS_COMMUNITY_DB:db};
const server=spawn(path.resolve('target/debug/spatial_community.exe'),[],{env,windowsHide:true,stdio:'ignore'});
const clients=[{},{ }];let keep=false,proxy;
async function req(c,url,body){const response=await fetch(base+url,{method:body?'POST':'GET',headers:{Origin:base,Cookie:c.cookie||'',...(body?{'Content-Type':'application/json'}:{})},body:body?JSON.stringify({csrf:c.csrf,...body}):undefined});const map=Object.fromEntries((c.cookie||'').split('; ').filter(Boolean).map(s=>s.split('=')));for(const h of response.headers.getSetCookie()){const[k,v]=h.split(';')[0].split('=');map[k]=v;}c.cookie=Object.entries(map).map(([k,v])=>k+'='+v).join('; ');return {status:response.status,data:await response.json()};}
async function boot(c){const r=await req(c,'/api/bootstrap');assert.equal(r.status,200);c.csrf=r.data.csrf;c.id=r.data.me?.id;return r.data;}
const action=(c,op,v)=>req(c,'/api/action',{op,request_id:randomUUID(),...v});
(async()=>{try{
 for(let i=0;i<80;i++){try{if((await fetch(base+'/api/health')).ok)break;}catch{}await new Promise(r=>setTimeout(r,100));}
 const [a,b]=clients;for(const [i,c]of clients.entries()){await boot(c);assert.equal((await req(c,'/api/auth',{mode:'register',handle:i?'local_guest':'rynat',name:i?'みな':'管理者です',password:'LocalOnly!'+randomUUID()})).status,200);await boot(c);}
 const input={name:'名古屋｜週末の写真さんぽ',description:'久屋大通公園の景色をゆっくり撮る、ローカル検証の予定。',place:'愛知県 名古屋市・久屋大通公園',address:'愛知県名古屋市中区',kind:'写真',start:'2026-10-12T14:00:00+09:00',end:'2026-10-12T15:00:00+09:00',lat:35.1802,lon:136.9066,venue_rights:true,location_confirmed:true,price_yen:0,completion_options:{visibility:'public',mode:'onsite',languages:'Japanese'}};
 const made=await action(a,'event',input);assert.equal(made.status,200);const id=made.data.id;
 assert.equal((await boot(a)).events.find(e=>e.id===id).my_status,'going');
 assert.equal((await action(b,'rsvp',{id,status:'going'})).status,200);assert.equal((await boot(b)).events.find(e=>e.id===id).my_status,'going');
 assert.equal((await action(a,'rsvp',{id,status:'none'})).status,200);assert.equal((await action(a,'event',{...input,id,description:'変更後の本文'})).status,200);assert.notEqual((await boot(a)).events.find(e=>e.id===id).my_status,'going');
 assert.equal((await action(a,'rsvp',{id,status:'going'})).status,200);
 const bin=path.resolve('target/debug/spatial_demo_import.exe'),manifest=path.resolve('data/regional-events-2026-q4.json');
 for(const extra of [[],['--apply'],['--apply']]){const result=spawnSync(bin,['--manifest',manifest,...extra],{env,windowsHide:true,encoding:'utf8'});assert.equal(result.status,0,result.stderr);assert(result.stdout.includes(extra.length?'VERIFIED':'DRY RUN'));}
 const result=await boot(a);const regional=result.events.filter(e=>e.id.startsWith('spid_region_2026_'));assert.equal(regional.length,705);assert.equal(result.accounts.filter(a=>a.handle?.startsWith('fukuru_')).length,46);assert.equal(regional.filter(e=>e.owner===a.id&&e.my_status==='going').length,15);assert(regional.every(e=>e.demo));assert(regional.every(e=>!e.name.includes('デモ')));
 const c={};await boot(c);assert.equal((await req(c,'/api/auth',{mode:'login',handle:'fukuru_01',password:'Anything123!'})).status,400);
 console.log('PASS creator auto-Join; cancel/edit/re-Join; member Join; direct import 705/46; repeat import unchanged; demo isolation; disabled logins');
 if(fixture){proxy=http.createServer(async(q,r)=>{try{const response=await fetch(base+q.url,{method:q.method,headers:{...q.headers,host:`127.0.0.1:${port}`,cookie:a.cookie,origin:base},...(q.method==='POST'?{body:q,duplex:'half'}:{})});r.writeHead(response.status,Object.fromEntries([...response.headers].filter(([k])=>!['content-encoding','content-length','transfer-encoding','set-cookie'].includes(k))));r.end(Buffer.from(await response.arrayBuffer()));}catch(e){r.writeHead(500);r.end('Local fixture unavailable');}}).listen(8797,'127.0.0.1');keep=true;console.log('LOCAL FIXTURE: http://127.0.0.1:8797/?view=demo');}
 }catch(e){console.error(e);process.exitCode=1;}finally{if(!keep){server.kill();proxy?.close();}}})();
