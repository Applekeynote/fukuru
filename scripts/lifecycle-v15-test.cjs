'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),{JSDOM}=require('../target/navigation-tools/node_modules/jsdom');
const dom=new JSDOM('<main id="cx-main"></main><dialog id="cx-dialog"></dialog>',{runScripts:'outside-only'}),w=dom.window;
let mono=0,check,stopped=0,closed=0;
Object.defineProperty(w.performance,'now',{value:()=>mono});w.Date.now=()=>0;w.setInterval=f=>(check=f,1);w.clearInterval=()=>{};w.HTMLDialogElement.prototype.close=()=>closed++;
w.stopMap=()=>stopped++;
const s=fs.readFileSync('assets/community.js','utf8');w.eval(`const $=s=>document.querySelector(s);let mapController={stop:window.stopMap},threadTimer=null;${s.slice(s.indexOf('let deadlineTimer='),s.indexOf('async function reload('))}window.arm=armDeadline;`);
w.arm({server_at:1798729199,stop_at:1798729200});assert.equal(stopped,0);mono=1000;check();assert.equal(stopped,1);assert.equal(closed,1);assert.match(w.document.body.textContent,/提供期間が終了/);dom.window.close();console.log('PASS live page stops location/AR and closes OAuth dialog at server deadline despite changed client date');
