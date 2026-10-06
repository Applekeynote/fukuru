const assert=require('node:assert/strict'),fs=require('node:fs');
const {JSDOM}=require('../target/navigation-tools/node_modules/jsdom');
const source=fs.readFileSync('assets/community.js','utf8'),dom=new JSDOM('<button data-going="e">Join</button><button data-going="e">Join</button><dialog id="cx-dialog"></dialog>',{runScripts:'outside-only'}),w=dom.window;
w.HTMLDialogElement.prototype.close=function(){this.removeAttribute('open');};
w.eval(`let state={me:{id:'me'}},chosen={id:'e',name:'Walk',owner:'other',end:'2030-01-01T00:00:00Z'},calls=0,handler;
const $=s=>document.querySelector(s),event=()=>chosen,needLogin=()=>true,esc=x=>String(x),toast=()=>{},render=()=>{},dialog=html=>{$('#cx-dialog').innerHTML=html;$('#cx-dialog').setAttribute('open','');},mutate=async(...args)=>{calls++;return handler(...args);};
${source.slice(source.indexOf('const attendancePending='),source.indexOf('function findControls'))}
window.test={toggleAttendance,saveAttendance,setHandler:h=>handler=h,setEvent:e=>chosen=e,calls:()=>calls};`);
(async()=>{
const t=w.test,button=w.document.querySelector('[data-going]');let resolve;
t.setHandler(()=>new Promise(r=>resolve=r));const pending=t.toggleAttendance(button);assert(button.disabled);assert.equal(button.getAttribute('aria-busy'),'true');await t.toggleAttendance(button);assert.equal(t.calls(),1);resolve();await pending;assert.equal(button.disabled,false);assert.equal(button.hasAttribute('aria-busy'),false);
t.setHandler(()=>{throw Error('Network failure');});await assert.rejects(t.toggleAttendance(button),/Network failure/);assert.equal(button.disabled,false);assert.equal(button.hasAttribute('aria-busy'),false);
t.setEvent({id:'e',name:'Ended',end:'2020-01-01T00:00:00Z',my_status:'going'});await t.toggleAttendance(button);assert(w.document.querySelector('[data-cancel-attendance]'));assert.equal(t.calls(),2);
t.setEvent({id:'e',owner:'me',demo:true,end:'2030-01-01T00:00:00Z'});t.setHandler(async()=>{});await t.toggleAttendance(button);assert.equal(t.calls(),3);
t.setEvent({id:'e',owner:'other',demo:true,end:'2030-01-01T00:00:00Z'});await assert.rejects(t.toggleAttendance(button),/デモ/);assert.equal(t.calls(),3);
const refine=fs.readFileSync('assets/refinement.js','utf8');const review=refine.slice(refine.indexOf('async function eventReview('),refine.indexOf('function enhance('));assert(!review.includes('c.request('));assert(!review.includes('/api/event-review'));
console.log('PASS Join busy state + duplicate suppression + failed-request recovery; ended cancellation; owner demo re-Join; no AI call in publish review');
})().catch(e=>{console.error(e);process.exitCode=1;}).finally(()=>w.close());
