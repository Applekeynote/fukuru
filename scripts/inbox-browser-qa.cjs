// Isolated local development data. This process never connects to production.
const {spawn}=require('node:child_process'),fs=require('node:fs'),path=require('node:path'),{randomUUID}=require('node:crypto');
const dir=process.env.INBOX_QA_DIR||path.join(__dirname,'../target/inbox-browser-qa-'+Date.now());fs.mkdirSync(dir,{recursive:true});const port=8810,base=`http://127.0.0.1:${port}`;
const child=spawn(process.env.INBOX_QA_BINARY||path.join(__dirname,'../target/debug',process.platform==='win32'?'spatial_community.exe':'spatial_community'),[],{cwd:dir,env:{...process.env,PORT:String(port),ATLAS_PUBLIC:'false',ATLAS_STORE:'local',ATLAS_COMMUNITY_DB:path.join(dir,'test.db'),PUBLIC_ORIGIN:base},windowsHide:true,stdio:'ignore'});
async function req(a,url,body){const r=await fetch(base+url,{method:body?'POST':'GET',headers:{Origin:base,Cookie:Object.entries(a.cookies).map(([k,v])=>k+'='+v).join('; '),'Content-Type':'application/json'},body:body?JSON.stringify({csrf:a.csrf,...body}):undefined});for(const c of r.headers.getSetCookie()){const [k,v]=c.split(';')[0].split('=');a.cookies[k]=v;}const data=await r.json();if(!r.ok)throw Error(JSON.stringify(data));return data;}
async function account(handle,name){const a={cookies:{}};a.csrf=(await req(a,'/api/bootstrap')).csrf;await req(a,'/api/auth',{mode:'register',handle,name,password:'Local!Inbox2026Only'});const boot=await req(a,'/api/bootstrap');a.csrf=boot.csrf;a.id=boot.me.id;return a;}
const act=(a,op,v)=>req(a,'/api/action',{op,request_id:randomUUID(),...v});
(async()=>{for(let i=0;i<60;i++){try{if((await fetch(base+'/api/health')).ok)break;}catch{}await new Promise(r=>setTimeout(r,200));}
if(fs.existsSync(path.join(dir,'ready'))){console.log('Local QA restarted: '+base);return;}
const users=[];for(const [handle,name] of [['local_inbox','りな'],['local_mio','美緒'],['local_haru','はる'],['local_sora','そら']])users.push(await account(handle,name));
for(const [name,members,body]of [['秋の街歩き',[1,2],'明日は駅の北口に15時集合でどうですか？'],['はじめての写真さんぽ',[1,2,3],'いい場所を見つけたので、写真を撮りに行きましょう。'],['美緒',[1],'会場の入口を確認してきました！']]){const room=await act(users[0],'room',{name,members:members.map(i=>users[i].id)});for(const i of members)await act(users[i],'room_invitation',{id:room.id,accept:true});await act(users[1],'message',{id:room.id,body});}
fs.writeFileSync(path.join(dir,'ready'),'Local test fixture');console.log('Local full application QA ready: '+base+'/?view=messages');console.log('Development account: local_inbox (local test fixture only)');
})().catch(e=>{console.error(e);child.kill();process.exit(1);});
process.on('SIGINT',()=>{child.kill();process.exit(0);});process.on('SIGTERM',()=>{child.kill();process.exit(0);});setInterval(()=>{},1000);
