'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),{webcrypto}=require('node:crypto');
const {JSDOM}=require('../target/navigation-tools/node_modules/jsdom');
const api=require('../assets/calendar-sync.js'),scope='openid https://www.googleapis.com/auth/calendar.app.created';
assert(api.scopeAllowed(scope));assert(!api.scopeAllowed(scope+' https://www.googleapis.com/auth/calendar.events'));
assert.throws(()=>api.calendarPath('primary'));assert.throws(()=>api.calendarPath('private@gmail.com'));
const tagged={id:'a',summary:'体験',extendedProperties:{private:{app:'fukuru',owner:'u'}}};
assert.equal(api.operations({owner:'u',partial:true,events:[]},[tagged]).length,0);
const dom=new JSDOM('<dialog id="cx-dialog"><div id="body"></div></dialog>',{runScripts:'outside-only',url:'https://example.test'}),w=dom.window;
Object.defineProperty(w,'crypto',{value:webcrypto});w.TextEncoder=TextEncoder;
let config,google='google-one',created=0,writes=0,reads=0;
const calendars=new Map(),links=new Map(),records=new Map();
const state={me:{id:'user'},google_client_id:'test-client'},plan={owner:'u',at:1791320000,events:[tagged]};
w.google={accounts:{oauth2:{initTokenClient:c=>(config=c,{requestAccessToken:o=>{assert.equal(o.include_granted_scopes,false);assert.equal(o.prompt,'select_account');}})}}};
w.fetch=async(url,options={})=>{
 url=String(url);assert(!url.includes('/primary/'));assert(!url.includes('calendarList'));assert(!url.includes('tokeninfo'));assert.match(options.headers.Authorization,/^Bearer /);
 const method=options.method||'GET';let body={};reads++;
 if(url==='https://openidconnect.googleapis.com/v1/userinfo')body={sub:google};
 else if(url==='https://www.googleapis.com/calendar/v3/calendars'&&method==='POST'){
  const id='calendar'+(++created)+'@group.calendar.google.com';body={...JSON.parse(options.body),id};calendars.set(id,body);records.set(id,[]);
 }else{
  const u=new URL(url),segments=u.pathname.split('/'),id=decodeURIComponent(segments[4]);assert(calendars.has(id));
  if(segments[5]!=='events')body=calendars.get(id);
  else if(method==='GET')body={items:records.get(id)};
  else if(method==='POST'){body=JSON.parse(options.body);records.get(id).push(body);writes++;}
  else if(method==='PATCH'){body=JSON.parse(options.body);Object.assign(records.get(id).find(x=>x.id===decodeURIComponent(segments[6])),body);writes++;}
 }
 return {ok:true,status:200,json:async()=>body};
};
w.eval(fs.readFileSync('assets/calendar-sync.js','utf8'));
const c={needLogin:()=>true,state:()=>state,toast:()=>{},dialog:html=>w.document.querySelector('#body').innerHTML=html,
 request:async(path,v)=>{if(path==='/api/google-calendar-plan')return structuredClone(plan);if(path==='/api/google-event')return structuredClone(tagged);
  assert.equal(path,'/api/google-calendar-link');const key=state.me.id+':'+v.fingerprint;if(v.op==='set')links.set(key,v.calendar_id);return {calendar_id:links.get(key)||''};}};
const button=()=>w.document.querySelector('[data-calendar-connect]');
(async()=>{
 await w.SpatialCalendar.open(c);assert.equal(config.scope,scope);assert.equal(config.include_granted_scopes,false);
 await config.callback({access_token:'test',scope:scope+' https://www.googleapis.com/auth/calendar.events'});assert.equal(reads,0);
 await config.callback({access_token:'test',scope});assert.equal(created,0);assert.equal(writes,0);
 await button().onclick();assert.equal(created,1);assert.equal(writes,1);assert.equal(links.size,1);
 await w.SpatialCalendar.open(c);await config.callback({access_token:'test',scope});await button().onclick();assert.equal(created,1);assert.equal(records.values().next().value.length,1);
 google='google-two';await w.SpatialCalendar.open(c);await config.callback({access_token:'test',scope});await button().onclick();assert.equal(created,2);assert.equal(links.size,2);
 google='google-one';await w.SpatialCalendar.open(c);const before=reads;w.document.querySelector('#cx-dialog').dispatchEvent(new w.Event('close'));await config.callback({access_token:'test',scope});assert.equal(reads,before);
 assert.equal(w.localStorage.length,0);assert.equal(w.sessionStorage.length,0);
 console.log('PASS minimal scopes, primary/list isolation, consent-before-write, account switching, stable reuse, dialog close and token memory');dom.window.close();
})().catch(e=>{console.error(e);dom.window.close();process.exitCode=1;});
