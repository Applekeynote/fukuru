const assert=require('node:assert/strict'),{spawn}=require('node:child_process'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),{randomUUID}=require('node:crypto');
const base='http://127.0.0.1:8796',dir=fs.mkdtempSync(path.join(os.tmpdir(),'spatial-v7-'));
const server=spawn(path.resolve('target/debug/spatial_community.exe'),[],{env:{...process.env,PORT:'8796',PUBLIC_ORIGIN:base,ADDITIONAL_PUBLIC_ORIGINS:'http://localhost:8796',ATLAS_PUBLIC:'false',ATLAS_STORE:'local',ATLAS_COMMUNITY_DB:path.join(dir,'test.db'),GOOGLE_SIGNIN_CLIENT_ID:'qa-client'},windowsHide:true,stdio:'ignore'});
const client=()=>({cookies:{},csrf:''});
async function req(c,url,body,origin=base){const r=await fetch(base+url,{method:body?'POST':'GET',headers:{Origin:origin,Cookie:Object.entries(c.cookies).map(([k,v])=>`${k}=${v}`).join('; '),...(body?{'Content-Type':'application/json'}:{})},body:body?JSON.stringify({csrf:c.csrf,...body}):undefined});for(const h of r.headers.getSetCookie()){const[k,v]=h.split(';')[0].split('=');c.cookies[k]=v;}const t=await r.text();let data;try{data=JSON.parse(t)}catch{data={error:t}}return {status:r.status,data};}
const ok=r=>{assert.equal(r.status,200,JSON.stringify(r.data));return r.data;},deny=r=>assert.notEqual(r.status,200);
async function boot(c){const d=ok(await req(c,'/api/bootstrap'));c.csrf=d.csrf;return d;}
const act=(c,op,v={})=>req(c,'/api/action',{op,request_id:randomUUID(),...(op==='profile'?{bio:'',theme:'sky',layout:'cards',interests:[]}:{}),...v});
(async()=>{try{
for(let n=0;n<80;n++){try{if((await fetch(base+'/api/health')).ok)break;}catch{}await new Promise(r=>setTimeout(r,100));}
const a=client(),b=client(),guest=client();await boot(guest);
for(const c of [a,b]){await boot(c);ok(await req(c,'/api/auth',{mode:'register',handle:'qa_'+randomUUID().slice(0,8),name:'ローカル検証',password:'LocalAcceptanceOnly!2026'}));await boot(c);}
deny(await req(guest,'/api/auth/google/nonce',{mode:'link'}));deny(await req(guest,'/api/auth/google/nonce',{mode:'login'},'https://evil.example'));
const first=ok(await req(guest,'/api/auth/google/nonce',{mode:'login'})),second=ok(await req(guest,'/api/auth/google/nonce',{mode:'login'},'http://localhost:8796'));assert.equal(first.nonce.length,64);assert.notEqual(first.nonce,second.nonce);assert.equal(second.client_id,'qa-client');
const enc=v=>Buffer.from(JSON.stringify(v)).toString('base64url');const payload={iss:'https://accounts.google.com',aud:'qa-client',exp:Math.floor(Date.now()/1000)+600,iat:Math.floor(Date.now()/1000),sub:'attacker',nonce:second.nonce};
deny(await req(guest,'/api/auth/google',{credential:`${enc({alg:'none'})}.${enc(payload)}.`}));payload.nonce=first.nonce;deny(await req(guest,'/api/auth/google',{credential:`${enc({alg:'RS256',kid:'fake'})}.${enc(payload)}.AAAA`}));assert.equal((await boot(guest)).me,null);
const chunk=(type,data)=>{const t=Buffer.from(type);let crc=0xffffffff;for(const x of Buffer.concat([t,data])){crc^=x;for(let i=0;i<8;i++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}const n=Buffer.alloc(4),c=Buffer.alloc(4);n.writeUInt32BE(data.length);c.writeUInt32BE((crc^0xffffffff)>>>0);return Buffer.concat([n,t,data,c]);};
const png=Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',Buffer.from([0,0,0,1,0,0,0,1,8,6,0,0,0])),chunk('IDAT',require('node:zlib').deflateSync(Buffer.from([0,50,120,200,255]))),chunk('IEND',Buffer.alloc(0))]).toString('base64');
const media=ok(await req(a,'/api/upload',{data:png})).url;assert.equal((await req(guest,media)).status,404);deny(await act(b,'profile',{name:'他の人',banner:media}));ok(await act(a,'profile',{name:'ローカル検証',banner:media,banner_alt:'海辺の写真'}));assert.equal((await req(guest,media)).status,200);assert.equal((await boot(a)).me.banner_alt,'海辺の写真');ok(await act(a,'profile',{name:'ローカル検証',banner:''}));assert.equal((await req(guest,media)).status,404);
console.log('PASS: Google nonce binding, CSRF origins, forged token rejection, banner ownership/publication/removal');
}finally{server.kill();}})().catch(e=>{console.error(e);process.exitCode=1;});
